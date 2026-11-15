import CMarkdown
import Foundation

/// The core's verdict on one workspace display path
/// (specs/workspace-search, `md_workspace_classify_path`). The core never
/// opens the path; it only validates and classifies the label.
public struct CoreWorkspacePath: Sendable, Equatable {
  /// The core accepts the path. A rejected path is never shown.
  public let isValid: Bool
  /// The final component has a Markdown extension (`md`, `markdown`, `mdx`).
  public let isMarkdown: Bool
  /// A component starts with `.`.
  public let isHidden: Bool
  /// The path has invisible characters, so it can display like another name.
  public let hasInvisible: Bool

  /// The most components a valid path has.
  public static let maxComponents = Int(MD_WORKSPACE_MAX_PATH_DEPTH)

  /// Classify a relative, `/`-separated display path. Pure and thread-safe.
  public static func classify(_ path: String) -> CoreWorkspacePath {
    var info = md_workspace_path_info()
    info.struct_size = UInt32(MemoryLayout<md_workspace_path_info>.size)
    var bytes = Array(path.utf8)
    let status = bytes.withUnsafeMutableBufferPointer { buffer in
      md_workspace_classify_path(buffer.baseAddress, buffer.count, &info)
    }
    guard status == MD_OK, info.error == UInt32(MD_PATH_VALID) else {
      return CoreWorkspacePath(
        isValid: false, isMarkdown: false, isHidden: false, hasInvisible: false)
    }
    return CoreWorkspacePath(
      isValid: true,
      isMarkdown: info.flags & UInt32(MD_PATH_MARKDOWN) != 0,
      isHidden: info.flags & UInt32(MD_PATH_HIDDEN) != 0,
      hasInvisible: info.flags & UInt32(MD_PATH_INVISIBLE) != 0)
  }
}

// MARK: Workspace search (MAC-002C)

/// A UTF-16 range in `text` for a UTF-8 byte range the core reported, or nil
/// when the range is out of bounds or splits a character.
func utf16Range(_ text: String, utf8 range: Range<Int>) -> NSRange? {
  let utf8 = text.utf8
  guard range.lowerBound >= 0, range.upperBound <= utf8.count,
    let start = utf8.index(utf8.startIndex, offsetBy: range.lowerBound, limitedBy: utf8.endIndex),
    let end = utf8.index(start, offsetBy: range.count, limitedBy: utf8.endIndex),
    let lower = start.samePosition(in: text.unicodeScalars),
    let upper = end.samePosition(in: text.unicodeScalars)
  else { return nil }
  return NSRange(lower..<upper, in: text)
}

/// File-name search options (`md_name_options`).
public struct CoreNameOptions: Sendable, Equatable {
  public var caseSensitive: Bool
  public var includeHidden: Bool
  /// Search every entry, not only Markdown.
  public var allFiles: Bool
  public var maxResults: UInt32
  public var maxEntries: UInt32

  public init(
    caseSensitive: Bool = false, includeHidden: Bool = false, allFiles: Bool = false,
    maxResults: UInt32 = UInt32(MD_NAMES_DEFAULT_MAX_RESULTS),
    maxEntries: UInt32 = UInt32(MD_NAMES_DEFAULT_MAX_ENTRIES)
  ) {
    self.caseSensitive = caseSensitive
    self.includeHidden = includeHidden
    self.allFiles = allFiles
    self.maxResults = maxResults
    self.maxEntries = maxEntries
  }
}

/// One file-name hit, best rank first.
public struct CoreNameHit: Sendable, Equatable {
  public enum Rank: UInt32, Sendable, Comparable {
    /// The file name starts with the query.
    case prefix = 1
    /// The query occurs elsewhere in the file name.
    case name
    /// Only in the folder part, or across a `/`.
    case path

    public static func < (lhs: Rank, rhs: Rank) -> Bool { lhs.rawValue < rhs.rawValue }
  }

  /// The index of the entry in the searched array.
  public let entry: Int
  public let rank: Rank
  /// The first occurrence in the entry's path, in UTF-16 units of that path.
  public let range: NSRange
}

public struct CoreNameResult: Sendable, Equatable {
  public let hits: [CoreNameHit]
  /// Every matching entry examined; more than `hits` when truncated.
  public let totalHits: Int
  public let truncated: Bool
  /// Only the first `maxEntries` entries were examined.
  public let entriesLimited: Bool
  public let skippedInvalid: Int
}

