import Darwin
import Foundation
import MarkdownCore
import Testing

@testable import MarkdownDocument

/// Folder grants, recents and their recovery states (MAC-002A). Synthetic
/// files in a private temporary directory only.
@Suite(.serialized)
@MainActor
struct FolderTests {
  private let directory: URL

  init() throws {
    directory = FileManager.default.temporaryDirectory
      .appendingPathComponent("markdown-folder-\(UUID().uuidString)")
    try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
  }

  private func folder(_ name: String) throws -> URL {
    let url = directory.appendingPathComponent(name, isDirectory: true)
    try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
    return url
  }

  @discardableResult
  private func write(_ url: URL, _ text: String = "# Doc\n") throws -> URL {
    try Data(text.utf8).write(to: url)
    return url
  }

  private func validation(_ capability: FolderCapability) async -> FolderError? {
    await Task.detached { () -> FolderError? in
      do throws(FolderError) {
        try capability.validate()
        return nil
      } catch { return error }
    }.value
  }

  private func readError(_ capability: DocumentCapability) async -> DocumentError? {
    do {
      _ = try await DocumentReader.read(capability, limit: CoreRuntime.maxSourceBytes)
      return nil
    } catch { return error as? DocumentError }
  }

  @Test func containment() throws {
    let root = try folder("root")
    let inside = try write(root.appendingPathComponent("a.md"))
    try FileManager.default.createDirectory(
      at: root.appendingPathComponent("sub"), withIntermediateDirectories: true)
    let nested = try write(root.appendingPathComponent("sub/b.md"))
    let outside = try write(directory.appendingPathComponent("outside.md"))
    try FileManager.default.createSymbolicLink(
      at: root.appendingPathComponent("escape.md"), withDestinationURL: outside)
    let granted = try FolderCapability(granted: root)

    #expect(try granted.document(inside).folder === granted.grant)
    #expect(granted.contains(nested))
    #expect(!granted.contains(outside))
    #expect(!granted.contains(root.appendingPathComponent("../outside.md")))
    #expect(!granted.contains(root.appendingPathComponent("escape.md")), "symlink escape")
    #expect(!granted.contains(root), "the folder itself isn't a document in it")
    #expect(!granted.contains(directory.appendingPathComponent("root-sibling/a.md")))
    #expect(throws: FolderError.outsideFolder) { try granted.document(outside) }
    // A missing file is still placed by its folder, so the read reports it.
    #expect(granted.contains(root.appendingPathComponent("missing.md")))
    #expect(throws: FolderError.notDirectory) {
      try FolderCapability(granted: URL(string: "https://example.com/folder")!)
    }
    #expect(!granted.description.contains(directory.path))
    #expect(!granted.grant.description.contains(directory.path))
  }

  @Test func revocationPropagates() async throws {
    let root = try folder("revoked")
    let granted = try FolderCapability(granted: root)
    let first = try granted.document(try write(root.appendingPathComponent("a.md")))
    let second = try granted.document(try write(root.appendingPathComponent("b.md")))
    #expect(await readError(first) == nil)
    #expect(await validation(granted) == nil)

    granted.revoke()
    #expect(first.isRevoked && second.isRevoked)
    #expect(await readError(first) == .accessRevoked)
    #expect(await readError(second) == .accessRevoked)
    #expect(await validation(granted) == .accessRevoked)
    #expect(throws: FolderError.accessRevoked) {
      try granted.document(root.appendingPathComponent("a.md"))
    }
    let token = await Task.detached { DocumentReader.revisionToken(first) }.value
    #expect(token == nil, "a revoked document has no current revision")

    // A new grant of the same folder is independent of the revoked one.
    let again = try FolderCapability(granted: root)
    #expect(await readError(try again.document(root.appendingPathComponent("a.md"))) == nil)
  }

  @Test func loaderReportsRevocation() async throws {
    let root = try folder("loader")
    let granted = try FolderCapability(granted: root)
    let loader = DocumentLoader()
    loader.open(try granted.document(try write(root.appendingPathComponent("a.md"))))
    try await settle(loader)
    #expect(loader.document != nil)
    granted.revoke()
    loader.reload()
    try await settle(loader)
    if case .failed(_, let error) = loader.state {
      #expect(error == .accessRevoked)
    } else {
      Issue.record("revoked reload must fail")
    }
    #expect(loader.coreAvailability == .unavailable(.accessRevoked))
    loader.clear()
    #expect(loader.capability == nil)
    await loader.close()
  }

  @Test func folderValidationErrors() async throws {
    #expect(
      await validation(try FolderCapability(granted: directory.appendingPathComponent("none")))
        == .notFound)
    let file = try write(directory.appendingPathComponent("file.md"))
    #expect(await validation(try FolderCapability(granted: file)) == .notDirectory)
    let denied = try folder("denied")
    let deniedGrant = try FolderCapability(granted: denied)
    if getuid() != 0 {
      #expect(chmod(denied.path, 0) == 0)
      #expect(await validation(deniedGrant) == .permissionDenied)
      #expect(chmod(denied.path, 0o755) == 0)
      #expect(await validation(deniedGrant) == nil)
      // Permission lost after the folder was read is a revoked grant.
      #expect(chmod(denied.path, 0) == 0)
      #expect(await validation(deniedGrant) == .accessRevoked)
      #expect(chmod(denied.path, 0o755) == 0)
    }
    for error in [
      FolderError.notFound, .volumeUnavailable, .permissionDenied, .accessRevoked, .notDirectory,
      .outsideFolder, .unreadable,
    ] {
      #expect(!error.message.contains("/") && !error.recovery.contains(directory.path))
    }
    #expect(FolderError.notFound.isRecoverable && !FolderError.outsideFolder.isRecoverable)
  }

  @Test func movedMissingAndDeniedRecents() async throws {
    let original = try folder("before")
    try write(original.appendingPathComponent("doc.md"))
    let folderItem = try #require(RecentItem.capture(original, kind: .folder))
    let documentItem = try #require(
      RecentItem.capture(original.appendingPathComponent("doc.md"), kind: .document))
    #expect(folderItem.resolve() == RecentResolution(status: .available, url: folderItem.url))

    // Moved: the bookmark finds the folder, and its document, at the new place.
    let moved = directory.appendingPathComponent("after", isDirectory: true)
    try FileManager.default.moveItem(at: original, to: moved)
    let foundFolder = folderItem.resolve()
    #expect(foundFolder.status == .moved)
    #expect(foundFolder.url?.resolvingSymlinksInPath().path == moved.resolvingSymlinksInPath().path)
    let foundDocument = documentItem.resolve()
    #expect(foundDocument.status == .moved)
    #expect(foundDocument.url?.lastPathComponent == "doc.md")

    // Denied: the item is there but can't be read.
    if getuid() != 0 {
      let movedItem = try #require(RecentItem.capture(moved, kind: .folder))
      #expect(chmod(moved.path, 0) == 0)
      #expect(movedItem.resolve().status == .permissionDenied)
      #expect(chmod(moved.path, 0o755) == 0)
    }

    // Missing: deleted outright.
    try FileManager.default.removeItem(at: moved)
    #expect(folderItem.resolve() == RecentResolution(status: .missing, url: nil))
    #expect(documentItem.resolve().status == .missing)
    #expect(RecentStatus.missing.summary != nil && RecentStatus.moved.summary == nil)
  }

  @Test func recentsArePersistentBoundedAndDeduplicated() throws {
    let storage = MemoryRecentsStorage()
    let recents = Recents(storage: storage)
    var changes = 0
    recents.onChange = { changes += 1 }
    var items: [RecentItem] = []
    for index in 0..<12 {
      let url = try write(directory.appendingPathComponent("doc\(index).md"))
      let item = try #require(RecentItem.capture(url, kind: .document))
      items.append(item)
      recents.note(item)
    }
    #expect(recents.documents.count == Recents.limit)
    #expect(recents.documents.first?.name == "doc11.md")
    #expect(!recents.documents.contains { $0.name == "doc0.md" || $0.name == "doc1.md" })
    #expect(changes == 12)

    // The same file again, a hard link to it, and a new id for its place.
    recents.note(items[5])
    #expect(recents.documents.first?.id == items[5].id && recents.documents.count == 10)
    let link = directory.appendingPathComponent("hardlink.md")
    try FileManager.default.linkItem(at: items[6].url, to: link)
    recents.note(try #require(RecentItem.capture(link, kind: .document)))
    #expect(recents.documents.count == 10, "a hard link is the same document")
    #expect(recents.documents.first?.name == "hardlink.md")
    let elsewhere = try write(directory.appendingPathComponent("relocated.md"))
    let relocated = try #require(RecentItem.capture(elsewhere, kind: .document, id: items[7].id))
    recents.note(relocated)
    #expect(recents.documents.filter { $0.id == items[7].id }.map(\.name) == ["relocated.md"])

    let folderItem = try #require(RecentItem.capture(try folder("f"), kind: .folder))
    recents.note(folderItem)
    #expect(recents.folders.map(\.id) == [folderItem.id] && recents.item(folderItem.id) != nil)

    // Persisted: a new store over the same storage sees the same lists.
    let reloaded = Recents(storage: storage)
    #expect(reloaded.documents == recents.documents && reloaded.folders == recents.folders)
    reloaded.remove(folderItem.id)
    #expect(reloaded.folders.isEmpty && Recents(storage: storage).folders.isEmpty)
    reloaded.clear()
    #expect(Recents(storage: storage).documents.isEmpty)
  }

  @Test func storedRecentsAreUntrusted() throws {
    let valid = try #require(
      RecentItem.capture(try write(directory.appendingPathComponent("ok.md")), kind: .document))
    func stored(_ object: [String: Any]) throws -> MemoryRecentsStorage {
      MemoryRecentsStorage(try JSONSerialization.data(withJSONObject: object))
    }
    let encoded = try JSONSerialization.jsonObject(with: JSONEncoder().encode(valid))
    var relative = try #require(encoded as? [String: Any])
    relative["path"] = "relative/ok.md"
    var hugeName = try #require(encoded as? [String: Any])
    hugeName["name"] = String(repeating: "x", count: 5000)
    hugeName["id"] = UUID().uuidString
    var wrongKind = try #require(encoded as? [String: Any])
    wrongKind["kind"] = "folder"
    wrongKind["id"] = UUID().uuidString

    // Malformed entries are dropped; the valid one survives.
    let mixed = Recents(
      storage: try stored([
        "version": 1, "documents": [relative, hugeName, wrongKind, encoded, encoded],
      ]))
    #expect(mixed.documents == [valid] && mixed.folders.isEmpty)
    // A structurally bad entry (missing bookmark, unknown kind, bad id) is
    // skipped on its own; the valid entry beside it survives.
    var noBookmark = try #require(encoded as? [String: Any])
    noBookmark.removeValue(forKey: "bookmark")
    var unknownKind = try #require(encoded as? [String: Any])
    unknownKind["kind"] = "workspace"
    var badID = try #require(encoded as? [String: Any])
    badID["id"] = 42
    let structural = Recents(
      storage: try stored([
        "version": 1, "documents": [noBookmark, unknownKind, badID, "text", encoded],
      ]))
    #expect(structural.documents == [valid])
    // Not JSON, or an unknown shape: empty, never a crash.
    #expect(Recents(storage: MemoryRecentsStorage(Data("not json".utf8))).documents.isEmpty)
    #expect(Recents(storage: try stored(["version": 0, "documents": [encoded]])).documents.isEmpty)

    // A newer version is used, but never overwritten.
    let future = try stored(["version": 99, "documents": [encoded], "extra": true])
    let before = future.load()
    let newer = Recents(storage: future)
    #expect(newer.readOnly && newer.documents == [valid])
    newer.clear()
    #expect(future.load() == before)
    // Also when nothing in the newer list can be decoded by this version.
    let alien = try stored([
      "version": 99, "documents": [unknownKind], "folders": "a newer shape",
    ])
    let untouched = alien.load()
    let alienRecents = Recents(storage: alien)
    #expect(alienRecents.readOnly && alienRecents.documents.isEmpty)
    alienRecents.note(valid)
    #expect(alien.load() == untouched, "a newer list is never overwritten")

    // An oversized file on disk is ignored before it is read.
    let big = directory.appendingPathComponent("recents.json")
    let handle = try FileHandle(forWritingTo: try write(big, ""))
    try handle.truncate(atOffset: UInt64(Recents.maxStoredBytes + 1))
    try handle.close()
    #expect(FileRecentsStorage(url: big).load() == nil)
  }

  @Test func fileStorageAndEnvironment() throws {
    let url = directory.appendingPathComponent("nested/recents.json")
    let storage = FileRecentsStorage(url: url, ownsFolder: true)
    let recents = Recents(storage: storage)
    recents.note(
      try #require(
        RecentItem.capture(try write(directory.appendingPathComponent("a.md")), kind: .document)))
    #expect(Recents(storage: FileRecentsStorage(url: url)).documents.count == 1)
    let mode = try FileManager.default.attributesOfItem(atPath: url.path)[.posixPermissions]
    #expect((mode as? NSNumber)?.intValue == 0o600)
    // An existing, wider folder is narrowed; no temporary file is left.
    #expect(chmod(url.deletingLastPathComponent().path, 0o755) == 0)
    recents.clear()
    let folderMode = try FileManager.default.attributesOfItem(
      atPath: url.deletingLastPathComponent().path)[.posixPermissions]
    #expect((folderMode as? NSNumber)?.intValue == 0o700)
    #expect(
      try FileManager.default.contentsOfDirectory(atPath: url.deletingLastPathComponent().path)
        == ["recents.json"])
    #expect(!recents.saveFailed)

    // A folder named through the override is never narrowed.
    let shared = try folder("shared")
    #expect(chmod(shared.path, 0o755) == 0)
    let override = try #require(
      FileRecentsStorage.standard(
        environment: ["MARKDOWN_RECENTS_PATH": shared.appendingPathComponent("r.json").path]))
    #expect(!override.ownsFolder)
    Recents(storage: override).clear()
    let sharedMode = try FileManager.default.attributesOfItem(atPath: shared.path)[
      .posixPermissions]
    #expect((sharedMode as? NSNumber)?.intValue == 0o755)
    // A save that can't be written is reported, and the list stays in memory.
    if getuid() != 0 {
      #expect(chmod(shared.path, 0o555) == 0)
      let blocked = Recents(storage: override)
      blocked.note(
        recents.documents.isEmpty
          ? try #require(
            RecentItem.capture(try write(directory.appendingPathComponent("b.md")), kind: .document)
          )
          : recents.documents[0])
      #expect(blocked.saveFailed && blocked.documents.count == 1)
      #expect(chmod(shared.path, 0o755) == 0)
    }
    #expect(
      FileRecentsStorage.standard(environment: ["MARKDOWN_RECENTS_PATH": url.path])?.url == url)
    #expect(FileRecentsStorage.standard(environment: [:])?.url.lastPathComponent == "recents.json")
  }

  private func settle(_ loader: DocumentLoader) async throws {
    let deadline = ContinuousClock.now.advanced(by: .seconds(20))
    while loader.isLoading && ContinuousClock.now < deadline {
      try await Task.sleep(for: .milliseconds(10))
    }
    #expect(!loader.isLoading)
  }
}
