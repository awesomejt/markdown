import AppKit
import Darwin
import MarkdownCore

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
final class AppDelegate: NSObject, NSApplicationDelegate {
  private(set) var windows: [ReaderWindowController] = []
  private let smokeTest: Bool

  init(smokeTest: Bool) {
    self.smokeTest = smokeTest
  }

  func applicationDidFinishLaunching(_ notification: Notification) {
    installMenus()
    newWindow(nil)
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

  @objc func newWindow(_ sender: Any?) {
    let controller = ReaderWindowController()
    controller.onClose = { [weak self] closed in
      self?.windows.removeAll { $0 === closed }
    }
    windows.append(controller)
    controller.showWindow(sender)
    controller.window?.makeKeyAndOrderFront(sender)
  }

  private func installMenus() {
    let bar = NSMenu()
    let appMenu = NSMenu(title: "Markdown")
    appMenu.addItem(
      withTitle: "About Markdown",
      action: #selector(NSApplication.orderFrontStandardAboutPanel(_:)), keyEquivalent: "")
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
    file.addItem(
      withTitle: "Close Window", action: #selector(NSWindow.performClose(_:)), keyEquivalent: "w")
    add(file, to: bar)

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

@MainActor
final class ReaderWindowController: NSWindowController, NSWindowDelegate {
  var onClose: ((ReaderWindowController) -> Void)?

  init() {
    let window = NSWindow(
      contentRect: NSRect(x: 0, y: 0, width: 900, height: 640),
      styleMask: [.titled, .closable, .miniaturizable, .resizable],
      backing: .buffered, defer: false
    )
    window.title = "Markdown"
    window.minSize = NSSize(width: 480, height: 320)
    window.isReleasedWhenClosed = false
    window.tabbingMode = .disallowed
    window.center()

    let content = NSView()
    let title = NSTextField(labelWithString: "No document open")
    title.font = .systemFont(ofSize: 22, weight: .semibold)
    title.textColor = .secondaryLabelColor
    title.setAccessibilityIdentifier("empty-document-label")
    title.translatesAutoresizingMaskIntoConstraints = false
    content.addSubview(title)
    NSLayoutConstraint.activate([
      title.centerXAnchor.constraint(equalTo: content.centerXAnchor),
      title.centerYAnchor.constraint(equalTo: content.centerYAnchor),
    ])
    window.contentView = content
    super.init(window: window)
    window.delegate = self
  }

  required init?(coder: NSCoder) {
    fatalError("Reader windows are constructed programmatically")
  }

  func windowWillClose(_ notification: Notification) {
    onClose?(self)
  }
}
