import Darwin
import Foundation
import UniformTypeIdentifiers

/// Why an image beside the document was not read. Messages name no path.
public enum ResourceError: Error, Sendable, Equatable {
  /// The resolved file is not inside the document's folder.
  case outsideFolder
  case notFound
  case permissionDenied
  case notRegularFile
  /// Not a raster image type, or SVG (MAC-001E does not decode SVG).
  case unsupportedType
  case tooLarge(limit: Int)
  case unreadable
}

/// Read-only access to images in the folder of one granted document (MAC-001E).
///
/// The core hands over a relative path that it has already decoded exactly
/// once. It is never decoded again or routed through a URL. The file must
/// resolve, through symlinks, to a regular image file inside the document's
/// folder or one of its subfolders, and it is opened with `O_NOFOLLOW_ANY`, so
/// a symlink swapped in after the check fails instead of escaping the folder.
/// This build is not sandboxed; a sandboxed build needs a folder grant
/// (MAC-002A, capability matrix CAP-04).
public struct DocumentResources: Sendable {
  /// The document's folder, with symlinks resolved; nil if it cannot be resolved.
  let root: String?

  public init(document: DocumentCapability) {
    root = Self.realPath(document.url.deletingLastPathComponent().path)
  }

  /// Read one image named by the core's decoded relative `path`, at most
  /// `limit` bytes. Call off the main thread.
  public func readImage(_ path: String, limit: Int) throws(ResourceError) -> Data {
    guard let root, !path.isEmpty, !path.hasPrefix("/"), !path.contains("\0") else {
      throw .outsideFolder
    }
    let candidate = root + "/" + path
    guard let resolved = Self.realPath(candidate) else { throw Self.map(errno: errno) }
    guard resolved.hasPrefix(root == "/" ? "/" : root + "/"), resolved != root else {
      throw .outsideFolder
    }
    let ext = (resolved as NSString).pathExtension
    guard let type = UTType(filenameExtension: ext), type.conforms(to: .image),
      !type.conforms(to: .svg)
    else { throw .unsupportedType }
    let fd = open(resolved, O_RDONLY | O_NONBLOCK | O_CLOEXEC | O_NOFOLLOW_ANY)
    guard fd >= 0 else { throw Self.map(errno: errno) }
    defer { Darwin.close(fd) }
    var info = stat()
    guard fstat(fd, &info) == 0 else { throw Self.map(errno: errno) }
    guard info.st_mode & S_IFMT == S_IFREG else { throw .notRegularFile }
    guard info.st_size >= 0, info.st_size <= limit else { throw .tooLarge(limit: limit) }
    var data = Data(count: Int(info.st_size))
    var filled = 0
    while filled < data.count {
      let count = data.withUnsafeMutableBytes {
        Darwin.read(fd, $0.baseAddress! + filled, $0.count - filled)
      }
      if count < 0 {
        if errno == EINTR { continue }
        throw Self.map(errno: errno)
      }
      if count == 0 { break }
      filled += count
    }
    // A file that shrank during the read is returned as read; growth past the
    // size checked above is not read at all.
    data.count = filled
    return data
  }

  private static func realPath(_ path: String) -> String? {
    guard let resolved = realpath(path, nil) else { return nil }
    defer { free(resolved) }
    return String(cString: resolved)
  }

  private static func map(errno code: Int32) -> ResourceError {
    switch code {
    case ENOENT, ENOTDIR: .notFound
    case EACCES, EPERM: .permissionDenied
    // O_NOFOLLOW_ANY: a component became a symlink after the containment check.
    case ELOOP: .outsideFolder
    case EISDIR: .notRegularFile
    default: .unreadable
    }
  }
}
