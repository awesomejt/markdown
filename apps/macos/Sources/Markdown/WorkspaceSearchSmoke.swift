import AppKit
import MarkdownCore
import MarkdownDocument
import MarkdownRender

/// The folder search inside the running app (MAC-002C): its menu items,
/// keyboard path from the field to the results, name and content results,
/// opening a match in an unchanged and in an edited document, a superseded
/// query, Stop, a match budget, and Close Folder. Synthetic files in a
/// private directory only.
@MainActor
enum WorkspaceSearchSmoke {
  static func run(delegate: AppDelegate, directory: URL, check: (Bool, String) throws -> Void)
    async throws
  {
    try check(delegate.windows.isEmpty, "search checks start without windows")
    let root = directory.appendingPathComponent("Search", isDirectory: true)
    func write(_ relative: String, _ text: String) throws {
      let url = root.appendingPathComponent(relative)
      try FileManager.default.createDirectory(
        at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
      try Data(text.utf8).write(to: url)
    }
    var guide = "# Guide\n\n"
    for index in 0..<80 { guide += "Paragraph \(index) of the guide.\n\n" }
    guide += "## Deep\n\nThe *needle* is deep in the guide.\n"
    try write("docs/guide.md", guide)
    try write("README.md", "# Readme\n\nA needle here, and another Needle.\n")
    try write("needle-notes.md", "# Notes\n\nNothing to find.\n")
    try write("other.md", "# Other\n\nOne more needle.\n")
    for index in 0..<400 { try write("many/note-\(index).md", "# Note\n\nA needle \(index).\n") }

    let find = try requireValue(
      NSApp.mainMenu?.item(withTitle: "Edit")?.submenu?.item(withTitle: "Find")?.submenu,
      "Find menu")
    let findInFolder = try requireValue(find.item(withTitle: "Find in Folder…"), "Find in Folder")
    try check(
      findInFolder.keyEquivalent == "f"
        && findInFolder.keyEquivalentModifierMask == [.command, .shift],
      "Find in Folder is Shift-Command-F")
    let searchItem = try requireValue(
      NSApp.mainMenu?.item(withTitle: "View")?.submenu?.item(withTitle: "Search"), "View/Search")
    try check(
      searchItem.keyEquivalent == "3"
        && searchItem.keyEquivalentModifierMask == [.command, .option],
      "View/Search is Option-Command-3")

    delegate.openFolder(root)
    try await waitUntil("the folder lists") {
      delegate.windows.count == 1 && delegate.windows[0].files.listing != nil
        && !delegate.windows[0].isListing
    }
    let reader = delegate.windows[0]
    let search = reader.search
    try check(
      NSApp.sendAction(#selector(ReaderWindowController.showSearch(_:)), to: reader, from: nil),
      "Find in Folder routes")
    try check(reader.sidebar.mode == .search, "Find in Folder shows the search")
    try check(
      reader.window?.firstResponder === search.field.currentEditor(),
      "Find in Folder puts the cursor in the search field")

    // Names first, then each document's matches; the status counts them.
    try await runSearch(search, "needle")
    let names = search.groups.first
    if case .names = names?.kind {
      try check(names?.rows.count == 1, "one file name matches")
    } else {
      try check(false, "file names come first")
    }
    let documents = search.groups.compactMap { group -> String? in
      if case .document(let item, _) = group.kind { return item.path }
      return nil
    }
    try check(
      documents.prefix(3) == ["docs/guide.md", "many/note-0.md", "many/note-1.md"]
        && documents.contains("README.md") && documents.count == 403,
      "content matches are grouped by document in the sidebar's order")
    try check(
      search.statusText.hasPrefix("1 file name, 404 matches in 403 documents."),
      "the status counts the results")
    note("search results: \(search.statusText)")

    // Down arrow moves from the field to the first match; Return opens it there.
    _ = search.control(
      search.field, textView: NSTextView(), doCommandBy: #selector(NSResponder.moveDown(_:)))
    try check(reader.window?.firstResponder === search.results, "Down arrow moves to the results")
    try check(
      search.selectedRow.map { "\($0.item.path)" } == "needle-notes.md",
      "the first result is selected")
    let deep = try requireValue(
      search.groups.first {
        if case .document(let item, _) = $0.kind { item.path == "docs/guide.md" } else { false }
      }?
      .rows.first, "the guide's match")
    search.select(deep)
    press(search.results, 0x0D, keyCode: 36)
    try await waitForMatch(reader, "guide.md")
    try check(
      reader.surface.isVisible(reader.surface.textView.selectedRange()),
      "the match deep in the document is scrolled into view")
    try check(
      reader.loader.capability?.folder === reader.folder?.grant,
      "a result opens through the folder grant")
    try check(reader.sidebar.mode == .search, "the results stay in the sidebar")

    // A second match in another document, by click.
    let readme = try requireValue(
      search.groups.first {
        if case .document(let item, _) = $0.kind { item.path == "README.md" } else { false }
      },
      "README matches")
    search.select(readme.rows[1])
    search.activateSelection()
    try await waitForMatch(reader, "README.md", text: "Needle")

    // A document edited since the search: its match is found by anchor.
    try write("other.md", "# Other\n\nA paragraph added above.\n\nOne more needle.\n")
    let other = try requireValue(
      search.groups.first {
        if case .document(let item, _) = $0.kind { item.path == "other.md" } else { false }
      }?
      .rows.first, "other.md's match")
    search.select(other)
    search.activateSelection()
    try await waitForMatch(reader, "other.md")
    note("search result navigation checked (span and anchor)")

    // A new query supersedes the running one; Stop ends a search early.
    search.field.stringValue = "needle"
    search.search()
    search.field.stringValue = "paragraph"
    search.search()
    try await waitUntil("the newer query finishes") { !search.isSearching }
    try check(
      search.groups.allSatisfy {
        if case .document(let item, _) = $0.kind {
          ["docs/guide.md", "other.md"].contains(item.path)
        } else {
          false
        }
      }, "only the newer query's results are shown")
    search.field.stringValue = "needle"
    search.search()
    search.stop(nil)
    try await waitUntil("a stopped search ends") { !search.isSearching }
    try check(
      search.summary?.cancelled == true && search.statusText.contains("Stop"),
      "Stop ends the search and says so")

    // A match budget stops the search and says the results are incomplete.
    search.limits = CoreContentOptions(maxTotalMatches: 5)
    try await runSearch(search, "needle")
    try check(
      search.summary?.stopped == .matches && search.statusText.contains("match limit"),
      "the match budget is reported")
    search.limits = CoreContentOptions()

    // Throughput over the 400-document folder (informal).
    let start = ContinuousClock.now
    try await runSearch(search, "absent")
    note(
      "content search of \(search.summary?.filesSearched ?? 0) documents in \(start.duration(to: .now).formatted(.units(allowed: [.milliseconds])))"
    )

    // Close Folder withdraws the search with the grant.
    reader.closeFolder(nil)
    try check(
      search.listing == nil && search.groups.isEmpty && reader.sidebar.mode == .outline,
      "Close Folder clears the search")
    for window in delegate.windows.map(\.window) { window?.performClose(nil) }
    try await waitUntil("search windows release native sessions") {
      guard let live = try? CoreRuntime.liveHandles() else { return false }
      return live.sessions == 0 && live.jobs == 0 && live.snapshots == 0
    }
    note("workspace search checked")
  }

  private static func runSearch(_ search: WorkspaceSearchController, _ text: String) async throws {
    search.field.stringValue = text
    search.search()
    try await waitUntil("the search for \(text) finishes") { !search.isSearching }
  }

  /// Wait until `title` is shown with the match selected.
  private static func waitForMatch(
    _ reader: ReaderWindowController, _ title: String, text: String = "needle"
  ) async throws {
    try await waitUntil("\(title) opens at its match") {
      guard reader.documentTitle == title, !reader.isBusy, !reader.isRevealingMatch else {
        return false
      }
      let textView = reader.surface.textView
      return (textView.string as NSString).substring(with: textView.selectedRange()) == text
    }
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

  private static func note(_ message: String) {
    FileHandle.standardOutput.write(Data("shell-smoke: \(message)\n".utf8))
  }
}
