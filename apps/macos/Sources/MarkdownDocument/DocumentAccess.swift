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
  /// The folder grant this document was opened through (MAC-002A), if any.
  /// Revoking it withdraws access to the document too.
  public let folder: FolderGrant?

  public init(granted url: URL, securityScoped: Bool = true) throws(DocumentError) {
    try self.init(granted: url, securityScoped: securityScoped, folder: nil)
  }

  init(granted url: URL, securityScoped: Bool, folder: FolderGrant?) throws(DocumentError) {
    guard url.isFileURL else { throw .unsupportedType }
    // The user chose this item; following its symlinks/aliases is the grant.
    self.url = url.standardizedFileURL.resolvingSymlinksInPath()
    self.securityScoped = securityScoped
    self.folder = folder
  }

  /// Whether the folder grant this document came through was revoked.
  public var isRevoked: Bool { folder?.isRevoked == true }

  /// Bracket a read with the document's and its folder's security scopes.
  func accessing<T, E: Error>(_ body: () throws(E) -> T) throws(E) -> T {
    let scoped = securityScoped && url.startAccessingSecurityScopedResource()
    defer { if scoped { url.stopAccessingSecurityScopedResource() } }
    guard let folder else { return try body() }
    return try folder.accessing(body)
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

  /// A bookmark to the file for state restoration, which also finds it
  /// after a move or rename on the same volume. This build is not
  /// sandboxed; a sandboxed one needs a security-scoped bookmark instead.
  public func bookmark() -> Data? {
    try? url.bookmarkData(options: [], includingResourceValuesForKeys: nil, relativeTo: nil)
  }

  /// Resolve a saved bookmark without UI or mounting a volume, so a
  /// relaunch never stalls on a disconnected server. Nil when the file is gone.
  public init?(bookmark: Data) {
    var stale = false
    guard
      let url = try? URL(
        resolvingBookmarkData: bookmark, options: [.withoutUI, .withoutMounting],
        relativeTo: nil, bookmarkDataIsStale: &stale),
      let capability = try? DocumentCapability(granted: url, securityScoped: false)
    else { return nil }
    self = capability
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
    // Busy is a core-admission timeout, not a file state: the file was never
    // read, so recorded availability must not change.
    case .busy,
      .notRegularFile, .unsupportedType, .tooLarge, .invalidText, .tooComplex, .parseFailed:
      nil
    }
  }
}

/// Stage timings for one bounded file read (MAC-003B).
public struct DocumentReadTimings: Sendable, Equatable {
  public let queueMilliseconds: Double
  public let coordinateMilliseconds: Double
  public let ioMilliseconds: Double
  public let totalMilliseconds: Double
}

