import Darwin
import Foundation
import MarkdownCore
import Testing

@testable import MarkdownDocument

/// Workspace listing, links between documents and file identity (MAC-002B).
/// Synthetic files in a private temporary directory only.
@Suite(.serialized)
struct WorkspaceTests {
  private let directory: URL

  init() throws {
    directory = FileManager.default.temporaryDirectory
      .appendingPathComponent("markdown-workspace-\(UUID().uuidString)")
    try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
  }

  private func make(_ relative: String, in root: URL, _ text: String = "# Doc\n") throws {
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

  private func names(_ items: [WorkspaceItem]) -> [String] { items.map(\.name) }

  @Test func classifiesThroughTheCore() {
    #expect(CoreWorkspacePath.classify("notes/a.md").isMarkdown)
    #expect(!CoreWorkspacePath.classify("a.md.txt").isMarkdown)
    #expect(!CoreWorkspacePath.classify(".md").isMarkdown)
    #expect(CoreWorkspacePath.classify(".git/a.md").isHidden)
    #expect(CoreWorkspacePath.classify("a\u{200B}b.md").hasInvisible)
    #expect(!CoreWorkspacePath.classify("\u{2764}\u{FE0F}.md").hasInvisible)
    #expect(!CoreWorkspacePath.classify("a\u{202E}b.md").isValid)
    #expect(!CoreWorkspacePath.classify("../a.md").isValid)
    #expect(!CoreWorkspacePath.classify("").isValid)
  }

  @Test func listsMarkdownFoldersFirstAndSorted() async throws {
    let root = directory.appendingPathComponent("root")
    for path in [
      "b.md", "a10.md", "a2.md", "notes.txt", "Guide/intro.md", "Guide/deep/x.markdown",
      "empty/readme.txt", ".hidden/secret.md", ".dot.md", "zeta/z.mdx",
    ] {
      try make(path, in: root)
    }
    let listing = try await list(root)
    // Folders first; numbers sort as Finder sorts them; no Markdown, no folder.
    #expect(names(listing.items) == ["Guide", "zeta", "a2.md", "a10.md", "b.md"])
    #expect(names(listing.item(at: "Guide")!.children) == ["deep", "intro.md"])
    #expect(listing.item(at: "Guide/deep/x.markdown")?.kind == .file)
    #expect(listing.fileCount == 6)
    #expect(listing.item(at: ".hidden") == nil && listing.item(at: ".dot.md") == nil)
    #expect(!listing.isTruncated)
    #expect(WorkspaceListing.ancestors(of: "Guide/deep/x.markdown") == ["Guide", "Guide/deep"])
    #expect(WorkspaceListing.ancestors(of: "b.md").isEmpty)

    let all = try await list(root, WorkspaceOptions(includeHidden: true, allFiles: true))
    #expect(
      names(all.items) == [
        ".hidden", "empty", "Guide", "zeta", ".dot.md", "a2.md", "a10.md",
        "b.md", "notes.txt",
      ])
    #expect(all.item(at: "notes.txt")?.isMarkdown == false)
    #expect(all.item(at: ".hidden/secret.md") != nil)
  }

  @Test func symlinksPackagesAndInvisibleNames() async throws {
    let root = directory.appendingPathComponent("links")
    let outside = directory.appendingPathComponent("outside")
    try make("inside.md", in: root)
    try make("sub/nested.md", in: root)
    try make("away.md", in: outside)
    try make("Bundle.app/Contents/readme.md", in: root)
    try make("zero\u{200B}width.md", in: root)
    let manager = FileManager.default
    try manager.createSymbolicLink(
      at: root.appendingPathComponent("alias.md"),
      withDestinationURL: root.appendingPathComponent("inside.md"))
    try manager.createSymbolicLink(
      at: root.appendingPathComponent("escape.md"),
      withDestinationURL: outside.appendingPathComponent("away.md"))
    try manager.createSymbolicLink(
      at: root.appendingPathComponent("loop"), withDestinationURL: root)
    try manager.createSymbolicLink(
      at: root.appendingPathComponent("dangling.md"),
      withDestinationURL: root.appendingPathComponent("missing.md"))

    let listing = try await list(root)
    #expect(listing.item(at: "alias.md")?.kind == .file)
    #expect(listing.item(at: "escape.md") == nil)
    #expect(listing.item(at: "loop") == nil)
    #expect(listing.item(at: "dangling.md") == nil)
    #expect(listing.item(at: "Bundle.app") == nil)
    #expect(listing.item(at: "zero\u{200B}width.md")?.hasInvisible == true)
    #expect(listing.item(at: "inside.md")?.hasInvisible == false)
    // A document's display path is found through symlinks.
    #expect(listing.path(of: root.appendingPathComponent("sub/nested.md")) == "sub/nested.md")
    #expect(listing.path(of: outside.appendingPathComponent("away.md")) == nil)
    #expect(listing.path(of: root) == nil)
  }

  @Test func unreadableSubfolderAndRootErrors() async throws {
    let root = directory.appendingPathComponent("perm")
    try make("ok.md", in: root)
    try make("locked/inside.md", in: root)
    let locked = root.appendingPathComponent("locked")
    try FileManager.default.setAttributes([.posixPermissions: 0o000], ofItemAtPath: locked.path)
    defer {
      try? FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: locked.path)
    }
    let listing = try await list(root)
    #expect(listing.item(at: "locked")?.isUnreadable == true)
    #expect(listing.item(at: "ok.md") != nil)

    let missing = directory.appendingPathComponent("missing")
    await #expect(throws: FolderError.notFound) { _ = try await list(missing) }

