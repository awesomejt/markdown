import CMarkdown
import Foundation

public struct CoreFindOptions: Sendable, Equatable {
  public var caseSensitive: Bool
  public var wholeWord: Bool
  public var maxMatches: UInt32

  public init(
    caseSensitive: Bool = false, wholeWord: Bool = false,
    maxMatches: UInt32 = UInt32(MD_FIND_DEFAULT_MAX_MATCHES)
  ) {
    self.caseSensitive = caseSensitive
    self.wholeWord = wholeWord
    self.maxMatches = maxMatches
  }
}

/// The part of one match inside one node: a UTF-8 byte range in that node's
/// display text (empty at 0 for breaks) and its source span.
public struct CoreFindSegment: Sendable, Equatable {
  public let node: UInt32
  public let text: Range<UInt32>
  public let source: Range<UInt32>
  public let exact: Bool
}

/// One match in reading order; `segments` indexes `CoreFindResult.segments`.
public struct CoreFindMatch: Sendable, Equatable {
  public let unit: UInt32
  public let source: Range<UInt32>
  public let exact: Bool
  public let segments: Range<Int>
}

/// Owned copies of one `md_snapshot_find` result for snapshot `generation`.
public struct CoreFindResult: Sendable {
  public let generation: UInt64
  public let matches: [CoreFindMatch]
  public let segments: [CoreFindSegment]
  /// More than `maxMatches` matches exist.
  public let truncated: Bool
}

extension CoreSession {
  /// Literal find over the session's current snapshot (specs/search). The
  /// call cannot be interrupted, so it runs on the shared core worker, after
  /// any queued parse; a request cancelled before it starts never runs. It
  /// fails with `superseded` unless the current snapshot is `generation`,
  /// so node indices always refer to the document the caller rendered.
  public func find(
    _ query: String, options: CoreFindOptions = CoreFindOptions(), generation: UInt64
  )
    async throws -> CoreFindResult
  {
    let bytes = Array(query.utf8)
    guard bytes.count <= Int(MD_FIND_MAX_QUERY_BYTES) else { throw CoreError.queryTooLong }
    try Task.checkCancellation()
    let gate = FindGate()
    return try await withTaskCancellationHandler {
      try await withCheckedThrowingContinuation { continuation in
        CoreRuntime.worker.async {
          guard !gate.isCancelled else {
            continuation.resume(throwing: CancellationError())
            return
          }
          continuation.resume(
            with: Result {
              try autoreleasepool { try self.runFind(bytes, options, generation) }
            })
        }
      }
    } onCancel: {
      gate.cancel()
    }
  }

  private func runFind(_ query: [UInt8], _ options: CoreFindOptions, _ generation: UInt64) throws
    -> CoreFindResult
  {
    precondition(!Thread.isMainThread)
    let snapshot = try currentSnapshot(generation: generation)
    defer { _ = md_snapshot_release(snapshot) }
    var settings = md_find_options()
    settings.struct_size = UInt32(MD_FIND_OPTIONS_SIZE)
    settings.flags =
      (options.caseSensitive ? UInt32(MD_FIND_CASE_SENSITIVE) : 0)
      | (options.wholeWord ? UInt32(MD_FIND_WHOLE_WORD) : 0)
    settings.max_matches = options.maxMatches
    settings.match_size = UInt32(MemoryLayout<md_find_match>.size)
    settings.segment_size = UInt32(MemoryLayout<md_match_segment>.size)
    let limit = Int(
      options.maxMatches == 0 ? UInt32(MD_FIND_DEFAULT_MAX_MATCHES) : options.maxMatches)
    // Size from max_matches so one call is usually enough (FFI host guidance);
    // a match spanning several nodes needs more segments, so allow one retry.
    var matches = [md_find_match](repeating: md_find_match(), count: limit)
    var segments = [md_match_segment](repeating: md_match_segment(), count: limit * 2)
    var result = md_find_result()
    for attempt in 0..<2 {
      result = md_find_result()
      result.struct_size = UInt32(MD_FIND_RESULT_SIZE)
      let status = query.withUnsafeBufferPointer { query in
        matches.withUnsafeMutableBufferPointer { matchBuffer in
          segments.withUnsafeMutableBufferPointer { segmentBuffer in
            md_snapshot_find(
              snapshot, query.baseAddress, query.count, &settings, matchBuffer.baseAddress,
              matchBuffer.count, segmentBuffer.baseAddress, segmentBuffer.count, &result)
          }
        }
      }
      if status == MD_ERR_BUFFER_TOO_SMALL, attempt == 0 {
        // A node continues at most one match, so segments stay below matches + nodes.
        guard result.match_count <= UInt64(limit),
          result.segment_count <= UInt64(limit) + 2 * CoreRuntime.maxNodes
        else { throw CoreError.invalidSnapshot }
        matches = [md_find_match](repeating: md_find_match(), count: Int(result.match_count))
        segments = [md_match_segment](
          repeating: md_match_segment(), count: Int(result.segment_count))
        continue
      }
      try checked(status)
      break
    }
    let matchCount = Int(result.match_count)
    let segmentCount = Int(result.segment_count)
    guard matchCount <= matches.count, segmentCount <= segments.count else {
      throw CoreError.invalidSnapshot
    }
    let exact = UInt32(MD_MATCH_EXACT)
    var ownedSegments: [CoreFindSegment] = []
    ownedSegments.reserveCapacity(segmentCount)
    for segment in segments[0..<segmentCount] {
      guard segment.text_start <= segment.text_end, segment.span_start <= segment.span_end else {
        throw CoreError.invalidSnapshot
      }
      ownedSegments.append(
        CoreFindSegment(
          node: segment.node, text: segment.text_start..<segment.text_end,
          source: segment.span_start..<segment.span_end, exact: segment.flags & exact != 0))
    }
    var ownedMatches: [CoreFindMatch] = []
    ownedMatches.reserveCapacity(matchCount)
    for match in matches[0..<matchCount] {
      let first = Int(match.first_segment)
      let end = first + Int(match.segment_count)
      guard match.span_start <= match.span_end, first <= end, end <= segmentCount else {
        throw CoreError.invalidSnapshot
      }
      ownedMatches.append(
        CoreFindMatch(
          unit: match.unit, source: match.span_start..<match.span_end,
          exact: match.flags & exact != 0, segments: first..<end))
    }
    return CoreFindResult(
      generation: generation, matches: ownedMatches, segments: ownedSegments,
      truncated: result.flags & UInt32(MD_FIND_TRUNCATED) != 0)
  }
}

/// Cancellation before the worker reaches a queued find.
private final class FindGate: @unchecked Sendable {
  private let lock = NSLock()
  private var cancelled = false
  var isCancelled: Bool { lock.withLock { cancelled } }
  func cancel() { lock.withLock { cancelled = true } }
}
