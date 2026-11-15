import CoreServices
import Darwin
import Foundation

/// Merges a burst of changes into one refresh (MAC-002E). A refresh is due
/// once changes have been quiet for `quiet`, or `maxDelay` after the first
/// change of the burst, whichever comes first, so a steady storm still
/// refreshes about once a second instead of never or for every event.
public struct ChangeCoalescer: Sendable, Equatable {
  public static let quiet: Duration = .milliseconds(150)
  public static let maxDelay: Duration = .milliseconds(1000)
  private var first: ContinuousClock.Instant?
  private var last: ContinuousClock.Instant?

  public init() {}

  public var isPending: Bool { first != nil }

  /// When the pending refresh is due, or nil when nothing is pending.
  public var deadline: ContinuousClock.Instant? {
    guard let first, let last else { return nil }
    return min(last + Self.quiet, first + Self.maxDelay)
  }

  /// Record a change at `now`.
  public mutating func note(_ now: ContinuousClock.Instant) {
    if first == nil { first = now }
    last = now
  }

  /// Whether a refresh is due at `now`; a due refresh is cleared.
  public mutating func take(_ now: ContinuousClock.Instant) -> Bool {
    guard let deadline, now >= deadline else { return false }
    first = nil
    last = nil
    return true
  }
}

/// What a workspace watcher reports.
public enum WorkspaceChange: Sendable, Equatable {
  /// Something inside the folder changed.
  case contents
  /// The folder itself was moved, deleted, recreated, or its volume was
  /// mounted or unmounted: check that it is still there before listing.
  case root
}

/// Watches a granted folder's tree for changes with an FSEvents stream
/// (MAC-002E). Events are coalesced (`ChangeCoalescer`) and reported on the
/// main actor without paths; the window lists the folder again. Changes
/// only inside hidden items are ignored unless hidden items are shown.
/// Dropped or overflowed event batches still report, since the window
/// always lists the whole folder again.
///
/// FSEvents watches by path and holds no descriptor, so it never keeps a
/// volume from unmounting. The stream watches the root path itself, so
/// moving the folder away and back, or its disk going and returning,
/// reports `.root`. Network volumes may not deliver events; the window also
/// lists again when it becomes key after losing the folder.
///
/// The stream retains the watcher until `cancel()`, which the owner must call.
public final class WorkspaceWatcher: @unchecked Sendable {
  private let queue = DispatchQueue(label: "io.github.awesomejt.markdown.workspace-watch")
  private let report: @MainActor @Sendable (WorkspaceChange) -> Void
  /// The folder's real path, as FSEvents reports it.
  private let root: String
  // Queue-confined state.
  private var stream: FSEventStreamRef?
  private var coalescer = ChangeCoalescer()
  private var rootChanged = false
  private var timerArmed = false
  private var cancelled = false
  private var includeHidden: Bool
  private var reports = 0

  /// Start watching `folder`. Nil when FSEvents can't create the stream.
  public init?(
    folder: FolderCapability, includeHidden: Bool,
    report: @escaping @MainActor @Sendable (WorkspaceChange) -> Void
  ) {
    root = FolderCapability.realPath(folder.url.path) ?? folder.url.path
    self.includeHidden = includeHidden
    self.report = report
    var context = FSEventStreamContext(
      version: 0, info: Unmanaged.passUnretained(self).toOpaque(),
      retain: { info in
        guard let info else { return nil }
        _ = Unmanaged<WorkspaceWatcher>.fromOpaque(info).retain()
        return info
      },
      release: { info in
        guard let info else { return }
        Unmanaged<WorkspaceWatcher>.fromOpaque(info).release()
      },
      copyDescription: nil)
    let flags = FSEventStreamCreateFlags(
      kFSEventStreamCreateFlagUseCFTypes | kFSEventStreamCreateFlagWatchRoot)
    guard
      let stream = folder.grant.accessing({
        FSEventStreamCreate(
          nil, Self.callback, &context, [root] as CFArray,
          FSEventStreamEventId(kFSEventStreamEventIdSinceNow), 0.05, flags)
      })
    else { return nil }
    self.stream = stream
    FSEventStreamSetDispatchQueue(stream, queue)
    guard FSEventStreamStart(stream) else {
      FSEventStreamInvalidate(stream)
      FSEventStreamRelease(stream)
      self.stream = nil
      return nil
    }
  }

