import AppKit
import Darwin
import MarkdownDocument
import MarkdownRender
import QuartzCore

/// Opt-in development measurements, launched by tools/measure.py (MAC-001G).
/// They print one JSON object per sample with counts, times and memory only,
/// never document text or paths, and then quit. Like the smoke, they keep
/// settings in memory and never touch the user's clipboard.
@MainActor
enum Measurement {
  enum Mode: Equatable {
    /// D-012: process launch to the first presented frame of the document
    /// delivered at launch.
    case launch
    /// Open, present and close the document `count` times in one process,
    /// reporting the physical footprint after each close.
    case repeated(Int)
    /// Off-screen jumps, scrolling, Select All and Copy, a text size change
    /// and accessibility queries on the document delivered at launch.
    case interaction

    /// `--measure launch|interaction|repeated=N`, optionally followed by a
    /// document path (Launch Services can deliver the document instead).
    init?(arguments: [String]) {
      guard let flag = arguments.firstIndex(of: "--measure"), arguments.count > flag + 1 else {
        return nil
      }
      let value = arguments[flag + 1]
      if value == "launch" {
        self = .launch
      } else if value == "interaction" {
        self = .interaction
      } else if value.hasPrefix("repeated="), let count = Int(value.dropFirst(9)), count > 0 {
        self = .repeated(count)
      } else {
        return nil
      }
    }

    /// Whether the document is opened at launch rather than by the mode.
    var opensAtLaunch: Bool {
      if case .repeated = self { return false }
      return true
    }
  }

  /// A document path given after the mode, for runs without Launch Services.
  static func documentArgument(_ arguments: [String]) -> URL? {
    guard let flag = arguments.firstIndex(of: "--measure"), arguments.count > flag + 2 else {
      return nil
    }
    return URL(fileURLWithPath: arguments[flag + 2])
  }

  static func run(_ mode: Mode, delegate: AppDelegate) async {
    do {
      switch mode {
      case .launch: try await launch(delegate)
      case .repeated(let count): try await repeated(count, delegate)
      case .interaction: try await interaction(delegate)
      }
      NSApp.terminate(nil)
    } catch {
      FileHandle.standardError.write(Data("measure: FAIL: \(error)\n".utf8))
      exit(1)
    }
  }

  private struct Failure: Error, CustomStringConvertible {
    let description: String
  }

  /// Times are milliseconds since the process started, from the kernel's
  /// process start time, so they include exec, dyld and AppKit start-up.
  private static func launch(_ delegate: AppDelegate) async throws {
    let started = try processStart()
    let window = try launchWindow(delegate)
    var marks: [String: Double] = ["did_finish_launching": since(started)]
    let presented = try await present(window) { marks[$0] = since(started) }
    marks["presented"] = presented.timeIntervalSince(started) * 1000
    try await complete(window)
    var row = describe(window)
    row["ms"] = marks
    row["footprint_mib"] = mebibytes(footprint())
    emit(row)
  }

  private static func repeated(_ count: Int, _ delegate: AppDelegate) async throws {
    guard let url = documentArgument(CommandLine.arguments) else {
      throw Failure(description: "repeated mode needs a document path")
    }
    let capability = try DocumentCapability(granted: url)
    // The launch window stays open and empty, as an idle app would.
    let baseline = footprint()
    for iteration in 1...count {
      let start = ContinuousClock.now
      let window = delegate.makeWindow(nil)
      window.loader.open(capability)
      _ = try await present(window) { _ in }
      let presented = start.duration(to: .now)
      try await complete(window)
      var row = describe(window)
      row["iteration"] = iteration
      row["presented_ms"] = milliseconds(presented)
      row["completed_ms"] = milliseconds(start.duration(to: .now))
      row["live_footprint_mib"] = mebibytes(footprint())
      window.window?.performClose(nil)
      // Closing cancels work and awaits native cleanup off the main actor.
      try await waitUntil("window closes") { !delegate.windows.contains { $0 === window } }
      await window.loader.close()
      try await Task.sleep(for: .milliseconds(200))
      row["baseline_footprint_mib"] = mebibytes(baseline)
      row["retained_footprint_mib"] = mebibytes(footprint())
      emit(row)
    }
  }