/// Content search options and limits (`md_content_options`). Zero limits
/// select the core's defaults; `snippetContext` is used as given.
public struct CoreContentOptions: Sendable, Equatable {
  public var caseSensitive: Bool
  public var wholeWord: Bool
  public var includeHidden: Bool
  public var maxFiles: UInt64
  public var maxTotalBytes: UInt64
  public var maxFileBytes: UInt32
  public var maxMatchesPerFile: UInt32
  public var maxTotalMatches: UInt32
  public var snippetContext: UInt32

  public init(
    caseSensitive: Bool = false, wholeWord: Bool = false, includeHidden: Bool = false,
    maxFiles: UInt64 = UInt64(MD_CONTENT_DEFAULT_MAX_FILES),
    maxTotalBytes: UInt64 = UInt64(MD_CONTENT_DEFAULT_MAX_TOTAL_BYTES),
    maxFileBytes: UInt32 = UInt32(MD_CONTENT_DEFAULT_MAX_FILE_BYTES),
    maxMatchesPerFile: UInt32 = UInt32(MD_CONTENT_DEFAULT_MAX_MATCHES_PER_FILE),
    maxTotalMatches: UInt32 = UInt32(MD_CONTENT_DEFAULT_MAX_TOTAL_MATCHES),
    snippetContext: UInt32 = UInt32(MD_CONTENT_DEFAULT_SNIPPET_CONTEXT)
  ) {
    self.caseSensitive = caseSensitive
    self.wholeWord = wholeWord
    self.includeHidden = includeHidden
    self.maxFiles = maxFiles
    self.maxTotalBytes = maxTotalBytes
    self.maxFileBytes = maxFileBytes
    self.maxMatchesPerFile = maxMatchesPerFile
    self.maxTotalMatches = maxTotalMatches
    self.snippetContext = snippetContext
  }

  /// The per-file size above which files are skipped unread.
  public var fileByteLimit: UInt64 {
    UInt64(maxFileBytes == 0 ? UInt32(MD_CONTENT_DEFAULT_MAX_FILE_BYTES) : maxFileBytes)
  }

  var matchesPerFile: Int {
    Int(
      maxMatchesPerFile == 0 ? UInt32(MD_CONTENT_DEFAULT_MAX_MATCHES_PER_FILE) : maxMatchesPerFile)
  }

  var raw: md_content_options {
    var options = md_content_options()
    options.struct_size = UInt32(MD_CONTENT_OPTIONS_SIZE)
    options.flags =
      (caseSensitive ? UInt32(MD_CONTENT_CASE_SENSITIVE) : 0)
      | (wholeWord ? UInt32(MD_CONTENT_WHOLE_WORD) : 0)
      | (includeHidden ? UInt32(MD_CONTENT_INCLUDE_HIDDEN) : 0)
    options.max_files = maxFiles
    options.max_total_bytes = maxTotalBytes
    options.max_file_bytes = maxFileBytes
    options.max_matches_per_file = maxMatchesPerFile
    options.max_total_matches = maxTotalMatches
    options.snippet_context = snippetContext
    options.hit_size = UInt32(MD_CONTENT_HIT_SIZE)
    return options
  }
}

/// Why a file wasn't searched; the search continues.
public enum CoreContentSkip: UInt32, Sendable, Equatable {
  case hidden = 1
  case notMarkdown, tooLarge, invalidUTF8, parseFailed
}

/// Why a search stopped reading files: a budget ran out, so the results may
/// be incomplete. Permanent for the search.
public enum CoreContentStop: UInt32, Sendable, Equatable {
  case files = 1
  case bytes, matches
}

/// A search's accounting, carried from file to file (`md_content_progress`).
public struct CoreContentProgress: Sendable, Equatable {
  public var stopped: CoreContentStop?
  public var filesRead: UInt64
  public var filesMatched: UInt64
  public var bytesRead: UInt64
  public var matches: UInt64
  public var filesTruncated: UInt64
  public var skippedTooLarge: UInt64
  public var skippedInvalidUTF8: UInt64
  public var skippedParseFailed: UInt64

