import CMarkdown
import Foundation

public enum CoreOutcome: UInt32, Sendable {
  case published = 1
  case unchanged, superseded, cancelled, failed, closed
}

public enum CoreParseFailure: UInt32, Sendable {
  case invalidUTF8 = 1
  case sourceLimit, eventLimit, nodeLimit, depthLimit, textLimit
  case sourceMapping, model, other
}

/// Adapter-observed read failures, reported to the session as external changes.
public enum CoreUnavailability: Sendable, Equatable {
  case deleted, permissionDenied, accessRevoked, busy, io
}

public enum CoreAvailability: Sendable, Equatable {
  case available
  case unavailable(CoreUnavailability)
}

public struct CoreTimings: Sendable {
  public let parseMilliseconds: Double
  public let encodeMilliseconds: Double
  public let copyMilliseconds: Double
}

public struct CoreResult: Sendable {
  public let outcome: CoreOutcome
  public let generation: UInt64
  public let latest: UInt64
  public let failure: CoreParseFailure?
  public let document: CoreDocument?
  public let timings: CoreTimings
}

/// All handle access is locked; heavy work runs on CoreRuntime.worker. Explicit
/// close and cancellation may race the worker through the thread-safe C ABI.
public final class CoreSession: @unchecked Sendable {
  private let lock = NSLock()
  private var handle: md_session = 0
  private var closing: Task<Void, Error>?

  public init(documentID: String) throws {
    try CoreRuntime.validate()
    var options = md_session_options()
    options.struct_size = UInt32(MemoryLayout<md_session_options>.size)
    options.model_version = UInt32(MD_MODEL_V2)
    options.max_source_bytes = CoreRuntime.maxSourceBytes
    options.max_nodes = CoreRuntime.maxNodes
    options.max_events = CoreRuntime.maxEvents
    options.max_text_bytes = CoreRuntime.maxTextBytes
    try Array(documentID.utf8).withUnsafeBufferPointer {
      try checked(md_session_create($0.baseAddress, $0.count, &options, &handle))
    }
  }

  deinit {
    let old = detach()
    if old != 0 {
      CoreRuntime.worker.async {
        autoreleasepool { _ = md_session_release(old) }
      }
    }
  }

  private func detach() -> md_session {
    lock.withLock {
      let old = handle
      handle = 0
      if old != 0 {
        var cancelled: UInt32 = 0
        _ = md_session_cancel(old, &cancelled)
      }
      return old
    }
  }

  /// Detach/cancel immediately, then await large model destruction on the worker.
  public func close() async throws {
    let operation = lock.withLock {
      if let closing { return closing }
      let old = handle
      handle = 0
      if old != 0 {
        var cancelled: UInt32 = 0
        _ = md_session_cancel(old, &cancelled)
      }
      let operation = Task.detached {
        try await withCheckedThrowingContinuation {
          (continuation: CheckedContinuation<Void, Error>) in
          CoreRuntime.worker.async {
            do {
              try autoreleasepool { try checked(md_session_release(old)) }
              continuation.resume()
            } catch { continuation.resume(throwing: error) }
          }
        }
      }
      closing = operation
      return operation
    }
    try await operation.value
  }

  public func cancel() throws {
    try lock.withLock {
      guard handle != 0 else { throw CoreError.closed }
      var cancelled: UInt32 = 0
      try checked(md_session_cancel(handle, &cancelled))
    }
  }

  /// Record a failed adapter read (no I/O): supersedes in-flight work and
  /// returns the new or already-recorded generation.
  @discardableResult
  public func recordUnavailable(_ change: CoreUnavailability) throws -> UInt64 {
    let (kind, reason) =
      switch change {
      case .deleted: (MD_CHANGE_DELETED, MD_REASON_NONE)
      case .permissionDenied: (MD_CHANGE_UNREADABLE, MD_REASON_PERMISSION_DENIED)
      case .accessRevoked: (MD_CHANGE_UNREADABLE, MD_REASON_ACCESS_REVOKED)
      case .busy: (MD_CHANGE_UNREADABLE, MD_REASON_BUSY)
      case .io: (MD_CHANGE_UNREADABLE, MD_REASON_IO)
      }
    return try lock.withLock {
      guard handle != 0 else { throw CoreError.closed }
      var disposition: UInt32 = 0
      var generation: UInt64 = 0
      try checked(
        md_session_external_change(
          handle, UInt32(kind), UInt32(reason), nil, 0, &disposition, &generation))
      guard disposition == UInt32(MD_DISPOSITION_RECORDED) else { throw CoreError.invalidSnapshot }
      return generation
    }
  }

