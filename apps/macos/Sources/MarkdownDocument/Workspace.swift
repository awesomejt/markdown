import Darwin
import Foundation
import MarkdownCore

/// What a workspace listing shows (MAC-002B).
public struct WorkspaceOptions: Sendable, Equatable {
  /// List hidden items (a component starting with `.`), as the core defines them.
  public var includeHidden: Bool
  /// List every file, not only Markdown, and keep folders with none.
  public var allFiles: Bool
  /// Directory entries examined before the listing stops (truncated).
  public var maxEntries: Int
  /// Folder levels below the root that are descended.
  public var maxDepth: Int

  public static let defaultMaxEntries = 100_000
  public static let defaultMaxDepth = CoreWorkspacePath.maxComponents - 1

  public init(
    includeHidden: Bool = false, allFiles: Bool = false,
    maxEntries: Int = WorkspaceOptions.defaultMaxEntries,
    maxDepth: Int = WorkspaceOptions.defaultMaxDepth
  ) {
    self.includeHidden = includeHidden
    self.allFiles = allFiles
    self.maxEntries = maxEntries
    self.maxDepth = maxDepth
  }
}

/// One file or folder in a listing. Immutable once listed; the browser
/// shows it by identity. Its description holds no path.
public final class WorkspaceItem: Sendable, CustomStringConvertible {
  public enum Kind: Sendable { case folder, file }

  public let kind: Kind
  /// The final component, exactly as the file system returned it.
  public let name: String
  /// The display path relative to the workspace root, `/`-separated. It is
  /// a label for the core and the browser, never decoded or used as access.
  public let path: String
  public let isMarkdown: Bool
  /// The name has invisible characters, so it can look like another name.
  public let hasInvisible: Bool
  /// A folder that couldn't be read; it has no children.
  public let isUnreadable: Bool
  /// Folders first, then by name as Finder sorts.
  public let children: [WorkspaceItem]

  init(
    kind: Kind, name: String, path: String, isMarkdown: Bool, hasInvisible: Bool,
    isUnreadable: Bool = false, children: [WorkspaceItem] = []
  ) {
    self.kind = kind
    self.name = name
    self.path = path
    self.isMarkdown = isMarkdown
    self.hasInvisible = hasInvisible
    self.isUnreadable = isUnreadable
    self.children = children
  }

  public var description: String { "WorkspaceItem(<redacted>)" }
}

/// A bounded snapshot of a granted folder's tree (MAC-002B). The adapter
/// enumerates under the folder's grant and asks the core to classify each
/// display path (`md_workspace_classify_path`): rejected names are skipped
/// and counted, hidden ones are left out unless asked for, and names with
/// invisible characters are kept and flagged. Symlinked folders and
/// packages are not descended, and a symlinked file is listed only when it
/// resolves inside the folder.
public struct WorkspaceListing: Sendable {
  public let folder: FolderCapability
  public let options: WorkspaceOptions
  /// The root's children.
  public let items: [WorkspaceItem]
  /// Files listed, at any depth.
  public let fileCount: Int
  public let entriesExamined: Int
  /// Names the core rejects (invalid UTF-8, controls, bidi formatting).
  public let skippedInvalid: Int
  /// The entry or depth limit stopped the listing early.
  public let isTruncated: Bool
  /// The root, through symlinks, when it was listed.
  let root: String
  private let index: [String: WorkspaceItem]

  /// The item at a display path, if listed.
  public func item(at path: String) -> WorkspaceItem? { index[path] }

  /// The display paths of the folders that contain `path`, outermost first.
  public static func ancestors(of path: String) -> [String] {
    var result: [String] = []
    var current = Substring(path)
    while let slash = current.lastIndex(of: "/") {
      current = current[..<slash]
      result.append(String(current))
    }
    return result.reversed()
  }

  /// Where an item is. Opening it still goes through `FolderCapability.document`.
  public func url(for item: WorkspaceItem) -> URL {
    URL(fileURLWithPath: root).appendingPathComponent(item.path)
  }

  /// The display path of a document inside the folder, through symlinks, or
  /// nil when it lies outside. Touches the filesystem; call off the main thread.
  public func path(of url: URL) -> String? {
    guard let resolved = FolderCapability.realPath(url.standardizedFileURL.path) else {
      return nil
    }
    let prefix = root == "/" ? "/" : root + "/"
    guard resolved.hasPrefix(prefix), resolved != root else { return nil }
    return String(resolved.dropFirst(prefix.count))
  }