  init(_ raw: md_content_progress) {
    stopped = raw.stopped == 0 ? nil : CoreContentStop(rawValue: raw.stopped)
    filesRead = raw.files_read
    filesMatched = raw.files_matched
    bytesRead = raw.bytes_read
    matches = raw.matches
    filesTruncated = raw.files_truncated
    skippedTooLarge = raw.skipped_too_large
    skippedInvalidUTF8 = raw.skipped_invalid_utf8
    skippedParseFailed = raw.skipped_parse_failed
  }
}

/// Where a content hit is in the searched bytes, for navigation once the
/// file is open: the span applies directly to a snapshot with the same
/// source length and SHA-256; otherwise resolve the anchor. The anchor is
/// private document-derived state: never log or upload it.
public struct CoreContentTarget: Sendable, Equatable {
  public let sourceLength: UInt64
  /// Lowercase hex.
  public let sourceSHA256: String
  /// Source byte range covering the match.
  public let span: Range<UInt32>
  public let anchor: Data?

  public init(sourceLength: UInt64, sourceSHA256: String, span: Range<UInt32>, anchor: Data?) {
    self.sourceLength = sourceLength
    self.sourceSHA256 = sourceSHA256
    self.span = span
    self.anchor = anchor
  }
}

/// One match in a file, in reading order, with a one-line visible-text
/// snippet. The snippet is for reading only; navigate with `target`.
public struct CoreContentHit: Sendable, Equatable {
  /// 1-based line of the match's first source byte.
  public let line: UInt64
  public let snippet: String
  /// The displayed match in `snippet`, in UTF-16 units.
  public let match: NSRange
  /// Text of the same block was left out before or after the snippet.
  public let leadingMore: Bool
  public let trailingMore: Bool
  public let target: CoreContentTarget
}

/// The hits of one searched file.
public struct CoreContentFile: Sendable, Equatable {
  public let hits: [CoreContentHit]
  /// More matches exist in this file than `hits` holds.
  public let truncated: Bool
}

public enum CoreContentAdmission: Sendable, Equatable {
  case read
  case skipped(CoreContentSkip)
  case stopped(CoreContentStop)
}

public enum CoreContentOutcome: Sendable, Equatable {
  case searched(CoreContentFile)
  case skipped(CoreContentSkip)
  case stopped(CoreContentStop)
}

public enum CoreWorkspaceSearch {
  /// Literal file-name search over display paths (`md_workspace_find_names`).
  /// An empty or whitespace-only query has no hits. Pure; call off the main
  /// thread for large workspaces.
  public static func findNames(
    _ paths: [String], query: String, options: CoreNameOptions = CoreNameOptions()
  ) throws -> CoreNameResult {
    let queryBytes = Array(query.utf8)
    guard queryBytes.count <= Int(MD_WORKSPACE_MAX_QUERY_BYTES) else {
      throw CoreError.queryTooLong
    }
    var packed: [UInt8] = []
    var entries: [md_workspace_entry] = []
    entries.reserveCapacity(paths.count)
    for (index, path) in paths.enumerated() {
      var entry = md_workspace_entry()
      entry.id = UInt64(index)
      entry.path_offset = UInt64(packed.count)
      entry.path_length = UInt32(clamping: path.utf8.count)
      packed.append(contentsOf: path.utf8.prefix(Int(entry.path_length)))
      entries.append(entry)
    }
    var settings = md_name_options()
    settings.struct_size = UInt32(MD_NAME_OPTIONS_SIZE)
    settings.flags =
      (options.caseSensitive ? UInt32(MD_NAMES_CASE_SENSITIVE) : 0)
      | (options.includeHidden ? UInt32(MD_NAMES_INCLUDE_HIDDEN) : 0)
      | (options.allFiles ? UInt32(MD_NAMES_ALL_FILES) : 0)
    settings.max_results = options.maxResults
    settings.max_entries = options.maxEntries
    settings.entry_size = UInt32(MD_WORKSPACE_ENTRY_SIZE)
    settings.hit_size = UInt32(MD_NAME_HIT_SIZE)
    let limit = Int(
      options.maxResults == 0 ? UInt32(MD_NAMES_DEFAULT_MAX_RESULTS) : options.maxResults)
    var hits = [md_name_hit](repeating: md_name_hit(), count: min(limit, max(paths.count, 1)))
    var result = md_name_result()
    for attempt in 0..<2 {
      result = md_name_result()
      result.struct_size = UInt32(MD_NAME_RESULT_SIZE)
      let status = packed.withUnsafeBufferPointer { packed in
        entries.withUnsafeBufferPointer { entries in
          queryBytes.withUnsafeBufferPointer { query in
            hits.withUnsafeMutableBufferPointer { hits in
              md_workspace_find_names(
                packed.baseAddress, packed.count, entries.baseAddress, entries.count,
                query.baseAddress, query.count, &settings, hits.baseAddress, hits.count, &result)
            }
          }
        }
      }
      if status == MD_ERR_BUFFER_TOO_SMALL, attempt == 0, result.hit_count <= UInt64(limit) {
        hits = [md_name_hit](repeating: md_name_hit(), count: Int(result.hit_count))
        continue
      }
      try checked(status)
      break
    }
    guard result.hit_count <= UInt64(hits.count) else { throw CoreError.invalidSnapshot }
    var owned: [CoreNameHit] = []
    owned.reserveCapacity(Int(result.hit_count))
    for hit in hits[0..<Int(result.hit_count)] {
      let index = Int(hit.entry)
      guard index < paths.count, hit.id == UInt64(index), hit.start <= hit.end,
        let rank = CoreNameHit.Rank(rawValue: hit.rank),
        let range = utf16Range(paths[index], utf8: Int(hit.start)..<Int(hit.end))
      else { throw CoreError.invalidSnapshot }
      owned.append(CoreNameHit(entry: index, rank: rank, range: range))
    }
    return CoreNameResult(
      hits: owned, totalHits: Int(clamping: result.total_hits),
      truncated: result.flags & UInt32(MD_NAMES_TRUNCATED) != 0,
      entriesLimited: result.flags & UInt32(MD_NAMES_ENTRIES_LIMITED) != 0,
      skippedInvalid: Int(clamping: result.skipped_invalid))
  }
}

