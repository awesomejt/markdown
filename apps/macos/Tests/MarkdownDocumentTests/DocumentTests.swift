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
}
