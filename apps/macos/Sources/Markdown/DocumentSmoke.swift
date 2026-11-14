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
    try check(reader.documentTitle == "Smoke", "loaded document is presented")
    try check(reader.surface.textView.string == "One\nText", "document text is rendered")
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
    try check(retried.documentTitle == "Found", "reopening retries a failed load")
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
      try check(reader.documentTitle == "Smoke", "reload recovers the document")
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
      generations.documentTitle == "Fast" && presented == ["Fast"],
      "only the newest request is presented")
    note("reload/revocation/generations checked")
    try await reading(delegate: delegate, file: file, check: check)
    try await richBlocks(delegate: delegate, directory: directory, file: file, check: check)
    try await ReadingControlsSmoke.run(delegate: delegate, file: file, check: check)
    try await RefreshSmoke.run(delegate: delegate, file: file, check: check)

    for window in delegate.windows.map(\.window) { window?.performClose(nil) }
    try await waitUntil("closed documents release native sessions") {
      guard let live = try? CoreRuntime.liveHandles() else { return false }
      return live.sessions == 0 && live.jobs == 0 && live.snapshots == 0
    }
    delegate.newWindow(nil)
  }

  /// Rendered reading: outline, outline and link navigation, external links
  /// kept in the app, the sidebar command and copy through the Edit menu.
  private static func reading(
    delegate: AppDelegate, file: (String, Data) throws -> URL, check: (Bool, String) throws -> Void
  ) async throws {
    var text = "# Guide\n\nSee [the end](#the-end) or [the site](https://example.com/).\n\n"
    for index in 0..<200 {
      text += "## Part \(index)\n\n- item *one*\n- item **two**\n\n```\ncode \(index)\n```\n\n"
      // Long enough that the window shows a lead first, then completes it.
      text += String(repeating: "Filler prose for a long document. ", count: 6) + "\n\n"
    }
    text += "## The End\n\nDone.\n"
    delegate.application(NSApp, open: [try file("guide.md", Data(text.utf8))])
    let reader = try requireWindow(delegate.windows.last)
    try await settle(reader)
    let surface = reader.surface
    let headings = reader.outline.headings
    try check(headings.count == 202, "outline lists every heading")
    try check(surface.document?.isComplete == true, "a long document completes after its lead")
    // VoiceOver's heading rotor reaches the last heading of the text view.
    let rotor = surface.textView.accessibilityCustomRotors().first { $0.type == .heading }
    let search = NSAccessibilityCustomRotor.SearchParameters()
    search.searchDirection = .previous
    try check(
      rotor.flatMap { $0.itemSearchDelegate?.rotor($0, resultFor: search) }?.targetRange
        == headings[201].range,
      "heading rotor finds the last heading")
    try check(reader.outline.table.numberOfRows == 202, "outline table shows every heading")
    try check(surface.textView.textLayoutManager != nil, "surface stays on TextKit 2")
    try check(!surface.textView.isEditable && surface.textView.isSelectable, "read-only selectable")
    reader.outline.table.selectRowIndexes(IndexSet(integer: 101), byExtendingSelection: false)
    try check(surface.isVisible(headings[101].range), "outline selection reveals its heading")
    try check(surface.textView.selectedRange() == headings[101].range, "heading is selected")
    try check(!surface.isVisible(headings[0].range), "navigation scrolled away from the top")
    reader.outline.table.selectRowIndexes(IndexSet(integer: 0), byExtendingSelection: false)
    var opened: [URL] = []
    surface.openExternal = { opened.append($0) }
    let string = surface.textView.string as NSString
    for (label, _) in [("the end", 0), ("the site", 1)] {
      let range = string.range(of: label)
      guard
        let link = surface.textView.textStorage?.attribute(
          .link, at: range.location, effectiveRange: nil)
      else { throw SmokeFailure(description: "missing link \(label)") }
      surface.textView.clicked(onLink: link, at: range.location)
    }
    try check(surface.isVisible(headings[201].range), "fragment link reveals its heading")
    try check(reader.outline.table.selectedRow == 201, "outline follows link navigation")
    try check(opened.map(\.absoluteString) == ["https://example.com/"], "external link is routed")

    // Like the other menu checks, route to the window: the background smoke
    // has no key window for a nil-targeted action.
    let toggle = try item("View", "Show Sidebar", key: "s", modifiers: [.command, .control])
    let split = reader.window?.contentViewController
    try check(NSApp.sendAction(toggle.action!, to: split, from: toggle), "sidebar command routes")
    try await waitUntil("sidebar collapses") { reader.isSidebarCollapsed }
    try check(NSApp.sendAction(toggle.action!, to: split, from: toggle), "sidebar command routes")
    try await waitUntil("sidebar expands") { !reader.isSidebarCollapsed }

    let span = NSRange(location: 0, length: string.range(of: "Part 0").location)
    reader.window?.makeFirstResponder(surface.textView)
    surface.textView.setSelectedRange(span)
    let copy = try item("Edit", "Copy", key: "c")
    // The user's clipboard is never touched: Copy is enabled on the focused
    // text view, which then writes its selection to a private pasteboard.
    try check(
      reader.window?.firstResponder === surface.textView
        && surface.textView.responds(to: copy.action!) && surface.textView.validateMenuItem(copy),
      "Copy is enabled for the document selection")
    let pasteboard = NSPasteboard(name: NSPasteboard.Name("markdown-smoke-\(UUID().uuidString)"))
    defer { pasteboard.releaseGlobally() }
    try check(
      surface.textView.writeSelection(
        to: pasteboard, types: surface.textView.writablePasteboardTypes)
        && pasteboard.string(forType: .string) == "Guide\nSee the end or the site.\n",
      "copy writes the selected plain text")
    note("rendering/outline/navigation/copy checked")
  }

  /// Rich blocks through the real loader wiring (MAC-001E): an image beside
  /// the document loads, a remote one and one outside its folder do not, and
  /// a table lays out as grid rows on TextKit 2.
  private static func richBlocks(
    delegate: AppDelegate, directory: URL, file: (String, Data) throws -> URL,
    check: (Bool, String) throws -> Void
  ) async throws {
    let bitmap = try requireValue(
      NSBitmapImageRep(
        bitmapDataPlanes: nil, pixelsWide: 40, pixelsHigh: 20, bitsPerSample: 8,
        samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB,
        bytesPerRow: 0, bitsPerPixel: 0), "bitmap")
    let png = try requireValue(bitmap.representation(using: .png, properties: [:]), "png")
    let folder = directory.appendingPathComponent("rich")
    try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
    try png.write(to: folder.appendingPathComponent("pic.png"))
    _ = try file("outside.png", png)
    let markdown = """
      # Rich

      ![local](pic.png) ![remote](https://example.com/a.png) ![outside](../outside.png)

      - [x] done

      | A | B |
      | - | -: |
      | x | 1 |
      """
    let document = folder.appendingPathComponent("rich.md")
    try Data(markdown.utf8).write(to: document)
    delegate.application(NSApp, open: [document])
    let reader = try requireWindow(delegate.windows.last)
    try await settle(reader)
    let surface = reader.surface
    let images = surface.document?.images ?? []
    try check(
      images.map(\.failure) == [nil, .remote, .outsideFolder],
      "local image loads; remote and outside-folder images are refused")
    let storage = surface.textView.textStorage
    try check(
      storage?.attribute(.attachment, at: images[0].range.location, effectiveRange: nil) != nil,
      "local image is an attachment")
    try check(surface.textView.string.contains("☑\tdone"), "task item shows its state")
    try check(surface.textView.string.contains("A\tB\nx\t1"), "table rows are tab-separated")
    try check(surface.textView.textLayoutManager != nil, "rich blocks stay on TextKit 2")
    note("tables/tasks/images/resource policy checked")
  }

  private static func settle(_ controller: ReaderWindowController) async throws {
    try await waitUntil("document load and rendering settle") { !controller.isBusy }
  }

  private static func waitUntil(_ message: String, condition: () -> Bool) async throws {
    let deadline = ContinuousClock.now.advanced(by: .seconds(10))
    while !condition() && ContinuousClock.now < deadline {
      try await Task.sleep(for: .milliseconds(20))
    }
    if !condition() { throw SmokeFailure(description: message) }
  }

  private static func item(
    _ menu: String, _ title: String, key: String,
    modifiers: NSEvent.ModifierFlags = .command
  ) throws -> NSMenuItem {
    guard let item = NSApp.mainMenu?.item(withTitle: menu)?.submenu?.item(withTitle: title),
      item.keyEquivalent == key, item.keyEquivalentModifierMask == modifiers, item.action != nil
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

func requireValue<T>(_ value: T?, _ name: String) throws -> T {
  guard let value else { throw SmokeFailure(description: "missing \(name)") }
  return value
}

private func requireWindow(_ controller: ReaderWindowController?) throws
  -> ReaderWindowController
{
  guard let controller else { throw SmokeFailure(description: "missing reader window") }
  return controller
}
