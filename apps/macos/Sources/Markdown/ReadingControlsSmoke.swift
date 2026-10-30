import AppKit
import MarkdownRender

/// Find, text size, appearance and Settings inside the running app
/// (MAC-001F). Settings are kept in memory, never in the user's defaults.
@MainActor
enum ReadingControlsSmoke {
  static func run(
    delegate: AppDelegate, file: (String, Data) throws -> URL, check: (Bool, String) throws -> Void
  ) async throws {
    let find = try item(["Edit", "Find"], "Find…", key: "f")
    let next = try item(["Edit", "Find"], "Find Next", key: "g")
    let previous = try item(
      ["Edit", "Find"], "Find Previous", key: "g", modifiers: [.command, .shift])
    let useSelection = try item(["Edit", "Find"], "Use Selection for Find", key: "e")

    var text =
      "# Unicode find\n\nCafé, café and cafe\u{301}. 👨\u{200D}👩\u{200D}👧 family; 中文; עברית.\n\n"
    for index in 0..<120 { text += "Paragraph \(index) mentions the CAFÉ again.\n\n" }
    delegate.application(NSApp, open: [try file("find.md", Data(text.utf8))])
    guard let reader = delegate.windows.last else { throw SmokeFailure(description: "no window") }
    try await waitUntil("document settles") { !reader.isBusy }
    let surface = reader.surface
    let string = surface.textView.string as NSString

    try check(reader.validateMenuItem(find), "Find is enabled for a document")
    try check(!reader.validateMenuItem(useSelection), "Use Selection needs a selection")
    try check(NSApp.sendAction(find.action!, to: reader, from: find), "Find command routes")
    try check(reader.isFindBarVisible, "Find shows the find bar")
    try check(
      reader.window?.firstResponder === reader.findBar.field.currentEditor(),
      "the find field has the focus")

    func search(_ query: String) async throws -> FindMatches {
      reader.findBar.field.stringValue = query
      _ = reader.findBar.field.sendAction(
        reader.findBar.field.action, to: reader.findBar.field.target)
      try await waitUntil("find completes") { !reader.isFinding }
      guard let matches = reader.findMatches else {
        throw SmokeFailure(description: "no matches for \(query)")
      }
      return matches
    }
    func texts(_ matches: FindMatches) -> [String] { matches.ranges.map(string.substring(with:)) }

    // Case-insensitive, NFC only: the decomposed spelling is another string.
    let cafe = try await search("café")
    try check(cafe.count == 122, "café matches every composed spelling")
    try check(
      Set(texts(cafe)) == ["Café", "café", "CAFÉ"], "highlights cover exactly the matched text")
    try check(surface.matches == cafe.ranges, "every match is highlighted")
    // While the field has the focus the current match is highlighted, not selected.
    try check(surface.currentMatch == 0, "the first match is current")
    try check(
      surface.textView.selectedRange().length == 0, "the field keeps the selection unchanged")
    try check(reader.findBar.status.stringValue == "1 of 122", "status counts matches")
    try check(NSApp.sendAction(next.action!, to: reader, from: next), "Find Next routes")
    try check(surface.currentMatch == 1, "Find Next goes to match 2")
    for _ in 0..<2 {
      try check(
        NSApp.sendAction(previous.action!, to: reader, from: previous), "Find Previous routes")
    }
    try check(surface.currentMatch == 121, "Find Previous wraps to the end")
    try check(surface.isVisible(cafe.ranges[121]), "the current match is revealed")
    try check(reader.findBar.status.stringValue == "122 of 122", "status follows navigation")
    // Return in the field goes to the next match, wrapping to the first.
    let editor = reader.findBar.field.currentEditor() as? NSTextView
    try check(editor != nil, "find field editor")
    _ = reader.findBar.control(
      reader.findBar.field, textView: editor!, doCommandBy: #selector(NSResponder.insertNewline(_:))
    )
    try check(surface.currentMatch == 0, "Return finds the next match")

    reader.findBar.matchCase.performClick(nil)
    try await waitUntil("case-sensitive find completes") { !reader.isFinding }
    try check(reader.findMatches.map(texts) == ["café"], "Match Case narrows the matches")
    reader.findBar.matchCase.performClick(nil)
    try check(texts(try await search("cafe\u{301}")) == ["cafe\u{301}"], "decomposed query")
    try check(
      texts(try await search("👨\u{200D}👩\u{200D}👧")) == ["👨\u{200D}👩\u{200D}👧"], "emoji sequence")
    try check(try await search("👨").ranges.isEmpty, "no match inside an emoji sequence")
    try check(texts(try await search("中文")) == ["中文"], "CJK")
    try check(texts(try await search("עברית")) == ["עברית"], "right-to-left text")
    try check(try await search("missing").ranges.isEmpty, "no match")
    try check(reader.findBar.status.stringValue == "Not found", "status reports no match")

    // Escape closes the bar and its highlights; the last match stays selected.
    let family = try await search("family")
    _ = reader.findBar.control(
      reader.findBar.field, textView: editor!,
      doCommandBy: #selector(NSResponder.cancelOperation(_:)))
    try check(!reader.isFindBarVisible && surface.matches.isEmpty, "Escape closes find")
    try check(
      surface.textView.selectedRange() == family.ranges[0], "the match becomes the selection")
    try check(reader.window?.firstResponder === surface.textView, "focus returns to the document")
    surface.textView.setSelectedRange(string.range(of: "mentions"))
    try check(reader.validateMenuItem(useSelection), "Use Selection is enabled")
    try check(
      NSApp.sendAction(useSelection.action!, to: reader, from: useSelection),
      "Use Selection for Find routes")
    try await waitUntil("selection find completes") { !reader.isFinding }
    try check(
      reader.findBar.query == "mentions" && reader.findMatches?.count == 120,
      "Use Selection for Find searches the selection")
    try check(reader.window?.firstResponder === surface.textView, "focus stays in the document")
    // With the focus in the document, Find Next selects each match.
    let mentions = try requireValue(reader.findMatches, "selection matches")
    try check(
      surface.textView.selectedRange() == mentions.ranges[0], "the document selects its match")
    try check(NSApp.sendAction(next.action!, to: reader, from: next), "Find Next routes")
    try check(surface.textView.selectedRange() == mentions.ranges[1], "Find Next selects match 2")
    reader.closeFindBar()
    note("find/Unicode find/find commands checked")

    try await textSize(delegate: delegate, reader: reader, check: check)
    try await appearanceAndSettings(delegate: delegate, reader: reader, check: check)
  }

  private static func textSize(
    delegate: AppDelegate, reader: ReaderWindowController, check: (Bool, String) throws -> Void
  ) async throws {
    let bigger = try item(["View"], "Make Text Bigger", key: "+")
    let smaller = try item(["View"], "Make Text Smaller", key: "-")
    let actual = try item(["View"], "Actual Size", key: "0")
    let surface = reader.surface
    let location = (surface.textView.string as NSString).range(of: "Paragraph 60").location
    func pointSize() -> CGFloat {
      (surface.textView.textStorage?.attribute(.font, at: location, effectiveRange: nil) as? NSFont)?
        .pointSize ?? 0
    }
    let base = pointSize()
    try check(!delegate.validateMenuItem(actual), "Actual Size is disabled at actual size")
    surface.textView.setSelectedRange(NSRange(location: location, length: 9))
    try check(NSApp.sendAction(bigger.action!, to: delegate, from: bigger), "Bigger routes")
    try await waitUntil("text re-renders larger") {
      !reader.isBusy && surface.document?.scale == 1.15
    }
    try check(abs(pointSize() - base * 1.15) < 0.01, "Make Text Bigger enlarges the text")
    try check(
      surface.textView.selectedRange() == NSRange(location: location, length: 9),
      "the selection survives a text size change")
    try check(
      delegate.preferences.store.object(forKey: ReaderPreferences.textScaleKey) as? Double == 1.15,
      "the text size is stored")
    while delegate.validateMenuItem(smaller) {
      try check(NSApp.sendAction(smaller.action!, to: delegate, from: smaller), "Smaller routes")
    }
    try await waitUntil("text re-renders smaller") {
      !reader.isBusy && surface.document?.scale == TextScale.steps.first
    }
    try check(pointSize() < base, "Make Text Smaller stops at the smallest size")
    try check(NSApp.sendAction(actual.action!, to: delegate, from: actual), "Actual Size routes")
    try await waitUntil("text re-renders at actual size") {
      !reader.isBusy && surface.document?.scale == 1
    }
    try check(pointSize() == base, "Actual Size restores the text")
    note("text size commands checked")
  }

  private static func appearanceAndSettings(
    delegate: AppDelegate, reader: ReaderWindowController, check: (Bool, String) throws -> Void
  ) async throws {
    let preferences = delegate.preferences
    for (title, name) in [("Dark", NSAppearance.Name.darkAqua), ("Light", .aqua)] {
      let option = try item(["View", "Appearance"], title, key: "", modifiers: [])
      try check(NSApp.sendAction(option.action!, to: delegate, from: option), "\(title) routes")
      try check(NSApp.appearance?.name == name, "\(title) appearance applies")
      try check(
        reader.window?.effectiveAppearance.bestMatch(from: [.aqua, .darkAqua]) == name,
        "\(title) reaches the reader window")
      _ = delegate.validateMenuItem(option)
      try check(option.state == .on, "\(title) is checked")
    }
    let system = try item(["View", "Appearance"], "System", key: "", modifiers: [])
    try check(NSApp.sendAction(system.action!, to: delegate, from: system), "System routes")
    try check(NSApp.appearance == nil, "System follows the system appearance")

    let settingsItem = try item(["Markdown"], "Settings…", key: ",")
    try check(
      NSApp.sendAction(settingsItem.action!, to: delegate, from: settingsItem), "Settings routes")
    guard let settings = delegate.settings, let window = settings.window, window.isVisible else {
      throw SmokeFailure(description: "Settings window is not shown")
    }
    settings.appearance.selectItem(withTitle: "Dark")
    _ = settings.appearance.sendAction(settings.appearance.action, to: settings.appearance.target)
    try check(
      preferences.appearance == .dark && NSApp.appearance?.name == .darkAqua, "Settings sets Dark")
    settings.textSize.integerValue = TextScale.steps.firstIndex(of: 1.5)!
    _ = settings.textSize.sendAction(settings.textSize.action, to: settings.textSize.target)
    try await waitUntil("Settings text size re-renders") {
      !reader.isBusy && reader.surface.document?.scale == 1.5
    }
    try check(settings.textSizeLabel.stringValue == "150%", "Settings shows the text size")
    let bigger = try item(["View"], "Make Text Bigger", key: "+")
    _ = NSApp.sendAction(bigger.action!, to: delegate, from: bigger)
    try check(settings.textSizeLabel.stringValue == "175%", "Settings follows the View menu")
    // A relaunch reads the same stored settings.
    let reread = ReaderPreferences(store: preferences.store)
    try check(reread.appearance == .dark && reread.textScale == 1.75, "settings persist")
    preferences.appearance = .system
    preferences.textScale = 1
    try await waitUntil("settings reset") { !reader.isBusy && reader.surface.document?.scale == 1 }
    try check(settings.appearance.titleOfSelectedItem == "System", "Settings follows resets")
    window.performClose(nil)
    note("appearance/settings checked")
  }

  private static func waitUntil(_ message: String, condition: () -> Bool) async throws {
    let deadline = ContinuousClock.now.advanced(by: .seconds(10))
    while !condition() && ContinuousClock.now < deadline {
      try await Task.sleep(for: .milliseconds(20))
    }
    if !condition() { throw SmokeFailure(description: message) }
  }

  private static func item(
    _ path: [String], _ title: String, key: String, modifiers: NSEvent.ModifierFlags = .command
  ) throws -> NSMenuItem {
    var menu = NSApp.mainMenu
    for name in path { menu = menu?.item(withTitle: name)?.submenu }
    guard let item = menu?.item(withTitle: title), item.keyEquivalent == key,
      item.keyEquivalentModifierMask == modifiers || key.isEmpty, item.action != nil
    else { throw SmokeFailure(description: "missing menu item \(path)/\(title)") }
    return item
  }

  private static func note(_ message: String) {
    FileHandle.standardOutput.write(Data("shell-smoke: \(message)\n".utf8))
  }
}
