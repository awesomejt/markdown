import Darwin
import Foundation
import MarkdownCore
import Testing

@testable import MarkdownDocument

/// Folder search over a listing (MAC-002C): names and contents in listing
/// order, the listing's filters, containment, budgets, read failures,
/// cancellation and revocation. Synthetic files in a private directory.
@Suite(.serialized)
struct WorkspaceSearchAdapterTests {
  private let directory: URL

  init() throws {
    directory = FileManager.default.temporaryDirectory
      .appendingPathComponent("markdown-search-\(UUID().uuidString)")
    try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
  }

  private func make(_ relative: String, in root: URL, _ text: String) throws {
    let url = root.appendingPathComponent(relative)
    try FileManager.default.createDirectory(
      at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
    try Data(text.utf8).write(to: url)
  }

  private func list(_ root: URL, _ options: WorkspaceOptions = WorkspaceOptions()) async throws
    -> WorkspaceListing
  {
    let folder = try FolderCapability(granted: root)
    return try await Task.detached { try WorkspaceListing.list(folder, options: options) }.value
  }

  private struct Collected: Sendable {
    var names: [String] = []
    var files: [(path: String, lines: [UInt64])] = []
    var summary = WorkspaceSearchSummary()
  }

  private func run(
    _ query: WorkspaceQuery, _ listing: WorkspaceListing,
    limits: CoreContentOptions = CoreContentOptions(),
    cancellation: WorkspaceSearchCancellation = WorkspaceSearchCancellation(),
    after cancelAfter: Int? = nil
  ) async throws -> Collected {
    try await Task.detached {
      var collected = Collected()
      collected.summary = try WorkspaceSearch.run(
        query, in: listing, limits: limits, cancellation: cancellation
      ) { update in
        switch update {
        case .names(let matches, _): collected.names = matches.map(\.path)
        case .file(let file, _):
          collected.files.append((file.item.path, file.hits.map(\.line)))
          if collected.files.count == cancelAfter { cancellation.cancel() }
        case .progress: break
        }
      }
      return collected
    }.value
  }

  @Test func searchesNamesThenContentsInListingOrder() async throws {
    let root = directory.appendingPathComponent("root")
    try make("b.md", in: root, "# B\n\nNothing here.\n")
    try make("guide/install.md", in: root, "# Install\n\nRun the installer.\n")
    try make("a.md", in: root, "# A\n\nFirst line.\n\nInstall it, then install more.\n")
    try make("notes.txt", in: root, "install")
    try make(".hidden/install.md", in: root, "install")
    try make("x.mdx", in: root, "install")
    let outside = directory.appendingPathComponent("outside.md")
    try Data("install".utf8).write(to: outside)
    try FileManager.default.createSymbolicLink(
      at: root.appendingPathComponent("escape.md"), withDestinationURL: outside)

    let listing = try await list(root)
    let result = try await run(WorkspaceQuery("install"), listing)
    #expect(result.names == ["guide/install.md"])
    // Folders first, as the sidebar lists them; .mdx isn't openable as text.
    #expect(result.files.map(\.path) == ["guide/install.md", "a.md"])
    #expect(result.files.last?.lines == [5, 5])
    #expect(result.summary.matches == 4 && result.summary.documents == 2)
    #expect(result.summary.finished && !result.summary.isIncomplete)

    // The listing's hidden choice applies to both searches.
    let hidden = try await list(root, WorkspaceOptions(includeHidden: true))
    let withHidden = try await run(WorkspaceQuery("install"), hidden)
    #expect(withHidden.names.contains(".hidden/install.md"))
    #expect(withHidden.files.map(\.path).contains(".hidden/install.md"))

    // Whole words and case come from the query.
    let exact = try await run(WorkspaceQuery("Install", caseSensitive: true), listing)
    #expect(exact.files.map(\.lines) == [[1], [5]])
    // Whole Words applies to document text; names stay literal.
    let words = try await run(WorkspaceQuery("instal", wholeWord: true), listing)
    #expect(words.files.isEmpty && words.names == ["guide/install.md"])
    let empty = try await run(WorkspaceQuery("  "), listing)
    #expect(empty.files.isEmpty && empty.summary.finished)
  }

  /// A decomposed file name matches a precomposed query.
  @Test func namesMatchInNFC() async throws {
    let root = directory.appendingPathComponent("nfc")
    try make("Cafe\u{301}.md", in: root, "# Menu\n")
    let result = try await run(WorkspaceQuery("Café"), try await list(root))
    #expect(result.names == ["Caf\u{E9}.md"])
  }

  @Test func budgetsAndFailuresAreCounted() async throws {
    let root = directory.appendingPathComponent("budgets")
    for index in 0..<5 { try make("note-\(index).md", in: root, "match \(index)\n") }
    try make("big.md", in: root, String(repeating: "match ", count: 400))
    try Data([0x6D, 0x61, 0x74, 0x63, 0x68, 0xFF]).write(to: root.appendingPathComponent("bad.md"))
    try make("locked.md", in: root, "match")
    chmod(root.appendingPathComponent("locked.md").path, 0)
    defer { chmod(root.appendingPathComponent("locked.md").path, 0o644) }
    let listing = try await list(root)

    let result = try await run(
      WorkspaceQuery("match"), listing, limits: CoreContentOptions(maxFileBytes: 1024))
    #expect(result.files.count == 5)
    #expect(result.summary.skippedTooLarge == 1 && result.summary.skippedInvalidUTF8 == 1)
    #expect(result.summary.unreadable == (getuid() == 0 ? 0 : 1))
    #expect(result.summary.stopped == nil)

    let stopped = try await run(
      WorkspaceQuery("match"), listing, limits: CoreContentOptions(maxFiles: 2))
    #expect(stopped.summary.stopped == .files && stopped.summary.isIncomplete)
    #expect(stopped.summary.filesSearched == 2)

    let matches = try await run(
      WorkspaceQuery("match"), listing, limits: CoreContentOptions(maxTotalMatches: 3))
    #expect(matches.summary.stopped == .matches && matches.summary.matches == 3)
  }

  @Test func cancellationStopsBetweenDocuments() async throws {
    let root = directory.appendingPathComponent("cancel")
    for index in 0..<20 { try make("n-\(index).md", in: root, "word\n") }
    let listing = try await list(root)
    let result = try await run(WorkspaceQuery("word"), listing, after: 3)
    #expect(result.files.count == 3)
    #expect(result.summary.cancelled && !result.summary.finished && result.summary.isIncomplete)

    let early = WorkspaceSearchCancellation()
    early.cancel()
    let none = try await run(WorkspaceQuery("word"), listing, cancellation: early)
    #expect(none.files.isEmpty && none.summary.cancelled)
  }

  @Test func revokingTheFolderStopsTheSearch() async throws {
    let root = directory.appendingPathComponent("revoke")
    for index in 0..<5 { try make("n-\(index).md", in: root, "word\n") }
    let listing = try await list(root)
    listing.folder.revoke()
    await #expect(throws: FolderError.accessRevoked) {
      try await run(WorkspaceQuery("word"), listing)
    }
  }

