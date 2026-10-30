import Darwin
import Foundation
import MarkdownCore
import UniformTypeIdentifiers

/// One file the user granted through the open panel, Finder/Launch Services or a
/// later recovery prompt. The core never sees this URL; it receives bytes and an
/// opaque revision token. Descriptions never contain the path.
public struct DocumentCapability: Sendable, Hashable, CustomStringConvertible {
  public let url: URL
  /// Present only for a URL that may carry a sandbox extension.
  let securityScoped: Bool

  public init(granted url: URL, securityScoped: Bool = true) throws(DocumentError) {
    guard url.isFileURL else { throw .unsupportedType }
    // The user chose this item; following its symlinks/aliases is the grant.
    self.url = url.standardizedFileURL.resolvingSymlinksInPath()
    self.securityScoped = securityScoped
  }

  public var displayName: String { FileManager.default.displayName(atPath: url.path) }
  public var description: String { "DocumentCapability(<redacted>)" }

  /// Extensionless files and any extension whose declared type is plain text.
  /// Markdown (`net.daringfireball.markdown` and its variants) conforms.
  public var isSupportedType: Bool {
    let ext = url.pathExtension
    guard !ext.isEmpty else { return true }
    return UTType(filenameExtension: ext)?.conforms(to: .plainText) == true
  }

  public static func == (lhs: Self, rhs: Self) -> Bool { lhs.url == rhs.url }
  public func hash(into hasher: inout Hasher) { hasher.combine(url) }
}

/// User-facing failure. Messages name the document by display name only.
public enum DocumentError: Error, Sendable, Equatable {
  case notFound
  case volumeUnavailable
  case permissionDenied
  case accessRevoked
  case notRegularFile(isDirectory: Bool)
  case unsupportedType
  case tooLarge(limit: UInt64)
  case invalidText
  case tooComplex
  case busy
  case unreadable(errno: Int32)
  case parseFailed

  public var message: String {
    switch self {
    case .notFound: "The document can’t be found."
    case .volumeUnavailable: "The disk containing the document is no longer available."
    case .permissionDenied: "You don’t have permission to read the document."
    case .accessRevoked: "Access to the document was lost."
    case .notRegularFile(true): "Folders can’t be opened as a document."
    case .notRegularFile(false): "The item isn’t a regular file."
    case .unsupportedType: "The document isn’t a Markdown or plain-text file."
    case .tooLarge(let limit):
      "The document is larger than the \(ByteCountFormatter.string(fromByteCount: Int64(limit), countStyle: .binary)) limit."
    case .invalidText: "The document isn’t valid UTF-8 text."
    case .tooComplex: "The document is too complex to display."
    case .busy: "Markdown is busy loading other documents."
    case .unreadable: "The document couldn’t be read."
    case .parseFailed: "The document couldn’t be processed."
    }
  }

  public var recovery: String {
    switch self {
    case .notFound:
      "It may have been moved, renamed or deleted. Open it again from its new location."
    case .volumeUnavailable: "Reconnect the disk or server, then reload."
    case .permissionDenied: "Check the file’s permissions in Finder, then open it again."
    case .accessRevoked: "Open the document again to grant access, or check its permissions."
    case .notRegularFile: "Choose a Markdown file instead."
    case .unsupportedType: "Choose a file with a Markdown or text extension."
    case .tooLarge, .tooComplex: "Split the document into smaller files."
    case .invalidText: "Convert the file to UTF-8 and open it again."
    case .busy, .unreadable, .parseFailed: "Try reloading the document."
    }
  }

  /// How the failed read is reported to the core session, if at all.
  var coreChange: CoreUnavailability? {
    switch self {
    case .notFound: .deleted
    case .permissionDenied: .permissionDenied
    case .accessRevoked: .accessRevoked
    case .volumeUnavailable, .unreadable: .io
    case .busy: .busy
    case .notRegularFile, .unsupportedType, .tooLarge, .invalidText, .tooComplex, .parseFailed:
      nil
    }
  }
}

/// A bounded byte snapshot of one regular file.
public struct DocumentBytes: Sendable {
  public let data: Data
  /// Opaque device/inode/size/mtime identity; holds no path.
  public let revisionToken: String
  let device: dev_t
  let volume: URL?
}

/// Cancels a queued or running read, including a coordinator wait.
final class ReadCancellation: @unchecked Sendable {
  private let lock = NSLock()
  private var cancelled = false
  let coordinator = NSFileCoordinator()

  var isCancelled: Bool { lock.withLock { cancelled } }
  func cancel() {
    lock.withLock { cancelled = true }
    coordinator.cancel()
  }
}

enum DocumentReader {
  static let chunkBytes = 256 * 1024
  /// One read at a time across all windows bounds in-flight file buffers.
  private static let queue = DispatchQueue(
    label: "io.github.awesomejt.markdown.read", qos: .userInitiated)

  static func read(_ capability: DocumentCapability, limit: UInt64) async throws -> DocumentBytes {
    let cancellation = ReadCancellation()
    return try await withTaskCancellationHandler {
      try await withCheckedThrowingContinuation { continuation in
        queue.async {
          continuation.resume(
            with: Result { try read(capability, limit: limit, cancellation: cancellation) })
        }
      }
    } onCancel: {
      cancellation.cancel()
    }
  }

