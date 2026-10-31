import AppKit
import MarkdownCore
import MarkdownDocument
import MarkdownRender

/// External refresh, anchors and restoration inside the running app
/// (MAC-001H), on synthetic files. Restoration runs through the real encode
/// and restore code with a private archive; the smoke never reads or writes
/// the app's saved state.
@MainActor
enum RefreshSmoke {
  static func run(
    delegate: AppDelegate, file: (String, Data) throws -> URL, check: (Bool, String) throws -> Void
  ) async throws {
    func sections(_ range: Range<Int>) -> String {
      range.map { "## Part \($0)\n\nProse for part \($0), with a *marked* word.\n" }
        .joined(separator: "\n")
    }
    let original = "# Live\n\n" + sections(0..<120)
    let url = try file("live.md", Data(original.utf8))
    delegate.application(NSApp, open: [url])
    guard let reader = delegate.windows.last else { throw SmokeFailure(description: "no window") }
    try await waitUntil("document settles") { !reader.isBusy }
    let surface = reader.surface

    /// The reader's place: part 80's heading at the top, a word selected.
    func place() throws {
      let index = try heading("Part 80")
      surface.reveal(heading: index)
      let string = surface.textView.string as NSString
      let heading = reader.outline.headings[index].range
      surface.textView.setSelectedRange(
        string.range(of: "marked", range: NSRange(heading.location..<string.length)))
    }
    func heading(_ title: String) throws -> Int {
      try requireValue(reader.outline.headings.firstIndex { $0.title == title }, title)
    }
    /// Part 80 is still at the top and its word still selected.
    func keptPlace(_ message: String) throws {
      let heading = reader.outline.headings[try heading("Part 80")].range
      let top = surface.topLocation()
      try check(abs(top - heading.location) <= 1, "\(message): the top line is kept")
      let selection = surface.textView.selectedRange()
      let string = surface.textView.string as NSString
      try check(
        string.substring(with: selection) == "marked" && selection.location > heading.location
          && selection.location < heading.location + 80,
        "\(message): the selection is kept")
    }
    func waitForGeneration(after generation: UInt64?, _ message: String) async throws {
      try await waitUntil(message) {
        surface.document?.generation != generation && !reader.isBusy
      }
    }

    try place()
    try keptPlace("placed")
    // An editor's atomic save inserting text above the reader's place.
    var generation = surface.document?.generation
    let edited =
      "# Live\n\nA new introduction.\n\n" + sections(200..<230) + "\n" + sections(0..<120)
    try Data(edited.utf8).write(to: url, options: .atomic)
    try await waitForGeneration(after: generation, "an external save is shown")
    try check(reader.outline.headings.count == 151, "the outline follows the new revision")
    try keptPlace("after an external save")
    try check(reader.noticeText == nil, "a current document shows no notice")

    // Deletion keeps the last version with a notice; the file's return clears it.
    generation = surface.document?.generation
    try FileManager.default.removeItem(at: url)
    try await waitUntil("a deleted document shows a notice") { reader.noticeText != nil }
    try check(
      reader.noticeText == DocumentError.notFound.message && !surface.scrollView.isHidden
        && surface.document?.generation == generation,
      "a deleted document stays readable")
    try check(
      reader.loader.coreAvailability == .unavailable(.deleted), "the session records the deletion")
    try Data(edited.utf8).write(to: url)
    try await waitUntil("the document's return clears the notice") {
      reader.noticeText == nil && !reader.isBusy
    }
    try check(surface.document?.generation == generation, "identical bytes are not re-rendered")
    try keptPlace("after the file returned")

    // Reload through the menu keeps the place too.
    guard let reload = NSApp.mainMenu?.item(withTitle: "File")?.submenu?.item(withTitle: "Reload")
    else { throw SmokeFailure(description: "missing Reload") }
    try check(NSApp.sendAction(reload.action!, to: reader, from: reload), "Reload routes")
    try await waitUntil("reload settles") { !reader.isBusy }
    try keptPlace("after Reload")
    note("external refresh/anchors/deletion checked")

    // Restoration: encode the window's state as AppKit would, close the
    // window, then restore it from the archive.
    await reader.makeBookmark()
    await reader.saveReadingPosition()
    try check(reader.savedPosition != nil, "the reading position is anchored")
    let archiver = NSKeyedArchiver(requiringSecureCoding: true)
    reader.window(try requireValue(reader.window, "window"), willEncodeRestorableState: archiver)
    archiver.finishEncoding()
    let archive = archiver.encodedData
    try check(!archive.isEmpty, "the window encodes its state")
    reader.window?.performClose(nil)
    try await waitUntil("window closes") { !delegate.windows.contains { $0 === reader } }
    let unarchiver = try NSKeyedUnarchiver(forReadingFrom: archive)
    let state = try requireValue(RestorationState(from: unarchiver), "restoration state")
    try check(state.position == reader.savedPosition && state.sidebar, "state round-trips")
    let restored = try requireValue(delegate.restoreWindow(state), "restored window")
    restored.showWindow(nil)
    try await waitUntil("restored document settles") {
      restored.surface.document != nil && !restored.isBusy
    }
    let headings = restored.outline.headings
    let part = try requireValue(headings.firstIndex { $0.title == "Part 80" }, "Part 80")
    try check(
      abs(restored.surface.topLocation() - headings[part].range.location) <= 1,
      "a restored window returns to the reader's place")
    try check(
      (restored.surface.textView.string as NSString).substring(
        with: restored.surface.textView.selectedRange()) == "marked",
      "a restored window keeps the selection")
    try check(delegate.restoreWindow(state) == nil, "a document already shown isn't restored twice")

    // Forged or stale state never crashes: a wrong version or a missing file
    // restores nothing, and unreadable anchors open the document at the top.
    let forged = NSKeyedArchiver(requiringSecureCoding: true)
    forged.encode(99, forKey: "readerVersion")
    forged.encode(state.bookmark as NSData, forKey: "documentBookmark")
    forged.finishEncoding()
    try check(
      RestorationState(from: try NSKeyedUnarchiver(forReadingFrom: forged.encodedData)) == nil,
      "another state version is ignored")
    restored.window?.performClose(nil)
    try await waitUntil("window closes") { !delegate.windows.contains { $0 === restored } }
    let garbage = RestorationState(
      bookmark: state.bookmark,
      position: SavedPosition(
        top: Data("not an anchor".utf8), selectionStart: Data(count: 4), selectionEnd: nil),
      sidebar: false)
    let atTop = try requireValue(delegate.restoreWindow(garbage), "window with unreadable anchors")
    atTop.showWindow(nil)
    try await waitUntil("document settles") { atTop.surface.document != nil && !atTop.isBusy }
    try check(
      atTop.surface.topLocation() == 0 && atTop.isSidebarCollapsed,
      "unreadable anchors open at the top, with the saved sidebar state")
    atTop.window?.performClose(nil)
    try await waitUntil("window closes") { !delegate.windows.contains { $0 === atTop } }
    try FileManager.default.removeItem(at: url)
    try check(delegate.restoreWindow(state) == nil, "a deleted document isn't restored")
    note("restoration checked")
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
