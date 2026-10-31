import AppKit
import MarkdownDocument
import MarkdownRender

/// What a restorable reader window saves (MAC-001H): a bookmark to its
/// document, the reading position as core anchors and the sidebar state.
/// AppKit keeps it with the window's frame in the app's saved state; it is
/// private document-derived data and is never logged.
struct RestorationState: Equatable {
  /// Bumped when the saved keys change; other versions are ignored.
  static let version = 1
  let bookmark: Data
  let position: SavedPosition?
  let sidebar: Bool

  private enum Key {
    static let version = "readerVersion"
    static let bookmark = "documentBookmark"
    static let top = "positionTop"
    static let selectionStart = "positionSelectionStart"
    static let selectionEnd = "positionSelectionEnd"
    static let sidebar = "sidebarShown"
  }

  /// Anchors are at most MD_ANCHOR_MAX_BYTES; a bookmark is a few KiB.
  static let maxAnchorBytes = 1024
  static let maxBookmarkBytes = 64 * 1024

  func encode(to coder: NSCoder) {
    coder.encode(Self.version, forKey: Key.version)
    coder.encode(bookmark as NSData, forKey: Key.bookmark)
    if let position {
      coder.encode(position.top as NSData, forKey: Key.top)
      coder.encode(position.selectionStart as NSData, forKey: Key.selectionStart)
      if let end = position.selectionEnd { coder.encode(end as NSData, forKey: Key.selectionEnd) }
    }
    coder.encode(sidebar, forKey: Key.sidebar)
  }

  /// Saved state is untrusted input: sizes are bounded, and anything
  /// malformed drops the position or the whole window, never crashes.
  init?(from coder: NSCoder) {
    guard coder.decodeInteger(forKey: Key.version) == Self.version,
      let bookmark = Self.data(coder, Key.bookmark, limit: Self.maxBookmarkBytes)
    else { return nil }
    self.bookmark = bookmark
    if let top = Self.data(coder, Key.top, limit: Self.maxAnchorBytes),
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

  init(bookmark: Data, position: SavedPosition?, sidebar: Bool) {
    self.bookmark = bookmark
    self.position = position
    self.sidebar = sidebar
  }

  private static func data(_ coder: NSCoder, _ key: String, limit: Int) -> Data? {
    guard let data = coder.decodeObject(of: NSData.self, forKey: key) as Data?,
      !data.isEmpty, data.count <= limit
    else { return nil }
    return data
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
