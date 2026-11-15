import Foundation
import Testing

@testable import MarkdownDocument

/// Folder watching (MAC-002E): coalescing, hidden items, the root moving
/// away and back, a change storm and cancellation. Synthetic files in a
/// private temporary directory only.
@Suite(.serialized)
@MainActor
struct WorkspaceWatcherTests {
  private let directory: URL

  init() throws {
    directory = FileManager.default.temporaryDirectory
      .appendingPathComponent("markdown-watch-\(UUID().uuidString)")
    try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
  }

  @Test func coalescerWaitsForQuietButNotForever() {
    var coalescer = ChangeCoalescer()
    let start = ContinuousClock.now
    var due = coalescer.take(start)
    #expect(coalescer.deadline == nil && !due)
    coalescer.note(start)
    #expect(coalescer.deadline == start + ChangeCoalescer.quiet)
    due = coalescer.take(start + .milliseconds(100))
    #expect(!due)
    // A steady stream of changes keeps moving the quiet deadline, up to the cap.
    var now = start
    var refreshes = 0
    while now < start + .seconds(3) {
      coalescer.note(now)
      if coalescer.take(now) { refreshes += 1 }
      now += .milliseconds(20)
    }
    #expect((2...4).contains(refreshes), "a 3 s storm refreshes about once a second")
    coalescer.note(now)
    due = coalescer.take(now + ChangeCoalescer.quiet)
    #expect(due)
    #expect(!coalescer.isPending)
  }

  @Test func watcherReportsChangesRootMovesAndStops() async throws {
    let root = directory.appendingPathComponent("Watched", isDirectory: true)
    try FileManager.default.createDirectory(
      at: root.appendingPathComponent(".hidden"), withIntermediateDirectories: true)
    let folder = try FolderCapability(granted: root)
    var changes: [WorkspaceChange] = []
    let watcher = try #require(
      WorkspaceWatcher(folder: folder, includeHidden: false) { changes.append($0) })
    defer { watcher.cancel() }
    // FSEvents starts delivering shortly after the stream starts.
    try await Task.sleep(for: .milliseconds(300))

    try Data("# A\n".utf8).write(to: root.appendingPathComponent("a.md"))
    try await waitUntil("a new file reports") { changes == [.contents] }

    // Changes only inside hidden items aren't reported while they're hidden.
    // FSEvents may deliver the write above in more than one batch: let it settle.
    try await Task.sleep(for: .milliseconds(600))
    let before = changes.count
    try Data("x".utf8).write(to: root.appendingPathComponent(".hidden/secret.md"))
    try await Task.sleep(for: .milliseconds(600))
    #expect(changes.count == before, "a hidden change is ignored")
    watcher.setIncludeHidden(true)
    try Data("y".utf8).write(to: root.appendingPathComponent(".hidden/secret.md"))
    try await waitUntil("a hidden change reports once shown") { changes.count > before }

    // Moving the folder away and back reports the root each time.
    let away = directory.appendingPathComponent("Away", isDirectory: true)
    try await Task.sleep(for: .milliseconds(400))
    changes.removeAll()
    try FileManager.default.moveItem(at: root, to: away)
    try await waitUntil("moving the folder away reports the root") { changes.contains(.root) }
    changes.removeAll()
    try FileManager.default.moveItem(at: away, to: root)
    try await waitUntil("moving it back reports the root") { changes.contains(.root) }

    // A storm of changes is coalesced into a few reports.
    try await Task.sleep(for: .milliseconds(400))
    let stormStart = watcher.reportCount
    let started = ContinuousClock.now
    for index in 0..<2_000 {
      try Data("# \(index)\n".utf8).write(to: root.appendingPathComponent("storm-\(index).md"))
    }
    let writing = ContinuousClock.now - started
    try await Task.sleep(for: .milliseconds(600))
    let stormReports = watcher.reportCount - stormStart
    let ceiling = Int((writing / ChangeCoalescer.maxDelay).rounded(.up)) + 2
    #expect(stormReports >= 1 && stormReports <= ceiling, "2,000 writes coalesce: \(stormReports)")

    watcher.cancel()
    try await Task.sleep(for: .milliseconds(100))
    let stopped = watcher.reportCount
    try Data("# late\n".utf8).write(to: root.appendingPathComponent("late.md"))
    try await Task.sleep(for: .milliseconds(600))
    #expect(watcher.reportCount == stopped, "a cancelled watcher stops reporting")
    try? FileManager.default.removeItem(at: directory)
  }

  private func waitUntil(
    _ message: Comment, seconds: Int = 5, _ condition: () -> Bool
  ) async throws {
    let deadline = ContinuousClock.now.advanced(by: .seconds(seconds))
    while !condition() && ContinuousClock.now < deadline {
      try await Task.sleep(for: .milliseconds(10))
    }
    #expect(condition(), message)
  }
}