/// One workspace content search: the query, its options and its progress,
/// file after file (`md_workspace_content_admit`/`_search`). Use it from one
/// thread at a time. Every `search` call in the process holds one shared
/// permit, so at most one bounded parse (the CORE-005C memory guard bounds
/// one call) runs at a time, however many windows search.
public final class CoreContentSearch: @unchecked Sendable {
  public let options: CoreContentOptions
  public private(set) var progress: CoreContentProgress
  private var raw = md_content_progress()
  private let query: [UInt8]
  private var settings: md_content_options
  private var hits: [md_content_hit]
  private var text: [UInt8]
  private var anchors: [UInt8]
  private static let permit = NSLock()

  /// Fails for a query over the core's limit. An empty query (after the
  /// core's whitespace normalization) fails on the first call.
  public init(query: String, options: CoreContentOptions = CoreContentOptions()) throws {
    self.query = Array(query.utf8)
    guard self.query.count <= Int(MD_WORKSPACE_MAX_QUERY_BYTES) else {
      throw CoreError.queryTooLong
    }
    self.options = options
    settings = options.raw
    raw.struct_size = UInt32(MD_CONTENT_PROGRESS_SIZE)
    progress = CoreContentProgress(raw)
    // Sized from the limits, so a file never needs a second (re-parsing) call.
    let capacity = options.matchesPerFile
    hits = [md_content_hit](repeating: md_content_hit(), count: capacity)
    text = [UInt8](repeating: 0, count: capacity * Int(MD_CONTENT_MAX_SNIPPET_BYTES))
    anchors = [UInt8](repeating: 0, count: capacity * Int(MD_ANCHOR_MAX_BYTES))
  }

  /// Whether to read a file of about `size` bytes at display path `path`.
  public func admit(path: String, size: UInt64) throws -> CoreContentAdmission {
    let pathBytes = Array(path.utf8)
    var outcome: UInt32 = 0
    var reason: UInt32 = 0
    let status = query.withUnsafeBufferPointer { query in
      pathBytes.withUnsafeBufferPointer { path in
        md_workspace_content_admit(
          query.baseAddress, query.count, &settings, path.baseAddress, path.count, size, &raw,
          &outcome, &reason)
      }
    }
    try checked(status)
    progress = CoreContentProgress(raw)
    switch outcome {
    case UInt32(MD_CONTENT_READ): return .read
    case UInt32(MD_CONTENT_SKIPPED):
      guard let skip = CoreContentSkip(rawValue: reason) else { throw CoreError.invalidSnapshot }
      return .skipped(skip)
    case UInt32(MD_CONTENT_STOPPED):
      guard let stop = CoreContentStop(rawValue: reason) else { throw CoreError.invalidSnapshot }
      return .stopped(stop)
    default: throw CoreError.invalidSnapshot
    }
  }

