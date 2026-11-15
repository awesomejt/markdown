import AppKit
import MarkdownCore
import MarkdownDocument
import MarkdownRender

/// Folder watching and workspace restoration inside the running app
/// (MAC-002E): added and removed files appear without losing expansion or
/// selection, a storm of changes costs a bounded number of listings,
/// hidden churn costs none, a folder moved away is reported and recovers
/// with its expansion when it comes back, and a window's folder, pane,
/// expanded folders, options and document round-trip through its saved
/// state, including forged and folder-only state. Restoration runs through
/// the real encode and restore code with a private archive. Synthetic files
/// in a private directory only.
@MainActor
enum WorkspaceWatchSmoke {
  static func run(delegate: AppDelegate, directory: URL, check: (Bool, String) throws -> Void)
    async throws
  {
    try check(delegate.windows.isEmpty, "watch checks start without windows")
    let manager = FileManager.default
    let root = directory.appendingPathComponent("Watch", isDirectory: true)
    func write(_ relative: String, _ text: String = "# Doc\n") throws {
      let url = root.appendingPathComponent(relative)
      try manager.createDirectory(
        at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
      try Data(text.utf8).write(to: url)
    }
    var readme = "# Readme\n\n"
    for index in 0..<60 { readme += "## Part \(index)\n\nParagraph \(index).\n\n" }
    try write("README.md", readme)
    try write("docs/a.md")
    try write("docs/deep/b.md")
    try write(".notes/hidden.md")
    try manager.createDirectory(
      at: root.appendingPathComponent(".git/objects"), withIntermediateDirectories: true)

    delegate.openFolder(root)
    try await waitUntil("the folder lists") {
      delegate.windows.count == 1 && delegate.windows[0].files.listing != nil
        && !delegate.windows[0].isListing && !delegate.windows[0].isOpeningFolder
    }
    let reader = delegate.windows[0]
    let browser = reader.files
    func listed(_ path: String) -> Bool { browser.listing?.item(at: path) != nil }
    func expanded(_ path: String) -> Bool {
      browser.listing?.item(at: path).map { browser.outlineView.isItemExpanded($0) } == true
    }
    for path in ["docs", "docs/deep"] {
      browser.outlineView.expandItem(try requireValue(browser.listing?.item(at: path), path))
    }
    browser.onOpen?(try requireValue(browser.listing?.item(at: "README.md"), "README"))
    try await waitUntil("the document opens and is selected") {
      reader.surface.document?.isComplete == true && !reader.isBusy
        && browser.selectedItem?.path == "README.md"
    }

    // Added and removed files appear by themselves; expansion and selection stay.
    try write("docs/new.md")
    try await waitUntil("an added file is listed") { listed("docs/new.md") }
    try check(
      expanded("docs") && expanded("docs/deep") && browser.selectedItem?.path == "README.md",
      "a refresh keeps the expanded folders and the selection")
    try manager.removeItem(at: root.appendingPathComponent("docs/a.md"))
    try await waitUntil("a removed file leaves the listing") { !listed("docs/a.md") }
    try check(reader.folderLost == nil, "a change in the folder isn't a loss")
    note("folder watching checked")

    // A storm of 1,500 new documents lists a bounded number of times.
    try await settle(reader)
    let before = reader.listingsShown
    let started = ContinuousClock.now
    for index in 0..<1_500 { try write("storm/note-\(index).md") }
    let writing = ContinuousClock.now - started
    try await waitUntil("the storm settles into one listing") {
      browser.listing?.item(at: "storm")?.children.count == 1_500 && !reader.isListing
    }
    let settled = ContinuousClock.now - started
    let listings = reader.listingsShown - before
    let ceiling = Int((settled / ChangeCoalescer.maxDelay).rounded(.up)) + 2
    try check(listings >= 1 && listings <= ceiling, "a storm costs bounded listings: \(listings)")
    try check(browser.selectedItem?.path == "README.md", "the storm keeps the selection")
    note(
      "storm checked (1,500 files written in \(milliseconds(writing)) ms, listed in"
        + " \(milliseconds(settled)) ms with \(listings) listings)")

    // A refresh doesn't restart a search the reader stopped.
    let search = reader.search
    search.field.stringValue = "Doc"
    search.search()
    search.stop(nil)
    try await waitUntil("the stopped search ends") { !search.isSearching }
    try check(search.summary?.cancelled == true, "Stop ends the folder search")
    try write("docs/after-stop.md")
    try await waitUntil("a change after Stop is listed") { listed("docs/after-stop.md") }
    try check(
      search.summary?.cancelled == true && !search.isSearching,
      "a refresh keeps a stopped search stopped")
    search.field.stringValue = ""
    search.search()

    // Churn in hidden items (a version-control folder, an editor's swap
    // file beside a visible document) lists nothing.
    try await settle(reader)
    let quiet = reader.listingsShown
    for index in 0..<200 { try write(".git/objects/\(index)", "x") }
    for index in 0..<20 { try write("docs/.a.md.swp", "swap \(index)") }
    try await Task.sleep(for: .milliseconds(800))
    try check(reader.listingsShown == quiet, "hidden churn doesn't list the folder")

    // The folder moved away is reported; moved back, it lists as it was.
    let away = directory.appendingPathComponent("Watch moved", isDirectory: true)
    try manager.moveItem(at: root, to: away)
    try await waitUntil("a moved folder is reported") {
      reader.folderLost == .notFound && browser.messageText == FolderError.notFound.message
    }
    try check(reader.search.listing == nil, "a lost folder can't be searched")
    let closeFolder = NSMenuItem(
      title: "", action: #selector(ReaderWindowController.closeFolder(_:)), keyEquivalent: "")
    try check(reader.validateMenuItem(closeFolder), "a lost folder can still be closed")
    try manager.moveItem(at: away, to: root)
    try await waitUntil("the folder recovers when it comes back") {
      reader.folderLost == nil && listed("docs/new.md") && !reader.isListing
    }
    try check(
      expanded("docs") && expanded("docs/deep"), "a recovered folder keeps its expanded folders")
    try await waitUntil("the document is still shown") {
      reader.surface.document != nil && !reader.isBusy && reader.noticeText == nil
    }
    note("folder loss and recovery checked")

    try await restoration(delegate, reader, root: root, check: check)
  }

  private static func restoration(
    _ delegate: AppDelegate, _ reader: ReaderWindowController, root: URL,
    check: (Bool, String) throws -> Void
  ) async throws {
    // Search pane, hidden files shown, a heading in view: encode, close, restore.
    reader.toggleHiddenFiles(nil)
    try await settle(reader)
    reader.showSearch(nil)
    let headings = reader.outline.headings
    let part = try requireValue(headings.firstIndex { $0.title == "Part 40" }, "Part 40")
    reader.surface.reveal(heading: part)
    await reader.makeBookmark()
    await reader.saveReadingPosition()
    let state = try encodeAndClose(reader)
    let workspace = try requireValue(state.workspace, "workspace state")
    try check(
      state.bookmark != nil && workspace.mode == .search && workspace.includeHidden
        && !workspace.allFiles && Set(workspace.expanded).isSuperset(of: ["docs", "docs/deep"]),
      "a folder window saves its folder, pane, options and expansion")

    let restored = try requireValue(delegate.restoreWindow(state), "restored folder window")
    restored.showWindow(nil)
    try await waitUntil("the restored folder and document settle") {
      restored.files.listing != nil && !restored.isListing && restored.surface.document != nil
        && !restored.isBusy
    }
    let folder = try requireValue(restored.folder, "restored folder")
    try check(folder.url.lastPathComponent == "Watch", "the folder is restored")
    try check(
      restored.loader.capability?.folder === folder.grant,
      "the document is restored through the folder's grant")
    try check(restored.sidebar.mode == .search, "the sidebar pane is restored")
    try check(restored.workspaceOptions.includeHidden, "the listing options are restored")
    try check(
      restored.files.listing?.item(at: ".notes/hidden.md") != nil,
      "the restored listing shows hidden items")
    for path in ["docs", "docs/deep"] {
      let item = try requireValue(restored.files.listing?.item(at: path), path)
      try check(restored.files.outlineView.isItemExpanded(item), "\(path) is expanded again")
    }
    try check(
      abs(restored.surface.topLocation() - restored.outline.headings[part].range.location) <= 1,
      "the document returns to the reader's place")
    try check(delegate.restoreWindow(state) == nil, "a shown document isn't restored twice")
    // The restored window keeps watching its folder.
    try Data("# Late\n".utf8).write(to: root.appendingPathComponent("late.md"))
    try await waitUntil("a restored window watches its folder") {
      restored.files.listing?.item(at: "late.md") != nil
    }
    note("workspace restoration checked")

    // A folder window showing no document restores the folder alone.
    restored.closeFolder(nil)
    try await waitUntil("the folder closes") { restored.folder == nil }
    restored.window?.performClose(nil)
    try await waitUntil("window closes") { delegate.windows.isEmpty }
    delegate.openFolder(root)
    try await waitUntil("the folder lists") {
      delegate.windows.count == 1 && delegate.windows[0].files.listing != nil
        && !delegate.windows[0].isListing
    }
    let empty = delegate.windows[0]
    try await waitUntil("the folder is bookmarked") {
      encode(empty).flatMap(\.workspace) != nil
    }
    let folderOnly = try requireValue(encode(empty), "folder-only state")
    try check(
      folderOnly.bookmark == nil && folderOnly.workspace?.mode == .files,
      "a folder window without a document saves only its folder")
    try check(delegate.restoreWindow(folderOnly) == nil, "a shown folder isn't restored twice")
    empty.closeFolder(nil)
    empty.window?.performClose(nil)
    try await waitUntil("window closes") { delegate.windows.isEmpty }
    let alone = try requireValue(delegate.restoreWindow(folderOnly), "restored folder alone")
    alone.showWindow(nil)
    try await waitUntil("the folder alone is restored") {
      alone.files.listing != nil && !alone.isListing && alone.loader.capability == nil
        && alone.sidebar.mode == .files
    }

    // A folder that no longer resolves leaves the document by itself; forged
    // expansion is bounded; neither crashes.
    let forged = RestorationState(
      bookmark: state.bookmark, position: nil, sidebar: true,
      workspace: RestorationState.WorkspaceState(
        bookmark: Data("not a bookmark".utf8), mode: .files, expanded: [], includeHidden: false,
        allFiles: false))
    alone.closeFolder(nil)
    alone.window?.performClose(nil)
    try await waitUntil("window closes") { delegate.windows.isEmpty }
    let documentOnly = try requireValue(delegate.restoreWindow(forged), "document without folder")
    documentOnly.showWindow(nil)
    try await waitUntil("the document opens by itself") {
      documentOnly.surface.document != nil && !documentOnly.isBusy
    }
    try check(
      documentOnly.folder == nil && documentOnly.loader.capability?.folder == nil,
      "a document whose folder is gone opens by itself")
    let archiver = NSKeyedArchiver(requiringSecureCoding: true)
    archiver.encode(2, forKey: "readerVersion")
    archiver.encode(workspace.bookmark as NSData, forKey: "folderBookmark")
    archiver.encode(99, forKey: "sidebarMode")
    let paths = (0..<2_000).map { "folder-\($0)" as NSString }
    archiver.encode(
      ([String(repeating: "x", count: 5_000) as NSString] + paths) as NSArray,
      forKey: "expandedFolders")
    archiver.finishEncoding()
    let bounded = try requireValue(
      RestorationState(from: try NSKeyedUnarchiver(forReadingFrom: archiver.encodedData)),
      "forged folder state")
    try check(
      bounded.bookmark == nil && bounded.workspace?.mode == .files
        && bounded.workspace?.expanded.count == RestorationState.WorkspaceState.maxExpanded,
      "forged folder state is bounded")
    documentOnly.window?.performClose(nil)
    try await waitUntil("window closes") { delegate.windows.isEmpty }
    note("forged and folder-only restoration checked")
  }

  /// Encode a window's state as AppKit would, close it, and decode it.
  private static func encodeAndClose(_ reader: ReaderWindowController) throws
    -> RestorationState
  {
    let state = try requireValue(encode(reader), "restoration state")
    reader.window?.performClose(nil)
    return state
  }

  private static func encode(_ reader: ReaderWindowController) -> RestorationState? {
    guard let window = reader.window else { return nil }
    let archiver = NSKeyedArchiver(requiringSecureCoding: true)
    reader.window(window, willEncodeRestorableState: archiver)
    archiver.finishEncoding()
    guard let unarchiver = try? NSKeyedUnarchiver(forReadingFrom: archiver.encodedData) else {
      return nil
    }
    return RestorationState(from: unarchiver)
  }

  /// Wait out any listing and changes still being coalesced.
  private static func settle(_ reader: ReaderWindowController) async throws {
    try await Task.sleep(for: ChangeCoalescer.maxDelay)
    try await waitUntil("listing settles") { !reader.isListing }
  }

  private static func milliseconds(_ duration: Duration) -> Int {
    Int(duration / .milliseconds(1))
  }

  private static func waitUntil(_ message: String, condition: () -> Bool) async throws {
    let deadline = ContinuousClock.now.advanced(by: .seconds(10))
    while !condition() && ContinuousClock.now < deadline {
      try await Task.sleep(for: .milliseconds(20))
    }
    if !condition() { throw SmokeFailure(description: message) }
  }

  private static func note(_ message: String) {
    FileHandle.standardOutput.write(Data("shell-smoke: \(message)\n".utf8))
  }
}