  /// Each timed step runs from its action to the presented frame that shows
  /// its result, unless it is a synchronous call timed on its own.
  private static func interaction(_ delegate: AppDelegate) async throws {
    let window = try launchWindow(delegate)
    _ = try await present(window) { _ in }
    try await complete(window)
    let surface = window.surface
    let text = surface.textView
    guard let document = surface.document, let last = document.headings.indices.last else {
      throw Failure(description: "interaction needs a document with headings")
    }
    var row = describe(window)
    var ms: [String: Double] = [:]

    // D-016 off-screen jumps, as an outline click makes them.
    for (name, index) in [
      ("jump_last_ms", last), ("jump_first_ms", 0), ("jump_middle_ms", last / 2),
    ] {
      let start = Date()
      window.outline.onSelect?(index)
      let range = document.headings[index].range
      let shown = await frame(of: text) { surface.isVisible(range) }
      ms[name] = shown.timeIntervalSince(start) * 1000
    }

    // Page down from the top: the main-thread time of each step's layout and
    // drawing, not display pacing.
    text.scroll(.zero)
    window.window?.displayIfNeeded()
    var steps: [Double] = []
    for _ in 0..<60 {
      let visible = text.visibleRect
      let start = ContinuousClock.now
      text.scroll(NSPoint(x: 0, y: visible.minY + visible.height))
      window.window?.displayIfNeeded()
      steps.append(milliseconds(start.duration(to: .now)))
    }
    steps.sort()
    ms["scroll_step_p50_ms"] = steps[steps.count / 2]
    ms["scroll_step_p95_ms"] = steps[steps.count * 95 / 100]
    ms["scroll_step_max_ms"] = steps.last

    // A text size change re-renders off the main thread and keeps the place.
    var start = ContinuousClock.now
    delegate.preferences.textScale = 1.5
    try await waitUntil("text size re-renders") {
      !window.isBusy && surface.document?.scale == 1.5
    }
    _ = await frame(of: text) { true }
    ms["text_size_ms"] = milliseconds(start.duration(to: .now))

    // Select All and Copy to a private pasteboard.
    let pasteboard = NSPasteboard(name: NSPasteboard.Name("measure-\(UUID().uuidString)"))
    defer { pasteboard.releaseGlobally() }
    start = ContinuousClock.now
    text.selectAll(nil)
    ms["select_all_ms"] = milliseconds(start.duration(to: .now))
    start = ContinuousClock.now
    _ = text.writeSelection(to: pasteboard, types: text.writablePasteboardTypes)
    ms["copy_ms"] = milliseconds(start.duration(to: .now))
    row["copied_utf16"] = (pasteboard.string(forType: .string) as NSString?)?.length ?? 0
    text.setSelectedRange(NSRange(location: 0, length: 0))

    // Accessibility queries VoiceOver makes while reading.
    let end = max(document.text.length - 1, 0)
    start = ContinuousClock.now
    let line = text.accessibilityLine(for: end)
    ms["ax_line_for_end_ms"] = milliseconds(start.duration(to: .now))
    start = ContinuousClock.now
    _ = text.accessibilityLine(for: end - 1)
    ms["ax_line_for_end_again_ms"] = milliseconds(start.duration(to: .now))
    row["footprint_after_ax_line_mib"] = mebibytes(footprint())
    start = ContinuousClock.now
    _ = text.accessibilityRange(forLine: line)
    ms["ax_range_for_line_ms"] = milliseconds(start.duration(to: .now))
    start = ContinuousClock.now
    _ = text.accessibilityFrame(for: document.headings[last].range)
    ms["ax_frame_last_heading_ms"] = milliseconds(start.duration(to: .now))
    start = ContinuousClock.now
    _ = text.accessibilityString(for: NSRange(location: max(end - 200, 0), length: min(200, end)))
    ms["ax_string_near_end_ms"] = milliseconds(start.duration(to: .now))
    start = ContinuousClock.now
    let value = text.accessibilityValue().map { ($0 as NSString).length } ?? 0
    ms["ax_value_ms"] = milliseconds(start.duration(to: .now))
    row["ax_value_utf16"] = value
    if let rotor = text.accessibilityCustomRotors().first(where: { $0.type == .heading }) {
      let parameters = NSAccessibilityCustomRotor.SearchParameters()
      parameters.searchDirection = .previous
      start = ContinuousClock.now
      let result = rotor.itemSearchDelegate?.rotor(rotor, resultFor: parameters)
      ms["ax_rotor_last_heading_ms"] = milliseconds(start.duration(to: .now))
      row["ax_rotor_last_heading_ok"] = result?.targetRange == document.headings[last].range
    }

    row["ms"] = ms
    row["footprint_mib"] = mebibytes(footprint())
    emit(row)
  }

