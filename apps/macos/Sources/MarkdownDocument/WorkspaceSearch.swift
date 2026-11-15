import Darwin
import Foundation
import MarkdownCore

/// What a workspace search looks for (MAC-002C).
public struct WorkspaceQuery: Sendable, Equatable {
  public var text: String
  public var caseSensitive: Bool
  public var wholeWord: Bool

  public init(_ text: String, caseSensitive: Bool = false, wholeWord: Bool = false) {
    self.text = text
    self.caseSensitive = caseSensitive
    self.wholeWord = wholeWord
  }

  /// Whether there is anything to search for.
  public var isEmpty: Bool { text.allSatisfy(\.isWhitespace) }
}

/// One file-name match: the listed document and the match in its path.
public struct WorkspaceNameMatch: Sendable {
  public let item: WorkspaceItem
  /// The path as matched and shown: normalized to NFC.
  public let path: String
  /// The match in `path`, in UTF-16 units.
  public let range: NSRange
}

/// The matches in one document, in reading order.
public struct WorkspaceFileMatches: Sendable {
  public let item: WorkspaceItem
  public let hits: [CoreContentHit]
  /// The document has more matches than the per-file limit.
  public let truncated: Bool
}

/// A finished (or stopped) search's accounting, for the summary line.
public struct WorkspaceSearchSummary: Sendable, Equatable {
  public var names = 0
  /// More file names match than are shown.
  public var namesTruncated = false
  public var documents = 0
  public var filesSearched = 0
  public var filesMatched = 0
  public var matches = 0
  public var filesTruncated = 0
  public var skippedTooLarge = 0
  public var skippedInvalidUTF8 = 0
  public var skippedTooComplex = 0
  /// Documents that couldn't be read (permission, I/O, a type change).
  public var unreadable = 0
  /// A budget stopped the search: the results may be incomplete.
  public var stopped: CoreContentStop?
  /// The listing it searched was itself cut short.
  public var listingTruncated = false
  public var cancelled = false
  public var finished = false
  /// Documents that grew past the size limit during their read.
  var grewTooLarge = 0

  public init() {}

  /// Whether the results may leave out matching documents.
  public var isIncomplete: Bool {
    stopped != nil || listingTruncated || cancelled || namesTruncated
  }
}

/// Progress delivered while a search runs, on the search's thread.
public enum WorkspaceSearchUpdate: Sendable {
  case names([WorkspaceNameMatch], WorkspaceSearchSummary)
  case file(WorkspaceFileMatches, WorkspaceSearchSummary)
  case progress(WorkspaceSearchSummary)
}

/// Stops a running search between files, and any read waiting for file
/// coordination.
public final class WorkspaceSearchCancellation: @unchecked Sendable {
  private let lock = NSLock()
  private var cancelled = false
  fileprivate let read = ReadCancellation()

  public init() {}
  public var isCancelled: Bool { lock.withLock { cancelled } }
  public func cancel() {
    lock.withLock { cancelled = true }
    read.cancel()
  }
}

/// Searches a folder's listed documents by name and content (MAC-002C), over
/// the core's workspace primitives (specs/workspace-search). The adapter owns
/// access: it searches exactly the documents a `WorkspaceListing` lists (so
/// the files sidebar's hidden and all-files choices apply, and symlinks that
/// leave the folder are never followed), and reads each one through the
/// folder's grant, one bounded read at a time. The core only sees display
/// paths and bytes. Nothing is indexed or kept between searches.
public enum WorkspaceSearch {
  /// File names are matched in NFC, so a decomposed name on disk matches a
  /// typed (precomposed) query. Document text is matched as written.
  public static func run(
    _ query: WorkspaceQuery, in listing: WorkspaceListing,
    limits: CoreContentOptions = CoreContentOptions(),
    cancellation: WorkspaceSearchCancellation = WorkspaceSearchCancellation(),
    report: (WorkspaceSearchUpdate) -> Void
  ) throws -> WorkspaceSearchSummary {
    dispatchPrecondition(condition: .notOnQueue(.main))
    var summary = WorkspaceSearchSummary()
    summary.listingTruncated = listing.isTruncated
    guard !query.isEmpty else {
      summary.finished = true
      return summary
    }
    let files = documents(in: listing)

    // File names first: one pure core call over the listing.
    let shown = files.map { $0.path.precomposedStringWithCanonicalMapping }
    let names = try CoreWorkspaceSearch.findNames(
      shown, query: query.text.precomposedStringWithCanonicalMapping,
      options: CoreNameOptions(
        caseSensitive: query.caseSensitive, includeHidden: listing.options.includeHidden,
        allFiles: listing.options.allFiles))
    let nameMatches = names.hits.map {
      WorkspaceNameMatch(item: files[$0.entry], path: shown[$0.entry], range: $0.range)
    }
    summary.names = names.totalHits
    summary.namesTruncated = names.truncated || names.entriesLimited
    report(.names(nameMatches, summary))

    // Then contents, document by document in listing order.
    var options = limits
    options.caseSensitive = query.caseSensitive
    options.wholeWord = query.wholeWord
    options.includeHidden = listing.options.includeHidden
    let search = try CoreContentSearch(query: query.text, options: options)
    var lastReport = ContinuousClock.now
    for item in files where item.isMarkdown {
      if cancellation.isCancelled {
        summary.cancelled = true
        break
      }
      if listing.folder.isRevoked { throw FolderError.accessRevoked }
      let hits = try searchFile(item, listing, search, cancellation, &summary)
      if let hits {
        report(.file(hits, summary))
        lastReport = .now
      } else if summary.stopped != nil {
        break
      } else if lastReport.duration(to: .now) > .milliseconds(100) {
        report(.progress(summary))
        lastReport = .now
      }
    }
    summary.finished = !summary.cancelled
    return summary
  }

