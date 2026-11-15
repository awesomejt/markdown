import AppKit
import Darwin
import MarkdownCore
import MarkdownDocument

/// Folder grants, recents and recovery inside the running app (MAC-002A):
/// Open Folder, Open… through the folder, Close Folder revocation across
/// windows, re-granting, Open Recent for moved, missing and denied items,
/// and Clear Menu. The open panel is replaced by scripted choices, and
/// recents stay in memory. Synthetic files in a private directory only.
@MainActor
enum FolderSmoke {
  private final class Chooser {
    var queue: [[URL]] = []
    var asked: [AppDelegate.Choice] = []
  }

  static func run(delegate: AppDelegate, directory: URL, check: (Bool, String) throws -> Void)
    async throws
  {
    let chooser = Chooser()
    let previous = delegate.choose
    delegate.choose = { choice, _ in
      chooser.asked.append(choice)
      return chooser.queue.isEmpty ? [] : chooser.queue.removeFirst()
    }
    defer { delegate.choose = previous }
    delegate.recents.clear()
    try check(delegate.windows.isEmpty, "folder checks start without windows")

    let root = directory.appendingPathComponent("Workspace", isDirectory: true)
    try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
    func write(_ url: URL, _ title: String) throws -> URL {
      try Data("---\ntitle: \(title)\n---\n# \(title)\n\nText.\n".utf8).write(to: url)
      return url
    }
    let alpha = try write(root.appendingPathComponent("alpha.md"), "Alpha")
    let beta = try write(root.appendingPathComponent("beta.md"), "Beta")
    let outside = try write(directory.appendingPathComponent("outside.md"), "Outside")

    // File > Open Folder… grants the folder to a new window.
    let openFolder = try item("File", "Open Folder…", key: "o", modifiers: [.command, .shift])
    let openDocument = try item("File", "Open…", key: "o")
    let closeFolder = try item("File", "Close Folder", key: "")
    try check(
      NSApp.mainMenu?.item(withTitle: "File")?.submenu?.item(withTitle: "Open Recent")?.submenu
        === delegate.openRecentMenu, "File menu has Open Recent")
    chooser.queue = [[root]]
    try check(
      NSApp.sendAction(openFolder.action!, to: openFolder.target, from: openFolder),
      "Open Folder command routes")
    try await waitUntil("folder window opens") {
      delegate.windows.count == 1 && delegate.windows[0].folder != nil
        && !delegate.windows[0].isOpeningFolder
    }
    let first = delegate.windows[0]
    let folder = try requireValue(first.folder, "granted folder")
    try check(chooser.asked.last?.kind == .folder, "Open Folder asks for a folder")
    try check(first.statusText == "“Workspace”", "folder window names its folder")
    try check(!first.statusText.contains(directory.path), "folder status names no path")
    try check(first.validateMenuItem(closeFolder), "Close Folder is enabled with a folder")
    try await waitUntil("folder is recorded as recent") { delegate.recents.folders.count == 1 }

    // Open… starts in the folder: a document inside opens through its grant
    // in the folder window; one outside opens by itself in a new window.
    chooser.queue = [[alpha, outside]]
    try check(
      NSApp.sendAction(openDocument.action!, to: openDocument.target, from: openDocument),
      "Open command routes")
    try await waitUntil("documents open") {
      delegate.windows.count == 2 && first.documentTitle != nil && !first.isBusy
        && !delegate.windows[1].isBusy && delegate.windows[1].documentTitle != nil
    }
    try check(chooser.asked.last?.directory == folder.url, "Open… starts in the window’s folder")
    try check(first.documentTitle == "Alpha", "the folder window shows its document")
    try check(first.loader.capability?.folder === folder.grant, "document carries the folder grant")
    try check(first.window?.subtitle == "Workspace", "window subtitle names the folder")
    let lone = delegate.windows[1]
    try check(
      lone.folder == nil && lone.loader.capability?.folder == nil,
      "a document outside the folder opens without its grant")

    // A second document through the folder gets its own window and the same grant.
    delegate.open([beta], folder: folder, preferring: first)
    try await waitUntil("second folder document opens") {
      delegate.windows.count == 3 && delegate.windows[2].documentTitle == "Beta"
        && !delegate.windows[2].isBusy
    }
    let second = delegate.windows[2]
    try check(
      second.folder == folder && second.loader.capability?.folder === folder.grant,
      "the new window shares the folder grant")
    try await waitUntil("documents are recorded as recent") {
      delegate.recents.documents.count == 3
    }
    note("open folder/open through folder checked")

    // Close Folder revokes the grant: its own document closes, and another
    // window reading through the folder loses access and offers to re-grant.
    try check(
      NSApp.sendAction(closeFolder.action!, to: first, from: closeFolder),
      "Close Folder command routes")
    try check(folder.isRevoked && first.folder == nil, "Close Folder revokes the grant")
    try await waitUntil("revocation reaches the other window") {
      !second.isBusy && second.statusText == DocumentError.accessRevoked.message
    }
    try check(first.statusText == "No document open", "the closing window is empty")
    try check(second.recoveryTitles.first == "Grant Access…", "revoked document offers access")
    try check(second.folder == nil, "every window sharing the grant drops it")
    try check(
      second.loader.coreAvailability == .unavailable(.accessRevoked),
      "core session records the revocation")
    chooser.queue = [[root]]
    second.perform(.grantAccess)
    try await waitUntil("re-granted folder reopens the document") {
      second.documentTitle == "Beta" && !second.isBusy && !second.isOpeningFolder
    }
    try check(chooser.asked.last?.directory == folder.url, "re-grant starts at the folder")
    try check(
      second.folder != nil && second.loader.capability?.folder === second.folder?.grant
        && second.folder?.isRevoked == false, "the document reads through the new grant")
    // A window left holding a document through a revoked grant recovers
    // when the same file is opened again by itself (Grok review 1, PR #82).
    let regranted = try requireValue(second.folder, "re-granted folder")
    delegate.open([alpha], folder: regranted, preferring: nil)
    try await waitUntil("alpha opens through the new grant") {
      first.documentTitle == "Alpha" && !first.isBusy && !first.isOpeningFolder
    }
    try check(
      first.loader.capability?.folder === regranted.grant, "the empty window takes the new grant")
    try check(
      NSApp.sendAction(closeFolder.action!, to: second, from: closeFolder),
      "Close Folder routes again")
    try await waitUntil("revocation reaches the alpha window") {
      !first.isBusy && first.statusText == DocumentError.accessRevoked.message
    }
    delegate.open([alpha])
    try await waitUntil("reopening the file recovers the revoked window") {
      first.documentTitle == "Alpha" && !first.isBusy
    }
    try check(
      delegate.windows.count == 3 && first.loader.capability?.folder == nil,
      "the same window reads the file by itself, without the revoked grant")
    first.reloadDocument(nil)
    try await waitUntil("reload after recovery uses the new capability") {
      first.documentTitle == "Alpha" && !first.isBusy
    }
    try await waitUntil("alpha is the newest recent document") {
      delegate.recents.documents.first?.name == "alpha.md"
    }
    note("close folder revocation/re-grant/reopen checked")

    // The Open Recent menu lists documents, then folders, then Clear Menu.
    delegate.menuNeedsUpdate(delegate.openRecentMenu)
    let titles = delegate.openRecentMenu.items.filter { !$0.isSeparatorItem }.map(\.title)
    // beta.md and outside.md loaded concurrently earlier, so their order may vary.
    try check(
      titles.first == "alpha.md" && Set(titles[1..<3]) == ["beta.md", "outside.md"]
        && titles.dropFirst(3) == ["Workspace", "Clear Menu"],
      "Open Recent lists recent items: \(titles)")

    // Moved: the folder's bookmark finds it, it opens there, its entry follows.
    for window in delegate.windows.map(\.window) { window?.performClose(nil) }
    try check(delegate.windows.isEmpty, "windows closed before recents checks")
    let folderID = try requireValue(delegate.recents.folders.first?.id, "recent folder")
    let moved = directory.appendingPathComponent("Moved Workspace", isDirectory: true)
    try FileManager.default.moveItem(at: root, to: moved)
    await delegate.openRecent(folderID)
    try await waitUntil("moved folder opens") {
      delegate.windows.count == 1 && delegate.windows[0].folder != nil
        && !delegate.windows[0].isOpeningFolder
    }
    let movedWindow = delegate.windows[0]
    try check(movedWindow.statusText == "“Moved Workspace”", "moved folder opens where it is now")
    try await waitUntil("moved folder entry follows it") {
      delegate.recents.item(folderID)?.name == "Moved Workspace"
    }
    try check(delegate.recents.folders.count == 1, "moving keeps one entry for the folder")

    // Missing: the window explains and offers Locate…; the located file
    // replaces the entry.
    let alphaID = try requireValue(
      delegate.recents.documents.first { $0.name == "alpha.md" }?.id, "recent alpha")
    try FileManager.default.removeItem(at: moved.appendingPathComponent("alpha.md"))
    await delegate.openRecent(alphaID)
    let missing = try requireValue(delegate.windows.last, "missing window")
    try check(
      missing !== movedWindow && missing.statusText == DocumentError.notFound.message,
      "a missing recent shows its error in its own window")
    try check(
      missing.recoveryTitles == ["Locate…", "Remove from Recents"], "missing recent offers Locate")
    let found = try write(directory.appendingPathComponent("found.md"), "Found")
    chooser.queue = [[found]]
    missing.perform(.locate)
    try await waitUntil("located document opens") {
      missing.documentTitle == "Found" && !missing.isBusy
    }
    try await waitUntil("located document replaces its entry") {
      delegate.recents.item(alphaID)?.name == "found.md"
    }
    try check(missing.recoveryTitles.isEmpty, "recovery buttons go once the document opens")

    // Remove from Recents drops a missing entry.
    let outsideID = try requireValue(
      delegate.recents.documents.first { $0.name == "outside.md" }?.id, "recent outside")
    try FileManager.default.removeItem(at: outside)
    await delegate.openRecent(outsideID)
    let removed = try requireValue(delegate.windows.last, "removal window")
    removed.perform(.removeRecent)
    try check(delegate.recents.item(outsideID) == nil, "Remove from Recents drops the entry")
    try check(removed.statusText == "No document open", "removal empties the window")
    note("open recent moved/missing/locate/remove checked")

    // Denied: the menu marks the folder, and Grant Access… recovers it.
    if getuid() != 0 {
      try check(chmod(moved.path, 0) == 0, "withdraw folder permission")
      delegate.refreshRecentStatus()
      try await waitUntil("recent status refreshes") {
        delegate.statusTask == nil && delegate.recentStatus[folderID] == .permissionDenied
      }
      delegate.menuNeedsUpdate(delegate.openRecentMenu)
      try check(
        delegate.openRecentMenu.items.contains {
          $0.title == "Moved Workspace — Permission denied"
        },
        "Open Recent marks a denied folder")
      for window in delegate.windows.map(\.window) { window?.performClose(nil) }
      await delegate.openRecent(folderID)
      let denied = try requireValue(delegate.windows.last, "denied window")
      try check(
        denied.statusText == FolderError.permissionDenied.message
          && denied.recoveryTitles == ["Grant Access…", "Remove from Recents"],
        "denied folder offers Grant Access")
      try check(chmod(moved.path, 0o755) == 0, "restore folder permission")
      chooser.queue = [[moved]]
      denied.perform(.grantAccess)
      try await waitUntil("re-granted folder opens") {
        denied.folder != nil && !denied.isOpeningFolder
      }
      try check(denied.statusText == "“Moved Workspace”", "Grant Access recovers the folder")
      note("denied folder recovery checked")
    }

    // Clear Menu empties both lists.
    delegate.menuNeedsUpdate(delegate.openRecentMenu)
    let clear = try requireValue(
      delegate.openRecentMenu.items.first { $0.title == "Clear Menu" }, "Clear Menu")
    try check(NSApp.sendAction(clear.action!, to: clear.target, from: clear), "Clear Menu routes")
    try check(
      delegate.recents.documents.isEmpty && delegate.recents.folders.isEmpty,
      "Clear Menu empties recents")

    for window in delegate.windows.map(\.window) { window?.performClose(nil) }
    try await waitUntil("folder windows release native sessions") {
      guard let live = try? CoreRuntime.liveHandles() else { return false }
      return live.sessions == 0 && live.jobs == 0 && live.snapshots == 0
    }
    note("folder access/recents checked")
  }

  private static func waitUntil(_ message: String, condition: () -> Bool) async throws {
    let deadline = ContinuousClock.now.advanced(by: .seconds(10))
    while !condition() && ContinuousClock.now < deadline {
      try await Task.sleep(for: .milliseconds(20))
    }
    if !condition() { throw SmokeFailure(description: message) }
  }

  private static func item(
    _ menu: String, _ title: String, key: String,
    modifiers: NSEvent.ModifierFlags = .command
  ) throws -> NSMenuItem {
    guard let item = NSApp.mainMenu?.item(withTitle: menu)?.submenu?.item(withTitle: title),
      item.keyEquivalent == key, item.keyEquivalentModifierMask == modifiers, item.action != nil
    else { throw SmokeFailure(description: "missing menu item \(menu)/\(title)") }
    return item
  }

  private static func note(_ message: String) {
    FileHandle.standardOutput.write(Data("shell-smoke: \(message)\n".utf8))
  }
}