  /// Run a short call on the live handle under the session lock.
  func withHandle<T>(_ body: (md_session) throws -> T) throws -> T {
    try lock.withLock {
      guard handle != 0 else { throw CoreError.closed }
      return try body(handle)
    }
  }

  public func availability() throws -> CoreAvailability {
    try lock.withLock {
      guard handle != 0 else { throw CoreError.closed }
      var info = md_session_status_info()
      info.struct_size = UInt32(MemoryLayout<md_session_status_info>.size)
      try checked(md_session_get_status(handle, &info))
      switch (info.availability, info.unreadable_reason) {
      case (UInt32(MD_AVAILABLE), _): return .available
      case (UInt32(MD_DELETED), _): return .unavailable(.deleted)
      case (UInt32(MD_UNREADABLE), UInt32(MD_REASON_PERMISSION_DENIED)):
        return .unavailable(.permissionDenied)
      case (UInt32(MD_UNREADABLE), UInt32(MD_REASON_ACCESS_REVOKED)):
        return .unavailable(.accessRevoked)
      case (UInt32(MD_UNREADABLE), UInt32(MD_REASON_BUSY)): return .unavailable(.busy)
      case (UInt32(MD_UNREADABLE), UInt32(MD_REASON_IO)): return .unavailable(.io)
      default: throw CoreError.invalidSnapshot
      }
    }
  }

  /// A new handle to the published snapshot, which must be `generation`.
  /// Parses publish only on the serial worker, so a caller on that worker
  /// keeps the status check and the handle consistent.
  func currentSnapshot(generation: UInt64) throws -> md_snapshot {
    try lock.withLock {
      guard handle != 0 else { throw CoreError.closed }
      var info = md_session_status_info()
      info.struct_size = UInt32(MemoryLayout<md_session_status_info>.size)
      try checked(md_session_get_status(handle, &info))
      guard info.published == generation else { throw CoreError.superseded }
      var snapshot: md_snapshot = 0
      try checked(md_session_current(handle, &snapshot))
      guard snapshot != 0 else { throw CoreError.superseded }
      return snapshot
    }
  }

  /// Parse a new revision. With `copyUnchanged` false, an `unchanged`
  /// outcome (the bytes equal the published source) skips copying the
  /// snapshot again and returns no document.
  public func parse(_ source: Data, revisionToken: String? = nil, copyUnchanged: Bool = true)
    async throws -> CoreResult
  {
    try Task.checkCancellation()
    try CoreRuntime.admission.reserve()
    let request = Request(source: source, token: revisionToken, copyUnchanged: copyUnchanged)
    return try await withTaskCancellationHandler {
      try await withCheckedThrowingContinuation { continuation in
        guard request.installContinuation(continuation) else { return }
        CoreRuntime.worker.async {
          guard let input = request.beginWork() else { return }
          let result = Result {
            try autoreleasepool { try self.run(input.0, input.1, request) }
          }
          request.complete(result)
        }
      }
    } onCancel: {
      request.cancel()
    }
  }

