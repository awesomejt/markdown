import CMarkdown
import CryptoKit
import Foundation
import Testing

@testable import MarkdownCore

@Suite(.serialized)
struct CoreTests {
  @Test func negotiationAndTypedErrors() throws {
    try CoreRuntime.validate()
    #if DEBUG
      #expect(CoreRuntime.packageConfiguration == "debug")
    #else
      #expect(CoreRuntime.packageConfiguration == "release")
    #endif
    let root = try #require(ProcessInfo.processInfo.environment["MARKDOWN_TEST_ROOT"])
    let archive = try Data(
      contentsOf: URL(fileURLWithPath: root)
        .appendingPathComponent("apps/macos/CorePackage/lib/libmarkdown_ffi.a"))
    let fingerprint = SHA256.hash(data: archive).map { String(format: "%02x", $0) }.joined()
    #expect(CoreRuntime.packageFingerprint == fingerprint)
    var info = md_abi_info()
    info.struct_size = UInt32(MemoryLayout<md_abi_info>.size)
    #expect(md_abi_negotiate(99, &info) == MD_ERR_VERSION_MISMATCH)
    #expect(info.abi_version == MD_ABI_VERSION)
    info.struct_size += 4
    #expect(md_abi_negotiate(UInt32(MD_ABI_VERSION), &info) == MD_ERR_STRUCT_SIZE)
    var options = md_session_options()
    options.struct_size = UInt32(MemoryLayout<md_session_options>.size)
    options.model_version = 99
    var session: md_session = 123
    let status = Array("test".utf8).withUnsafeBufferPointer {
      md_session_create($0.baseAddress, $0.count, &options, &session)
    }
    #expect(status == MD_ERR_UNSUPPORTED_MODEL)
    #expect(session == 0)
    do {
      _ = try CoreSession(documentID: "")
      Issue.record("empty document ID accepted")
    } catch CoreError.status(let code) { #expect(code == MD_ERR_INVALID_DOCUMENT_ID) }
    #expect(try CoreRuntime.liveHandles() == CoreHandleCounts(sessions: 0, jobs: 0, snapshots: 0))
  }

  @Test func ownedV2ValuesAndGenerations() async throws {
    let session = try CoreSession(documentID: "native")
    let source = Data(
      "---\ntitle: Café\ntags: [one, two]\n---\n# Heading\n\n[label][ref]\n\n[ref]: https://example.com\n"
        .utf8)
    let first = try await session.parse(source, revisionToken: "a")
    #expect(first.outcome == .published && first.generation == 1)
    let document = try #require(first.document)
    #expect(document.title == "Café")
    #expect(document.source == source)
    #expect(document.frontMatterRange != nil)
    #expect(
      try document.properties.map { try document.string($0.key) }.sorted() == ["tags", "title"])
    #expect(try document.stringList.map { try document.string($0) } == ["one", "two"])
    #expect(document.links.count == 1)
    let link = try #require(document.links.first)
    #expect(link.source_kind == MD_SOURCE_REFERENCE)
    #expect(link.target_kind == MD_TARGET_EXTERNAL)
    #expect(try document.string(link.target) == "https://example.com/")
    let unchanged = try await session.parse(source, revisionToken: "a")
    #expect(unchanged.outcome == .unchanged && unchanged.generation == 2)
    let newer = try await session.parse(Data("# Newer".utf8), revisionToken: "b")
    #expect(newer.outcome == .published && newer.generation == 3)
    try await session.close()
    try await session.close()
    #expect(try document.string(link.destination) == "https://example.com")
    #expect(document.source == source)  // Owned values outlive the session.
    // Each copied document keeps only its snapshot handle, for anchors.
    #expect(try CoreRuntime.liveHandles() == CoreHandleCounts(sessions: 0, jobs: 0, snapshots: 3))
    withExtendedLifetime((first, unchanged, newer)) {}
  }

