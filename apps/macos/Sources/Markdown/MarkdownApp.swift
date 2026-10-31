import AppKit
import Darwin
import MarkdownCore
import MarkdownDocument
import MarkdownRender
import UniformTypeIdentifiers

@main
enum MarkdownApp {
  @MainActor
  static func main() {
    do { try CoreRuntime.validate() } catch {
      FileHandle.standardError.write(Data("Core initialization failed: \(error)\n".utf8))
      exit(EXIT_FAILURE)
    }
    let app = NSApplication.shared
    let delegate = AppDelegate(smokeTest: CommandLine.arguments.contains("--smoke-test"))
    app.setActivationPolicy(.regular)
    app.delegate = delegate
    app.run()
    withExtendedLifetime(delegate) {}
  }
}

@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate, NSMenuItemValidation {
  private(set) var windows: [ReaderWindowController] = []
  private let smokeTest: Bool
  let preferences: ReaderPreferences
  private(set) var settings: SettingsWindowController?

  init(smokeTest: Bool) {
    self.smokeTest = smokeTest
    // The smoke never reads or writes the user's settings.
    preferences = ReaderPreferences(
      store: smokeTest ? MemoryPreferenceStore() : UserDefaults.standard)
    appliedScale = preferences.textScale
    super.init()
    preferences.onChange = { [weak self] in self?.preferencesChanged() }
  }

  func applicationWillFinishLaunching(_ notification: Notification) {
    NSApp.appearance = preferences.appearance.appearance
    installMenus()
  }

  /// The text size the windows were last rendered at.
  private var appliedScale: CGFloat

  private func preferencesChanged() {
    NSApp.appearance = preferences.appearance.appearance
    settings?.refresh()
    let scale = preferences.textScale
    guard scale != appliedScale else { return }
    appliedScale = scale
    for window in windows { window.applyTextScale() }
  }

  @objc func showSettings(_ sender: Any?) {
    let controller = settings ?? SettingsWindowController(preferences: preferences)
    settings = controller
    controller.showWindow(sender)
    controller.window?.makeKeyAndOrderFront(sender)
  }

  @objc func setAppearance(_ sender: NSMenuItem) {
    guard let value = sender.representedObject as? String,
      let appearance = ReaderPreferences.Appearance(rawValue: value)
    else { return }
    preferences.appearance = appearance
  }

  @objc func makeTextBigger(_ sender: Any?) {
    if let scale = TextScale.larger(preferences.textScale) { preferences.textScale = scale }
  }

  @objc func makeTextSmaller(_ sender: Any?) {
    if let scale = TextScale.smaller(preferences.textScale) { preferences.textScale = scale }
  }

  @objc func makeTextActualSize(_ sender: Any?) {
    preferences.textScale = TextScale.standard
  }

  func validateMenuItem(_ menuItem: NSMenuItem) -> Bool {
    switch menuItem.action {
    case #selector(setAppearance(_:)):
      menuItem.state =
        menuItem.representedObject as? String == preferences.appearance.rawValue ? .on : .off
      return true
    case #selector(makeTextBigger(_:)): return TextScale.larger(preferences.textScale) != nil
    case #selector(makeTextSmaller(_:)): return TextScale.smaller(preferences.textScale) != nil
    case #selector(makeTextActualSize(_:)): return preferences.textScale != TextScale.standard
    default: return true
    }
  }

  func applicationDidFinishLaunching(_ notification: Notification) {
    // Documents delivered at launch (Finder, `open`) already created windows.
    if windows.isEmpty { newWindow(nil) }
    NSApp.activate()
    if smokeTest {
      // Exercise the running AppKit application, not just its constructors.
      DispatchQueue.main.asyncAfter(deadline: .now() + 0.2) { [self] in
        Task { await ShellSmoke.run(delegate: self) }
      }
    }
  }

  func applicationShouldHandleReopen(_ sender: NSApplication, hasVisibleWindows: Bool) -> Bool {
    if !hasVisibleWindows {
      if let existing = windows.first {
        existing.window?.deminiaturize(nil)
        existing.showWindow(nil)
      } else {
        newWindow(nil)
      }
    }
    return true
  }

  /// Finder, Launch Services and `open -a` deliver granted file URLs here.
  func application(_ sender: NSApplication, open urls: [URL]) {
    open(urls)
  }

  @objc func openDocument(_ sender: Any?) {
    let panel = NSOpenPanel()
    panel.allowedContentTypes = [UTType("net.daringfireball.markdown") ?? .plainText, .plainText]
    panel.allowsMultipleSelection = true
    panel.canChooseDirectories = false
    panel.begin { [weak self, panel] response in
      if response == .OK { self?.open(panel.urls) }
    }
  }

  /// Focus a window already showing the file, else reuse an empty window, else
  /// open a new one. Each window owns its document and core session.
  func open(_ urls: [URL]) {
    for url in urls {
      guard let capability = try? DocumentCapability(granted: url) else { continue }
      if let existing = windows.first(where: { $0.loader.capability == capability }) {
        // Choosing a file again retries a failed load, e.g. after reconnecting.
        if case .failed = existing.loader.state { existing.loader.reload() }
        existing.showWindow(nil)
        continue
      }
      let target =
        windows.first(where: { $0.window?.isKeyWindow == true && $0.loader.capability == nil })
        ?? windows.first(where: { $0.loader.capability == nil }) ?? makeWindow(nil)
      target.loader.open(capability)
      target.showWindow(nil)
    }
  }

  @objc func newWindow(_ sender: Any?) {
    makeWindow(sender)
  }

  @discardableResult
  private func makeWindow(_ sender: Any?) -> ReaderWindowController {
    let controller = ReaderWindowController(preferences: preferences)
    controller.onClose = { [weak self] closed in
      self?.windows.removeAll { $0 === closed }
    }
    windows.append(controller)
    controller.showWindow(sender)
    controller.window?.makeKeyAndOrderFront(sender)
    return controller
  }

  private func installMenus() {
    let bar = NSMenu()
    let appMenu = NSMenu(title: "Markdown")
    appMenu.addItem(
      withTitle: "About Markdown",
      action: #selector(NSApplication.orderFrontStandardAboutPanel(_:)), keyEquivalent: "")
    appMenu.addItem(.separator())
    let settings = appMenu.addItem(
      withTitle: "Settings…", action: #selector(showSettings(_:)), keyEquivalent: ",")
    settings.target = self
    appMenu.addItem(.separator())
    appMenu.addItem(
      withTitle: "Hide Markdown", action: #selector(NSApplication.hide(_:)), keyEquivalent: "h")
    let hideOthers = appMenu.addItem(
      withTitle: "Hide Others", action: #selector(NSApplication.hideOtherApplications(_:)),
      keyEquivalent: "h")
    hideOthers.keyEquivalentModifierMask = [.command, .option]
    appMenu.addItem(
      withTitle: "Show All", action: #selector(NSApplication.unhideAllApplications(_:)),
      keyEquivalent: "")
    appMenu.addItem(.separator())
    appMenu.addItem(
      withTitle: "Quit Markdown", action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q"
    )
    add(appMenu, to: bar)

    let file = NSMenu(title: "File")
    let new = file.addItem(
      withTitle: "New Window", action: #selector(newWindow(_:)), keyEquivalent: "n")
    new.target = self
    let open = file.addItem(
      withTitle: "Open…", action: #selector(openDocument(_:)), keyEquivalent: "o")
    open.target = self
    file.addItem(
      withTitle: "Reload", action: #selector(ReaderWindowController.reloadDocument(_:)),
      keyEquivalent: "r")
    file.addItem(.separator())
    file.addItem(
      withTitle: "Close Window", action: #selector(NSWindow.performClose(_:)), keyEquivalent: "w")
    add(file, to: bar)

    let edit = NSMenu(title: "Edit")
    edit.addItem(withTitle: "Copy", action: #selector(NSText.copy(_:)), keyEquivalent: "c")
    edit.addItem(
      withTitle: "Select All", action: #selector(NSText.selectAll(_:)), keyEquivalent: "a")
    edit.addItem(.separator())
    let find = NSMenu(title: "Find")
    find.addItem(
      withTitle: "Find…", action: #selector(ReaderWindowController.showFindBar(_:)),
      keyEquivalent: "f")
    find.addItem(
      withTitle: "Find Next", action: #selector(ReaderWindowController.findNextMatch(_:)),
      keyEquivalent: "g")
    let previous = find.addItem(
      withTitle: "Find Previous", action: #selector(ReaderWindowController.findPreviousMatch(_:)),
      keyEquivalent: "g")
    previous.keyEquivalentModifierMask = [.command, .shift]
    find.addItem(
      withTitle: "Use Selection for Find",
      action: #selector(ReaderWindowController.useSelectionForFind(_:)), keyEquivalent: "e")
    add(find, to: edit)
    add(edit, to: bar)

    let view = NSMenu(title: "View")
    let sidebar = view.addItem(
      withTitle: "Show Sidebar", action: #selector(NSSplitViewController.toggleSidebar(_:)),
      keyEquivalent: "s")
    sidebar.keyEquivalentModifierMask = [.command, .control]
    view.addItem(.separator())
    for (title, action, key) in [
      ("Actual Size", #selector(makeTextActualSize(_:)), "0"),
      ("Make Text Bigger", #selector(makeTextBigger(_:)), "+"),
      ("Make Text Smaller", #selector(makeTextSmaller(_:)), "-"),
    ] {
      view.addItem(withTitle: title, action: action, keyEquivalent: key).target = self
    }
    // Command-= (unshifted) also makes text bigger, as in other Mac apps.
    let equals = view.addItem(
      withTitle: "Make Text Bigger", action: #selector(makeTextBigger(_:)), keyEquivalent: "=")
    equals.target = self
    equals.isHidden = true
    equals.allowsKeyEquivalentWhenHidden = true
    view.addItem(.separator())
    let appearance = NSMenu(title: "Appearance")
    for option in ReaderPreferences.Appearance.allCases {
      let item = appearance.addItem(
        withTitle: option.title, action: #selector(setAppearance(_:)), keyEquivalent: "")
      item.representedObject = option.rawValue
      item.target = self
    }
    add(appearance, to: view)
    add(view, to: bar)

    let window = NSMenu(title: "Window")
    window.addItem(
      withTitle: "Minimize", action: #selector(NSWindow.performMiniaturize(_:)), keyEquivalent: "m")
    window.addItem(
      withTitle: "Zoom", action: #selector(NSWindow.performZoom(_:)), keyEquivalent: "")
    window.addItem(.separator())
    window.addItem(
      withTitle: "Bring All to Front", action: #selector(NSApplication.arrangeInFront(_:)),
      keyEquivalent: "")
    add(window, to: bar)
    NSApp.windowsMenu = window
    NSApp.mainMenu = bar
  }

  private func add(_ menu: NSMenu, to bar: NSMenu) {
    let item = NSMenuItem(title: menu.title, action: nil, keyEquivalent: "")
    item.submenu = menu
    bar.addItem(item)
  }
}