  /// Stop watching and release the stream. A report already on its way may
  /// still arrive.
  public func cancel() {
    queue.async {
      self.cancelled = true
      guard let stream = self.stream else { return }
      self.stream = nil
      FSEventStreamStop(stream)
      FSEventStreamInvalidate(stream)
      FSEventStreamRelease(stream)
    }
  }

  /// Report changes inside hidden items too, when the listing shows them.
  public func setIncludeHidden(_ value: Bool) {
    queue.async { self.includeHidden = value }
  }

  /// Reports delivered so far (tests and the storm check).
  public var reportCount: Int { queue.sync { reports } }

  private static let callback: FSEventStreamCallback = { _, info, count, paths, flags, _ in
    guard let info else { return }
    let watcher = Unmanaged<WorkspaceWatcher>.fromOpaque(info).takeUnretainedValue()
    let names = unsafeBitCast(paths, to: NSArray.self) as? [String] ?? []
    let flags = Array(UnsafeBufferPointer(start: flags, count: count))
    watcher.handle(names, flags)
  }

  private func handle(_ paths: [String], _ flags: [FSEventStreamEventFlags]) {
    dispatchPrecondition(condition: .onQueue(queue))
    guard !cancelled else { return }
    let rootFlags = FSEventStreamEventFlags(
      kFSEventStreamEventFlagRootChanged | kFSEventStreamEventFlagMount
        | kFSEventStreamEventFlagUnmount)
    let rescanFlags = FSEventStreamEventFlags(
      kFSEventStreamEventFlagMustScanSubDirs | kFSEventStreamEventFlagUserDropped
        | kFSEventStreamEventFlagKernelDropped)
    var relevant = false
    for (index, flag) in flags.enumerated() {
      if flag & rootFlags != 0 {
        rootChanged = true
        relevant = true
      } else if flag & rescanFlags != 0 {
        relevant = true
      } else if includeHidden || index >= paths.count || !isHidden(paths[index]) {
        relevant = true
      }
    }
    guard relevant else { return }
    coalescer.note(.now)
    arm()
  }

  /// Whether `path` lies inside a hidden item of the folder: a component
  /// below the root starting with `.`, as the core defines hidden.
  private func isHidden(_ path: String) -> Bool {
    var path = Substring(path)
    while path.count > 1, path.hasSuffix("/") { path = path.dropLast() }
    let prefix = root == "/" ? "/" : root + "/"
    guard path.hasPrefix(prefix) else { return false }
    return path.dropFirst(prefix.count).split(separator: "/").contains { $0.hasPrefix(".") }
  }

  private func arm() {
    guard !timerArmed, let deadline = coalescer.deadline else { return }
    timerArmed = true
    let wait = max(Duration.zero, deadline - .now)
    let (seconds, attoseconds) = wait.components
    let nanoseconds = Int(seconds) * 1_000_000_000 + Int(attoseconds / 1_000_000_000)
    queue.asyncAfter(deadline: .now() + .nanoseconds(nanoseconds)) { [weak self] in
      self?.fire()
    }
  }

  private func fire() {
    timerArmed = false
    guard !cancelled else { return }
    guard coalescer.take(.now) else {
      // More changes arrived and moved the deadline.
      arm()
      return
    }
    let change: WorkspaceChange = rootChanged ? .root : .contents
    rootChanged = false
    reports += 1
    let report = report
    Task { @MainActor in report(change) }
  }
}