  @Test func parseFailuresAndRecovery() async throws {
    let session = try CoreSession(documentID: "errors")

    let invalid = try await session.parse(Data([0xff]))
    #expect(invalid.outcome == .failed && invalid.failure == .invalidUTF8)
    let tooLarge = try await session.parse(
      Data(repeating: 97, count: Int(CoreRuntime.maxSourceBytes) + 1))
    #expect(tooLarge.outcome == .failed && tooLarge.failure == .sourceLimit)
    #expect(tooLarge.document == nil)
    do {
      _ = try await session.parse(Data(), revisionToken: String(repeating: "a", count: 257))
      Issue.record("oversized revision token accepted")
    } catch CoreError.status(let code) { #expect(code == MD_ERR_INVALID_REVISION_TOKEN) }
    let recovered = try await session.parse(Data("ok".utf8))
    #expect(recovered.outcome == .published && recovered.generation > tooLarge.generation)
    try await session.close()
  }

  @Test func cancellationBeforeWorkerAndDuringLargeCall() async throws {
    let session = try CoreSession(documentID: "cancel")

    let barrier = DispatchSemaphore(value: 0)
    CoreRuntime.worker.async { barrier.wait() }
    let queued = Task { try await session.parse(Data("queued".utf8)) }
    queued.cancel()
    barrier.signal()
    do {
      _ = try await queued.value
      Issue.record("cancelled queued work returned a document")
    } catch is CancellationError {}
    let source = Data(String(repeating: "*a ", count: Int(CoreRuntime.maxSourceBytes) / 3).utf8)
    let active = Task { try await session.parse(source) }
    let deadline = ContinuousClock.now.advanced(by: .seconds(5))
    while try CoreRuntime.liveHandles().jobs == 0 && ContinuousClock.now < deadline {
      try await Task.sleep(for: .milliseconds(1))
    }
    #expect(try CoreRuntime.liveHandles().jobs == 1)
    active.cancel()
    do {
      _ = try await active.value
      Issue.record("cancelled large call returned a document")
    } catch is CancellationError {}
    #expect(try CoreRuntime.liveHandles().jobs == 0)
    #expect(try CoreRuntime.liveHandles().snapshots == 0)
    try await session.close()
  }

  @Test func repeatedLifecycle() async throws {
    for _ in 0..<200 {
      let session = try CoreSession(documentID: "lifecycle")
      let result = try await session.parse(Data("# Title\n\ntext".utf8))
      #expect(result.document?.nodes.isEmpty == false)
      try await session.close()
    }
    await Self.drainWorker()
    #expect(try CoreRuntime.liveHandles() == CoreHandleCounts(sessions: 0, jobs: 0, snapshots: 0))
  }

  @Test func closeDuringWorkAndRejectFurtherUse() async throws {
    let session = try CoreSession(documentID: "close")
    let source = Data(String(repeating: "*a ", count: Int(CoreRuntime.maxSourceBytes) / 3).utf8)
    let active = Task { try await session.parse(source) }
    let deadline = ContinuousClock.now.advanced(by: .seconds(5))
    while try CoreRuntime.liveHandles().jobs == 0 && ContinuousClock.now < deadline {
      try await Task.sleep(for: .milliseconds(1))
    }
    #expect(try CoreRuntime.liveHandles().jobs == 1)
    try await session.close()
    do {
      _ = try await active.value
      Issue.record("closed session returned late work")
    } catch CoreError.closed {}
    do {
      _ = try await session.parse(Data())
      Issue.record("closed session accepted bytes")
    } catch CoreError.closed {}
    await Self.drainWorker()
    #expect(try CoreRuntime.liveHandles() == CoreHandleCounts(sessions: 0, jobs: 0, snapshots: 0))
  }

  @Test func concurrentCloseWaitsForCleanup() async throws {
    let session = try CoreSession(documentID: "double-close")
    _ = try await session.parse(Data("owned".utf8))
    async let first: Void = session.close()
    async let second: Void = session.close()
    try await first
    try await second
    await Self.drainWorker()
    #expect(try CoreRuntime.liveHandles() == CoreHandleCounts(sessions: 0, jobs: 0, snapshots: 0))
  }

  @Test func corpusAndPerformanceProfiles() async throws {
    let root = try #require(ProcessInfo.processInfo.environment["MARKDOWN_TEST_ROOT"])
    let directory = URL(fileURLWithPath: root)
    let session = try CoreSession(documentID: "corpus")

    var inputs = Self.corpusFiles(directory.appendingPathComponent("fixtures/markdown"))
    let profiles = directory.appendingPathComponent("build/fixtures/performance")
    inputs += try FileManager.default.contentsOfDirectory(
      at: profiles, includingPropertiesForKeys: nil
    )
    .filter { $0.pathExtension == "md" }
    #expect(inputs.count >= 12)
    for url in inputs.sorted(by: { $0.path < $1.path }) {
      let source = try Data(contentsOf: url)
      let result = try await session.parse(source)
      #expect(result.outcome == .published || result.outcome == .unchanged)
      let doc = try #require(result.document)
      #expect(doc.source == source)
      let hash = SHA256.hash(data: source).map { String(format: "%02x", $0) }.joined()
      #expect(doc.sourceSHA256 == hash)
      for node in doc.nodes {
        #expect(node.span_start <= node.span_end && Int(node.span_end) <= source.count)
        _ = try doc.string(node.id)
        _ = try doc.string(node.text)
        _ = try doc.string(node.extra)
      }
    }
    try await session.close()
  }

  private static func corpusFiles(_ directory: URL) -> [URL] {
    guard
      let enumerator = FileManager.default.enumerator(
        at: directory, includingPropertiesForKeys: nil)
    else { return [] }
    var files: [URL] = []
    for case let url as URL in enumerator where url.pathExtension == "md" { files.append(url) }
    return files
  }

  @Test func queuedCancellationAdmitsReplacementBeforeWorkerRuns() async throws {
    let session = try CoreSession(documentID: "queued-replacement")
    let barrier = DispatchSemaphore(value: 0)
    CoreRuntime.worker.async { _ = barrier.wait(timeout: .now() + 10) }
    defer { barrier.signal() }
    func waitForPending(_ count: Int) async throws {
      let deadline = ContinuousClock.now.advanced(by: .seconds(2))
      while CoreRuntime.admission.pending != count && ContinuousClock.now < deadline {
        try await Task.sleep(for: .milliseconds(1))
      }
      try #require(CoreRuntime.admission.pending == count)
    }
    let first = Task { try await session.parse(Data("first".utf8)) }
    try await waitForPending(1)
    let queued = Task { try await session.parse(Data("cancelled".utf8)) }
    try await waitForPending(2)
    queued.cancel()
    try await waitForPending(1)
    do {
      _ = try await queued.value
      Issue.record("cancelled queued request returned")
    } catch is CancellationError {}
    let replacement = Task { try await session.parse(Data("replacement".utf8)) }
    try await waitForPending(2)
    barrier.signal()
    #expect(try await first.value.outcome == .published)
    #expect(try await replacement.value.outcome == .published)
    #expect(CoreRuntime.admission.pending == 0)
    try await session.close()
    await Self.drainWorker()
    // The two published results keep their documents' snapshots.
    #expect(try CoreRuntime.liveHandles() == CoreHandleCounts(sessions: 0, jobs: 0, snapshots: 2))
    withExtendedLifetime((first, replacement)) {}
  }

  @Test func queuedWorkIsBounded() async throws {
    let session = try CoreSession(documentID: "admission")

    let barrier = DispatchSemaphore(value: 0)
    CoreRuntime.worker.async { barrier.wait() }
    let first = Task { try await session.parse(Data("first".utf8)) }
    let second = Task { try await session.parse(Data("second".utf8)) }
    let deadline = ContinuousClock.now.advanced(by: .seconds(5))
    while CoreRuntime.admission.pending < 2 && ContinuousClock.now < deadline {
      try await Task.sleep(for: .milliseconds(1))
    }
    #expect(CoreRuntime.admission.pending == 2)
    do {
      _ = try await session.parse(Data("third".utf8))
      Issue.record("unbounded work admitted")
    } catch CoreError.busy {}
    first.cancel()
    second.cancel()
    barrier.signal()
    for task in [first, second] {
      do {
        _ = try await task.value
        Issue.record("cancelled queued request returned")
      } catch is CancellationError {}
    }
    #expect(CoreRuntime.admission.pending == 0)
    try await session.close()
  }

  /// Find runs over the session's current snapshot, only for the generation
  /// the caller presents, and follows the core's Unicode rules (MAC-001F).
  @Test func findOverCurrentSnapshot() async throws {
    let session = try CoreSession(documentID: "find")
    let source =
      "# Straße\n\nCafé and cafe\u{301} and CAFÉ.\n\nσοφός ΣΟΦΌΣ\n\n"
      + "👨\u{200D}👩\u{200D}👧 family, 我中文好 and עברית.\n\n`code café` and **caf**é\n"
    let bytes = Array(source.utf8)
    let first = try await session.parse(Data(bytes))
    let generation = first.generation
    func find(_ query: String, _ options: CoreFindOptions = CoreFindOptions()) async throws
      -> [String]
    {
      let result = try await session.find(query, options: options, generation: generation)
      #expect(result.generation == generation)
      for match in result.matches {
        #expect(!match.segments.isEmpty && match.segments.upperBound <= result.segments.count)
      }
      return result.matches.map {
        String(
          decoding: bytes[Int($0.source.lowerBound)..<Int($0.source.upperBound)], as: UTF8.self)
      }
    }
    // Case folding without normalization: NFD café is a different string.
    #expect(try await find("café") == ["Café", "CAFÉ", "café", "caf**é"])
    #expect(try await find("café", CoreFindOptions(caseSensitive: true)) == ["café", "caf**é"])
    #expect(try await find("cafe\u{301}") == ["cafe\u{301}"])
    #expect(try await find("cafe").isEmpty, "no match inside a combining sequence")
    #expect(try await find("STRAẞE") == ["Straße"])
    #expect(try await find("STRASSE").isEmpty, "simple folding only")
    #expect(try await find("σοφός") == ["σοφός", "ΣΟΦΌΣ"])
    #expect(try await find("👨").isEmpty, "no match inside a ZWJ sequence")
    #expect(try await find("👨\u{200D}👩\u{200D}👧") == ["👨\u{200D}👩\u{200D}👧"])
    #expect(try await find("中文", CoreFindOptions(wholeWord: true)) == ["中文"])
    #expect(try await find("עברית") == ["עברית"])
    #expect(try await find("caf", CoreFindOptions(wholeWord: true)).isEmpty)
    #expect(try await find("  ").isEmpty)
    let limited = try await session.find(
      "caf", options: CoreFindOptions(maxMatches: 2), generation: generation)
    #expect(limited.matches.count == 2 && limited.truncated)
    do {
      _ = try await session.find(String(repeating: "a", count: 4097), generation: generation)
      Issue.record("over-long query accepted")
    } catch CoreError.queryTooLong {}
    do {
      _ = try await session.find("café", generation: generation + 1)
      Issue.record("unpublished generation searched")
    } catch CoreError.superseded {}

    // A newer snapshot supersedes the old generation.
    let second = try await session.parse(Data("café again\n".utf8))
    #expect(second.generation > generation)
    do {
      _ = try await session.find("café", generation: generation)
      Issue.record("superseded generation searched")
    } catch CoreError.superseded {}
    let current = try await session.find("café", generation: second.generation)
    #expect(current.matches.count == 1)

    // A find cancelled while queued behind other work never runs.
    let barrier = DispatchSemaphore(value: 0)
    CoreRuntime.worker.async { barrier.wait() }
    let queued = Task { try await session.find("café", generation: second.generation) }
    try await Task.sleep(for: .milliseconds(20))
    queued.cancel()
    barrier.signal()
    do {
      _ = try await queued.value
      Issue.record("cancelled find returned")
    } catch is CancellationError {}

    try await session.close()
    do {
      _ = try await session.find("café", generation: second.generation)
      Issue.record("closed session searched")
    } catch CoreError.closed {}
    await Self.drainWorker()
    // The two parse results keep their documents' snapshots.
    #expect(try CoreRuntime.liveHandles() == CoreHandleCounts(sessions: 0, jobs: 0, snapshots: 2))
    withExtendedLifetime((first, second)) {}
  }

  /// Each copied document keeps its snapshot for anchors (MAC-001H): a
  /// position anchored in an old revision resolves in the next one, and
  /// change notices follow the session's revision tokens.
  @Test func anchorsAndExternalChanges() async throws {
    let session = try CoreSession(documentID: "anchors")
    let old = "# One\n\nFirst paragraph.\n\n## Two\n\nSecond paragraph.\n"
    let first = try #require(try await session.parse(Data(old.utf8), revisionToken: "a").document)
    #expect(try session.externalChange(token: "a") == .unchanged)
    #expect(try session.externalChange(token: "b") == .readRequired)
    #expect(try session.externalChange(token: nil) == .readRequired)
    let offset = UInt64(Array(old.utf8).count - "Second paragraph.\n".utf8.count + 7)
    let anchor = try first.snapshot.anchor(atSource: offset)
    #expect(!anchor.isEmpty && anchor.count <= Int(MD_ANCHOR_MAX_BYTES))
    #expect(try first.snapshot.resolve(anchor)?.sourceOffset == offset)

    // Text inserted above moves the block; the anchor follows it exactly.
    let new = "# One\n\nInserted.\n\nFirst paragraph.\n\n## Two\n\nSecond paragraph.\n"
    let second = try await session.parse(Data(new.utf8), revisionToken: "b")
    let document = try #require(second.document)
    let moved = try #require(try document.snapshot.resolve(anchor))
    #expect(moved.quality == .exact)
    #expect(moved.sourceOffset == offset + UInt64("Inserted.\n\n".utf8.count))
    // The old snapshot is still usable after the session moved on.
    #expect(try first.snapshot.resolve(anchor)?.sourceOffset == offset)

    // A refresh of identical bytes needn't copy the snapshot again.
    let unchanged = try await session.parse(
      Data(new.utf8), revisionToken: "c", copyUnchanged: false)
    #expect(unchanged.outcome == .unchanged && unchanged.document == nil)
    #expect(try session.externalChange(token: "c") == .unchanged)

    // Forged anchors throw; an empty document loses every anchor.
    #expect(throws: CoreError.self) { try document.snapshot.resolve(Data("{}".utf8)) }
    #expect(throws: CoreError.self) { try document.snapshot.resolve(Data(count: 2048)) }
    let empty = try #require(try await session.parse(Data(), revisionToken: "d").document)
    #expect(try empty.snapshot.resolve(anchor) == nil)
    #expect(throws: CoreError.self) { try empty.snapshot.anchor(atSource: 0) }
    try await session.close()
    #expect(throws: CoreError.self) { try session.externalChange(token: "a") }
  }

  /// Copied documents hold their snapshot handles until they are released;
  /// the last release runs on the core worker.
  @Test func documentsReleaseTheirSnapshots() async throws {
    do {
      let session = try CoreSession(documentID: "release")
      let document = try await session.parse(Data("# Kept".utf8)).document
      try await session.close()
      #expect(try CoreRuntime.liveHandles().snapshots == 1)
      #expect(document?.nodes.isEmpty == false)
    }
    await Self.drainWorker()
    #expect(try CoreRuntime.liveHandles() == CoreHandleCounts(sessions: 0, jobs: 0, snapshots: 0))
  }

  /// Wait for releases already queued on the core worker.
  static func drainWorker() async {
    await withCheckedContinuation { continuation in
      CoreRuntime.worker.async { continuation.resume() }
    }
  }
}
