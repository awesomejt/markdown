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
///
/// A `DocumentWatcher` reports external changes (MAC-001H). The loader asks
/// the core session whether the file's current revision token needs a read
/// and, if so, refreshes in the background: the presented document stays in
/// place until the new one is loaded, and a failed refresh keeps it as
/// `stale`. Notices that arrive during a load are checked once it finishes.
@MainActor
public final class DocumentLoader {
  public enum State: Sendable {
    case empty
    case loading(DocumentCapability)
    case loaded(LoadedDocument)
    case failed(DocumentCapability, DocumentError)
    /// A refresh failed (the file was deleted, became unreadable or is
    /// invalid); the last loaded version is still presented.
    case stale(LoadedDocument, DocumentError)
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
  private var watcher: DocumentWatcher?
  /// The current request is a background refresh.
  private var refreshing = false
  /// A change notice arrived during a load; check again when it finishes.
  private var recheck = false
  private var checkTask: Task<Void, Never>?

  /// Session factory; tests inject a failure.
  var makeSession: (String) throws -> CoreSession = { try CoreSession(documentID: $0) }

  /// Bound on waiting for core admission while other windows are parsing.
  nonisolated static let busyTimeout = Duration.seconds(30)

  public init() {}

  public var capability: DocumentCapability? {
    switch state {
    case .empty: nil
    case .loading(let capability), .failed(let capability, _): capability
    case .loaded(let loaded), .stale(let loaded, _): loaded.capability
    }
  }

  /// The presented document, current or stale.
  public var document: LoadedDocument? {
    switch state {
    case .loaded(let loaded), .stale(let loaded, _): loaded
    case .empty, .loading, .failed: nil
    }
  }

  /// Whether a background refresh, or a check for one, is running.
  public var isRefreshing: Bool { refreshing || checkTask != nil }

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
        watcher = DocumentWatcher(url: capability.url) { [weak self] in
          self?.checkForChanges()
        }
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

  /// Respond to a possible external change (a watcher report, or the window
  /// becoming key): read again only when the session says the file's current
  /// revision token needs it. The stat runs off the main actor, because a
  /// network volume can be slow.
  public func checkForChanges() {
    guard let session, let capability = sessionCapability else { return }
    watcher?.rearm()
    if task != nil || checkTask != nil {
      recheck = true
      return
    }
    let id = request
    checkTask = Task { [weak self] in
      let token = await Task.detached { DocumentReader.revisionToken(capability) }.value
      // A newer request (an open or reload) replaced this check.
      guard let self, id == self.request, self.checkTask != nil else { return }
      self.checkTask = nil
      guard self.session === session, self.task == nil else { return }
      // A missing or unreadable file needs a read to report it; so does any
      // token the session hasn't accepted.
      switch try? session.externalChange(token: token) {
      case .unchanged?, .pending?: break
      case .readRequired?, nil: self.start(capability, refresh: true)
      }
    }
  }

  /// Cancel pending work and await the session's native cleanup.
  public func close() async {
    request &+= 1
    task?.cancel()
    task = nil
    checkTask?.cancel()
    checkTask = nil
    watcher?.cancel()
    watcher = nil
    refreshing = false
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
    checkTask?.cancel()
    checkTask = nil
    watcher?.cancel()
    watcher = nil
    refreshing = false
    lastRead = nil
    if let old = session {
      session = nil
      Task { try? await old.close() }
    }
    sessionCapability = nil
  }

  /// Start a request. A refresh leaves the presented state in place until
  /// it finishes, and doesn't copy a snapshot whose bytes are unchanged.
  private func start(_ capability: DocumentCapability, refresh: Bool = false) {
    guard let session else { return }
    request &+= 1
    let id = request
    task?.cancel()
    checkTask?.cancel()
    checkTask = nil
    // A notice that arrives from here on is checked after this request.
    recheck = false
    refreshing = refresh
    if !refresh { state = .loading(capability) }
    let previous = lastRead
    task = Task { [weak self] in
      let outcome = await Self.load(
        capability, session: session, previous: previous, copyUnchanged: !refresh)
      self?.finish(id, capability, outcome)
    }
  }

  private enum Outcome: Sendable {
    case document(CoreDocument, DocumentBytes)
    /// A refresh read the bytes already published.
    case unchanged(DocumentBytes)
    case failure(DocumentError)
    case discarded
  }

  private func finish(_ id: UInt64, _ capability: DocumentCapability, _ outcome: Outcome) {
    guard id == request else {
      discardedResults += 1
      return
    }
    task = nil
    let refresh = refreshing
    refreshing = false
    switch outcome {
    case .document(let document, let bytes):
      lastRead = (bytes.device, bytes.volume)
      state = .loaded(
        LoadedDocument(
          capability: capability, document: document, revisionToken: bytes.revisionToken))
    case .unchanged(let bytes):
      lastRead = (bytes.device, bytes.volume)
      // The session has its availability back; so does a stale document.
      if let shown = document {
        state = .loaded(
          LoadedDocument(
            capability: capability, document: shown.document, revisionToken: bytes.revisionToken))
      }
    case .failure(var error):
      // Permission lost after a successful read is a revoked grant.
      if error == .permissionDenied, lastRead != nil { error = .accessRevoked }
      // Only the current request records availability, so a late failure
      // can't supersede a newer successful load in the core session.
      if let change = error.coreChange { _ = try? session?.recordUnavailable(change) }
      if refresh, let shown = document {
        state = .stale(shown, error)
      } else {
        state = .failed(capability, error)
      }
    case .discarded:
      // The current request is never cancelled by this loader; treat an
      // unexpected core cancellation as a failure rather than loading forever.
      // A refresh keeps what is presented.
      if !refresh || document == nil { state = .failed(capability, .parseFailed) }
    }
    if recheck {
      recheck = false
      checkForChanges()
    }
  }

  private nonisolated static func load(
    _ capability: DocumentCapability, session: CoreSession,
    previous: (device: dev_t, volume: URL?)?, copyUnchanged: Bool
  ) async -> Outcome {
    do {
      let bytes = try await DocumentReader.read(capability, limit: CoreRuntime.maxSourceBytes)
      let result = try await parse(bytes, session: session, copyUnchanged: copyUnchanged)
      switch result.outcome {
      case .unchanged where !copyUnchanged:
        return .unchanged(bytes)
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
  private nonisolated static func parse(
    _ bytes: DocumentBytes, session: CoreSession, copyUnchanged: Bool
  ) async throws -> CoreResult {
    let deadline = ContinuousClock.now.advanced(by: busyTimeout)
    while true {
      do {
        return try await session.parse(
          bytes.data, revisionToken: bytes.revisionToken, copyUnchanged: copyUnchanged)
      } catch CoreError.busy where ContinuousClock.now < deadline {
        try await Task.sleep(for: .milliseconds(20))
      }
    }
  }
}
