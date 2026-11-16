import AppKit
import MarkdownCore

/// What this build is (MAC-002F, R1.5): the app version, the semantic
/// contract and core package the app negotiated at startup, and the source
/// commit `build.py` recorded in `BuildInfo.json`. `--version` prints it and
/// the About panel shows it. No path is ever included.
struct BuildDiagnostics {
  let version: String
  let build: String
  let core: CoreDiagnostics
  /// The source commit, abbreviated, with `-dirty` for a modified tree; nil
  /// for a bundle without a build record.
  let sourceCommit: String?

  init(core: CoreDiagnostics, bundle: Bundle = .main) {
    self.core = core
    version = bundle.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "?"
    build = bundle.object(forInfoDictionaryKey: "CFBundleVersion") as? String ?? "?"
    sourceCommit = Self.sourceCommit(bundle)
  }

  private static func sourceCommit(_ bundle: Bundle) -> String? {
    guard let url = bundle.url(forResource: "BuildInfo", withExtension: "json"),
      let data = try? Data(contentsOf: url),
      let record = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
      let commit = record["source_commit"] as? String, !commit.isEmpty
    else { return nil }
    let dirty = record["source_dirty"] as? Bool == true
    return String(commit.prefix(12)) + (dirty ? "-dirty" : "")
  }

  /// The facts, one per line, after the app's name and version.
  var lines: [String] {
    var lines = [
      "Semantic contract: \(core.contract)",
      "Core package: \(core.package)",
    ]
    if let sourceCommit { lines.append("Source: \(sourceCommit)") }
    return lines
  }

  /// `--version`: one line, so scripts can read it.
  var versionLine: String {
    "Markdown \(version) (\(build)); " + lines.joined(separator: "; ")
  }

  /// The About panel's credits.
  var credits: NSAttributedString {
    let style = NSMutableParagraphStyle()
    style.alignment = .center
    return NSAttributedString(
      string: "A local, read-only native Markdown reader.\n\n" + lines.joined(separator: "\n"),
      attributes: [
        .font: NSFont.systemFont(ofSize: NSFont.smallSystemFontSize),
        .foregroundColor: NSColor.secondaryLabelColor, .paragraphStyle: style,
      ])
  }
}