  private func run(_ source: Data, _ token: String?, _ request: Request) throws -> CoreResult {
    let copyUnchanged = request.copyUnchanged
    precondition(!Thread.isMainThread)
    try request.checkCancellation()
    var job: md_job = 0
    var generation: UInt64 = 0
    let tokenBytes = Array((token ?? "").utf8)
    try lock.withLock {
      guard handle != 0 else { throw CoreError.closed }
      try source.withUnsafeBytes { bytes in
        try tokenBytes.withUnsafeBufferPointer { token in
          try checked(
            md_session_submit(
              handle, bytes.bindMemory(to: UInt8.self).baseAddress,
              bytes.count, token.baseAddress, token.count, &job, &generation))
        }
      }
    }
    request.installJob(job)
    defer { request.release() }
    var result = md_job_result()
    result.struct_size = UInt32(MemoryLayout<md_job_result>.size)
    let start = ContinuousClock.now
    try checked(md_job_run(job, &result))
    let parsed = ContinuousClock.now
    // The copied document keeps the handle; otherwise release it here.
    var snapshot = result.snapshot
    defer { if snapshot != 0 { _ = md_snapshot_release(snapshot) } }
    try request.checkCancellation()
    try lock.withLock { if handle == 0 { throw CoreError.closed } }
    guard let outcome = CoreOutcome(rawValue: result.outcome) else {
      throw CoreError.invalidSnapshot
    }
    let failure = CoreParseFailure(rawValue: result.failure)
    if outcome == .failed && failure == nil { throw CoreError.invalidSnapshot }
    var document: CoreDocument?
    var encoded = parsed
    var copied = parsed
    if outcome == .published || (outcome == .unchanged && copyUnchanged) {
      guard snapshot != 0 else { throw CoreError.invalidSnapshot }
      var view = md_snapshot_view()
      view.struct_size = UInt32(MemoryLayout<md_snapshot_view>.size)
      try checked(md_snapshot_get_view(snapshot, &view))
      encoded = .now
      // The owner releases the handle from here on, even if the copy throws.
      let owner = CoreSnapshot(taking: snapshot)
      snapshot = 0
      document = try CoreDocument(copying: view, snapshot: owner)
      copied = .now
    }
    try request.checkCancellation()
    try lock.withLock { if handle == 0 { throw CoreError.closed } }
    func ms(_ a: ContinuousClock.Instant, _ b: ContinuousClock.Instant) -> Double {
      let parts = a.duration(to: b).components
      return Double(parts.seconds) * 1000 + Double(parts.attoseconds) / 1e15
    }
    return CoreResult(
      outcome: outcome, generation: result.generation, latest: result.latest,
      failure: failure, document: document,
      timings: CoreTimings(
        parseMilliseconds: ms(start, parsed), encodeMilliseconds: ms(parsed, encoded),
        copyMilliseconds: ms(encoded, copied)))
  }
}

/// Queued cancellation clears this request's input and releases admission before
/// the worker reaches its lightweight tombstone. Running work keeps its slot
/// until completion. All terminal paths resume/release exactly once.
private final class Request: @unchecked Sendable {
  private enum Phase { case queued, running, finished }
  private let lock = NSLock()
  private var phase: Phase = .queued
  private var source: Data?
  private var token: String?
  private var continuation: CheckedContinuation<CoreResult, Error>?
  private var cancelled = false
  private var job: md_job = 0
  let copyUnchanged: Bool

  init(source: Data, token: String?, copyUnchanged: Bool) {
    self.source = source
    self.token = token
    self.copyUnchanged = copyUnchanged
  }

  func installContinuation(_ callback: CheckedContinuation<CoreResult, Error>) -> Bool {
    let installed = lock.withLock {
      guard phase != .finished else { return false }
      continuation = callback
      return true
    }
    if !installed { callback.resume(throwing: CancellationError()) }
    return installed
  }

  func beginWork() -> (Data, String?)? {
    lock.withLock {
      guard phase == .queued, let source else { return nil }
      let input = (source, token)
      self.source = nil
      token = nil
      phase = .running
      return input
    }
  }

  func checkCancellation() throws {
    if lock.withLock({ cancelled }) { throw CancellationError() }
  }

  func installJob(_ value: md_job) {
    lock.withLock {
      job = value
      if cancelled { _ = md_job_cancel(job) }
    }
  }

  func cancel() {
    let callback: CheckedContinuation<CoreResult, Error>? = lock.withLock {
      cancelled = true
      if phase == .queued {
        phase = .finished
        source = nil
        token = nil
        let callback = continuation
        continuation = nil
        CoreRuntime.admission.release()
        return callback
      }
      if phase == .running && job != 0 { _ = md_job_cancel(job) }
      return nil
    }
    callback?.resume(throwing: CancellationError())
  }

  func release() {
    lock.withLock {
      _ = md_job_release(job)
      job = 0
    }
  }

  func complete(_ result: Result<CoreResult, Error>) {
    let delivery: (CheckedContinuation<CoreResult, Error>?, Result<CoreResult, Error>) =
      lock.withLock {
        precondition(phase == .running)
        phase = .finished
        let callback = continuation
        continuation = nil
        CoreRuntime.admission.release()
        return (callback, cancelled ? .failure(CancellationError()) : result)
      }
    delivery.0?.resume(with: delivery.1)
  }
}