  /// Search one file's bytes. Bounded by `maxFileBytes`; not interruptible,
  /// so a caller cancels between files. Call off the main thread.
  public func search(path: String, content: Data) throws -> CoreContentOutcome {
    precondition(!Thread.isMainThread)
    let pathBytes = Array(path.utf8)
    var result = md_content_result()
    result.struct_size = UInt32(MD_CONTENT_RESULT_SIZE)
    let status = Self.permit.withLock {
      query.withUnsafeBufferPointer { query in
        pathBytes.withUnsafeBufferPointer { path in
          content.withUnsafeBytes { content in
            hits.withUnsafeMutableBufferPointer { hits in
              text.withUnsafeMutableBufferPointer { text in
                anchors.withUnsafeMutableBufferPointer { anchors in
                  md_workspace_content_search(
                    query.baseAddress, query.count, &settings, path.baseAddress, path.count,
                    content.bindMemory(to: UInt8.self).baseAddress, content.count, &raw,
                    hits.baseAddress, hits.count, text.baseAddress, text.count,
                    anchors.baseAddress, anchors.count, &result)
                }
              }
            }
          }
        }
      }
    }
    // The buffers are sized from the limits, so BUFFER_TOO_SMALL is a bug.
    try checked(status)
    progress = CoreContentProgress(raw)
    switch result.outcome {
    case UInt32(MD_CONTENT_SEARCHED): return .searched(try owned(result))
    case UInt32(MD_CONTENT_SKIPPED):
      guard let skip = CoreContentSkip(rawValue: result.reason) else {
        throw CoreError.invalidSnapshot
      }
      return .skipped(skip)
    case UInt32(MD_CONTENT_STOPPED):
      guard let stop = CoreContentStop(rawValue: result.reason) else {
        throw CoreError.invalidSnapshot
      }
      return .stopped(stop)
    default: throw CoreError.invalidSnapshot
    }
  }

  private func owned(_ result: md_content_result) throws -> CoreContentFile {
    let count = Int(clamping: result.hit_count)
    guard count <= hits.count, result.text_length <= UInt64(text.count),
      result.anchor_length <= UInt64(anchors.count)
    else { throw CoreError.invalidSnapshot }
    let sha = withUnsafeBytes(of: result.source_sha256) { String(decoding: $0, as: UTF8.self) }
    var owned: [CoreContentHit] = []
    owned.reserveCapacity(count)
    for hit in hits[0..<count] {
      let textStart = Int(hit.text_offset)
      let textEnd = textStart + Int(hit.text_length)
      let anchorStart = Int(hit.anchor_offset)
      let anchorEnd = anchorStart + Int(hit.anchor_length)
      guard textEnd <= Int(result.text_length), anchorEnd <= Int(result.anchor_length),
        hit.span_start <= hit.span_end, UInt64(hit.span_end) <= result.source_length,
        hit.match_start <= hit.match_end, Int(hit.match_end) <= Int(hit.text_length),
        let snippet = String(validating: text[textStart..<textEnd], as: UTF8.self),
        let match = utf16Range(snippet, utf8: Int(hit.match_start)..<Int(hit.match_end))
      else { throw CoreError.invalidSnapshot }
      let hasAnchor = hit.flags & UInt32(MD_HIT_ANCHOR) != 0 && hit.anchor_length > 0
      owned.append(
        CoreContentHit(
          line: hit.line, snippet: snippet, match: match,
          leadingMore: hit.flags & UInt32(MD_HIT_LEADING_MORE) != 0,
          trailingMore: hit.flags & UInt32(MD_HIT_TRAILING_MORE) != 0,
          target: CoreContentTarget(
            sourceLength: result.source_length, sourceSHA256: sha,
            span: hit.span_start..<hit.span_end,
            anchor: hasAnchor ? Data(anchors[anchorStart..<anchorEnd]) : nil)))
    }
    return CoreContentFile(hits: owned, truncated: result.flags & UInt32(MD_CONTENT_TRUNCATED) != 0)
  }
}