  private static func launchWindow(_ delegate: AppDelegate) throws -> ReaderWindowController {
    guard let window = delegate.windows.first(where: { $0.loader.capability != nil }) else {
      throw Failure(description: "no document was delivered at launch")
    }
    return window
  }

  /// Wait for the window to install its document and for the run-loop turn
  /// that lays it out, draws and commits it, then for the next display
  /// frame, which presents that commit. Returns that frame's presentation time.
  private static func present(
    _ window: ReaderWindowController, mark: @escaping (String) -> Void
  ) async throws -> Date {
    guard window.surface.document == nil else {
      throw Failure(description: "document was installed before the measurement")
    }
    let text = window.surface.textView
    await withCheckedContinuation { (continuation: CheckedContinuation<Void, Never>) in
      window.onMilestone = { milestone in
        switch milestone {
        case .loaded: mark("loaded")
        case .completed: mark("completed")
        case .installed:
          mark("installed")
          // TextKit can lay out the viewport a turn or more later, so wait
          // for a turn whose commit includes laid-out document text.
          afterCommit(when: { hasLaidOutViewport(text) }) { continuation.resume() }
        }
      }
    }
    mark("committed")
    guard window.surface.document != nil else { throw Failure(description: "document failed") }
    return await FrameWaiter.nextFrame(of: text)
  }

  private static func hasLaidOutViewport(_ text: NSTextView) -> Bool {
    guard let range = text.textLayoutManager?.textViewportLayoutController.viewportRange else {
      return false
    }
    return !range.isEmpty
  }

  /// The presentation time of the first frame committed once `condition`
  /// holds at the end of a run-loop turn.
  private static func frame(of view: NSView, when condition: @escaping @MainActor () -> Bool)
    async -> Date
  {
    await withCheckedContinuation { (continuation: CheckedContinuation<Void, Never>) in
      afterCommit(when: condition) { continuation.resume() }
    }
    return await FrameWaiter.nextFrame(of: view)
  }

  /// Run `body` at the end of the first run-loop turn, from this one on,
  /// where `condition` holds. A turn ends with AppKit's display cycle (layout
  /// and drawing) and Core Animation's commit; both are before-waiting
  /// observers, and this one is ordered after them.
  private static func afterCommit(
    when condition: @escaping @MainActor () -> Bool, _ body: @escaping @MainActor () -> Void
  ) {
    let observer = CFRunLoopObserverCreateWithHandler(
      nil, CFRunLoopActivity.beforeWaiting.rawValue, true, CFIndex.max
    ) { observer, _ in
      MainActor.assumeIsolated {
        guard condition() else { return }
        CFRunLoopObserverInvalidate(observer)
        body()
      }
    }
    CFRunLoopAddObserver(CFRunLoopGetMain(), observer, .commonModes)
  }

