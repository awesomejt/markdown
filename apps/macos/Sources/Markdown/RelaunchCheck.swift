import AppKit
import MarkdownDocument

/// Opt-in development check of real state restoration (MAC-001H), launched
/// twice by tools/check.py with `-NSQuitAlwaysKeepsWindows YES`. `prepare`
/// opens the document, puts its "Part 80" heading at the top with a word
/// selected, saves the position and quits; `verify` expects AppKit to have
/// restored that window at that place, then closes every window, so the
/// app's saved state is left empty, and quits. Settings stay in memory.
@MainActor
enum RelaunchCheck {
  enum Phase: String {
    case prepare, verify
  }

  /// `--relaunch-check prepare|verify <path>`.
  static func arguments(_ arguments: [String]) -> (Phase, URL)? {
    guard let flag = arguments.firstIndex(of: "--relaunch-check"), arguments.count > flag + 2,
      let phase = Phase(rawValue: arguments[flag + 1])
    else { return nil }
    return (phase, URL(fileURLWithPath: arguments[flag + 2]))
  }

  private struct Failure: Error, CustomStringConvertible {
    let description: String
  }

  static func run(_ phase: Phase, document url: URL, delegate: AppDelegate) async {
    do {
      let capability = try DocumentCapability(granted: url)
      if phase == .prepare { delegate.open([url]) }
      let reader = try await settled(capability, delegate)
      let headings = reader.outline.headings
      guard let index = headings.firstIndex(where: { $0.title == "Part 80" }) else {
        throw Failure(description: "missing heading")
      }
      let heading = headings[index].range
      let string = reader.surface.textView.string as NSString
      let word = string.range(of: "marked", range: NSRange(heading.location..<string.length))
      switch phase {
      case .prepare:
        reader.surface.reveal(heading: index)
        reader.surface.textView.setSelectedRange(word)
        await reader.saveReadingPosition()
        guard reader.savedPosition != nil else { throw Failure(description: "position not saved") }
        reader.invalidateRestorableState()
        note("prepared")
      case .verify:
        guard delegate.windows.count == 1 else {
          throw Failure(description: "expected one restored window: \(delegate.windows.count)")
        }
        guard abs(reader.surface.topLocation() - heading.location) <= 1,
          reader.surface.textView.selectedRange() == word
        else { throw Failure(description: "restored window lost the reading position") }
        for window in delegate.windows.map(\.window) { window?.performClose(nil) }
        note("pass (restored window, document, top line and selection)")
      }
      NSApp.terminate(nil)
    } catch {
      FileHandle.standardError.write(Data("relaunch-check: FAIL: \(error)\n".utf8))
      exit(1)
    }
  }

  private static func settled(_ capability: DocumentCapability, _ delegate: AppDelegate)
    async throws -> ReaderWindowController
  {
    let deadline = ContinuousClock.now.advanced(by: .seconds(10))
    while ContinuousClock.now < deadline {
      if let reader = delegate.windows.first(where: { $0.loader.capability == capability }),
        reader.surface.document?.isComplete == true, !reader.isBusy
      {
        return reader
      }
      try await Task.sleep(for: .milliseconds(20))
    }
    throw Failure(description: "document did not load")
  }

  private static func note(_ message: String) {
    FileHandle.standardOutput.write(Data("relaunch-check: \(message)\n".utf8))
  }
}
