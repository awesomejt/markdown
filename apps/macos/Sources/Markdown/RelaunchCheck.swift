import AppKit
import MarkdownDocument

/// Opt-in development check of real state restoration (MAC-001H), launched
/// twice by tools/check.py with `-NSQuitAlwaysKeepsWindows YES`. `prepare`
/// opens the document, puts its "Part 80" heading at the top with a word
/// selected, saves the position and quits; `verify` expects AppKit to have
/// restored that window at that place, then closes every window, so the
/// app's saved state is left empty, and quits. Settings stay in memory.
///
/// Given a folder instead (MAC-002E), `prepare` opens it, expands `docs`,
/// opens `docs/live.md` from the files sidebar, switches to the search pane
/// and saves the same position; `verify` expects the folder back with that
/// pane and expansion, and the document through the folder's grant.
@MainActor
enum RelaunchCheck {
  enum Phase: String {
    case prepare, verify
  }

  /// `--relaunch-check prepare|verify <path>`.
  static func arguments(_ arguments: [String]) -> (Phase, URL)? {
    guard let flag = arguments.firstIndex(of: "--relaunch-check"), arguments.count > flag + 2,
      let phase = Phase(rawValue: arguments[flag + 1])
    else { return nil }
    return (phase, URL(fileURLWithPath: arguments[flag + 2]))
  }

  private struct Failure: Error, CustomStringConvertible {
    let description: String
  }

  static func run(_ phase: Phase, document url: URL, delegate: AppDelegate) async {
    do {
      let isFolder = (try? url.resourceValues(forKeys: [.isDirectoryKey]))?.isDirectory == true
      let document = isFolder ? url.appendingPathComponent("docs/live.md") : url
      let capability = try DocumentCapability(granted: document)
      if phase == .prepare {
        if isFolder {
          try await openThroughFolder(url, delegate)
        } else {
          delegate.open([url])
          await delegate.opensFinished()
        }
      }
      let reader = try await settled(capability, delegate)
      let headings = reader.outline.headings
      guard let index = headings.firstIndex(where: { $0.title == "Part 80" }) else {
        throw Failure(description: "missing heading")
      }
      let heading = headings[index].range
      let string = reader.surface.textView.string as NSString
      let word = string.range(of: "marked", range: NSRange(heading.location..<string.length))
      switch phase {
      case .prepare:
        reader.surface.reveal(heading: index)
        reader.surface.textView.setSelectedRange(word)
        if isFolder {
          reader.showSearch(nil)
          try await until("folder bookmarked") { reader.isFolderBookmarked }
        }
        await reader.makeBookmark()
        await reader.saveReadingPosition()
        guard reader.savedPosition != nil else { throw Failure(description: "position not saved") }
        reader.invalidateRestorableState()
        note("prepared")
      case .verify:
        guard delegate.windows.count == 1 else {
          throw Failure(description: "expected one restored window: \(delegate.windows.count)")
        }
        guard abs(reader.surface.topLocation() - heading.location) <= 1,
          reader.surface.textView.selectedRange() == word
        else { throw Failure(description: "restored window lost the reading position") }
        if isFolder { try await verifyFolder(reader, capability) }
        for window in delegate.windows.map(\.window) { window?.performClose(nil) }
        note(
          isFolder
            ? "pass (restored folder, pane, expansion, document through the folder, top line and selection)"
            : "pass (restored window, document, top line and selection)")
      }
      NSApp.terminate(nil)
    } catch {
      FileHandle.standardError.write(Data("relaunch-check: FAIL: \(error)\n".utf8))
      exit(1)
    }
  }

  private static func openThroughFolder(_ url: URL, _ delegate: AppDelegate) async throws {
    delegate.openFolder(url)
    try await until("folder lists") {
      delegate.windows.count == 1 && delegate.windows[0].files.listing != nil
        && !delegate.windows[0].isListing
    }
    let files = delegate.windows[0].files
    guard let docs = files.listing?.item(at: "docs"),
      let live = files.listing?.item(at: "docs/live.md")
    else { throw Failure(description: "folder listing is missing docs/live.md") }
    files.outlineView.expandItem(docs)
    files.onOpen?(live)
  }

  private static func verifyFolder(
    _ reader: ReaderWindowController, _ capability: DocumentCapability
  ) async throws {
    try await until("restored folder lists") { reader.files.listing != nil && !reader.isListing }
    guard let folder = reader.folder, reader.loader.capability?.folder === folder.grant else {
      throw Failure(description: "the document wasn't restored through its folder")
    }
    guard reader.sidebar.mode == .search else {
      throw Failure(description: "the sidebar pane wasn't restored")
    }
    guard let docs = reader.files.listing?.item(at: "docs"),
      reader.files.outlineView.isItemExpanded(docs)
    else { throw Failure(description: "the expanded folder wasn't restored") }
  }

  private static func until(_ message: String, condition: () -> Bool) async throws {
    let deadline = ContinuousClock.now.advanced(by: .seconds(10))
    while !condition() && ContinuousClock.now < deadline {
      try await Task.sleep(for: .milliseconds(20))
    }
    if !condition() { throw Failure(description: message) }
  }

  private static func settled(_ capability: DocumentCapability, _ delegate: AppDelegate)
    async throws -> ReaderWindowController
  {
    let deadline = ContinuousClock.now.advanced(by: .seconds(10))
    while ContinuousClock.now < deadline {
      if let reader = delegate.windows.first(where: { $0.loader.capability == capability }),
        reader.surface.document?.isComplete == true, !reader.isBusy
      {
        return reader
      }
      try await Task.sleep(for: .milliseconds(20))
    }
    throw Failure(description: "document did not load")
  }

  private static func note(_ message: String) {
    FileHandle.standardOutput.write(Data("relaunch-check: \(message)\n".utf8))
  }
}
