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
    #expect(document.source == source)  // No native lifetime retained by the Swift value.
    #expect(try CoreRuntime.liveHandles() == CoreHandleCounts(sessions: 0, jobs: 0, snapshots: 0))
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
    #expect(try CoreRuntime.liveHandles() == CoreHandleCounts(sessions: 0, jobs: 0, snapshots: 0))
  }

  @Test func concurrentCloseWaitsForCleanup() async throws {
    let session = try CoreSession(documentID: "double-close")
    _ = try await session.parse(Data("owned".utf8))
    async let first: Void = session.close()
    async let second: Void = session.close()
    try await first
    try await second
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
    #expect(try CoreRuntime.liveHandles() == CoreHandleCounts(sessions: 0, jobs: 0, snapshots: 0))
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
}
