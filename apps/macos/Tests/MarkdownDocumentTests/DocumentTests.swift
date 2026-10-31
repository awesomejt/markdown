import Darwin
import Foundation
import MarkdownCore
import Testing

@testable import MarkdownDocument

@Suite(.serialized)
@MainActor
struct DocumentTests {
  private let directory: URL

  init() throws {
    directory = FileManager.default.temporaryDirectory
      .appendingPathComponent("markdown-document-\(UUID().uuidString)")
    try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
  }

  private func file(_ name: String, _ contents: Data) throws -> DocumentCapability {
    let url = directory.appendingPathComponent(name)
    try contents.write(to: url)
    return try DocumentCapability(granted: url, securityScoped: false)
  }

  private func capability(_ name: String) throws -> DocumentCapability {
    try DocumentCapability(granted: directory.appendingPathComponent(name), securityScoped: false)
  }

  private func readError(_ capability: DocumentCapability) async -> DocumentError? {
    do {
      _ = try await DocumentReader.read(capability, limit: CoreRuntime.maxSourceBytes)
      return nil
    } catch { return error as? DocumentError }
  }

  private func settle(_ loader: DocumentLoader) async throws {
    let deadline = ContinuousClock.now.advanced(by: .seconds(20))
    while loader.isLoading && ContinuousClock.now < deadline {
      try await Task.sleep(for: .milliseconds(10))
    }
    #expect(!loader.isLoading)
  }

  private func expectNoHandles() throws {
    let live = try CoreRuntime.liveHandles()
    #expect(live.sessions == 0 && live.jobs == 0 && live.snapshots == 0)
  }

  private func failure(_ loader: DocumentLoader) -> DocumentError? {
    if case .failed(_, let error) = loader.state { return error }
    return nil
  }

  private func loadedTitle(_ loader: DocumentLoader) -> String? {
    if case .loaded(let loaded) = loader.state { return loaded.document.title }
    return nil
  }

