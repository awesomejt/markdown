import AppKit
import Darwin
import MarkdownCore
import MarkdownDocument

/// Document open/error/generation checks inside the running app (MAC-001C).
/// Uses only synthetic files in a private temporary directory.
@MainActor
enum DocumentSmoke {
  static func run(delegate: AppDelegate, check: (Bool, String) throws -> Void) async throws {
    let directory = FileManager.default.temporaryDirectory
      .appendingPathComponent("markdown-smoke-\(UUID().uuidString)")
    try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(at: directory) }
    func file(_ name: String, _ contents: Data) throws -> URL {
      let url = directory.appendingPathComponent(name)
      try contents.write(to: url)
      return url
    }

    let types = Bundle.main.infoDictionary?["CFBundleDocumentTypes"] as? [[String: Any]]
    try check(
      types?.contains {
        ($0["LSItemContentTypes"] as? [String])?.contains("net.daringfireball.markdown") == true
          && $0["CFBundleTypeRole"] as? String == "Viewer"
      } == true, "bundle declares the Markdown viewer document type")

    // Launch Services delivery reuses the empty window.
    try check(delegate.windows.count == 1, "one empty window before opening")
    let valid = try file("valid.md", Data("---\ntitle: Smoke\n---\n# One\n\nText\n".utf8))
    delegate.application(NSApp, open: [valid])
    let reader = try requireWindow(delegate.windows.first)
    try check(delegate.windows.count == 1, "opening reuses the empty window")
    try await settle(reader)
    try check(reader.statusText == "Smoke", "loaded document title is presented")
    try check(reader.window?.title == "valid.md", "window names the document")
    try check(reader.window?.representedURL != nil, "window represents the opened file")
    delegate.application(NSApp, open: [valid])
    try check(delegate.windows.count == 1, "reopening a shown document focuses its window")

    // Each failure is typed and presented in its own window.
    let folder = directory.appendingPathComponent("folder.md")
    try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: false)
    let large = try file("large.md", Data())
    let handle = try FileHandle(forWritingTo: large)
    try handle.truncate(atOffset: CoreRuntime.maxSourceBytes + 1)
    try handle.close()
    let failures: [(URL, DocumentError)] = [
      (directory.appendingPathComponent("missing.md"), .notFound),
      (folder, .notRegularFile(isDirectory: true)),
      (try file("image.png", Data([0x89, 0x50])), .unsupportedType),
      (large, .tooLarge(limit: CoreRuntime.maxSourceBytes)),
      (try file("invalid.md", Data([0x23, 0x20, 0xff])), .invalidText),
    ]
    for (url, expected) in failures {
      delegate.application(NSApp, open: [url])
      let window = try requireWindow(delegate.windows.last)
      try await settle(window)
      try check(
        window.statusText == expected.message && window.detailText == expected.recovery,
        "\(url.lastPathComponent) presents its typed error")
      try check(window.window?.representedURL == nil, "failed document has no represented file")
    }
    try check(delegate.windows.count == 1 + failures.count, "each document opens its own window")
    let missing = failures[0].0
    try Data("---\ntitle: Found\n---\n".utf8).write(to: missing)
    delegate.application(NSApp, open: [missing])
    let retried = try requireWindow(delegate.windows[1])
    try check(delegate.windows.count == 1 + failures.count, "reopening a failed file reuses it")
    try await settle(retried)
    try check(retried.statusText == "Found", "reopening retries a failed load")
    note("open/association/errors checked")

    // Reload through the menu after permission is withdrawn, then restored.
    let reload = try item("File", "Reload", key: "r")
    if getuid() != 0 {
      try check(chmod(valid.path, 0) == 0, "withdraw read permission")
      try check(NSApp.sendAction(reload.action!, to: reader, from: reload), "Reload command routes")
      try await settle(reader)
      try check(
        reader.statusText == DocumentError.accessRevoked.message, "revoked access is presented")
      try check(
        reader.loader.coreAvailability == .unavailable(.accessRevoked),
        "core session records the revocation")
      try check(chmod(valid.path, 0o644) == 0, "restore read permission")
      try check(NSApp.sendAction(reload.action!, to: reader, from: reload), "Reload command routes")
      try await settle(reader)
      try check(reader.statusText == "Smoke", "reload recovers the document")
    }

    // A slow document replaced before it finishes is never presented.
    var text = "---\ntitle: Slow\n---\n"
    for index in 0..<40_000 { text += "Paragraph \(index) with *emphasis*.\n\n" }
    let slow = try file("slow.md", Data(text.utf8))
    let fast = try file("fast.md", Data("---\ntitle: Fast\n---\n# Fast\n".utf8))
    var presented: [String] = []
    let generations = try requireWindow(delegate.windows.last)
    let present = generations.loader.onChange
    generations.loader.onChange = { state in
      present?(state)
      if case .loaded(let loaded) = state { presented.append(loaded.document.title ?? "") }
    }
    _ = try item("File", "Open…", key: "o")
    generations.loader.open(try DocumentCapability(granted: slow))
    generations.loader.open(try DocumentCapability(granted: fast))
    try await settle(generations)
    try await waitUntil("stale result completes") { generations.loader.discardedResults == 1 }
    try check(
      generations.statusText == "Fast" && presented == ["Fast"],
      "only the newest request is presented")
    note("reload/revocation/generations checked")

    for window in delegate.windows.map(\.window) { window?.performClose(nil) }
    try await waitUntil("closed documents release native sessions") {
      guard let live = try? CoreRuntime.liveHandles() else { return false }
      return live.sessions == 0 && live.jobs == 0 && live.snapshots == 0
    }
    delegate.newWindow(nil)
  }

  private static func settle(_ controller: ReaderWindowController) async throws {
    try await waitUntil("document load settles") { !controller.loader.isLoading }
  }

  private static func waitUntil(_ message: String, condition: () -> Bool) async throws {
    let deadline = ContinuousClock.now.advanced(by: .seconds(10))
    while !condition() && ContinuousClock.now < deadline {
      try await Task.sleep(for: .milliseconds(20))
    }
    if !condition() { throw SmokeFailure(description: message) }
  }

  private static func item(_ menu: String, _ title: String, key: String) throws -> NSMenuItem {
    guard let item = NSApp.mainMenu?.item(withTitle: menu)?.submenu?.item(withTitle: title),
      item.keyEquivalent == key, item.keyEquivalentModifierMask == .command, item.action != nil
    else { throw SmokeFailure(description: "missing menu item \(menu)/\(title)") }
    return item
  }

  private static func note(_ message: String) {
    FileHandle.standardOutput.write(Data("shell-smoke: \(message)\n".utf8))
  }
}

struct SmokeFailure: Error, CustomStringConvertible {
  let description: String
}

private func requireWindow(_ controller: ReaderWindowController?) throws
  -> ReaderWindowController
{
  guard let controller else { throw SmokeFailure(description: "missing reader window") }
  return controller
}