/// A bounded byte snapshot of one regular file.
public struct DocumentBytes: Sendable {
  public let data: Data
  /// Opaque device/inode/size/mtime identity; holds no path.
  public let revisionToken: String
  let device: dev_t
  let volume: URL?
  public let timings: DocumentReadTimings
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
    let queuedAt = ContinuousClock.now
    return try await withTaskCancellationHandler {
      try await withCheckedThrowingContinuation { continuation in
        queue.async {
          let startedAt = ContinuousClock.now
          continuation.resume(
            with: Result {
              try read(
                capability, limit: limit, cancellation: cancellation,
                queueMilliseconds: milliseconds(queuedAt, startedAt))
            })
        }
      }
    } onCancel: {
      cancellation.cancel()
    }
  }

  /// Coordinated, bounded read off the main thread; checks cancellation between
  /// chunks and rejects growth past `limit` without reading further. A
  /// workspace search scan reads `coordinated: false`: the same bounded,
  /// non-blocking read and descriptor checks, without the file coordinator,
  /// which cost about 0.6 ms per file (MAC-002C). Opening a result is a
  /// normal coordinated read.
  static func read(
    _ capability: DocumentCapability, limit: UInt64, cancellation: ReadCancellation,
    coordinated: Bool = true, queueMilliseconds: Double = 0
  ) throws -> DocumentBytes {
    dispatchPrecondition(condition: .notOnQueue(.main))
    let readStart = ContinuousClock.now
    if cancellation.isCancelled { throw CancellationError() }
    guard capability.isSupportedType else { throw DocumentError.unsupportedType }
    // A revoked folder grant withdraws its documents before any file access.
    guard !capability.isRevoked else { throw DocumentError.accessRevoked }
    return try capability.accessing { () throws -> DocumentBytes in
      guard coordinated else {
        let (bytes, ioMs) = try readFile(
          at: capability.url, limit: limit, cancellation: cancellation)
        let totalMs = queueMilliseconds + milliseconds(readStart, .now)
        return DocumentBytes(
          data: bytes.data, revisionToken: bytes.revisionToken, device: bytes.device,
          volume: bytes.volume,
          timings: DocumentReadTimings(
            queueMilliseconds: queueMilliseconds, coordinateMilliseconds: 0,
            ioMilliseconds: ioMs, totalMilliseconds: totalMs))
      }
      return try readScoped(
        capability, limit: limit, cancellation: cancellation,
        queueMilliseconds: queueMilliseconds, readStart: readStart)
    }
  }

  private static func readScoped(
    _ capability: DocumentCapability, limit: UInt64, cancellation: ReadCancellation,
    queueMilliseconds: Double, readStart: ContinuousClock.Instant
  ) throws -> DocumentBytes {
    // The coordinator opens the item itself and would block on a FIFO; reject
    // non-regular items first. fstat after our own open still guards a swap.
    var info = stat()
    // Capture errno inside the closure, before any cleanup can overwrite it.
    let failure = capability.url.withUnsafeFileSystemRepresentation { path in
      path.map { stat($0, &info) == 0 ? 0 : errno } ?? ENOENT
    }
    guard failure == 0 else { throw map(errno: failure) }
    let kind = info.st_mode & S_IFMT
    guard kind == S_IFREG else { throw DocumentError.notRegularFile(isDirectory: kind == S_IFDIR) }
    var result: Result<(DocumentBytes, Double), Error> = .failure(CancellationError())
    var coordinationError: NSError?
    let coordinateStart = ContinuousClock.now
    cancellation.coordinator.coordinate(
      readingItemAt: capability.url, options: [.withoutChanges], error: &coordinationError
    ) { url in
      result = Result { try readFile(at: url, limit: limit, cancellation: cancellation) }
    }
    let coordinateEnd = ContinuousClock.now
    if let coordinationError { throw map(coordinationError) }
    let (bytes, ioMs) = try result.get()
    let coordinateTotal = milliseconds(coordinateStart, coordinateEnd)
    let coordinateOverhead = max(0, coordinateTotal - ioMs)
    let totalMs = queueMilliseconds + milliseconds(readStart, .now)
    return DocumentBytes(
      data: bytes.data, revisionToken: bytes.revisionToken, device: bytes.device,
      volume: bytes.volume,
      timings: DocumentReadTimings(
        queueMilliseconds: queueMilliseconds, coordinateMilliseconds: coordinateOverhead,
        ioMilliseconds: ioMs, totalMilliseconds: totalMs))
  }

  private static func readFile(at url: URL, limit: UInt64, cancellation: ReadCancellation)
    throws -> (DocumentBytes, Double)
  {
    let ioStart = ContinuousClock.now
    // O_NONBLOCK keeps a FIFO or device from blocking open(); fstat on the
    // opened descriptor avoids a check-then-open race.
    let (fd, failure) = url.withUnsafeFileSystemRepresentation { path -> (Int32, Int32) in
      guard let path else { return (-1, ENOENT) }
      let fd = open(path, O_RDONLY | O_NONBLOCK | O_CLOEXEC)
      return (fd, fd >= 0 ? 0 : errno)
    }
    guard fd >= 0 else { throw map(errno: failure) }
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
    let ioMs = milliseconds(ioStart, .now)
    let placeholder = DocumentReadTimings(
      queueMilliseconds: 0, coordinateMilliseconds: 0, ioMilliseconds: ioMs,
      totalMilliseconds: ioMs)
    return (
      DocumentBytes(
        data: data, revisionToken: token(info, size: data.count), device: info.st_dev,
        volume: volume, timings: placeholder),
      ioMs
    )
  }

  private static func milliseconds(_ a: ContinuousClock.Instant, _ b: ContinuousClock.Instant)
    -> Double
  {
    let parts = a.duration(to: b).components
    return Double(parts.seconds) * 1000 + Double(parts.attoseconds) / 1e15
  }

  /// The file's current revision token, as a read would record it, or nil
  /// when it can't be examined. A stat only: no coordination and no read.
  static func revisionToken(_ capability: DocumentCapability) -> String? {
    dispatchPrecondition(condition: .notOnQueue(.main))
    // A revoked grant has no current revision: the loader reads to report it.
    guard !capability.isRevoked else { return nil }
    return capability.accessing { () -> String? in
      var info = stat()
      let found = capability.url.withUnsafeFileSystemRepresentation { path in
        path.map { stat($0, &info) == 0 } ?? false
      }
      guard found, info.st_mode & S_IFMT == S_IFREG else { return nil }
      return token(info, size: Int(info.st_size))
    }
  }

  /// Opaque device/inode/size/mtime identity; holds no path.
  static func token(_ info: stat, size: Int) -> String {
    let mtime = info.st_mtimespec
    return "\(info.st_dev):\(info.st_ino):\(size):\(mtime.tv_sec).\(mtime.tv_nsec)"
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
