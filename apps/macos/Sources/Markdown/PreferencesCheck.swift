import AppKit
import MarkdownDocument
import MarkdownRender

/// Opt-in development check of persistent settings (MAC-002D), launched
/// twice by tools/check.py against a throwaway user defaults suite that it
/// seeds with MAC-001F (version 1) settings. `prepare` expects them upgraded
/// to version 2, changes every setting through the Settings window and
/// quits; `verify` expects the changes back, checks that new windows and
/// restoration follow them, restores the defaults, deletes the suite and
/// quits. It never reads or writes the user's own settings.
@MainActor
enum PreferencesCheck {
  enum Phase: String {
    case prepare, verify
  }

  struct Request {
    let phase: Phase
    /// The user defaults suite standing in for the app's settings.
    let suite: String
    /// A document to build saved window state for.
    let document: URL
  }

  /// `--preferences-check prepare|verify <suite> <document>`.
  static func arguments(_ arguments: [String]) -> Request? {
    guard let flag = arguments.firstIndex(of: "--preferences-check"),
      arguments.count > flag + 3,
      let phase = Phase(rawValue: arguments[flag + 1]),
      arguments[flag + 2].hasPrefix("io.github.awesomejt.markdown.preferences-check.")
    else { return nil }
    return Request(
      phase: phase, suite: arguments[flag + 2],
      document: URL(fileURLWithPath: arguments[flag + 3]))
  }

  private struct Failure: Error, CustomStringConvertible {
    let description: String
  }

  private static func check(_ condition: Bool, _ message: String) throws {
    if !condition { throw Failure(description: message) }
  }

  static func run(_ request: Request, delegate: AppDelegate) async {
    do {
      let preferences = delegate.preferences
      try check(
        preferences.store is UserDefaults, "the check must use its own user defaults suite")
      delegate.showSettings(nil)
      guard let settings = delegate.settings else { throw Failure(description: "no Settings") }
      switch request.phase {
      case .prepare: try prepare(preferences, settings, delegate)
      case .verify: try await verify(request, preferences, settings, delegate)
      }
      settings.window?.close()
      for window in delegate.windows.map(\.window) { window?.close() }
      if request.phase == .verify {
        UserDefaults.standard.removePersistentDomain(forName: request.suite)
      } else {
        (preferences.store as? UserDefaults)?.synchronize()
      }
      note(request.phase == .prepare ? "prepared" : "pass")
      NSApp.terminate(nil)
    } catch {
      FileHandle.standardError.write(Data("preferences-check: FAIL: \(error)\n".utf8))
      exit(1)
    }
  }

  /// The seeded MAC-001F settings: dark, an off-step 1.2 text size and a
  /// malformed sidebar value.
  private static func prepare(
    _ preferences: ReaderPreferences, _ settings: SettingsWindowController,
    _ delegate: AppDelegate
  ) throws {
    let store = preferences.store
    try check(preferences.migration == .upgraded(from: 1), "v1 settings are upgraded")
    try check(ReaderPreferences.storedVersion(store) == 2, "the upgrade stores version 2")
    try check(
      store.object(forKey: ReaderPreferences.textScaleKey) as? Double == 1.15,
      "the upgrade snaps the text size to a step")
    try check(
      store.object(forKey: ReaderPreferences.showsSidebarKey) == nil,
      "the upgrade drops a malformed setting")
    try check(
      preferences.appearance == .dark && NSApp.appearance?.name == .darkAqua,
      "the upgrade keeps the appearance")
    try check(
      preferences.showsSidebar && preferences.reopensWindows, "new settings have defaults")
    try check(delegate.windows.first?.window?.isRestorable == true, "windows are restorable")
    note("upgraded version 1 settings")

    // Change every setting through the Settings window's own controls.
    settings.appearance.selectItem(withTitle: "Light")
    try send(settings.appearance)
    settings.textSize.integerValue = try step(1.5)
    try send(settings.textSize)
    settings.showsSidebar.performClick(nil)
    settings.reopensWindows.performClick(nil)
    try check(
      preferences.appearance == .light && preferences.textScale == 1.5
        && !preferences.showsSidebar && !preferences.reopensWindows,
      "Settings changes every setting")
    try check(NSApp.appearance?.name == .aqua, "Settings sets Light")
    try check(
      delegate.windows.first?.window?.isRestorable == false,
      "turning reopening off stops saving open windows")
    let window = delegate.makeWindow(nil)
    try check(window.isSidebarCollapsed, "new windows follow the sidebar setting")
  }

