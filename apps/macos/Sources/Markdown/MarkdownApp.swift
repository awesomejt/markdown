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
    let delegate = AppDelegate(
      smokeTest: CommandLine.arguments.contains("--smoke-test"),
      measurement: Measurement.Mode(arguments: CommandLine.arguments),
      relaunchCheck: RelaunchCheck.arguments(CommandLine.arguments))
    app.setActivationPolicy(.regular)
    app.delegate = delegate
    app.run()
    withExtendedLifetime(delegate) {}
  }
}

@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate, NSMenuItemValidation, NSMenuDelegate {
  private(set) var windows: [ReaderWindowController] = []
  private let smokeTest: Bool
  private let measurement: Measurement.Mode?
  let preferences: ReaderPreferences
  private(set) var settings: SettingsWindowController?
  /// Windows are saved and restored across relaunches, except in the smoke
  /// and measurements, which never read or write the user's saved state.
  let restoresWindows: Bool

  private let relaunchCheck: (RelaunchCheck.Phase, URL)?
  /// Recent documents and folders (MAC-002A). The development checks keep
  /// them in memory, so they never read or change the user's list.
  let recents: Recents
  /// Each recent item's status when last checked, for the Open Recent menu.
  private(set) var recentStatus: [UUID: RecentStatus] = [:]
  private var statusRequest: UInt64 = 0
  private(set) var statusTask: Task<Void, Never>?
  let openRecentMenu = NSMenu(title: "Open Recent")
  /// Chooses a document or folder for Open, Open Folder and recovery; the
  /// smoke replaces the open panel with synthetic choices.
  var choose: (Choice, NSWindow?) async -> [URL] = { await AppDelegate.runPanel($0, window: $1) }

  init(
    smokeTest: Bool, measurement: Measurement.Mode? = nil,
    relaunchCheck: (RelaunchCheck.Phase, URL)? = nil
  ) {
    self.smokeTest = smokeTest
    self.measurement = measurement
    self.relaunchCheck = relaunchCheck
    restoresWindows = !smokeTest && measurement == nil
    // The development checks never read or write the user's settings.
    preferences = ReaderPreferences(
      store: smokeTest || measurement != nil || relaunchCheck != nil
        ? MemoryPreferenceStore() : UserDefaults.standard)
    appliedScale = preferences.textScale
    let isolated = smokeTest || measurement != nil || relaunchCheck != nil
    let storage: any RecentsStorage =
      (isolated ? nil : FileRecentsStorage.standard()) ?? MemoryRecentsStorage()
    recents = Recents(storage: storage)
    super.init()
    recents.onChange = { [weak self] in self?.refreshRecentStatus() }
    openRecentMenu.delegate = self
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
    if measurement?.opensAtLaunch == true,
      let url = Measurement.documentArgument(CommandLine.arguments)
    {
      open([url])
    }
    // Documents delivered at launch (Finder, `open`) already created windows.
    if windows.isEmpty { newWindow(nil) }
    NSApp.activate()
    if let measurement {
      Task { await Measurement.run(measurement, delegate: self) }
    }
    if let (phase, url) = relaunchCheck {
      Task { await RelaunchCheck.run(phase, document: url, delegate: self) }
    }
    if smokeTest {
      // Exercise the running AppKit application, not just its constructors.
      DispatchQueue.main.asyncAfter(deadline: .now() + 0.2) { [self] in
        Task { await ShellSmoke.run(delegate: self) }
      }
    }
  }

  func applicationDidBecomeActive(_ notification: Notification) {
    refreshRecentStatus()
  }

  func applicationSupportsSecureRestorableState(_ app: NSApplication) -> Bool { true }

  /// Recreate a reader window from its saved state (ReaderWindowRestoration).
  func restoreWindow(from coder: NSCoder) -> ReaderWindowController? {
    guard restoresWindows, let state = RestorationState(from: coder) else { return nil }
    return restoreWindow(state)
  }

  /// Open a saved document at its position in a new, unshown window. Nil
  /// when its bookmark no longer resolves or a window already shows it.
  func restoreWindow(_ state: RestorationState) -> ReaderWindowController? {
    guard let capability = DocumentCapability(bookmark: state.bookmark),
      !windows.contains(where: { $0.loader.capability == capability })
    else { return nil }
    let controller = makeWindow(nil, show: false)
    controller.restore(state, document: capability)
    return controller
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

  /// What an open panel asks for.
  struct Choice: Equatable {
    enum Kind { case documents, document, folder }
    let kind: Kind
    /// Where the panel starts.
    let directory: URL?
    let message: String?
  }

  /// The key reader window, else the main one, else the only one.
  var keyReader: ReaderWindowController? {
    windows.first { $0.window?.isKeyWindow == true }
      ?? windows.first { $0.window?.isMainWindow == true }
      ?? (windows.count == 1 ? windows.first : nil)
  }

  @objc func openDocument(_ sender: Any?) {
    // Open… starts in the key window's folder and opens through its grant.
    let folderWindow = keyReader.flatMap { $0.folder == nil ? nil : $0 }
    let choice = Choice(kind: .documents, directory: folderWindow?.folder?.url, message: nil)
    Task {
      let urls = await choose(choice, folderWindow?.window)
      if let folderWindow, let folder = folderWindow.folder {
        open(urls, folder: folder, preferring: folderWindow)
      } else {
        open(urls)
      }
    }
  }

  @objc func openFolder(_ sender: Any?) {
    Task {
      let urls = await choose(Choice(kind: .folder, directory: nil, message: nil), nil)
      if let url = urls.first { openFolder(url) }
    }
  }

  /// Grant a folder to a window: the key empty window, another empty one,
  /// or a new one. A window already showing the folder is focused.
  func openFolder(_ url: URL, recent: RecentReference? = nil) {
    guard let folder = try? FolderCapability(granted: url) else { return }
    if let existing = windows.first(where: { $0.folder == folder }) {
      existing.showWindow(nil)
      return
    }
    let target = emptyWindow() ?? makeWindow(nil)
    target.recent = recent
    target.showFolder(folder)
    target.showWindow(nil)
  }

  /// An empty window with no folder, preferring the key window.
  private func emptyWindow() -> ReaderWindowController? {
    let empty = windows.filter {
      $0.loader.capability == nil && $0.folder == nil && $0.recovery == nil
        && !$0.isOpeningFolder
    }
    return empty.first { $0.window?.isKeyWindow == true } ?? empty.first
  }

  /// Focus a window already showing the file, else reuse an empty window, else
  /// open a new one. Each window owns its document and core session.
  func open(_ urls: [URL]) {
    for url in urls {
      guard let capability = try? DocumentCapability(granted: url) else { continue }
      open(capability)
    }
  }

  /// Open documents through `folder`. One outside it opens by itself; the
  /// first inside it uses `preferring` when that window is still empty.
  func open(_ urls: [URL], folder: FolderCapability, preferring window: ReaderWindowController?) {
    var preferred = window
    for url in urls {
      guard let capability = try? folder.document(url) else {
        open([url])
        continue
      }
      open(capability, folder: folder, preferring: preferred)
      preferred = nil
    }
  }

  // MARK: Placing documents in windows (MAC-002B)

  /// The opens not yet placed, run one at a time in the order requested.
  private var openChain: Task<Void, Never>?

  /// Run `body` after every earlier open has placed its document, since
  /// finding the window already showing a file is asynchronous.
  private func enqueueOpen(_ body: @escaping @MainActor () async -> Void) {
    let previous = openChain
    let task = Task { @MainActor in
      await previous?.value
      await body()
    }
    openChain = task
    Task { @MainActor [weak self] in
      await task.value
      if self?.openChain == task { self?.openChain = nil }
    }
  }

  /// Wait until every requested open has been placed in a window.
  func opensFinished() async {
    while let chain = openChain {
      await chain.value
      if openChain == chain { break }
    }
  }

  /// The window showing the same file as `capability`: the same resolved
  /// URL, or else the same device and inode, so a hard link or a case
  /// variant of the name finds it too. The identities are read off the
  /// main thread, since a disconnected server can stall a stat.
  func existingWindow(for capability: DocumentCapability) async -> ReaderWindowController? {
    if let match = windows.first(where: { $0.loader.capability == capability }) { return match }
    let candidates = windows.compactMap { window in window.loader.capability.map { (window, $0) } }
    guard !candidates.isEmpty else { return nil }
    let shown = candidates.map(\.1)
    let index = await Task.detached { () -> Int? in
      guard let target = capability.fileIdentity() else { return nil }
      return shown.firstIndex { $0.fileIdentity() == target }
    }.value
    guard let index else { return nil }
    let (window, held) = candidates[index]
    // The window may have closed or moved on during the lookup.
    guard windows.contains(where: { $0 === window }), window.loader.capability == held else {
      return nil
    }
    return window
  }

  private func open(
    _ capability: DocumentCapability, folder: FolderCapability? = nil,
    recent: RecentReference? = nil, preferring preferred: ReaderWindowController? = nil
  ) {
    enqueueOpen { [weak self] in
      guard let self else { return }
      if let existing = await self.existingWindow(for: capability) {
        self.reuse(existing, for: capability, folder: folder, recent: recent)
        return
      }
      if let target = preferred, let folder, target.folder == folder,
        target.loader.capability == nil, self.windows.contains(where: { $0 === target })
      {
        target.recent = nil
        target.loader.open(capability)
        target.showWindow(nil)
        return
      }
      let target = self.emptyWindow() ?? self.makeWindow(nil)
      target.recent = recent
      if let folder {
        target.showFolder(folder, then: capability.url)
      } else {
        target.loader.open(capability)
      }
      target.showWindow(nil)
    }
  }

  /// Bring forward the window already showing the file, recovering it when
  /// it failed or holds the file through a revoked (or other) grant.
  private func reuse(
    _ existing: ReaderWindowController, for capability: DocumentCapability,
    folder: FolderCapability?, recent: RecentReference?
  ) {
    let shown = existing.loader.capability
    let failed: Bool
    switch existing.loader.state {
    case .failed, .stale: failed = true
    case .empty, .loading, .loaded: failed = false
    }
    if shown?.isRevoked == true || (failed && shown?.folder !== capability.folder) {
      // Open the new capability there instead of retrying the old one.
      existing.recent = recent ?? existing.recent
      if let folder {
        existing.showFolder(folder, then: capability.url)
      } else {
        existing.loader.open(capability)
      }
    } else if failed {
      // Choosing a file again retries a failed load, e.g. after reconnecting.
      existing.loader.reload()
    }
    existing.showWindow(nil)
  }

  /// Open a document from a window's files sidebar or a link in it: bring
  /// forward the window already showing it, else show it in `controller`,
  /// which keeps its folder. `anchor` is a heading to show.
  func navigate(
    to capability: DocumentCapability, anchor: String?, from controller: ReaderWindowController
  ) {
    enqueueOpen { [weak self] in
      guard let self else { return }
      if let existing = await self.existingWindow(for: capability) {
        if existing !== controller {
          self.reuse(existing, for: capability, folder: nil, recent: nil)
        }
        if let anchor { existing.reveal(anchor: anchor) }
        existing.window?.makeKeyAndOrderFront(nil)
        return
      }
      guard self.windows.contains(where: { $0 === controller }) else { return }
      controller.recent = nil
      controller.navigate(to: capability, anchor: anchor)
    }
  }

  // MARK: Recents (MAC-002A)

  /// Record a document or folder a window opened; a recovered recent item
  /// keeps its identity, so its window can still remove it.
  private func remember(_ url: URL, kind: RecentItem.Kind, for controller: ReaderWindowController) {
    let reference = controller.recent?.kind == kind ? controller.recent : nil
    let id = reference?.id ?? UUID()
    Task {
      guard
        let item = await Task.detached(operation: { RecentItem.capture(url, kind: kind, id: id) })
          .value
      else { return }
      recents.note(item)
      if controller.recent == nil || controller.recent == reference {
        controller.recent = RecentReference(id: item.id, kind: kind)
      }
    }
  }

  /// Check every recent item off the main thread; only the newest check
  /// publishes. The menu shows the statuses last found.
  func refreshRecentStatus() {
    statusRequest &+= 1
    let id = statusRequest
    let items = recents.documents + recents.folders
    statusTask?.cancel()
    statusTask = Task {
      let found = await Task.detached {
        Dictionary(uniqueKeysWithValues: items.map { ($0.id, $0.resolve().status) })
      }.value
      guard id == statusRequest else { return }
      recentStatus = found
      statusTask = nil
    }
  }

  func menuNeedsUpdate(_ menu: NSMenu) {
    guard menu === openRecentMenu else { return }
    menu.removeAllItems()
    for item in recents.documents { menu.addItem(recentMenuItem(item)) }
    if !recents.documents.isEmpty, !recents.folders.isEmpty { menu.addItem(.separator()) }
    for item in recents.folders { menu.addItem(recentMenuItem(item)) }
    if !menu.items.isEmpty { menu.addItem(.separator()) }
    let clear = menu.addItem(
      withTitle: "Clear Menu", action: #selector(clearRecents(_:)), keyEquivalent: "")
    clear.target = self
  }

  private func recentMenuItem(_ item: RecentItem) -> NSMenuItem {
    let summary = recentStatus[item.id]?.summary
    let title = summary.map { "\(item.name) — \($0)" } ?? item.name
    let menuItem = NSMenuItem(title: title, action: #selector(openRecent(_:)), keyEquivalent: "")
    menuItem.target = self
    menuItem.representedObject = item.id
    let symbol =
      summary != nil ? "exclamationmark.triangle" : item.kind == .folder ? "folder" : "doc.text"
    menuItem.image = NSImage(systemSymbolName: symbol, accessibilityDescription: nil)
    return menuItem
  }

  @objc func clearRecents(_ sender: Any?) {
    recents.clear()
  }

  @objc func openRecent(_ sender: NSMenuItem) {
    guard let id = sender.representedObject as? UUID else { return }
    Task { await openRecent(id) }
  }

  /// Open a recent item where it is now. A moved item opens from its new
  /// location and its entry follows it; one that can't be opened shows its
  /// error and recovery in a window.
  func openRecent(_ id: UUID) async {
    guard let item = recents.item(id) else { return }
    let resolution = await Task.detached { item.resolve() }.value
    recentStatus[id] = resolution.status
    let reference = RecentReference(id: item.id, kind: item.kind)
    guard let url = resolution.url else {
      let target = emptyWindow() ?? makeWindow(nil)
      target.showUnavailable(item, status: resolution.status)
      target.showWindow(nil)
      return
    }
    switch item.kind {
    case .document:
      guard let capability = try? DocumentCapability(granted: url) else { return }
      open(capability, recent: reference)
      await opensFinished()
    case .folder:
      openFolder(url, recent: reference)
    }
  }

  // MARK: Recovery (MAC-002A)

  /// Run a window's recovery: retry, locate or re-grant the item, or remove
  /// its recent entry. A located item replaces its recent entry.
  func recover(_ action: RecoveryAction, _ controller: ReaderWindowController) {
    guard let recovery = controller.recovery else { return }
    switch (action, recovery.subject) {
    case (.removeRecent, _):
      if let id = controller.recent?.id { recents.remove(id) }
      controller.clearRecovery()
    case (.tryAgain, .document(let capability)):
      if controller.loader.capability == capability {
        controller.reloadDocument(nil)
      } else {
        controller.loader.open(capability)
      }
    case (.tryAgain, .folder(let folder)):
      guard let fresh = try? FolderCapability(granted: folder.url) else { return }
      controller.showFolder(fresh)
    case (.grantAccess, .document(let capability)) where capability.folder != nil:
      // A document opened through a folder gets its access back with the folder.
      let folderURL = capability.folder!.url
      Task {
        let choice = Choice(
          kind: .folder, directory: folderURL,
          message: "Choose the folder again to grant access to it.")
        guard let url = await choose(choice, controller.window).first,
          let folder = try? FolderCapability(granted: url)
        else { return }
        controller.showFolder(folder, then: folder.contains(capability.url) ? capability.url : nil)
      }
    case (.locate, .document(let capability)), (.grantAccess, .document(let capability)):
      let folder = controller.folder
      Task {
        let choice = Choice(
          kind: .document, directory: Self.nearestFolder(capability.url),
          message: action == .locate
            ? "Locate the document." : "Choose the document again to grant access to it.")
        guard let url = await choose(choice, controller.window).first else { return }
        if let folder, let inside = try? folder.document(url) {
          controller.loader.open(inside)
        } else if let chosen = try? DocumentCapability(granted: url) {
          controller.loader.open(chosen)
        }
      }
    case (.locate, .folder(let folder)), (.grantAccess, .folder(let folder)):
      Task {
        let choice = Choice(
          kind: .folder,
          directory: Self.nearestFolder(
            action == .locate ? folder.url.deletingLastPathComponent() : folder.url),
          message: action == .locate
            ? "Locate the folder." : "Choose the folder again to grant access to it.")
        guard let url = await choose(choice, controller.window).first,
          let chosen = try? FolderCapability(granted: url)
        else { return }
        controller.showFolder(chosen)
      }
    }
  }

  /// The closest existing folder at or above `url`, to start a panel in.
  nonisolated static func nearestFolder(_ url: URL) -> URL? {
    var candidate = url
    var isDirectory: ObjCBool = false
    while candidate.path != "/" {
      if FileManager.default.fileExists(atPath: candidate.path, isDirectory: &isDirectory),
        isDirectory.boolValue
      {
        return candidate
      }
      candidate.deleteLastPathComponent()
    }
    return nil
  }

  /// The system open panel, as a sheet on `window` when there is one.
  static func runPanel(_ choice: Choice, window: NSWindow?) async -> [URL] {
    let panel = NSOpenPanel()
    switch choice.kind {
    case .documents, .document:
      panel.allowedContentTypes = [
        UTType("net.daringfireball.markdown") ?? .plainText, .plainText,
      ]
      panel.allowsMultipleSelection = choice.kind == .documents
      panel.canChooseDirectories = false
      panel.canChooseFiles = true
    case .folder:
      panel.canChooseDirectories = true
      panel.canChooseFiles = false
      panel.allowsMultipleSelection = false
      panel.prompt = "Open"
    }
    panel.directoryURL = choice.directory
    if let message = choice.message { panel.message = message }
    let response: NSApplication.ModalResponse
    if let window {
      response = await panel.beginSheetModal(for: window)
    } else {
      response = await withCheckedContinuation { continuation in
        panel.begin { continuation.resume(returning: $0) }
      }
    }
    return response == .OK ? panel.urls : []
  }

  @objc func newWindow(_ sender: Any?) {
    makeWindow(sender)
  }

  /// A new reader window. A restored one is shown by AppKit, not here.
  @discardableResult
  func makeWindow(_ sender: Any?, show: Bool = true) -> ReaderWindowController {
    let controller = ReaderWindowController(preferences: preferences)
    controller.onClose = { [weak self] closed in
      self?.windows.removeAll { $0 === closed }
    }
    controller.onLoaded = { [weak self] controller, capability in
      self?.remember(capability.url, kind: .document, for: controller)
    }
    controller.onFolderReady = { [weak self] controller, folder in
      self?.remember(folder.url, kind: .folder, for: controller)
    }
    controller.onFolderClosed = { [weak self] closing, grant in
      // Documents other windows opened through the folder lose access now.
      for window in self?.windows ?? [] where window !== closing {
        window.folderRevoked(grant)
      }
    }
    controller.onRecover = { [weak self] action, controller in
      self?.recover(action, controller)
    }
    controller.onNavigate = { [weak self] controller, capability, anchor in
      self?.navigate(to: capability, anchor: anchor, from: controller)
    }
    if restoresWindows { controller.enableRestoration() }
    windows.append(controller)
    if show {
      controller.showWindow(sender)
      controller.window?.makeKeyAndOrderFront(sender)
    }
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
    let openFolder = file.addItem(
      withTitle: "Open Folder…", action: #selector(openFolder(_:)), keyEquivalent: "o")
    openFolder.keyEquivalentModifierMask = [.command, .shift]
    openFolder.target = self
    let recent = file.addItem(withTitle: "Open Recent", action: nil, keyEquivalent: "")
    recent.submenu = openRecentMenu
    file.addItem(.separator())
    file.addItem(
      withTitle: "Reload", action: #selector(ReaderWindowController.reloadDocument(_:)),
      keyEquivalent: "r")
    file.addItem(.separator())
    file.addItem(
      withTitle: "Close Folder", action: #selector(ReaderWindowController.closeFolder(_:)),
      keyEquivalent: "")
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
    let showFiles = view.addItem(
      withTitle: "Files", action: #selector(ReaderWindowController.showFiles(_:)),
      keyEquivalent: "1")
    showFiles.keyEquivalentModifierMask = [.command, .option]
    let showOutline = view.addItem(
      withTitle: "Outline", action: #selector(ReaderWindowController.showOutline(_:)),
      keyEquivalent: "2")
    showOutline.keyEquivalentModifierMask = [.command, .option]
    view.addItem(.separator())
    let hidden = view.addItem(
      withTitle: "Show Hidden Files",
      action: #selector(ReaderWindowController.toggleHiddenFiles(_:)), keyEquivalent: ".")
    hidden.keyEquivalentModifierMask = [.command, .shift]
    view.addItem(
      withTitle: "Show All Files", action: #selector(ReaderWindowController.toggleAllFiles(_:)),
      keyEquivalent: "")
    view.addItem(
      withTitle: "Refresh Files", action: #selector(ReaderWindowController.refreshFiles(_:)),
      keyEquivalent: "")
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