  /// The listed files, in the sidebar's order.
  static func documents(in listing: WorkspaceListing) -> [WorkspaceItem] {
    var result: [WorkspaceItem] = []
    result.reserveCapacity(listing.fileCount)
    func walk(_ items: [WorkspaceItem]) {
      for item in items {
        switch item.kind {
        case .file: result.append(item)
        case .folder: walk(item.children)
        }
      }
    }
    walk(listing.items)
    return result
  }

  /// Search one document. Returns its matches when it has any, and records
  /// skips, stops and read failures in `summary`.
  private static func searchFile(
    _ item: WorkspaceItem, _ listing: WorkspaceListing, _ search: CoreContentSearch,
    _ cancellation: WorkspaceSearchCancellation, _ summary: inout WorkspaceSearchSummary
  ) throws -> WorkspaceFileMatches? {
    defer { update(&summary, search.progress) }
    let capability: DocumentCapability
    do throws(FolderError) {
      capability = try listing.folder.document(listing.url(for: item))
    } catch {
      if error == .accessRevoked { throw error }
      // Moved out, or now a symlink that leaves the folder.
      summary.unreadable += 1
      return nil
    }
    // Only what the reader can open (`.mdx` isn't a text type here).
    guard capability.isSupportedType else { return nil }
    let size = fileSize(capability)
    if let size {
      switch try search.admit(path: item.path, size: size) {
      case .read: break
      case .skipped: return nil
      case .stopped(let reason):
        summary.stopped = reason
        return nil
      }
    }
    let bytes: Data
    do {
      bytes = try DocumentReader.read(
        capability, limit: search.options.fileByteLimit, cancellation: cancellation.read,
        coordinated: false
      ).data
    } catch is CancellationError {
      return nil
    } catch DocumentError.accessRevoked {
      throw FolderError.accessRevoked
    } catch DocumentError.tooLarge {
      // It grew past the limit after the size check.
      summary.grewTooLarge += 1
      return nil
    } catch {
      summary.unreadable += 1
      return nil
    }
    switch try search.search(path: item.path, content: bytes) {
    case .searched(let file) where !file.hits.isEmpty:
      return WorkspaceFileMatches(item: item, hits: file.hits, truncated: file.truncated)
    case .searched, .skipped: return nil
    case .stopped(let reason):
      summary.stopped = reason
      return nil
    }
  }

  private static func update(
    _ summary: inout WorkspaceSearchSummary, _ progress: CoreContentProgress
  ) {
    summary.filesSearched = Int(clamping: progress.filesRead)
    summary.filesMatched = Int(clamping: progress.filesMatched)
    summary.documents = summary.filesMatched
    summary.matches = Int(clamping: progress.matches)
    summary.filesTruncated = Int(clamping: progress.filesTruncated)
    // Growth past the limit during a read is counted here too.
    summary.skippedTooLarge = Int(clamping: progress.skippedTooLarge) + summary.grewTooLarge
    summary.skippedInvalidUTF8 = Int(clamping: progress.skippedInvalidUTF8)
    summary.skippedTooComplex = Int(clamping: progress.skippedParseFailed)
    if let stopped = progress.stopped { summary.stopped = stopped }
  }

  /// The document's size from a stat, or nil when it can't be examined (the
  /// read then reports why).
  private static func fileSize(_ capability: DocumentCapability) -> UInt64? {
    capability.accessing { () -> UInt64? in
      var info = stat()
      let found = capability.url.withUnsafeFileSystemRepresentation { path in
        path.map { stat($0, &info) == 0 } ?? false
      }
      guard found, info.st_mode & S_IFMT == S_IFREG, info.st_size >= 0 else { return nil }
      return UInt64(info.st_size)
    }
  }
}