  private static func verify(
    _ request: Request, _ preferences: ReaderPreferences, _ settings: SettingsWindowController,
    _ delegate: AppDelegate
  ) async throws {
    let store = preferences.store
    try check(preferences.migration == .current, "version 2 settings are not migrated again")
    try check(
      preferences.appearance == .light && preferences.textScale == 1.5
        && !preferences.showsSidebar && !preferences.reopensWindows,
      "settings persist across launches")
    try check(NSApp.appearance?.name == .aqua, "the appearance applies at launch")
    try check(
      settings.appearance.titleOfSelectedItem == "Light"
        && settings.textSizeLabel.stringValue == "150%" && settings.showsSidebar.state == .off
        && settings.reopensWindows.state == .off && settings.restoreDefaults.isEnabled,
      "Settings shows the stored settings")
    guard let launched = delegate.windows.first else { throw Failure(description: "no window") }
    try check(launched.isSidebarCollapsed, "the launch window follows the sidebar setting")
    try check(launched.window?.isRestorable == false, "windows aren't saved with reopening off")

    // Saved state for a real document is declined while reopening is off.
    let saved = try savedState(request.document)
    try check(
      delegate.restoreWindow(from: try saved()) == nil, "reopening off declines restoration")
    settings.reopensWindows.performClick(nil)
    try check(preferences.reopensWindows, "Settings turns reopening on")
    try check(launched.window?.isRestorable == true, "turning reopening on saves open windows")
    guard let restored = delegate.restoreWindow(from: try saved()) else {
      throw Failure(description: "reopening on restores saved state")
    }
    try check(!restored.isSidebarCollapsed, "a restored window keeps its saved sidebar")
    restored.window?.close()
    try check(delegate.windows.count == 1, "the restored window closed")
    note("restoration follows the reopen setting")

    settings.restoreDefaults.performClick(nil)
    try check(preferences.isDefault, "Restore Defaults restores every setting")
    try check(
      ReaderPreferences.settingKeys.allSatisfy { store.object(forKey: $0) == nil }
        && ReaderPreferences.storedVersion(store) == 2,
      "Restore Defaults clears the stored settings and keeps the version")
    try check(NSApp.appearance == nil, "Restore Defaults follows the system appearance")
    try check(
      settings.appearance.titleOfSelectedItem == "System"
        && settings.textSizeLabel.stringValue == "100%" && settings.showsSidebar.state == .on
        && settings.reopensWindows.state == .on && !settings.restoreDefaults.isEnabled,
      "Settings shows the defaults")
    try check(!delegate.makeWindow(nil).isSidebarCollapsed, "new windows show the sidebar again")
  }

  /// A fresh decoder over saved window state for the document each call.
  private static func savedState(_ document: URL) throws -> () throws -> NSCoder {
    guard let bookmark = try DocumentCapability(granted: document).bookmark() else {
      throw Failure(description: "document bookmark")
    }
    let archiver = NSKeyedArchiver(requiringSecureCoding: true)
    RestorationState(bookmark: bookmark, position: nil, sidebar: true).encode(to: archiver)
    archiver.finishEncoding()
    let data = archiver.encodedData
    return { try NSKeyedUnarchiver(forReadingFrom: data) }
  }

  private static func send(_ control: NSControl) throws {
    try check(
      NSApp.sendAction(control.action!, to: control.target, from: control), "control sends")
  }

  private static func step(_ scale: CGFloat) throws -> Int {
    guard let index = TextScale.steps.firstIndex(of: scale) else {
      throw Failure(description: "text size step")
    }
    return index
  }

  private static func note(_ message: String) {
    FileHandle.standardOutput.write(Data("preferences-check: \(message)\n".utf8))
  }
}
