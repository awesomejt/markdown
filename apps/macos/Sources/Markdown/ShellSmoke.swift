import AppKit
import Darwin

/// Opt-in development check, launched by tools/check.py in a bounded child process.
@MainActor
enum ShellSmoke {
  private struct Failure: Error, CustomStringConvertible {
    let description: String
  }

  private final class WeakController {
    weak var value: ReaderWindowController?
    init(_ value: ReaderWindowController?) { self.value = value }
  }

  static func run(delegate: AppDelegate) async {
    do {
      try check(
        Bundle.main.bundleIdentifier == "dev.markdown.reader", "must run inside the app bundle")
      try check(NSApp.isRunning, "application event loop is running")
      try check(delegate.windows.count == 1, "launch creates one window")
      let first = try window(delegate)
      try check(
        first.isVisible && first.styleMask.contains(.resizable), "visible resizable native window")
      try check(
        first.contentView?.subviews.contains {
          $0.accessibilityIdentifier() == "empty-document-label"
        } == true,
        "accessible native empty state")
      let new = try item("File", "New Window", key: "n")
      try check(
        NSApp.sendAction(new.action!, to: new.target, from: new), "New Window command routes")
      try check(delegate.windows.count == 2, "New Window creates another controller")
      let close = try item("File", "Close Window", key: "w")
      let closedController = try closeLastWindow(delegate, using: close)
      try check(delegate.windows.count == 1, "closing releases controller ownership")
      // AppKit releases its closing-window bookkeeping on a later run-loop turn.
      for _ in 0..<40 {
        if closedController.value == nil { break }
        try await Task.sleep(for: .milliseconds(50))
      }
      try check(closedController.value == nil, "closed controller is not retained")
      first.performClose(nil)
      try check(
        delegate.windows.isEmpty && NSApp.isRunning, "closing last window keeps app running")
      _ = delegate.applicationShouldHandleReopen(NSApp, hasVisibleWindows: false)
      try check(delegate.windows.count == 1, "Dock reopen creates a window")
      _ = delegate.applicationShouldHandleReopen(NSApp, hasVisibleWindows: true)
      try check(delegate.windows.count == 1, "reopen with a visible window creates no duplicate")
      let reopened = try window(delegate)
      let minimize = try item("Window", "Minimize", key: "m")
      try check(
        NSApp.sendAction(minimize.action!, to: reopened, from: minimize), "Minimize command routes")
      try check(reopened.isMiniaturized, "window minimizes")
      _ = delegate.applicationShouldHandleReopen(NSApp, hasVisibleWindows: false)
      try check(
        delegate.windows.count == 1 && !reopened.isMiniaturized && reopened.isVisible,
        "Dock reopen restores existing minimized window")
      _ = try item("Window", "Zoom", key: "")
      _ = try item("Markdown", "About Markdown", key: "")
      _ = try item("Markdown", "Hide Markdown", key: "h")
      let quit = try item("Markdown", "Quit Markdown", key: "q")
      print(
        "shell-smoke: pass (bundle, event loop, windows, commands, close/reopen, minimize, ownership)"
      )
      try check(NSApp.sendAction(quit.action!, to: NSApp, from: quit), "Quit command routes")
    } catch {
      FileHandle.standardError.write(Data("shell-smoke: FAIL: \(error)\n".utf8))
      exit(1)
    }
  }

  private static func closeLastWindow(_ delegate: AppDelegate, using item: NSMenuItem) throws
    -> WeakController
  {
    // Drop the test's own strong window reference and autoreleased AppKit temporaries.
    try autoreleasepool {
      let reference = WeakController(delegate.windows.last)
      let closing = try window(delegate)
      try check(
        NSApp.sendAction(item.action!, to: closing, from: item), "Close Window command routes")
      return reference
    }
  }

  private static func window(_ delegate: AppDelegate) throws -> NSWindow {
    guard let window = delegate.windows.last?.window else {
      throw Failure(description: "missing reader window")
    }
    return window
  }

  private static func item(_ menu: String, _ title: String, key: String) throws -> NSMenuItem {
    guard let item = NSApp.mainMenu?.item(withTitle: menu)?.submenu?.item(withTitle: title) else {
      throw Failure(description: "missing menu item \(menu)/\(title)")
    }
    try check(
      item.keyEquivalent == key && item.keyEquivalentModifierMask == .command,
      "shortcut for \(title)")
    guard item.action != nil else { throw Failure(description: "missing action for \(title)") }
    return item
  }

  private static func check(_ value: Bool, _ message: String) throws {
    if !value { throw Failure(description: message) }
  }
}
