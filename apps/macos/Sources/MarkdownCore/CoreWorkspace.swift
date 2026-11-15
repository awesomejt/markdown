import CMarkdown
import Foundation

/// The core's verdict on one workspace display path
/// (specs/workspace-search, `md_workspace_classify_path`). The core never
/// opens the path; it only validates and classifies the label.
public struct CoreWorkspacePath: Sendable, Equatable {
  /// The core accepts the path. A rejected path is never shown.
  public let isValid: Bool
  /// The final component has a Markdown extension (`md`, `markdown`, `mdx`).
  public let isMarkdown: Bool
  /// A component starts with `.`.
  public let isHidden: Bool
  /// The path has invisible characters, so it can display like another name.
  public let hasInvisible: Bool

  /// The most components a valid path has.
  public static let maxComponents = Int(MD_WORKSPACE_MAX_PATH_DEPTH)

  /// Classify a relative, `/`-separated display path. Pure and thread-safe.
  public static func classify(_ path: String) -> CoreWorkspacePath {
    var info = md_workspace_path_info()
    info.struct_size = UInt32(MemoryLayout<md_workspace_path_info>.size)
    var bytes = Array(path.utf8)
    let status = bytes.withUnsafeMutableBufferPointer { buffer in
      md_workspace_classify_path(buffer.baseAddress, buffer.count, &info)
    }
    guard status == MD_OK, info.error == UInt32(MD_PATH_VALID) else {
      return CoreWorkspacePath(
        isValid: false, isMarkdown: false, isHidden: false, hasInvisible: false)
    }
    return CoreWorkspacePath(
      isValid: true,
      isMarkdown: info.flags & UInt32(MD_PATH_MARKDOWN) != 0,
      isHidden: info.flags & UInt32(MD_PATH_HIDDEN) != 0,
      hasInvisible: info.flags & UInt32(MD_PATH_INVISIBLE) != 0)
  }
}
