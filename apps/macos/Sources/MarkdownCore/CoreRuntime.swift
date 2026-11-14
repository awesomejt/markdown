import CMarkdown
import Foundation

public enum CoreError: Error, Sendable, CustomStringConvertible {
  case status(UInt32)
  case incompatibleABI
  case invalidSnapshot
  case snapshotBudget
  case closed
  case busy
  /// The session no longer publishes the requested snapshot generation.
  case superseded
  /// A find query over MD_FIND_MAX_QUERY_BYTES.
  case queryTooLong
  /// The snapshot has no anchorable block (an empty document).
  case noBlocks

  public var description: String {
    switch self {
    case .status(let code): String(cString: md_status_name(code))
    case .incompatibleABI: "Incompatible core ABI/model/layout"
    case .invalidSnapshot: "Invalid core snapshot"
    case .snapshotBudget: "Core snapshot exceeds the host copy budget"
    case .closed: "Core session closed"
    case .busy: "Core worker capacity reached"
    case .superseded: "Core snapshot superseded"
    case .queryTooLong: "Find query too long"
    case .noBlocks: "Snapshot has no anchorable block"
    }
  }
}

func checked(_ status: md_status) throws {
  if status != MD_OK { throw CoreError.status(status) }
}

public struct CoreHandleCounts: Sendable, Equatable {
  public let sessions: UInt64
  public let jobs: UInt64
  public let snapshots: UInt64
}

public enum CoreRuntime {
  public static let packageConfiguration = MD_PACKAGE_CONFIGURATION
  public static let packageFingerprint = MD_PACKAGE_FINGERPRINT
  /// Host policy: enough for the accepted 5 MiB profiles, not the core's 8 MiB default.
  public static let maxSourceBytes: UInt64 = 5 * 1024 * 1024
  // Frozen 5 MiB prose uses 461,177 nodes and about 932,000 events.
  public static let maxNodes: UInt64 = 500_000
  public static let maxEvents: UInt64 = 1_000_000
  public static let maxTextBytes: UInt64 = 16 * 1024 * 1024
  public static let maxSnapshotBytes: UInt64 = 128 * 1024 * 1024

  /// All expensive parse/flat-view/copy work across windows shares one serial worker.
  static let worker = DispatchQueue(label: "io.github.awesomejt.markdown.core", qos: .userInitiated)
  static let admission = CoreAdmission()

  public static func validate() throws {
    var info = md_abi_info()
    info.struct_size = UInt32(MemoryLayout<md_abi_info>.size)
    try checked(md_abi_negotiate(UInt32(MD_ABI_VERSION), &info))
    let sizes: [(UInt32, Int)] = [
      (info.session_options_size, MemoryLayout<md_session_options>.size),
      (info.session_status_size, MemoryLayout<md_session_status_info>.size),
      (info.job_result_size, MemoryLayout<md_job_result>.size),
      (info.snapshot_view_size, MemoryLayout<md_snapshot_view>.size),
      (info.node_size, MemoryLayout<md_node>.size),
      (info.intent_size, MemoryLayout<md_intent>.size),
      (info.outline_entry_size, MemoryLayout<md_outline_entry>.size),
      (info.diagnostic_size, MemoryLayout<md_diagnostic>.size),
      (info.property_size, MemoryLayout<md_property>.size),
      (info.anchor_resolution_size, MemoryLayout<md_anchor_resolution>.size),
      (UInt32(MD_FIND_OPTIONS_SIZE), MemoryLayout<md_find_options>.size),
      (UInt32(MD_FIND_RESULT_SIZE), MemoryLayout<md_find_result>.size),
      (UInt32(MD_FIND_MATCH_SIZE), MemoryLayout<md_find_match>.size),
      (UInt32(MD_MATCH_SEGMENT_SIZE), MemoryLayout<md_match_segment>.size),
      (UInt32(MD_READ_OPTIONS_SIZE), MemoryLayout<md_read_options>.size),
      (UInt32(MD_READ_STATS_SIZE), MemoryLayout<md_read_stats>.size),
      (UInt32(MD_READ_SECTION_SIZE), MemoryLayout<md_read_section>.size),
      (UInt32(MD_WORKSPACE_PATH_INFO_SIZE), MemoryLayout<md_workspace_path_info>.size),
      (UInt32(MD_WORKSPACE_ENTRY_SIZE), MemoryLayout<md_workspace_entry>.size),
      (UInt32(MD_NAME_OPTIONS_SIZE), MemoryLayout<md_name_options>.size),
      (UInt32(MD_NAME_RESULT_SIZE), MemoryLayout<md_name_result>.size),
      (UInt32(MD_NAME_HIT_SIZE), MemoryLayout<md_name_hit>.size),
      (UInt32(MD_CONTENT_OPTIONS_SIZE), MemoryLayout<md_content_options>.size),
      (UInt32(MD_CONTENT_PROGRESS_SIZE), MemoryLayout<md_content_progress>.size),
      (UInt32(MD_CONTENT_RESULT_SIZE), MemoryLayout<md_content_result>.size),
      (UInt32(MD_CONTENT_HIT_SIZE), MemoryLayout<md_content_hit>.size),
    ]
    guard info.abi_version == MD_ABI_VERSION,
      info.model_versions & (1 << UInt32(MD_MODEL_V2)) != 0,
      info.pointer_size == UInt32(MemoryLayout<UnsafeRawPointer>.size),
      info.anchor_version == 1, info.max_anchor_bytes == MD_ANCHOR_MAX_BYTES,
      sizes.allSatisfy({ UInt32($0.1) == $0.0 })
    else { throw CoreError.incompatibleABI }
  }

  public static func liveHandles() throws -> CoreHandleCounts {
    var sessions: UInt64 = 0
    var jobs: UInt64 = 0
    var snapshots: UInt64 = 0
    try checked(md_live_handles(&sessions, &jobs, &snapshots))
    return CoreHandleCounts(sessions: sessions, jobs: jobs, snapshots: snapshots)
  }
}

/// One running parse plus at most one queued call; reject rather than retaining
/// an unbounded series of caller byte buffers behind the serial worker.
final class CoreAdmission: @unchecked Sendable {
  private let lock = NSLock()
  private var count = 0
  var pending: Int { lock.withLock { count } }
  func reserve() throws {
    try lock.withLock {
      guard count < 2 else { throw CoreError.busy }
      count += 1
    }
  }
  func release() { lock.withLock { count -= 1 } }
}
