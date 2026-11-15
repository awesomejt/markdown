import Darwin
import Foundation

/// One recent document or folder (MAC-002A). The bookmark is the persistent
/// grant: it finds the item again after a move or rename on the same
/// volume. The last known path and file identity are private app state used
/// to de-duplicate and to notice a move; they are never logged or shown.
public struct RecentItem: Codable, Sendable, Equatable, Identifiable {
  public enum Kind: String, Codable, Sendable { case document, folder }

  public let id: UUID
  public let kind: Kind
  public var name: String
  public var path: String
  public var bookmark: Data
  /// Device and inode when recorded: a hard link or a case variant of the
  /// same file is the same recent item.
  public var device: UInt64?
  public var inode: UInt64?

  public var url: URL { URL(fileURLWithPath: path) }

  /// Record `url` as a recent item. Makes a bookmark and stats the item, so
  /// call off the main thread. Nil when the item can't be bookmarked.
  public static func capture(_ url: URL, kind: Kind, id: UUID = UUID()) -> RecentItem? {
    let resolved = url.standardizedFileURL.resolvingSymlinksInPath()
    guard let bookmark = Bookmarks.make(resolved) else { return nil }
    var info = stat()
    let found = resolved.withUnsafeFileSystemRepresentation { path in
      path.map { stat($0, &info) == 0 } ?? false
    }
    return RecentItem(
      id: id, kind: kind, name: FileManager.default.displayName(atPath: resolved.path),
      path: resolved.path, bookmark: bookmark, device: found ? UInt64(info.st_dev) : nil,
      inode: found ? UInt64(info.st_ino) : nil)
  }

  func isSameItem(as other: RecentItem) -> Bool {
    guard kind == other.kind else { return false }
    if path == other.path { return true }
    if let device, let inode, device == other.device, inode == other.inode { return true }
    return false
  }

  /// Where the item is now and whether it can be read. Resolves the
  /// bookmark without UI or mounting, so a disconnected server never
  /// stalls; call off the main thread.
  public func resolve() -> RecentResolution {
    if let found = Bookmarks.resolve(bookmark) {
      let current = found.standardizedFileURL.resolvingSymlinksInPath()
      // A bookmark follows an item into the Trash (the home volume's
      // `.Trash`, or another volume's `.Trashes`); that is a deletion.
      if current.pathComponents.contains(where: { $0 == ".Trash" || $0 == ".Trashes" }) {
        return RecentResolution(status: .missing, url: nil)
      }
      let status = Self.check(current, kind: kind)
      if status != .available { return RecentResolution(status: status, url: nil) }
      return RecentResolution(
        status: current.path == path ? .available : .moved, url: current)
    }
    // The bookmark is gone; the last known path may still be there (a
    // bookmark made on another system), or its disk may be disconnected.
    let status = Self.check(url, kind: kind)
    if status == .available { return RecentResolution(status: .available, url: url) }
    if status == .missing, Self.volumeIsMissing(path) {
      return RecentResolution(status: .volumeUnavailable, url: nil)
    }
    return RecentResolution(status: status, url: nil)
  }

  private static func check(_ url: URL, kind: Kind) -> RecentStatus {
    var info = stat()
    let failure = url.withUnsafeFileSystemRepresentation { path in
      path.map { stat($0, &info) == 0 ? 0 : errno } ?? ENOENT
    }
    switch failure {
    case 0: break
    case EACCES, EPERM: return .permissionDenied
    case ENXIO, ENODEV, ESTALE, ETIMEDOUT: return .volumeUnavailable
    default: return .missing
    }
    let readable: Int32
    switch kind {
    case .document:
      guard info.st_mode & S_IFMT == S_IFREG else { return .missing }
      readable = url.withUnsafeFileSystemRepresentation { path in
        path.map { access($0, R_OK) == 0 ? 0 : errno } ?? ENOENT
      }
    case .folder:
      guard info.st_mode & S_IFMT == S_IFDIR else { return .missing }
      readable = url.withUnsafeFileSystemRepresentation { path -> Int32 in
        guard let path, let dir = opendir(path) else { return errno }
        closedir(dir)
        return 0
      }
    }
    switch readable {
    case 0: return .available
    case EACCES, EPERM: return .permissionDenied
    default: return .missing
    }
  }

  /// A path under /Volumes whose volume isn't mounted.
  private static func volumeIsMissing(_ path: String) -> Bool {
    let components = (path as NSString).pathComponents
    guard components.count > 2, components[1] == "Volumes" else { return false }
    return !FileManager.default.fileExists(atPath: "/Volumes/" + components[2])
  }
}