  /// A 1,000-document folder of about 10 KiB each, searched with no match
  /// and with one match per document. An informal measurement, printed.
  @Test func searchesALargeFolder() async throws {
    let root = directory.appendingPathComponent("large")
    let paragraph = String(
      repeating: "Some *emphasised* prose with `code` and a [link](x.md). ", count: 20)
    let body = (0..<9).map { "## Part \($0)\n\n\(paragraph)\n" }.joined(separator: "\n")
    for group in 0..<10 {
      for index in 0..<100 {
        try make("g\(group)/doc-\(index).md", in: root, body + "\nNeedle \(group)-\(index).\n")
      }
    }
    let listing = try await list(root)
    #expect(listing.fileCount == 1_000)
    let clock = ContinuousClock()
    var none = Collected()
    let missing = try await clock.measure {
      none = try await run(WorkspaceQuery("absent"), listing)
    }
    #expect(none.summary.filesSearched == 1_000 && none.files.isEmpty)
    var hits = Collected()
    let found = try await clock.measure {
      hits = try await run(WorkspaceQuery("needle"), listing)
    }
    #expect(hits.files.count == 1_000 && hits.summary.matches == 1_000)
    print(
      "workspace-search: 1,000 documents (\(body.utf8.count / 1024) KiB each): no match \(missing), 1,000 matches \(found)"
    )
  }
}
