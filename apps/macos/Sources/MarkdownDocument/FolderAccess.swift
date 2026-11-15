import Darwin
import Foundation

/// Why a granted folder, or a document through it, can't be used (MAC-002A).
/// Messages name the folder by display name only, never by path.
public enum FolderError: Error, Sendable, Equatable {
  /// Missing, or moved where its bookmark can't find it.
  case notFound
  case volumeUnavailable
  case permissionDenied
  /// The grant was withdrawn (Close Folder), or permission was lost after
  /// the folder was first read.
  case accessRevoked
  case notDirectory
  /// The document isn't inside the granted folder.
  case outsideFolder
  case unreadable

  public var message: String {
    switch self {
    case .notFound: "The folder can’t be found."
    case .volumeUnavailable: "The disk containing the folder is no longer available."
    case .permissionDenied: "You don’t have permission to read the folder."
    case .accessRevoked: "Access to the folder was lost."
    case .notDirectory: "The item isn’t a folder."
    case .outsideFolder: "The document isn’t inside the open folder."
    case .unreadable: "The folder couldn’t be read."
    }
  }

  public var recovery: String {
    switch self {
    case .notFound: "It may have been moved, renamed or deleted. Locate it to open it again."
    case .volumeUnavailable: "Reconnect the disk or server, then try again."
    case .permissionDenied: "Check the folder’s permissions in Finder, then grant access again."
    case .accessRevoked: "Grant access to the folder again to keep reading it."
    case .notDirectory: "Choose a folder instead."
    case .outsideFolder: "Open the document by itself, or open the folder that contains it."
    case .unreadable: "Try again."
    }
  }

  /// Whether the window offers a way back: locating, re-granting or retrying.
  public var isRecoverable: Bool {
    switch self {
    case .notFound, .volumeUnavailable, .permissionDenied, .accessRevoked: true
    case .notDirectory, .outsideFolder, .unreadable: false
    }
  }
}

/// The shared, revocable part of one folder grant. Every document opened
/// through the folder holds the same grant, so revoking it withdraws access
/// from all of them at once. It holds no path in its description.
public final class FolderGrant: @unchecked Sendable, CustomStringConvertible {
  /// The granted folder, for re-granting it; never logged.
  public let url: URL
  let securityScoped: Bool
  private let lock = NSLock()
  private var revoked = false
  /// Where the folder was when it was last read, to tell a disconnected
  /// disk from a deleted folder.
  private var lastRead: (device: dev_t, volume: URL?)?

  init(url: URL, securityScoped: Bool) {
    self.url = url
    self.securityScoped = securityScoped
  }

  public var isRevoked: Bool { lock.withLock { revoked } }
  public func revoke() { lock.withLock { revoked = true } }
  public var description: String { "FolderGrant(<redacted>)" }

  fileprivate var readLocation: (device: dev_t, volume: URL?)? { lock.withLock { lastRead } }
  fileprivate func recordRead(device: dev_t, volume: URL?) {
    lock.withLock { lastRead = (device, volume) }
  }

  /// Bracket filesystem work with the folder's security scope, if it has one.
  func accessing<T, E: Error>(_ body: () throws(E) -> T) throws(E) -> T {
    let scoped = securityScoped && url.startAccessingSecurityScopedResource()
    defer { if scoped { url.stopAccessingSecurityScopedResource() } }
    return try body()
  }
}

/// One folder the user granted through Open Folder…, a recent folder or a
/// recovery prompt (MAC-002A). Documents opened through it must resolve,
/// through symlinks, inside it; they share its revocable `FolderGrant`.
/// The core never sees the folder's URL.
public struct FolderCapability: Sendable, Hashable, CustomStringConvertible {
  public let url: URL
  public let grant: FolderGrant

  public init(granted url: URL, securityScoped: Bool = true) throws(FolderError) {
    guard url.isFileURL else { throw .notDirectory }
    // The user chose this folder; following its symlinks/aliases is the grant.
    let resolved = url.standardizedFileURL.resolvingSymlinksInPath()
    self.url = resolved
    grant = FolderGrant(url: resolved, securityScoped: securityScoped)
  }