/// What resolving a recent item found.
public enum RecentStatus: Sendable, Equatable {
  case available
  /// Found through its bookmark at a new location.
  case moved
  case missing
  case volumeUnavailable
  case permissionDenied

  /// A short, path-free description for the Open Recent menu.
  public var summary: String? {
    switch self {
    case .available, .moved: nil
    case .missing: "Can’t be found"
    case .volumeUnavailable: "Disk not available"
    case .permissionDenied: "Permission denied"
    }
  }
}

public struct RecentResolution: Sendable, Equatable {
  public let status: RecentStatus
  /// The item's current location when it can be opened.
  public let url: URL?
}

/// Where recents are kept.
public protocol RecentsStorage: Sendable {
  func load() -> Data?
  func save(_ data: Data) throws
}

/// A JSON file under Application Support, written atomically.
public struct FileRecentsStorage: RecentsStorage {
  public let url: URL

  public init(url: URL) { self.url = url }

  /// `MARKDOWN_RECENTS_PATH`, else Application Support/<bundle id>/recents.json.
  public static func standard(environment: [String: String] = ProcessInfo.processInfo.environment)
    -> FileRecentsStorage?
  {
    if let path = environment["MARKDOWN_RECENTS_PATH"], !path.isEmpty {
      return FileRecentsStorage(url: URL(fileURLWithPath: path))
    }
    guard
      let support = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)
        .first
    else { return nil }
    let folder = support.appendingPathComponent(
      Bundle.main.bundleIdentifier ?? "io.github.awesomejt.markdown", isDirectory: true)
    return FileRecentsStorage(url: folder.appendingPathComponent("recents.json"))
  }

  public func load() -> Data? {
    // Bounded before reading: a planted huge file is ignored, not loaded.
    guard let size = (try? url.resourceValues(forKeys: [.fileSizeKey]))?.fileSize,
      size <= Recents.maxStoredBytes
    else { return nil }
    return try? Data(contentsOf: url)
  }

  /// Written to a 0600 temporary file in the same folder, synced, then
  /// renamed over the list, so it is never readable by others, even briefly.
  public func save(_ data: Data) throws {
    let folder = url.deletingLastPathComponent()
    try FileManager.default.createDirectory(
      at: folder, withIntermediateDirectories: true, attributes: [.posixPermissions: 0o700])
    // Ours alone, also when the folder already existed with wider access.
    guard chmod(folder.path, 0o700) == 0 else { throw POSIXError(.init(rawValue: errno) ?? .EIO) }
    let temporary = folder.appendingPathComponent(".recents-\(UUID().uuidString).tmp")
    let fd = open(temporary.path, O_WRONLY | O_CREAT | O_EXCL | O_CLOEXEC | O_NOFOLLOW, 0o600)
    guard fd >= 0 else { throw POSIXError(.init(rawValue: errno) ?? .EIO) }
    var written = 0
    let failure: Int32 = data.withUnsafeBytes { buffer in
      while written < buffer.count {
        let count = Darwin.write(fd, buffer.baseAddress! + written, buffer.count - written)
        if count < 0 {
          if errno == EINTR { continue }
          return errno
        }
        written += count
      }
      return fsync(fd) == 0 ? 0 : errno
    }
    Darwin.close(fd)
    guard failure == 0, rename(temporary.path, url.path) == 0 else {
      let code = failure != 0 ? failure : errno
      unlink(temporary.path)
      throw POSIXError(.init(rawValue: code) ?? .EIO)
    }
  }
}

/// Keeps recents in memory only: the development checks never touch the
/// user's list.
public final class MemoryRecentsStorage: RecentsStorage, @unchecked Sendable {
  private let lock = NSLock()
  private var data: Data?

  public init(_ data: Data? = nil) { self.data = data }
  public func load() -> Data? { lock.withLock { data } }
  public func save(_ data: Data) throws { lock.withLock { self.data = data } }
}

/// The recent documents and folders, newest first, at most `limit` of each
/// (MAC-002A). The stored list is untrusted input: its size, count and
/// fields are bounded, and anything malformed is dropped. A list written by
/// a newer version is used but never overwritten.
@MainActor
public final class Recents {
  public static let limit = 10
  nonisolated static let maxStoredBytes = 1024 * 1024
  nonisolated static let maxBookmarkBytes = 64 * 1024
  nonisolated static let maxFieldBytes = 4096
  static let version = 1

