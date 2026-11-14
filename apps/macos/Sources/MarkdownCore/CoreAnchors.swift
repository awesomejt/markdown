import CMarkdown
import Foundation

/// One published snapshot handle, kept with the copied document so a reading
/// position can be anchored in it after the session has moved on. The session
/// holds the same snapshot while it is published, so keeping the handle costs
/// nothing until a newer generation replaces it. The last release, which may
/// destroy a large model, runs on the core worker.
public final class CoreSnapshot: @unchecked Sendable {
  private let handle: md_snapshot

  init(taking handle: md_snapshot) {
    self.handle = handle
  }

  deinit {
    let old = handle
    CoreRuntime.worker.async { autoreleasepool { _ = md_snapshot_release(old) } }
  }

  /// The serialized block anchor (specs/session) for an absolute source byte
  /// offset; between blocks it snaps forward. Anchors are private
  /// document-derived state: never log or upload them.
  public func anchor(atSource offset: UInt64) throws -> Data {
    var buffer = [UInt8](repeating: 0, count: Int(MD_ANCHOR_MAX_BYTES))
    var length = 0
    let status = md_snapshot_anchor_at(handle, offset, &buffer, buffer.count, &length)
    if status == MD_ERR_NO_BLOCKS { throw CoreError.noBlocks }
    try checked(status)
    return Data(buffer[..<min(length, buffer.count)])
  }

  /// Resolve an anchor from any snapshot of the document, including one saved
  /// before a relaunch. Nil when it is lost (no anchorable block). The bytes
  /// are untrusted; a malformed or other-version anchor throws.
  public func resolve(_ anchor: Data) throws -> CoreAnchorResolution? {
    var resolution = md_anchor_resolution()
    resolution.struct_size = UInt32(MemoryLayout<md_anchor_resolution>.size)
    let status = anchor.withUnsafeBytes { bytes in
      md_snapshot_resolve_anchor(
        handle, bytes.bindMemory(to: UInt8.self).baseAddress, bytes.count, &resolution)
    }
    if status == MD_ERR_NO_BLOCKS { return nil }
    try checked(status)
    guard let quality = CoreAnchorResolution.Quality(rawValue: resolution.quality) else {
      return nil
    }
    return CoreAnchorResolution(
      quality: quality, node: resolution.node, sourceOffset: resolution.source_offset)
  }
}

/// Where an anchor landed in a snapshot: its quality, the anchorable block's
/// node index and the absolute source byte offset (a UTF-8 boundary).
public struct CoreAnchorResolution: Sendable, Equatable {
  public enum Quality: UInt32, Sendable {
    case exact = 1
    case relocated, nearest
  }

  public let quality: Quality
  public let node: UInt32
  public let sourceOffset: UInt64
}

/// The session's answer to a change notice for the document (specs/session).
public enum CoreChangeDisposition: Sendable, Equatable {
  /// The notice matches the bytes already accepted; nothing to read.
  case unchanged
  /// The adapter must read the document again.
  case readRequired
  /// The notice matches the submission in flight.
  case pending
}

extension CoreSession {
  /// Ask whether a file whose revision token is now `token` needs reading
  /// (no I/O). A missing token always needs a read.
  public func externalChange(token: String?) throws -> CoreChangeDisposition {
    let bytes = Array((token ?? "").utf8)
    return try withHandle { handle in
      var disposition: UInt32 = 0
      var generation: UInt64 = 0
      try bytes.withUnsafeBufferPointer { token in
        try checked(
          md_session_external_change(
            handle, UInt32(MD_CHANGE_MODIFIED), UInt32(MD_REASON_NONE), token.baseAddress,
            token.count, &disposition, &generation))
      }
      switch disposition {
      case UInt32(MD_DISPOSITION_UNCHANGED): return .unchanged
      case UInt32(MD_DISPOSITION_READ_REQUIRED): return .readRequired
      case UInt32(MD_DISPOSITION_PENDING): return .pending
      default: throw CoreError.invalidSnapshot
      }
    }
  }
}