  /// Coordinated, bounded read off the main thread; checks cancellation between
  /// chunks and rejects growth past `limit` without reading further.
  static func read(_ capability: DocumentCapability, limit: UInt64, cancellation: ReadCancellation)
    throws -> DocumentBytes
  {
    dispatchPrecondition(condition: .notOnQueue(.main))
    if cancellation.isCancelled { throw CancellationError() }
    guard capability.isSupportedType else { throw DocumentError.unsupportedType }
    let scoped = capability.securityScoped && capability.url.startAccessingSecurityScopedResource()
    defer { if scoped { capability.url.stopAccessingSecurityScopedResource() } }
    // The coordinator opens the item itself and would block on a FIFO; reject
    // non-regular items first. fstat after our own open still guards a swap.
    var info = stat()
    let found = capability.url.withUnsafeFileSystemRepresentation { path in
      path.map { stat($0, &info) == 0 } ?? false
    }
    guard found else { throw map(errno: errno) }
    let kind = info.st_mode & S_IFMT
    guard kind == S_IFREG else { throw DocumentError.notRegularFile(isDirectory: kind == S_IFDIR) }
    var result: Result<DocumentBytes, Error> = .failure(CancellationError())
    var coordinationError: NSError?
    cancellation.coordinator.coordinate(
      readingItemAt: capability.url, options: [.withoutChanges], error: &coordinationError
    ) { url in
      result = Result { try readFile(at: url, limit: limit, cancellation: cancellation) }
    }
    if let coordinationError { throw map(coordinationError) }
    return try result.get()
  }

  private static func readFile(at url: URL, limit: UInt64, cancellation: ReadCancellation)
    throws -> DocumentBytes
  {
    // O_NONBLOCK keeps a FIFO or device from blocking open(); fstat on the
    // opened descriptor avoids a check-then-open race.
    let fd = url.withUnsafeFileSystemRepresentation { path -> Int32 in
      guard let path else { return -1 }
      return open(path, O_RDONLY | O_NONBLOCK | O_CLOEXEC)
    }
    guard fd >= 0 else { throw map(errno: errno) }
    defer { Darwin.close(fd) }
    var info = stat()
    guard fstat(fd, &info) == 0 else { throw map(errno: errno) }
    let kind = info.st_mode & S_IFMT
    guard kind == S_IFREG else { throw DocumentError.notRegularFile(isDirectory: kind == S_IFDIR) }
    guard info.st_size >= 0, UInt64(info.st_size) <= limit else {
      throw DocumentError.tooLarge(limit: limit)
    }
    var data = Data(capacity: Int(info.st_size))
    var buffer = [UInt8](repeating: 0, count: chunkBytes)
    while true {
      if cancellation.isCancelled { throw CancellationError() }
      let count = buffer.withUnsafeMutableBytes { Darwin.read(fd, $0.baseAddress, chunkBytes) }
      if count < 0 {
        if errno == EINTR { continue }
        throw map(errno: errno)
      }
      if count == 0 { break }
      guard UInt64(data.count + count) <= limit else { throw DocumentError.tooLarge(limit: limit) }
      data.append(contentsOf: buffer[..<count])
    }
    let volume = try? url.resourceValues(forKeys: [.volumeURLKey]).volume
    let mtime = info.st_mtimespec
    let token = "\(info.st_dev):\(info.st_ino):\(data.count):\(mtime.tv_sec).\(mtime.tv_nsec)"
    return DocumentBytes(data: data, revisionToken: token, device: info.st_dev, volume: volume)
  }

  static func map(errno code: Int32) -> DocumentError {
    switch code {
    case ENOENT, ENOTDIR: .notFound
    case EACCES, EPERM: .permissionDenied
    case EISDIR: .notRegularFile(isDirectory: true)
    case ENXIO, ENODEV, ESTALE, ETIMEDOUT: .volumeUnavailable
    default: .unreadable(errno: code)
    }
  }

  private static func map(_ error: NSError) -> Error {
    if error.domain == NSCocoaErrorDomain {
      switch error.code {
      case NSUserCancelledError: return CancellationError()
      case NSFileReadNoSuchFileError, NSFileNoSuchFileError: return DocumentError.notFound
      case NSFileReadNoPermissionError: return DocumentError.permissionDenied
      default: break
      }
    }
    if let posix = error.userInfo[NSUnderlyingErrorKey] as? NSError,
      posix.domain == NSPOSIXErrorDomain
    {
      return map(errno: Int32(posix.code))
    }
    return DocumentError.unreadable(errno: EIO)
  }

  /// After a previous successful read, a missing item on a volume that is no
  /// longer mounted at the same place is a disconnected disk, not a deletion.
  static func volumeIsGone(device: dev_t, volume: URL?) -> Bool {
    guard let volume else { return false }
    var info = stat()
    let found = volume.withUnsafeFileSystemRepresentation { path in
      path.map { stat($0, &info) == 0 } ?? false
    }
    return !found || info.st_dev != device
  }
}
