import Darwin
import Foundation

/// Watches one document for external changes with a kqueue vnode source.
/// A write, growth, rename, deletion or revocation reports a change. After
/// a rename, deletion or revocation the watcher re-arms on the path: an
/// atomic save replaces the file, so the new one is watched; if nothing is
/// there it watches the parent folder and reports once the path reappears.
/// Bursts are coalesced into one report after `delayMilliseconds`. Reports
/// arrive on the main actor and name no path; the loader decides whether to
/// read, and ignores a report from a watcher it has replaced.
///
/// The descriptors use `O_EVTONLY`, so watching never keeps a volume from
/// unmounting. Network and provider volumes may not deliver events; the
/// window also checks the file when it becomes key.
public final class DocumentWatcher: @unchecked Sendable {
  /// Long enough to merge an editor's write-then-rename into one report.
  public static let delayMilliseconds = 100
  private let url: URL
  private let queue = DispatchQueue(label: "io.github.awesomejt.markdown.watch")
  private let report: @MainActor @Sendable () -> Void
  // Queue-confined state.
  private var source: DispatchSourceFileSystemObject?
  private var watchingFolder = false
  private var scheduled = false
  private var cancelled = false

  /// Start watching `url`, a file the user granted.
  public init(url: URL, report: @escaping @MainActor @Sendable () -> Void) {
    self.url = url
    self.report = report
    queue.async { self.arm() }
  }

  deinit {
    source?.cancel()
  }

  /// Stop watching. A report already on its way may still arrive.
  public func cancel() {
    queue.async {
      self.cancelled = true
      self.source?.cancel()
      self.source = nil
    }
  }

  /// Re-arm a watcher that lost its file and folder, for example after a
  /// volume was unmounted and mounted again.
  public func rearm() {
    queue.async {
      guard !self.cancelled, self.source == nil else { return }
      self.arm()
    }
  }

  /// Whether a vnode source is active (tests).
  public var isArmed: Bool { queue.sync { source != nil && !watchingFolder } }

  private func arm() {
    dispatchPrecondition(condition: .onQueue(queue))
    source?.cancel()
    source = nil
    if let file = open(url) {
      watch(file, folder: false, events: [.write, .extend, .delete, .rename, .revoke, .link])
    } else if let folder = open(url.deletingLastPathComponent()) {
      watch(folder, folder: true, events: [.write, .delete, .rename, .revoke])
      // The file may have appeared before the folder was watched.
      if let file = open(url) {
        source?.cancel()
        watch(file, folder: false, events: [.write, .extend, .delete, .rename, .revoke, .link])
        schedule()
      }
    }
  }

  private func open(_ url: URL) -> Int32? {
    let fd = url.withUnsafeFileSystemRepresentation { path in
      path.map { Darwin.open($0, O_EVTONLY | O_CLOEXEC) } ?? -1
    }
    return fd >= 0 ? fd : nil
  }

  private func watch(_ fd: Int32, folder: Bool, events: DispatchSource.FileSystemEvent) {
    let source = DispatchSource.makeFileSystemObjectSource(
      fileDescriptor: fd, eventMask: events, queue: queue)
    source.setEventHandler { [weak self, unowned source] in
      self?.handle(source.data, folder: folder)
    }
    source.setCancelHandler { Darwin.close(fd) }
    self.source = source
    watchingFolder = folder
    source.activate()
  }

  private func handle(_ events: DispatchSource.FileSystemEvent, folder: Bool) {
    guard !cancelled else { return }
    if folder {
      // Something in the folder changed: report only once the file is back.
      // A vanished folder leaves the watcher disarmed until `rearm`.
      if !events.isDisjoint(with: [.delete, .rename, .revoke]) {
        source?.cancel()
        source = nil
        schedule()
        return
      }
      arm()
      if !watchingFolder { schedule() }
      return
    }
    if !events.isDisjoint(with: [.delete, .rename, .revoke]) { arm() }
    schedule()
  }

  private func schedule() {
    guard !scheduled else { return }
    scheduled = true
    queue.asyncAfter(deadline: .now() + .milliseconds(Self.delayMilliseconds)) { [weak self] in
      guard let self else { return }
      self.scheduled = false
      guard !self.cancelled else { return }
      let report = self.report
      Task { @MainActor in report() }
    }
  }
}