  /// Wait until the rest of a long document is shown after its lead.
  private static func complete(_ window: ReaderWindowController) async throws {
    try await waitUntil("document completes") { !window.isBusy }
    window.onMilestone = nil
    guard window.surface.document?.isComplete == true else {
      throw Failure(description: "document did not complete")
    }
  }

  private static func describe(_ window: ReaderWindowController) -> [String: Any] {
    let document = window.surface.document
    return [
      "utf16_length": document?.text.length ?? 0,
      "headings": document?.headings.count ?? 0,
      "configuration": configuration,
    ]
  }

  #if DEBUG
    static let configuration = "debug"
  #else
    static let configuration = "release"
  #endif

  private static func waitUntil(_ message: String, condition: () -> Bool) async throws {
    let deadline = ContinuousClock.now.advanced(by: .seconds(10))
    while !condition() && ContinuousClock.now < deadline {
      try await Task.sleep(for: .milliseconds(10))
    }
    if !condition() { throw Failure(description: message) }
  }

  /// The kernel's start time for this process.
  static func processStart() throws -> Date {
    var info = kinfo_proc()
    var size = MemoryLayout<kinfo_proc>.stride
    var name: [Int32] = [CTL_KERN, KERN_PROC, KERN_PROC_PID, getpid()]
    guard sysctl(&name, 4, &info, &size, nil, 0) == 0 else {
      throw Failure(description: "process start time unavailable")
    }
    let start = info.kp_proc.p_un.__p_starttime
    return Date(timeIntervalSince1970: Double(start.tv_sec) + Double(start.tv_usec) / 1e6)
  }

  private static func since(_ start: Date) -> Double { Date().timeIntervalSince(start) * 1000 }

  private static func milliseconds(_ duration: Duration) -> Double {
    let parts = duration.components
    return Double(parts.seconds) * 1000 + Double(parts.attoseconds) / 1e15
  }

  private static func mebibytes(_ bytes: UInt64) -> Double { Double(bytes) / 1_048_576 }

  /// The physical footprint the kernel charges the process for (Activity
  /// Monitor's Memory column).
  static func footprint() -> UInt64 {
    var info = task_vm_info_data_t()
    var count = mach_msg_type_number_t(
      MemoryLayout<task_vm_info_data_t>.size / MemoryLayout<integer_t>.size)
    let result = withUnsafeMutablePointer(to: &info) {
      $0.withMemoryRebound(to: integer_t.self, capacity: Int(count)) {
        task_info(mach_task_self_, task_flavor_t(TASK_VM_INFO), $0, &count)
      }
    }
    return result == KERN_SUCCESS ? info.phys_footprint : 0
  }

  private static func emit(_ row: [String: Any]) {
    guard let bytes = try? JSONSerialization.data(withJSONObject: row, options: [.sortedKeys])
    else { return }
    FileHandle.standardOutput.write(bytes + Data("\n".utf8))
  }
}

/// Resumes once at the view's next display-link frame.
@MainActor
private final class FrameWaiter: NSObject {
  private var continuation: CheckedContinuation<Date, Never>?
  private var link: CADisplayLink?

  static func nextFrame(of view: NSView) async -> Date {
    let waiter = FrameWaiter()
    return await withCheckedContinuation { continuation in
      waiter.continuation = continuation
      let link = view.displayLink(target: waiter, selector: #selector(tick(_:)))
      waiter.link = link
      link.add(to: .main, forMode: .common)
    }
  }

  /// The frame being prepared presents at `targetTimestamp`; the text was
  /// committed before it, so it is on screen by then.
  @objc private func tick(_ link: CADisplayLink) {
    link.invalidate()
    self.link = nil
    let ahead = link.targetTimestamp - CACurrentMediaTime()
    continuation?.resume(returning: Date().addingTimeInterval(max(ahead, 0)))
    continuation = nil
  }
}