  /// List `folder`. Call off the main thread: it reads every directory up to
  /// the limits. `isCancelled` is checked between folders.
  public static func list(
    _ folder: FolderCapability, options: WorkspaceOptions = WorkspaceOptions(),
    isCancelled: @escaping @Sendable () -> Bool = { false }
  ) throws -> WorkspaceListing {
    dispatchPrecondition(condition: .notOnQueue(.main))
    if folder.isRevoked { throw FolderError.accessRevoked }
    return try folder.grant.accessing { () throws -> WorkspaceListing in
      guard let root = FolderCapability.realPath(folder.url.path) else {
        throw folder.map(errno: errno)
      }
      var walk = Walk(
        root: root, options: options, grant: folder.grant, isCancelled: isCancelled)
      guard let items = try walk.children(of: "", depth: 0) else {
        throw folder.map(errno: walk.rootError)
      }
      return WorkspaceListing(
        folder: folder, options: options, items: items, fileCount: walk.fileCount,
        entriesExamined: walk.examined, skippedInvalid: walk.skippedInvalid,
        isTruncated: walk.truncated, root: root, index: walk.index)
    }
  }
}

/// The depth-first enumeration behind `WorkspaceListing.list`.
private struct Walk {
  let root: String
  let options: WorkspaceOptions
  let grant: FolderGrant
  let isCancelled: @Sendable () -> Bool
  var examined = 0
  var fileCount = 0
  var skippedInvalid = 0
  var truncated = false
  var rootError: Int32 = 0
  var index: [String: WorkspaceItem] = [:]

  init(
    root: String, options: WorkspaceOptions, grant: FolderGrant,
    isCancelled: @escaping @Sendable () -> Bool
  ) {
    self.root = root
    self.options = options
    self.grant = grant
    self.isCancelled = isCancelled
  }

  private func absolute(_ relative: String) -> String {
    relative.isEmpty ? root : (root == "/" ? "/" : root + "/") + relative
  }

  /// The listed children of the folder at `relative`, or nil when it can't
  /// be opened (the root's error is kept in `rootError`).
  mutating func children(of relative: String, depth: Int) throws -> [WorkspaceItem]? {
    if isCancelled() { throw CancellationError() }
    // Close Folder withdraws access mid-listing too.
    if grant.isRevoked { throw FolderError.accessRevoked }
    guard let directory = opendir(absolute(relative)) else {
      if relative.isEmpty { rootError = errno }
      return nil
    }
    // Read the names first, so the directory is closed before descending.
    var names: [(name: String, type: UInt8)] = []
    while let entry = readdir(directory) {
      let (name, type) = withUnsafePointer(to: &entry.pointee.d_name) { pointer in
        pointer.withMemoryRebound(to: CChar.self, capacity: Int(MAXPATHLEN)) {
          (String(validatingCString: $0), entry.pointee.d_type)
        }
      }
      if name == "." || name == ".." { continue }
      if examined >= options.maxEntries {
        truncated = true
        break
      }
      examined += 1
      guard let name else {
        skippedInvalid += 1
        continue
      }
      names.append((name, type))
    }
    closedir(directory)

    var folders: [WorkspaceItem] = []
    var files: [WorkspaceItem] = []
    for (name, type) in names {
      let path = relative.isEmpty ? name : relative + "/" + name
      let verdict = CoreWorkspacePath.classify(path)
      guard verdict.isValid else {
        skippedInvalid += 1
        continue
      }
      if verdict.isHidden, !options.includeHidden { continue }
      let invisible = CoreWorkspacePath.classify(name).hasInvisible
      switch kind(of: path, type: type) {
      case .folder:
        guard depth < options.maxDepth else {
          truncated = true
          continue
        }
        if truncated { continue }
        let listed = try children(of: path, depth: depth + 1)
        let item = WorkspaceItem(
          kind: .folder, name: name, path: path, isMarkdown: false, hasInvisible: invisible,
          isUnreadable: listed == nil, children: listed ?? [])
        // Markdown-only listings leave out folders without any Markdown.
        if listed?.isEmpty == true, !options.allFiles { continue }
        folders.append(item)
        index[path] = item
      case .file:
        guard options.allFiles || verdict.isMarkdown else { continue }
        let item = WorkspaceItem(
          kind: .file, name: name, path: path, isMarkdown: verdict.isMarkdown,
          hasInvisible: invisible)
        files.append(item)
        index[path] = item
        fileCount += 1
      case nil:
        continue
      }
    }
    let byName: (WorkspaceItem, WorkspaceItem) -> Bool = {
      $0.name.localizedStandardCompare($1.name) == .orderedAscending
    }
    return folders.sorted(by: byName) + files.sorted(by: byName)
  }