  @Test func capabilityAndTypes() throws {
    #expect(try capability("a.md").isSupportedType)
    #expect(try capability("a.markdown").isSupportedType)
    #expect(try capability("README").isSupportedType)
    #expect(try capability("notes.txt").isSupportedType)
    #expect(try !capability("image.png").isSupportedType)
    #expect(try !capability("page.html").isSupportedType)
    #expect(throws: DocumentError.unsupportedType) {
      try DocumentCapability(granted: URL(string: "https://example.com/a.md")!)
    }
    let granted = try capability("a.md")
    #expect(!granted.description.contains(directory.path))
    #expect(granted == (try capability("./a.md")))
  }

  @Test func boundedReadsAndTypedErrors() async throws {
    let valid = try file("valid.md", Data("# Valid\n".utf8))
    let first = try await DocumentReader.read(valid, limit: CoreRuntime.maxSourceBytes)
    #expect(first.data == Data("# Valid\n".utf8))
    #expect(!first.revisionToken.contains(directory.path))
    try Data("# Changed\n".utf8).write(to: valid.url)
    let second = try await DocumentReader.read(valid, limit: CoreRuntime.maxSourceBytes)
    #expect(second.revisionToken != first.revisionToken)

    #expect(await readError(try capability("missing.md")) == .notFound)
    try FileManager.default.createDirectory(
      at: directory.appendingPathComponent("folder.md"), withIntermediateDirectories: false)
    #expect(await readError(try capability("folder.md")) == .notRegularFile(isDirectory: true))
    // A FIFO must be rejected without blocking in open().
    #expect(mkfifo(directory.appendingPathComponent("pipe.md").path, 0o600) == 0)
    #expect(await readError(try capability("pipe.md")) == .notRegularFile(isDirectory: false))
    #expect(await readError(try file("image.png", Data([0x89, 0x50]))) == .unsupportedType)

    // Sparse: rejected from its size without reading 5 MiB.
    let large = try file("large.md", Data())
    let handle = try FileHandle(forWritingTo: large.url)
    try handle.truncate(atOffset: CoreRuntime.maxSourceBytes + 1)
    try handle.close()
    #expect(await readError(large) == .tooLarge(limit: CoreRuntime.maxSourceBytes))
    let exact = try file("exact.md", Data(repeating: 0x61, count: Int(CoreRuntime.maxSourceBytes)))
    #expect(
      try await DocumentReader.read(exact, limit: CoreRuntime.maxSourceBytes).data.count
        == Int(CoreRuntime.maxSourceBytes))

    if getuid() != 0 {
      let denied = try file("denied.md", Data("# Denied".utf8))
      #expect(chmod(denied.url.path, 0) == 0)
      #expect(await readError(denied) == .permissionDenied)
    }

    let cancelled = ReadCancellation()
    cancelled.cancel()
    let outcome = await Task.detached {
      Result {
        try DocumentReader.read(valid, limit: CoreRuntime.maxSourceBytes, cancellation: cancelled)
      }
    }.value
    #expect(throws: CancellationError.self) { try outcome.get() }
  }

  @Test func newestRequestPublishesOnly() async throws {
    // A slow ~3 MiB document, then a small one before the first completes.
    var text = "---\ntitle: Big\n---\n"
    for index in 0..<40_000 { text += "Paragraph \(index) with *emphasis* and `code`.\n\n" }
    let big = try file("big.md", Data(text.utf8))
    let small = try file("small.md", Data("---\ntitle: Small\n---\n# Small\n".utf8))
    let loader = DocumentLoader()
    var published: [String?] = []
    loader.onChange = { state in
      if case .loaded(let loaded) = state { published.append(loaded.document.title) }
    }
    loader.open(big)
    loader.open(small)
    try await settle(loader)
    #expect(loadedTitle(loader) == "Small")
    let deadline = ContinuousClock.now.advanced(by: .seconds(20))
    while loader.discardedResults < 1 && ContinuousClock.now < deadline {
      try await Task.sleep(for: .milliseconds(10))
    }
    #expect(loader.discardedResults == 1)
    #expect(published == ["Small"])

    // Rapid reloads of one document: only the last request publishes.
    for _ in 0..<5 { loader.reload() }
    try await settle(loader)
    #expect(loadedTitle(loader) == "Small")
    while loader.discardedResults < 5 && ContinuousClock.now < deadline {
      try await Task.sleep(for: .milliseconds(10))
    }
    #expect(loader.discardedResults == 5)
    await loader.close()
    try expectNoHandles()
  }

  @Test func revokedDeletedAndRecoveredAccess() async throws {
    let loader = DocumentLoader()
    let document = try file("doc.md", Data("---\ntitle: Doc\n---\nText\n".utf8))
    loader.open(document)
    try await settle(loader)
    #expect(loadedTitle(loader) == "Doc")
    #expect(loader.coreAvailability == .available)

    if getuid() != 0 {
      #expect(chmod(document.url.path, 0) == 0)
      loader.reload()
      try await settle(loader)
      #expect(failure(loader) == .accessRevoked)
      #expect(loader.coreAvailability == .unavailable(.accessRevoked))
      #expect(chmod(document.url.path, 0o644) == 0)
      loader.reload()
      try await settle(loader)
      #expect(loadedTitle(loader) == "Doc")
      #expect(loader.coreAvailability == .available)
    }

    try FileManager.default.removeItem(at: document.url)
    loader.reload()
    try await settle(loader)
    #expect(failure(loader) == .notFound)
    #expect(loader.coreAvailability == .unavailable(.deleted))

    // First-open permission failure stays a plain denial, not a revocation.
    if getuid() != 0 {
      let denied = try file("denied.md", Data("# Denied".utf8))
      #expect(chmod(denied.url.path, 0) == 0)
      loader.open(denied)
      try await settle(loader)
      #expect(failure(loader) == .permissionDenied)
    }
    loader.open(try file("invalid.md", Data([0x23, 0x20, 0xff])))
    try await settle(loader)
    #expect(failure(loader) == .invalidText)
    loader.open(try file("photo.jpg", Data([0xff, 0xd8])))
    try await settle(loader)
    #expect(failure(loader) == .unsupportedType)
    await loader.close()
    try expectNoHandles()
  }

  @Test func reloadRetriesFailedSessionCreation() async throws {
    let loader = DocumentLoader()
    loader.makeSession = { _ in throw CoreError.closed }
    let document = try file("retry.md", Data("---\ntitle: Retry\n---\n".utf8))
    loader.open(document)
    #expect(failure(loader) == .parseFailed)
    loader.makeSession = { try CoreSession(documentID: $0) }
    loader.reload()
    try await settle(loader)
    #expect(loadedTitle(loader) == "Retry")
    #expect(DocumentError.busy.coreChange == nil)  // Admission timeout, not file state.
    await loader.close()
    try expectNoHandles()
  }

  @Test func detachedVolumeIsUnavailable() async throws {
    let image = directory.appendingPathComponent("volume.dmg")
    let mount = directory.appendingPathComponent("mount")
    try hdiutil(["create", "-size", "4m", "-fs", "HFS+", "-volname", "MarkdownTest", image.path])
    try hdiutil(["attach", "-nobrowse", "-mountpoint", mount.path, image.path])
    var attached = true
    defer { if attached { try? hdiutil(["detach", "-force", mount.path]) } }
    let url = mount.appendingPathComponent("remote.md")
    try Data("---\ntitle: Remote\n---\n".utf8).write(to: url)
    let loader = DocumentLoader()
    loader.open(try DocumentCapability(granted: url, securityScoped: false))
    try await settle(loader)
    #expect(loadedTitle(loader) == "Remote")
    try hdiutil(["detach", mount.path])
    attached = false
    loader.reload()
    try await settle(loader)
    #expect(failure(loader) == .volumeUnavailable)
    #expect(loader.coreAvailability == .unavailable(.io))
    await loader.close()
  }

  private func hdiutil(_ arguments: [String]) throws {
    let process = Process()
    process.executableURL = URL(fileURLWithPath: "/usr/bin/hdiutil")
    process.arguments = arguments + ["-quiet"]
    try process.run()
    process.waitUntilExit()
    try #require(process.terminationStatus == 0, "hdiutil \(arguments[0]) failed")
  }

  /// Images resolve inside the document's folder only, through symlinks, and
  /// must be regular raster image files within the size limit (MAC-001E).
  @Test func resourcesStayInTheDocumentFolder() throws {
    let manager = FileManager.default
    let folder = directory.appendingPathComponent("docs")
    try manager.createDirectory(
      at: folder.appendingPathComponent("img"), withIntermediateDirectories: true)
    let document = folder.appendingPathComponent("readme.md")
    try Data("# R\n".utf8).write(to: document)
    let png = Data([0x89, 0x50, 0x4E, 0x47])
    try png.write(to: folder.appendingPathComponent("img/a.png"))
    try png.write(to: folder.appendingPathComponent("literal%2e.png"))
    try png.write(to: directory.appendingPathComponent("outside.png"))
    try Data("<svg/>".utf8).write(to: folder.appendingPathComponent("vector.svg"))
    try Data("text".utf8).write(to: folder.appendingPathComponent("notes.txt"))
    try manager.createDirectory(
      at: folder.appendingPathComponent("folder.png"), withIntermediateDirectories: false)
    #expect(mkfifo(folder.appendingPathComponent("pipe.png").path, 0o600) == 0)
    try manager.createSymbolicLink(
      at: folder.appendingPathComponent("escape.png"),
      withDestinationURL: directory.appendingPathComponent("outside.png"))
    try manager.createSymbolicLink(
      at: folder.appendingPathComponent("inside.png"),
      withDestinationURL: folder.appendingPathComponent("img/a.png"))
    try manager.createSymbolicLink(
      at: folder.appendingPathComponent("up"), withDestinationURL: directory)
    let large = folder.appendingPathComponent("large.png")
    #expect(manager.createFile(atPath: large.path, contents: nil))
    let handle = try FileHandle(forWritingTo: large)
    try handle.truncate(atOffset: 101)
    try handle.close()

    let resources = DocumentResources(
      document: try DocumentCapability(granted: document, securityScoped: false))
    func read(_ path: String) -> Result<Data, ResourceError> {
      Result { () throws(ResourceError) in try resources.readImage(path, limit: 100) }
    }
    #expect(try read("img/a.png").get() == png)
    #expect(try read("./img/../img/a.png").get() == png)
    #expect(try read("inside.png").get() == png)
    // The core's path is decoded exactly once; a literal percent stays literal.
    #expect(try read("literal%2e.png").get() == png)
    #expect(read("literal..png") == .failure(.notFound))
    let expected: [(String, ResourceError)] = [
      ("../outside.png", .outsideFolder),
      ("escape.png", .outsideFolder),
      ("up/outside.png", .outsideFolder),
      ("/etc/hosts", .outsideFolder),
      ("..", .outsideFolder),
      ("missing.png", .notFound),
      ("vector.svg", .unsupportedType),
      ("notes.txt", .unsupportedType),
      ("folder.png", .notRegularFile),
      ("pipe.png", .notRegularFile),
      ("large.png", .tooLarge(limit: 100)),
    ]
    for (path, error) in expected {
      #expect(read(path) == .failure(error), "\(path)")
    }
    if getuid() != 0 {
      let denied = folder.appendingPathComponent("img/a.png")
      #expect(chmod(denied.path, 0) == 0)
      #expect(read("img/a.png") == .failure(.permissionDenied))
      #expect(chmod(denied.path, 0o644) == 0)
    }
  }

  @Test func messagesNameNoPath() {
    let errors: [DocumentError] = [
      .notFound, .volumeUnavailable, .permissionDenied, .accessRevoked,
      .notRegularFile(isDirectory: true), .notRegularFile(isDirectory: false), .unsupportedType,
      .tooLarge(limit: CoreRuntime.maxSourceBytes), .invalidText, .tooComplex, .busy,
      .unreadable(errno: EIO), .parseFailed,
    ]
    for error in errors {
      #expect(!error.message.isEmpty && !error.recovery.isEmpty)
    }
    #expect(DocumentError.tooLarge(limit: CoreRuntime.maxSourceBytes).message.contains("5 MB"))
  }

  // MARK: External changes (MAC-001H)

  private func waitUntil(
    _ message: Comment, seconds: Int = 5, _ condition: () -> Bool
  ) async throws {
    let deadline = ContinuousClock.now.advanced(by: .seconds(seconds))
    while !condition() && ContinuousClock.now < deadline {
      try await Task.sleep(for: .milliseconds(10))
    }
    #expect(condition(), message)
  }

  private func titled(_ title: String) -> Data { Data("---\ntitle: \(title)\n---\nBody\n".utf8) }

  private func stale(_ loader: DocumentLoader) -> (String?, DocumentError)? {
    if case .stale(let loaded, let error) = loader.state { return (loaded.document.title, error) }
    return nil
  }

  /// Writes, an atomic replacement, deletion and reappearance each report;
  /// the watcher follows the replacement file and the reappeared path.
  @Test func watcherFollowsReplacementAndDeletion() async throws {
    let url = try file("watched.md", Data("one".utf8)).url
    var reports = 0
    let watcher = DocumentWatcher(url: url) { reports += 1 }
    defer { watcher.cancel() }
    try await waitUntil("watcher arms") { watcher.isArmed }
    func expectReport(_ action: String, _ change: () throws -> Void) async throws {
      let before = reports
      try change()
      try await waitUntil("\(action) reports") { reports > before }
    }
    try await expectReport("a write") { try Data("two".utf8).write(to: url) }
    // Data.write(options: .atomic) writes a temporary file and renames it over.
    try await expectReport("an atomic save") {
      try Data("three".utf8).write(to: url, options: .atomic)
    }
    try await waitUntil("watcher follows the replacement") { watcher.isArmed }
    try await expectReport("a write to the replacement") {
      let handle = try FileHandle(forWritingTo: url)
      try handle.seekToEnd()
      try handle.write(contentsOf: Data(" more".utf8))
      try handle.close()
    }
    try await expectReport("a deletion") { try FileManager.default.removeItem(at: url) }
    #expect(!watcher.isArmed, "a deleted file leaves the folder watched")
    // Unrelated folder changes don't report while the file is missing.
    let quiet = reports
    _ = try file("other.md", Data("x".utf8))
    try await Task.sleep(for: .milliseconds(DocumentWatcher.delayMilliseconds * 3))
    #expect(reports == quiet)
    try await expectReport("reappearance") { try Data("four".utf8).write(to: url) }
    try await waitUntil("watcher follows the reappeared file") { watcher.isArmed }
    // A burst is coalesced into one report.
    let burst = reports
    for index in 0..<10 { try Data("burst \(index)".utf8).write(to: url) }
    try await waitUntil("a burst reports") { reports > burst }
    try await Task.sleep(for: .milliseconds(DocumentWatcher.delayMilliseconds * 3))
    #expect(reports - burst <= 2, "a burst is coalesced")
    watcher.cancel()
    let cancelled = reports
    try Data("five".utf8).write(to: url)
    try await Task.sleep(for: .milliseconds(DocumentWatcher.delayMilliseconds * 3))
    #expect(reports == cancelled, "a cancelled watcher stops reporting")
  }

  /// The loader refreshes in the background: the presented document stays
  /// until the new revision loads, a deleted file keeps it as stale and the
  /// session records the deletion, and identical bytes never reparse.
  @Test func loaderRefreshesExternalChanges() async throws {
    let capability = try file("live.md", titled("v1"))
    let loader = DocumentLoader()
    var states: [String] = []
    loader.onChange = { state in
      switch state {
      case .empty: states.append("empty")
      case .loading: states.append("loading")
      case .loaded(let loaded): states.append(loaded.document.title ?? "")
      case .failed: states.append("failed")
      case .stale: states.append("stale")
      }
    }
    loader.open(capability)
    try await settle(loader)
    #expect(loadedTitle(loader) == "v1")
    let first = try #require(loader.document?.document.generation)

    try titled("v2").write(to: capability.url)
    try await waitUntil("an external write is presented") { loadedTitle(loader) == "v2" }
    #expect(states == ["loading", "v1", "v2"], "a refresh never shows a loading state")

    // A newer mtime with identical bytes reads, but doesn't reparse or copy.
    let generation = try #require(loader.document?.document.generation)
    #expect(generation > first)
    try FileManager.default.setAttributes(
      [.modificationDate: Date().addingTimeInterval(5)], ofItemAtPath: capability.url.path)
    loader.checkForChanges()
    try await waitUntil("the check finishes") { !loader.isRefreshing }
    #expect(loader.document?.document.generation == generation)

    // An unchanged file needs no read at all.
    let reads = states.count
    loader.checkForChanges()
    try await waitUntil("the check finishes") { !loader.isRefreshing }
    #expect(states.count == reads)

    try FileManager.default.removeItem(at: capability.url)
    try await waitUntil("a deletion leaves the document stale") { stale(loader) != nil }
    #expect(stale(loader)?.0 == "v2" && stale(loader)?.1 == .notFound)
    #expect(loader.coreAvailability == .unavailable(.deleted))
    #expect(loader.capability == capability && loader.document?.document.title == "v2")

    // The same bytes coming back restore it without a new snapshot.
    try titled("v2").write(to: capability.url)
    try await waitUntil("the file's return is presented") { loadedTitle(loader) == "v2" }
    #expect(loader.document?.document.generation == generation)
    #expect(loader.coreAvailability == .available)

    // A burst of saves ends on the last one, and results arrive in order.
    for index in 3...12 {
      try titled("v\(index)").write(to: capability.url, options: .atomic)
      try await Task.sleep(for: .milliseconds(15))
    }
    try await waitUntil("the last save is presented") { loadedTitle(loader) == "v12" }
    try await Task.sleep(for: .milliseconds(DocumentWatcher.delayMilliseconds * 3))
    try await waitUntil("refreshes settle") { !loader.isRefreshing }
    #expect(loadedTitle(loader) == "v12")
    let versions = states.compactMap { $0.hasPrefix("v") ? Int($0.dropFirst()) : nil }
    #expect(versions == versions.sorted(), "an older revision is never presented after a newer one")

    // An invalid save keeps the last good revision; Reload then shows the error.
    try Data([0x23, 0x20, 0xff]).write(to: capability.url)
    try await waitUntil("an invalid save leaves the document stale") { stale(loader) != nil }
    #expect(stale(loader)?.1 == .invalidText && stale(loader)?.0 == "v12")
    loader.reload()
    try await settle(loader)
    #expect(failure(loader) == .invalidText)
    await loader.close()
    try expectNoHandles()
  }

  /// A file that is missing when opened loads by itself once it appears.
  @Test func missingFileLoadsWhenItAppears() async throws {
    let capability = try capability("later.md")
    let loader = DocumentLoader()
    loader.open(capability)
    try await settle(loader)
    #expect(failure(loader) == .notFound)
    try titled("Later").write(to: capability.url)
    try await waitUntil("the new file loads") { loadedTitle(loader) == "Later" }
    await loader.close()
    try expectNoHandles()
  }
}