  public var displayName: String { FileManager.default.displayName(atPath: url.path) }
  public var description: String { "FolderCapability(<redacted>)" }
  public var isRevoked: Bool { grant.isRevoked }

  /// Withdraw the grant from the folder and every document opened through it.
  public func revoke() { grant.revoke() }

  /// Check that the folder is still there, a folder and listable. Touches the
  /// filesystem; call off the main thread.
  public func validate() throws(FolderError) {
    if grant.isRevoked { throw .accessRevoked }
    try grant.accessing { () throws(FolderError) in
      var info = stat()
      let failure = url.withUnsafeFileSystemRepresentation { path in
        path.map { stat($0, &info) == 0 ? 0 : errno } ?? ENOENT
      }
      if failure != 0 { throw map(errno: failure) }
      guard info.st_mode & S_IFMT == S_IFDIR else { throw .notDirectory }
      // Listing is what a workspace needs; `access` alone can't see ACLs.
      let listable = url.withUnsafeFileSystemRepresentation { path -> Int32 in
        guard let path, let dir = opendir(path) else { return errno }
        closedir(dir)
        return 0
      }
      if listable != 0 { throw map(errno: listable) }
      let volume = try? url.resourceValues(forKeys: [.volumeURLKey]).volume
      grant.recordRead(device: info.st_dev, volume: volume)
    }
  }

  /// A document capability for `url` that carries this folder's grant, when
  /// `url` resolves inside the folder. Resolves symlinks, so call off the
  /// main thread for anything but a URL the user just chose.
  public func document(_ url: URL) throws(FolderError) -> DocumentCapability {
    if grant.isRevoked { throw .accessRevoked }
    guard url.isFileURL else { throw .outsideFolder }
    guard let root = Self.realPath(self.url.path) else { throw .notFound }
    let resolved = Self.resolvedPlacement(url)
    guard let resolved, resolved != root, resolved.hasPrefix(root == "/" ? "/" : root + "/")
    else { throw .outsideFolder }
    do {
      return try DocumentCapability(
        granted: URL(fileURLWithPath: resolved), securityScoped: false, folder: grant)
    } catch {
      throw .outsideFolder
    }
  }

  /// Whether `url` lies inside the folder, judged by its resolved path.
  public func contains(_ url: URL) -> Bool { (try? document(url)) != nil }

  func map(errno code: Int32) -> FolderError {
    switch code {
    case ENOENT:
      if let last = grant.readLocation,
        DocumentReader.volumeIsGone(device: last.device, volume: last.volume)
      {
        return .volumeUnavailable
      }
      return .notFound
    case ENOTDIR: return .notDirectory
    // Permission lost after a successful read is a withdrawn grant.
    case EACCES, EPERM: return grant.readLocation == nil ? .permissionDenied : .accessRevoked
    case ENXIO, ENODEV, ESTALE, ETIMEDOUT: return .volumeUnavailable
    default: return .unreadable
    }
  }

  /// Where `url` is, through symlinks. A missing item, even in missing
  /// folders, is placed by its nearest existing ancestor, so containment is
  /// judged and the read reports it missing. `..` is resolved lexically first,
  /// so the missing remainder has none.
  static func resolvedPlacement(_ url: URL) -> String? {
    var existing = url.standardizedFileURL
    var missing: [String] = []
    while true {
      if let real = realPath(existing.path) {
        return missing.reversed().reduce(real) { ($0 as NSString).appendingPathComponent($1) }
      }
      guard existing.path != "/", !existing.lastPathComponent.isEmpty else { return nil }
      missing.append(existing.lastPathComponent)
      existing.deleteLastPathComponent()
    }
  }

  static func realPath(_ path: String) -> String? {
    guard let resolved = realpath(path, nil) else { return nil }
    defer { free(resolved) }
    return String(cString: resolved)
  }

  public static func == (lhs: Self, rhs: Self) -> Bool { lhs.url == rhs.url }
  public func hash(into hasher: inout Hasher) { hasher.combine(url) }
}