  public private(set) var documents: [RecentItem] = []
  public private(set) var folders: [RecentItem] = []
  public var onChange: (() -> Void)?
  private let storage: RecentsStorage
  /// The stored list is from a newer version; keep it as it is.
  public private(set) var readOnly = false

  private struct Stored: Encodable {
    var version: Int
    var documents: [RecentItem]
    var folders: [RecentItem]
  }

  /// Read first and alone, so a newer list is protected even when nothing
  /// else in it can be decoded.
  private struct Header: Decodable { var version: Int }

  /// The entry lists, each entry decoded on its own: one malformed entry (an
  /// unknown kind, a missing or mistyped field) is skipped, not the list.
  private struct Lists: Decodable {
    var documents: [Entry]?
    var folders: [Entry]?
  }

  private struct Entry: Decodable {
    let item: RecentItem?
    init(from decoder: any Decoder) throws { item = try? RecentItem(from: decoder) }
  }

  public init(storage: RecentsStorage) {
    self.storage = storage
    guard let data = storage.load(), data.count <= Self.maxStoredBytes,
      let header = try? JSONDecoder().decode(Header.self, from: data)
    else { return }
    if header.version > Self.version { readOnly = true }
    guard header.version >= 1, let lists = try? JSONDecoder().decode(Lists.self, from: data)
    else { return }
    documents = Self.sanitize(lists.documents?.compactMap(\.item) ?? [], kind: .document)
    folders = Self.sanitize(lists.folders?.compactMap(\.item) ?? [], kind: .folder)
  }

  private static func sanitize(_ items: [RecentItem], kind: RecentItem.Kind) -> [RecentItem] {
    var kept: [RecentItem] = []
    for item in items where kept.count < limit {
      guard item.kind == kind, !item.bookmark.isEmpty, item.bookmark.count <= maxBookmarkBytes,
        !item.name.isEmpty, item.name.utf8.count <= maxFieldBytes,
        item.path.hasPrefix("/"), item.path.utf8.count <= maxFieldBytes,
        !item.path.contains("\0"),
        !kept.contains(where: { $0.isSameItem(as: item) || $0.id == item.id })
      else { continue }
      kept.append(item)
    }
    return kept
  }

  public func item(_ id: UUID) -> RecentItem? {
    documents.first { $0.id == id } ?? folders.first { $0.id == id }
  }

  /// Put `item` first in its list, replacing any entry for the same item
  /// or with the same id (a recent item found at a new location).
  public func note(_ item: RecentItem) {
    update(item.kind) { list in
      list.removeAll { $0.isSameItem(as: item) || $0.id == item.id }
      list.insert(item, at: 0)
      if list.count > Self.limit { list.removeLast(list.count - Self.limit) }
    }
  }

  public func remove(_ id: UUID) {
    guard let old = item(id) else { return }
    update(old.kind) { $0.removeAll { $0.id == id } }
  }

  public func clear() {
    documents = []
    folders = []
    persist()
    onChange?()
  }

  private func update(_ kind: RecentItem.Kind, _ change: (inout [RecentItem]) -> Void) {
    switch kind {
    case .document: change(&documents)
    case .folder: change(&folders)
    }
    persist()
    onChange?()
  }

  private func persist() {
    guard !readOnly else { return }
    let stored = Stored(version: Self.version, documents: documents, folders: folders)
    guard let data = try? JSONEncoder().encode(stored) else { return }
    try? storage.save(data)
  }
}

/// Bookmarks for recents. This build isn't sandboxed, so a plain bookmark
/// is the persistent grant; a sandboxed process makes and resolves
/// security-scoped bookmarks instead.
enum Bookmarks {
  static var sandboxed: Bool {
    ProcessInfo.processInfo.environment["APP_SANDBOX_CONTAINER_ID"] != nil
  }

  static func make(_ url: URL) -> Data? {
    let options: URL.BookmarkCreationOptions = sandboxed ? [.withSecurityScope] : []
    return try? url.bookmarkData(
      options: options, includingResourceValuesForKeys: nil, relativeTo: nil)
  }

  static func resolve(_ data: Data) -> URL? {
    var options: URL.BookmarkResolutionOptions = [.withoutUI, .withoutMounting]
    if sandboxed { options.insert(.withSecurityScope) }
    var stale = false
    return try? URL(
      resolvingBookmarkData: data, options: options, relativeTo: nil, bookmarkDataIsStale: &stale)
  }
}
