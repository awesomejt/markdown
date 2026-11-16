import AppKit
import MarkdownDocument
import MarkdownRender

/// What a restorable reader window saves: a bookmark to its document, the
/// reading position as core anchors and the sidebar state (MAC-001H), and
/// since version 2 its folder (MAC-002E). AppKit keeps it with the window's
/// frame in the app's saved state; it is private document-derived data and
/// is never logged.
struct RestorationState: Equatable {
  /// Bumped when the saved keys change. Version 1 (a document only) is
  /// still read; other versions are ignored.
  static let version = 2
  static let readable: ClosedRange<Int> = 1...2
  /// The document, or nil for a folder window showing none.
  let bookmark: Data?
  let position: SavedPosition?
  let sidebar: Bool
  let workspace: WorkspaceState?

  private enum Key {
    static let version = "readerVersion"
    static let bookmark = "documentBookmark"
    static let top = "positionTop"
    static let selectionStart = "positionSelectionStart"
    static let selectionEnd = "positionSelectionEnd"
    static let sidebar = "sidebarShown"
    static let folder = "folderBookmark"
    static let mode = "sidebarMode"
    static let expanded = "expandedFolders"
    static let hidden = "showHiddenFiles"
    static let allFiles = "showAllFiles"
  }

  /// Anchors are at most MD_ANCHOR_MAX_BYTES; a bookmark is a few KiB.
  static let maxAnchorBytes = 1024
  static let maxBookmarkBytes = 64 * 1024

  func encode(to coder: NSCoder) {
    coder.encode(Self.version, forKey: Key.version)
    if let bookmark { coder.encode(bookmark as NSData, forKey: Key.bookmark) }
    if let position {
      coder.encode(position.top as NSData, forKey: Key.top)
      coder.encode(position.selectionStart as NSData, forKey: Key.selectionStart)
      if let end = position.selectionEnd { coder.encode(end as NSData, forKey: Key.selectionEnd) }
    }
    coder.encode(sidebar, forKey: Key.sidebar)
    if let workspace {
      coder.encode(workspace.bookmark as NSData, forKey: Key.folder)
      coder.encode(workspace.mode.rawValue, forKey: Key.mode)
      coder.encode(workspace.expanded.map { $0 as NSString } as NSArray, forKey: Key.expanded)
      coder.encode(workspace.includeHidden, forKey: Key.hidden)
      coder.encode(workspace.allFiles, forKey: Key.allFiles)
    }
  }

  /// Saved state is untrusted input: sizes are bounded, and anything
  /// malformed drops the position, the folder or the whole window, never
  /// crashes.
  init?(from coder: NSCoder) {
    let version = coder.decodeInteger(forKey: Key.version)
    guard Self.readable.contains(version) else { return nil }
    let bookmark = Self.data(coder, Key.bookmark, limit: Self.maxBookmarkBytes)
    let workspace = version >= 2 ? WorkspaceState(from: coder) : nil
    guard bookmark != nil || workspace != nil else { return nil }
    self.bookmark = bookmark
    self.workspace = workspace
    if bookmark != nil, let top = Self.data(coder, Key.top, limit: Self.maxAnchorBytes),
      let start = Self.data(coder, Key.selectionStart, limit: Self.maxAnchorBytes)
    {
      position = SavedPosition(
        top: top, selectionStart: start,
        selectionEnd: Self.data(coder, Key.selectionEnd, limit: Self.maxAnchorBytes))
    } else {
      position = nil
    }
    sidebar =
      coder.containsValue(forKey: Key.sidebar) ? coder.decodeBool(forKey: Key.sidebar) : true
  }

  init(bookmark: Data?, position: SavedPosition?, sidebar: Bool, workspace: WorkspaceState? = nil) {
    self.bookmark = bookmark
    self.position = bookmark == nil ? nil : position
    self.sidebar = sidebar
    self.workspace = workspace
  }

  fileprivate static func data(_ coder: NSCoder, _ key: String, limit: Int) -> Data? {
    guard let data = coder.decodeObject(of: NSData.self, forKey: key) as Data?,
      !data.isEmpty, data.count <= limit
    else { return nil }
    return data
  }

  /// A window's folder (MAC-002E): a bookmark to it, the sidebar pane, the
  /// expanded folders as display paths, and what the listing shows. The
  /// paths are only looked up in a fresh listing, never used as access.
  struct WorkspaceState: Equatable {
    let bookmark: Data
    let mode: SidebarController.Mode
    let expanded: [String]
    let includeHidden: Bool
    let allFiles: Bool

    /// Expanded folders kept, and the longest display path kept.
    static let maxExpanded = 512
    static let maxPathBytes = 4096

    init(
      bookmark: Data, mode: SidebarController.Mode, expanded: [String], includeHidden: Bool,
      allFiles: Bool
    ) {
      self.bookmark = bookmark
      self.mode = mode
      self.expanded = Self.bounded(expanded)
      self.includeHidden = includeHidden
      self.allFiles = allFiles
    }

    fileprivate init?(from coder: NSCoder) {
      guard
        let bookmark = RestorationState.data(
          coder, Key.folder, limit: RestorationState.maxBookmarkBytes)
      else { return nil }
      self.bookmark = bookmark
      mode = SidebarController.Mode(rawValue: coder.decodeInteger(forKey: Key.mode)) ?? .files
      let paths =
        coder.decodeArrayOfObjects(ofClass: NSString.self, forKey: Key.expanded) as [NSString]?
      expanded = Self.bounded((paths ?? []).map { $0 as String })
      includeHidden = coder.decodeBool(forKey: Key.hidden)
      allFiles = coder.decodeBool(forKey: Key.allFiles)
    }

    private static func bounded(_ paths: [String]) -> [String] {
      Array(
        paths.lazy.filter { !$0.isEmpty && $0.utf8.count <= maxPathBytes }.prefix(maxExpanded))
    }
  }
}

/// Recreates reader windows when AppKit restores the app's windows at
/// launch. A document whose bookmark no longer resolves isn't restored.
final class ReaderWindowRestoration: NSObject, NSWindowRestoration {
  static let identifier = NSUserInterfaceItemIdentifier("reader")

  static func restoreWindow(
    withIdentifier identifier: NSUserInterfaceItemIdentifier, state: NSCoder,
    completionHandler: @escaping (NSWindow?, (any Error)?) -> Void
  ) {
    MainActor.assumeIsolated {
      guard identifier == Self.identifier, let delegate = NSApp.delegate as? AppDelegate,
        let controller = delegate.restoreWindow(from: state)
      else {
        completionHandler(nil, CocoaError(.userCancelled))
        return
      }
      completionHandler(controller.window, nil)
    }
  }
}