    let folder = try FolderCapability(granted: root)
    folder.revoke()
    await #expect(throws: FolderError.accessRevoked) {
      _ = try await Task.detached { try WorkspaceListing.list(folder) }.value
    }
  }

  @Test func listingIsBoundedAndCancellable() async throws {
    let root = directory.appendingPathComponent("bounded")
    for index in 0..<50 { try make("f\(index).md", in: root) }
    var depth = ""
    for level in 0..<8 { depth += (depth.isEmpty ? "" : "/") + "d\(level)" }
    try make(depth + "/deep.md", in: root)

    let few = try await list(root, WorkspaceOptions(maxEntries: 10))
    #expect(few.isTruncated && few.entriesExamined == 10)

    let shallow = try await list(root, WorkspaceOptions(maxDepth: 3))
    #expect(shallow.isTruncated)
    #expect(shallow.item(at: depth + "/deep.md") == nil)

    let folder = try FolderCapability(granted: root)
    await #expect(throws: CancellationError.self) {
      _ = try await Task.detached { try WorkspaceListing.list(folder, isCancelled: { true }) }
        .value
    }
  }

  /// Large-folder browsing: 20,000 Markdown files in 200 folders list off
  /// the main thread within a generous bound, and the limit truncates.
  @Test func largeFolder() async throws {
    let root = directory.appendingPathComponent("large")
    let manager = FileManager.default
    let body = Data("# Doc\n".utf8)
    for group in 0..<200 {
      let folder = root.appendingPathComponent("group-\(group)")
      try manager.createDirectory(at: folder, withIntermediateDirectories: true)
      for index in 0..<100 {
        try body.write(to: folder.appendingPathComponent("note-\(index).md"))
      }
    }
    let clock = ContinuousClock()
    let start = clock.now
    let listing = try await list(root)
    let elapsed = clock.now - start
    #expect(listing.fileCount == 20_000)
    #expect(listing.items.count == 200)
    #expect(listing.item(at: "group-7")?.children.count == 100)
    #expect(elapsed < .seconds(10), "listed 20,000 files in \(elapsed)")

    let capped = try await list(root, WorkspaceOptions(maxEntries: 5_000))
    #expect(capped.isTruncated && capped.fileCount < 5_000)
  }

  @Test func linksStayInsideTheirFolder() async throws {
    let root = directory.appendingPathComponent("linked")
    try make("docs/a.md", in: root)
    try make("docs/b c.md", in: root)
    try make("docs/literal%2e.md", in: root)
    try make("other/c.md", in: root)
    try make("image.png", in: root, "png")
    try make("away.md", in: directory)
    let folder = try FolderCapability(granted: root)
    let source = try folder.document(root.appendingPathComponent("docs/a.md"))

    let result = await Task.detached { () -> [Result<DocumentCapability, LinkError>] in
      [
        "b c.md", "../other/c.md", "literal%2e.md", "../../away.md", "../image.png", "/etc/x.md",
        "missing.md",
      ]
      .map { path in
        Result { () throws(LinkError) in try source.linkedDocument(path, through: folder) }
      }
    }.value
    let resolved = root.resolvingSymlinksInPath()
    #expect(try result[0].get().url.lastPathComponent == "b c.md")
    #expect(try result[0].get().folder === folder.grant)
    #expect(
      try result[1].get().url.standardizedFileURL.path
        == resolved.appendingPathComponent("other/c.md").path)
    // A decoded path is never decoded again.
    #expect(try result[2].get().url.lastPathComponent == "literal%2e.md")
    #expect(throws: LinkError.outsideFolder(granted: true)) { try result[3].get() }
    #expect(throws: LinkError.unsupportedType) { try result[4].get() }
    #expect(throws: LinkError.invalid) { try result[5].get() }
    #expect(try result[6].get().url.lastPathComponent == "missing.md")

    // A document opened by itself may link inside its own folder only.
    let alone = try DocumentCapability(granted: root.appendingPathComponent("docs/a.md"))
    let standalone = await Task.detached { () -> [Result<DocumentCapability, LinkError>] in
      ["b c.md", "../other/c.md"].map { path in
        Result { () throws(LinkError) in try alone.linkedDocument(path, through: nil) }
      }
    }.value
    #expect(try standalone[0].get().folder == nil)
    #expect(throws: LinkError.outsideFolder(granted: false)) { try standalone[1].get() }

    folder.revoke()
    let revoked = await Task.detached {
      Result { () throws(LinkError) in try source.linkedDocument("b c.md", through: folder) }
    }.value
    #expect(throws: LinkError.accessRevoked) { try revoked.get() }
  }

  @Test func fileIdentityFindsHardLinks() async throws {
    let root = directory.appendingPathComponent("identity")
    try make("a.md", in: root)
    try FileManager.default.linkItem(
      at: root.appendingPathComponent("a.md"), to: root.appendingPathComponent("hard.md"))
    try make("b.md", in: root)
    let a = try DocumentCapability(granted: root.appendingPathComponent("a.md"))
    let hard = try DocumentCapability(granted: root.appendingPathComponent("hard.md"))
    let b = try DocumentCapability(granted: root.appendingPathComponent("b.md"))
    let missing = try DocumentCapability(granted: root.appendingPathComponent("none.md"))
    let ids = await Task.detached { [a, hard, b, missing].map { $0.fileIdentity() } }.value
    #expect(a != hard)
    #expect(ids[0] != nil && ids[0] == ids[1])
    #expect(ids[0] != ids[2])
    #expect(ids[3] == nil)
  }
}
