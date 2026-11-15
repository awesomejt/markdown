import AppKit
import MarkdownCore
import MarkdownDocument
import MarkdownRender

/// The files sidebar, links between documents and file-identity window
/// lookup inside the running app (MAC-002B): listing through the core's
/// classification, keyboard navigation and opening, hidden/all-file
/// toggles, relative links with anchors through the folder grant, refused
/// links, a hard link and a case variant finding the open window, and a
/// large folder. Synthetic files in a private directory only.
@MainActor
enum WorkspaceSmoke {
  static func run(delegate: AppDelegate, directory: URL, check: (Bool, String) throws -> Void)
    async throws
  {
    try check(delegate.windows.isEmpty, "workspace checks start without windows")
    let manager = FileManager.default
    let root = directory.appendingPathComponent("Browse", isDirectory: true)
    func write(_ relative: String, _ text: String) throws -> URL {
      let url = root.appendingPathComponent(relative)
      try manager.createDirectory(
        at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
      try Data(text.utf8).write(to: url)
      return url
    }
    var guideText = "# Guide\n\n"
    for index in 0..<40 { guideText += "Paragraph \(index) of the guide.\n\n" }
    guideText += "## Install\n\nSteps.\n"
    let guide = try write("docs/guide.md", guideText)
    _ = try write(
      "README.md",
      "# Readme\n\nSee the [guide](docs/guide.md#install), [away](../away.md),"
        + " [picture](docs/picture.png) and [nothing](docs/missing.md).\n")
    _ = try write("docs/notes.txt", "notes")
    _ = try write("docs/picture.png", "png")
    _ = try write(".hidden/secret.md", "# Secret\n")
    _ = try write("odd\u{200B}name.md", "# Odd\n")
    _ = try write("empty/readme.txt", "text")
    let hardLink = root.appendingPathComponent("docs/guide-link.md")
    try manager.linkItem(at: guide, to: hardLink)
    try Data("# Away\n".utf8).write(to: directory.appendingPathComponent("away.md"))

    for (title, key, modifiers) in [
      ("Files", "1", [.command, .option]), ("Outline", "2", [.command, .option]),
      ("Show Hidden Files", ".", [.command, .shift]), ("Show All Files", "", .command),
      ("Refresh Files", "", .command),
    ] as [(String, String, NSEvent.ModifierFlags)] {
      _ = try item("View", title, key: key, modifiers: modifiers)
    }

    // A granted folder lists its Markdown in the files sidebar, off the main thread.
    delegate.openFolder(root)
    try await waitUntil("folder lists its files") {
      delegate.windows.count == 1 && delegate.windows[0].files.listing != nil
        && !delegate.windows[0].isListing && !delegate.windows[0].isOpeningFolder
    }
    let reader = delegate.windows[0]
    var problems: [String] = []
    reader.presentProblem = { _, message, _ in problems.append(message) }
    let browser = reader.files
    let listing = try requireValue(browser.listing, "listing")
    try check(reader.sidebar.hasFiles && reader.sidebar.mode == .files, "folder shows its files")
    try check(!reader.sidebar.picker.isHidden, "sidebar offers files and outline")
    try check(
      listing.items.map(\.name) == ["docs", "odd\u{200B}name.md", "README.md"],
      "only Markdown is listed, folders first, hidden and empty folders left out")
    try check(
      listing.item(at: "docs")?.children.map(\.name) == ["guide-link.md", "guide.md"],
      "a folder lists its Markdown documents")
    try check(browser.outlineView.numberOfRows == 3, "folders start collapsed")
    try check(reader.statusText == "“Browse”", "folder window names its folder")

    // A name with invisible characters is marked for VoiceOver.
    let odd = browser.outlineView.row(forItem: listing.item(at: "odd\u{200B}name.md"))
    let oddLabel = browser.outlineView.view(atColumn: 0, row: odd, makeIfNecessary: true)?
      .accessibilityLabel()
    try check(
      oddLabel?.contains("invisible characters") == true, "an invisible-character name is marked")

    // Keyboard: Right expands a folder, Down moves, Return opens a document.
    reader.showFiles(nil)
    try check(
      reader.window?.firstResponder === browser.outlineView, "Files focuses the files sidebar")
    browser.outlineView.selectRowIndexes(IndexSet(integer: 0), byExtendingSelection: false)
    press(browser.outlineView, NSRightArrowFunctionKey, keyCode: 124)
    try check(browser.outlineView.numberOfRows == 5, "Right arrow expands the folder")
    press(browser.outlineView, NSDownArrowFunctionKey, keyCode: 125)
    press(browser.outlineView, NSDownArrowFunctionKey, keyCode: 125)
    try check(browser.selectedItem?.path == "docs/guide.md", "Down arrow moves the selection")
    press(browser.outlineView, 0x0D, keyCode: 36)
    try await waitUntil("Return opens the document") {
      reader.documentTitle == "guide.md" && !reader.isBusy
    }
    try check(
      reader.loader.capability?.folder === reader.folder?.grant,
      "a document opened from the sidebar carries the folder grant")
    try check(reader.window?.subtitle == "Browse", "the window keeps its folder")
    press(browser.outlineView, NSLeftArrowFunctionKey, keyCode: 123)
    try check(browser.selectedItem?.path == "docs", "Left arrow moves to the parent folder")
    press(browser.outlineView, NSLeftArrowFunctionKey, keyCode: 123)
    try check(browser.outlineView.numberOfRows == 3, "Left arrow collapses the folder")
    note("files sidebar listing/keyboard checked")

    // Opening the README from the sidebar replaces the document in this window.
    let readme = browser.outlineView.row(forItem: listing.item(at: "README.md"))
    browser.outlineView.selectRowIndexes(IndexSet(integer: readme), byExtendingSelection: false)
    browser.activateSelection()
    try await waitUntil("README opens in the folder window") {
      reader.documentTitle == "README.md" && !reader.isBusy
    }
    try check(delegate.windows.count == 1, "sidebar navigation stays in the window")

    // A relative link opens through the folder grant, at its heading.
    try click("guide", in: reader)
    try await waitUntil("a link opens the linked document") {
      reader.documentTitle == "guide.md" && !reader.isBusy
    }
    let install = reader.outline.headings.firstIndex { $0.title == "Install" }
    try check(
      install != nil && reader.outline.table.selectedRow == install,
      "the link's anchor shows its heading")
    try await waitUntil("the sidebar selects the linked document") {
      browser.selectedItem?.path == "docs/guide.md"
    }
    try check(
      reader.loader.capability?.folder === reader.folder?.grant,
      "the linked document shares the grant")
    browser.outlineView.selectRowIndexes(
      IndexSet(integer: browser.outlineView.row(forItem: listing.item(at: "README.md"))),
      byExtendingSelection: false)
    browser.activateSelection()
    try await waitUntil("README opens again") {
      reader.documentTitle == "README.md" && !reader.isBusy
    }

    // Links that leave the folder or lead to another kind of file are refused.
    try click("away", in: reader)
    try await waitUntil("a link outside the folder is refused") { problems.count == 1 }
    try check(
      problems.last == LinkError.outsideFolder(granted: true).message,
      "a link outside the folder explains itself")
    try click("picture", in: reader)
    try await waitUntil("a link to an image is refused") { problems.count == 2 }
    try check(problems.last == LinkError.unsupportedType.message, "an image link explains itself")
    try check(reader.documentTitle == "README.md", "refused links keep the document")
    // A missing target opens and reports its error, with recovery.
    try click("nothing", in: reader)
    try await waitUntil("a link to a missing document reports it") {
      reader.statusText == DocumentError.notFound.message && !reader.isBusy
    }
    try check(reader.recoveryTitles == ["Locate…"], "a missing linked document offers Locate")
    note("links between documents checked")

    // The window already showing a file is found by identity, off the main
    // thread: a hard link and a case variant of the name focus it.
    browser.outlineView.expandItem(listing.item(at: "docs"))
    browser.outlineView.selectRowIndexes(
      IndexSet(integer: browser.outlineView.row(forItem: listing.item(at: "docs/guide.md"))),
      byExtendingSelection: false)
    browser.activateSelection()
    try await waitUntil("guide opens") { reader.documentTitle == "guide.md" && !reader.isBusy }
    try await waitUntil("the window records its file's identity") { reader.knownIdentity != nil }
    delegate.open([hardLink])
    await delegate.opensFinished()
    try check(delegate.windows.count == 1, "a hard link focuses the window showing the file")
    let variant = root.appendingPathComponent("DOCS/GUIDE.MD")
    if manager.fileExists(atPath: variant.path) {
      delegate.open([variant])
      await delegate.opensFinished()
      try check(delegate.windows.count == 1, "a case variant focuses the window showing the file")
    }
    // From another window, the sidebar brings the showing window forward.
    delegate.open([directory.appendingPathComponent("away.md")])
    await delegate.opensFinished()
    try await waitUntil("a second window opens") {
      delegate.windows.count == 2 && !delegate.windows[1].isBusy
    }
    let other = delegate.windows[1]
    delegate.navigate(
      to: try requireValue(reader.loader.capability, "guide"), target: .heading("install"),
      from: other)
    await delegate.opensFinished()
    try check(
      delegate.windows.count == 2 && other.documentTitle == "away.md",
      "navigation to a shown file reuses its window")
    other.window?.performClose(nil)
    note("file-identity window lookup checked")

    // Hidden and non-Markdown files on request; the listing keeps expansion.
    try check(
      NSApp.sendAction(
        #selector(ReaderWindowController.toggleHiddenFiles(_:)), to: reader, from: nil),
      "Show Hidden Files routes")
    try await waitUntil("hidden files are listed") {
      !reader.isListing && browser.listing?.item(at: ".hidden/secret.md") != nil
    }
    try check(
      browser.outlineView.isItemExpanded(browser.listing?.item(at: "docs")),
      "relisting keeps expanded folders")
    try check(
      NSApp.sendAction(#selector(ReaderWindowController.toggleAllFiles(_:)), to: reader, from: nil),
      "Show All Files routes")
    try await waitUntil("all files are listed") {
      !reader.isListing && browser.listing?.item(at: "docs/notes.txt") != nil
        && browser.listing?.item(at: "empty") != nil
    }
    reader.toggleHiddenFiles(nil)
    reader.toggleAllFiles(nil)
    try await waitUntil("defaults are listed again") {
      !reader.isListing && browser.listing?.item(at: "docs/notes.txt") == nil
        && browser.listing?.item(at: ".hidden") == nil
    }
    reader.showOutline(nil)
    try check(reader.sidebar.mode == .outline, "Outline shows the document outline")
    reader.showFiles(nil)
    try check(reader.sidebar.mode == .files, "Files shows the files again")

    // Close Folder withdraws the files sidebar with the grant.
    reader.closeFolder(nil)
    try check(
      !reader.sidebar.hasFiles && browser.listing == nil && reader.sidebar.mode == .outline,
      "Close Folder removes the files sidebar")
    note("hidden/all files and close folder checked")

    // A large folder lists off the main thread and opens collapsed.
    let big = directory.appendingPathComponent("Big", isDirectory: true)
    let body = Data("# Note\n".utf8)
    for group in 0..<50 {
      let folder = big.appendingPathComponent("group-\(group)")
      try manager.createDirectory(at: folder, withIntermediateDirectories: true)
      for index in 0..<100 { try body.write(to: folder.appendingPathComponent("note-\(index).md")) }
    }
    let start = ContinuousClock.now
    delegate.openFolder(big)
    try await waitUntil("large folder lists") {
      delegate.windows.contains {
        $0.folder?.url.lastPathComponent == "Big" && $0.files.listing != nil
      }
    }
    let elapsed = start.duration(to: .now)
    let large = try requireValue(delegate.windows.first { $0.folder != nil }, "large folder window")
    try check(large.files.listing?.fileCount == 5_000, "the large folder lists every document")
    try check(large.files.outlineView.numberOfRows == 50, "the large folder starts collapsed")
    large.files.outlineView.expandItem(nil, expandChildren: true)
    try check(large.files.outlineView.numberOfRows == 5_050, "every folder expands")
    note(
      "large folder (5,000 documents) listed in \(elapsed.formatted(.units(allowed: [.milliseconds])))"
    )

    for window in delegate.windows.map(\.window) { window?.performClose(nil) }
    try await waitUntil("workspace windows release native sessions") {
      guard let live = try? CoreRuntime.liveHandles() else { return false }
      return live.sessions == 0 && live.jobs == 0 && live.snapshots == 0
    }
    note("workspace browsing checked")
  }

  /// Click the link whose text is `text`, as the text view would.
  private static func click(_ text: String, in reader: ReaderWindowController) throws {
    let textView = reader.surface.textView
    let range = (textView.string as NSString).range(of: text)
    guard range.location != NSNotFound,
      let link = textView.textStorage?.attribute(.link, at: range.location, effectiveRange: nil)
    else { throw SmokeFailure(description: "missing link \(text)") }
    _ = reader.surface.textView(textView, clickedOnLink: link, at: range.location)
  }

  private static func press(_ view: NSView, _ character: Int, keyCode: UInt16) {
    let characters = String(Character(UnicodeScalar(UInt32(character))!))
    guard
      let event = NSEvent.keyEvent(
        with: .keyDown, location: .zero, modifierFlags: [], timestamp: 0,
        windowNumber: view.window?.windowNumber ?? 0, context: nil, characters: characters,
        charactersIgnoringModifiers: characters, isARepeat: false, keyCode: keyCode)
    else { return }
    view.keyDown(with: event)
  }

  private static func waitUntil(_ message: String, condition: () -> Bool) async throws {
    let deadline = ContinuousClock.now.advanced(by: .seconds(10))
    while !condition() && ContinuousClock.now < deadline {
      try await Task.sleep(for: .milliseconds(20))
    }
    if !condition() { throw SmokeFailure(description: message) }
  }

  private static func item(
    _ menu: String, _ title: String, key: String, modifiers: NSEvent.ModifierFlags
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
