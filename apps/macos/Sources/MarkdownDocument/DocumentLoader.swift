import Foundation
import MarkdownCore

public struct LoadedDocument: Sendable {
  public let capability: DocumentCapability
  public let document: CoreDocument
  public let revisionToken: String
}

/// One window's document. Every open/reload takes a new request number; only
/// the newest request publishes, so a slow older read or parse is never shown.
/// Reads and parses run off the main actor; state changes happen on it.
@MainActor
public final class DocumentLoader {
  public enum State: Sendable {
    case empty
    case loading(DocumentCapability)
    case loaded(LoadedDocument)
    case failed(DocumentCapability, DocumentError)
  }

  public private(set) var state: State = .empty {
    didSet { onChange?(state) }
  }
  public var onChange: ((State) -> Void)?
  /// Completed results dropped because a newer request replaced them.
  public private(set) var discardedResults = 0

  private var request: UInt64 = 0
  private var task: Task<Void, Never>?
  private var session: CoreSession?
  private var sessionCapability: DocumentCapability?
  private var lastRead: (device: dev_t, volume: URL?)?

  /// Session factory; tests inject a failure.
  var makeSession: (String) throws -> CoreSession = { try CoreSession(documentID: $0) }

  /// Bound on waiting for core admission while other windows are parsing.
  nonisolated static let busyTimeout = Duration.seconds(30)

  public init() {}

  public var capability: DocumentCapability? {
    switch state {
    case .empty: nil
    case .loading(let capability), .failed(let capability, _): capability
    case .loaded(let loaded): loaded.capability
    }
  }

  /// The core session's recorded availability for the current document.
  public var coreAvailability: CoreAvailability? { try? session?.availability() }

  /// Find in the presented snapshot. Fails with `CoreError.superseded` once
  /// the session has moved past `generation`, and `closed` without a session.
  public func find(_ query: String, options: CoreFindOptions, generation: UInt64)
    async throws -> CoreFindResult
  {
    guard let session else { throw CoreError.closed }
    return try await session.find(query, options: options, generation: generation)
  }

  public var isLoading: Bool {
    if case .loading = state { return true }
    return false
  }

  public func open(_ capability: DocumentCapability) {
    if capability != sessionCapability {
      retireSession()
      do {
        // A random logical identity: the core never receives the path.
        session = try makeSession(UUID().uuidString)
        sessionCapability = capability
      } catch {
        request &+= 1
        task?.cancel()
        state = .failed(capability, .parseFailed)
        return
      }
    }
    start(capability)
  }

  public func reload() {
    guard let capability = sessionCapability ?? self.capability else { return }
    // A failed session creation left no session: retry the whole open.
    if session == nil { open(capability) } else { start(capability) }
  }

  /// Cancel pending work and await the session's native cleanup.
  public func close() async {
    request &+= 1
    task?.cancel()
    task = nil
    let old = session
    session = nil
    sessionCapability = nil
    lastRead = nil
    state = .empty
    try? await old?.close()
  }

  private func retireSession() {
    task?.cancel()
    task = nil
    lastRead = nil
    if let old = session {
      session = nil
      Task { try? await old.close() }
    }
    sessionCapability = nil
  }

  private func start(_ capability: DocumentCapability) {
    guard let session else { return }
    request &+= 1
    let id = request
    task?.cancel()
    state = .loading(capability)
    let previous = lastRead
    task = Task { [weak self] in
      let outcome = await Self.load(capability, session: session, previous: previous)
      self?.finish(id, capability, outcome)
    }
  }

  private enum Outcome: Sendable {
    case document(CoreDocument, DocumentBytes)
    case failure(DocumentError)
    case discarded
  }

  private func finish(_ id: UInt64, _ capability: DocumentCapability, _ outcome: Outcome) {
    guard id == request else {
      discardedResults += 1
      return
    }
    task = nil
    switch outcome {
    case .document(let document, let bytes):
      lastRead = (bytes.device, bytes.volume)
      state = .loaded(
        LoadedDocument(
          capability: capability, document: document, revisionToken: bytes.revisionToken))
    case .failure(var error):
      // Permission lost after a successful read is a revoked grant.
      if error == .permissionDenied, lastRead != nil { error = .accessRevoked }
      // Only the current request records availability, so a late failure
      // can't supersede a newer successful load in the core session.
      if let change = error.coreChange { _ = try? session?.recordUnavailable(change) }
      state = .failed(capability, error)
    case .discarded:
      // The current request is never cancelled by this loader; treat an
      // unexpected core cancellation as a failure rather than loading forever.
      state = .failed(capability, .parseFailed)
    }
  }

  private nonisolated static func load(
    _ capability: DocumentCapability, session: CoreSession,
    previous: (device: dev_t, volume: URL?)?
  ) async -> Outcome {
    do {
      let bytes = try await DocumentReader.read(capability, limit: CoreRuntime.maxSourceBytes)
      let result = try await parse(bytes, session: session)
      switch result.outcome {
      case .published, .unchanged:
        guard let document = result.document else { return .failure(.parseFailed) }
        return .document(document, bytes)
      case .superseded, .cancelled, .closed:
        return .discarded
      case .failed:
        switch result.failure {
        case .invalidUTF8: return .failure(.invalidText)
        case .sourceLimit: return .failure(.tooLarge(limit: CoreRuntime.maxSourceBytes))
        case .eventLimit, .nodeLimit, .depthLimit, .textLimit: return .failure(.tooComplex)
        default: return .failure(.parseFailed)
        }
      }
    } catch is CancellationError {
      return .discarded
    } catch let error as DocumentError {
      if error == .notFound, let previous,
        DocumentReader.volumeIsGone(device: previous.device, volume: previous.volume)
      {
        return .failure(.volumeUnavailable)
      }
      return .failure(error)
    } catch CoreError.busy {
      return .failure(.busy)
    } catch CoreError.closed {
      return .discarded
    } catch {
      return .failure(.parseFailed)
    }
  }

  /// Admission is shared by all windows (one running, one queued); wait briefly
  /// for a slot instead of failing a load that merely overlaps another window's.
  private nonisolated static func parse(_ bytes: DocumentBytes, session: CoreSession)
    async throws -> CoreResult
  {
    let deadline = ContinuousClock.now.advanced(by: busyTimeout)
    while true {
      do {
        return try await session.parse(bytes.data, revisionToken: bytes.revisionToken)
      } catch CoreError.busy where ContinuousClock.now < deadline {
        try await Task.sleep(for: .milliseconds(20))
      }
    }
  }
}