  /// A folder to descend, a file to list, or nil for anything else: devices,
  /// FIFOs, packages, symlinked folders and symlinks that leave the root.
  private func kind(of relative: String, type: UInt8) -> WorkspaceItem.Kind? {
    let path = absolute(relative)
    var resolvedType = type
    if Int32(type) == DT_UNKNOWN {
      var info = stat()
      guard lstat(path, &info) == 0 else { return nil }
      switch info.st_mode & S_IFMT {
      case S_IFDIR: resolvedType = UInt8(DT_DIR)
      case S_IFREG: resolvedType = UInt8(DT_REG)
      case S_IFLNK: resolvedType = UInt8(DT_LNK)
      default: return nil
      }
    }
    switch Int32(resolvedType) {
    case DT_DIR:
      // A package (an app, a bundle) is one item in Finder, not a folder.
      if !(relative as NSString).pathExtension.isEmpty,
        (try? URL(fileURLWithPath: path).resourceValues(forKeys: [.isPackageKey]))?.isPackage
          == true
      {
        return nil
      }
      return .folder
    case DT_REG:
      return .file
    case DT_LNK:
      var info = stat()
      guard stat(path, &info) == 0, info.st_mode & S_IFMT == S_IFREG,
        let resolved = FolderCapability.realPath(path),
        resolved.hasPrefix(root == "/" ? "/" : root + "/")
      else { return nil }
      return .file
    default:
      return nil
    }
  }
}

// MARK: Links to other documents (MAC-002B)

/// Why a link to another document can't be followed. Messages name no path.
public enum LinkError: Error, Sendable, Equatable {
  /// The target resolves outside the folder the document was opened
  /// through, or outside the document's own folder when it has none.
  case outsideFolder(granted: Bool)
  /// Not a Markdown or plain-text file.
  case unsupportedType
  case invalid
  case accessRevoked

  public var message: String {
    switch self {
    case .outsideFolder(true): "The linked document is outside the open folder."
    case .outsideFolder(false): "The linked document is outside this document’s folder."
    case .unsupportedType: "The link doesn’t lead to a Markdown or text document."
    case .invalid: "The link can’t be followed."
    case .accessRevoked: "Access to the folder was lost."
    }
  }

  public var recovery: String {
    switch self {
    case .outsideFolder(true): "Open the folder that contains both documents."
    case .outsideFolder(false):
      "Open the folder that contains both documents to follow links between them."
    case .unsupportedType: "Open it from Finder instead."
    case .invalid: "Check the link in the document’s source."
    case .accessRevoked: "Grant access to the folder again to follow its links."
    }
  }
}

extension DocumentCapability {
  /// The document a relative link in this one leads to. The core decoded
  /// `path` exactly once; it is joined to this document's folder as is, never
  /// decoded again. A document opened through a folder grant may link
  /// anywhere inside that folder and the target shares the grant; one opened
  /// by itself may link inside its own folder, as its images may (MAC-001E).
  /// Resolves symlinks, so call off the main thread. A missing target is
  /// returned, so the read reports it.
  public func linkedDocument(_ path: String, through folder: FolderCapability?) throws(LinkError)
    -> DocumentCapability
  {
    guard !path.isEmpty, !path.hasPrefix("/"), !path.contains("\0") else { throw .invalid }
    let base = url.deletingLastPathComponent().path
    let candidate = URL(fileURLWithPath: (base as NSString).appendingPathComponent(path))
    let target: DocumentCapability
    if let folder, self.folder === folder.grant {
      do throws(FolderError) {
        target = try folder.document(candidate)
      } catch {
        throw error == .accessRevoked ? .accessRevoked : .outsideFolder(granted: true)
      }
    } else {
      guard let root = FolderCapability.realPath(base) else { throw .outsideFolder(granted: false) }
      let standardized = candidate.standardizedFileURL
      let resolved =
        FolderCapability.realPath(standardized.path)
        ?? FolderCapability.realPath(standardized.deletingLastPathComponent().path).map {
          ($0 as NSString).appendingPathComponent(standardized.lastPathComponent)
        }
      guard let resolved, resolved != root, resolved.hasPrefix(root == "/" ? "/" : root + "/")
      else { throw .outsideFolder(granted: false) }
      do {
        target = try DocumentCapability(
          granted: URL(fileURLWithPath: resolved), securityScoped: false, folder: nil)
      } catch {
        throw .invalid
      }
    }
    guard target.isSupportedType else { throw .unsupportedType }
    return target
  }
}

// MARK: File identity (MAC-002B)

/// The device and inode of a file: a hard link or a case variant of a path
/// is the same file. Holds no path.
public struct FileIdentity: Sendable, Hashable {
  public let device: UInt64
  public let inode: UInt64
}

extension DocumentCapability {
  /// The file's identity now, or nil when it can't be examined (missing,
  /// revoked). A stat only; call off the main thread, since a disconnected
  /// server can stall it.
  public func fileIdentity() -> FileIdentity? {
    guard !isRevoked else { return nil }
    return accessing { () -> FileIdentity? in
      var info = stat()
      let found = url.withUnsafeFileSystemRepresentation { path in
        path.map { stat($0, &info) == 0 } ?? false
      }
      guard found else { return nil }
      return FileIdentity(device: UInt64(info.st_dev), inode: UInt64(info.st_ino))
    }
  }
}
