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

  public func parse(_ source: Data, revisionToken: String? = nil) async throws -> CoreResult {
    try Task.checkCancellation()
    try CoreRuntime.admission.reserve()
    defer { CoreRuntime.admission.release() }
    let request = Request()
    return try await withTaskCancellationHandler {
      try Task.checkCancellation()
      return try await withCheckedThrowingContinuation { continuation in
        CoreRuntime.worker.async {
          let result = Result {
            try autoreleasepool { try self.run(source, revisionToken, request) }
          }
          continuation.resume(with: result)
        }
      }
    } onCancel: {
      request.cancel()
    }
  }

  private func run(_ source: Data, _ token: String?, _ request: Request) throws -> CoreResult {
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
    request.install(job)
    defer { request.release() }
    var result = md_job_result()
    result.struct_size = UInt32(MemoryLayout<md_job_result>.size)
    let start = ContinuousClock.now
    try checked(md_job_run(job, &result))
    let parsed = ContinuousClock.now
    defer { if result.snapshot != 0 { _ = md_snapshot_release(result.snapshot) } }
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
    if outcome == .published || outcome == .unchanged {
      guard result.snapshot != 0 else { throw CoreError.invalidSnapshot }
      var view = md_snapshot_view()
      view.struct_size = UInt32(MemoryLayout<md_snapshot_view>.size)
      try checked(md_snapshot_get_view(result.snapshot, &view))
      encoded = .now
      document = try CoreDocument(copying: view)
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

/// The lock prevents a cancellation callback from using an already released job.
private final class Request: @unchecked Sendable {
  private let lock = NSLock()
  private var cancelled = false
  private var job: md_job = 0

  func checkCancellation() throws {
    if lock.withLock({ cancelled }) { throw CancellationError() }
  }

  func install(_ value: md_job) {
    lock.withLock {
      job = value
      if cancelled { _ = md_job_cancel(job) }
    }
  }

  func cancel() {
    lock.withLock {
      cancelled = true
      if job != 0 { _ = md_job_cancel(job) }
    }
  }

  func release() {
    lock.withLock {
      _ = md_job_release(job)
      job = 0
    }
  }
}
