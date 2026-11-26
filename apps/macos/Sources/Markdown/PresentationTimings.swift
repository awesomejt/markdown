import Foundation

/// Stage-level attribution for load→first-install presentation (MAC-003C).
struct PresentationTimings: Sendable, Equatable {
  /// Delay from the loaded milestone to the background render beginning.
  let scheduleMilliseconds: Double
  /// Off-main `DocumentRenderer.render` time through first installable text.
  let renderMilliseconds: Double
  /// Main-thread `surface.show` / outline / properties install time.
  let installMilliseconds: Double
  /// End-to-end wall time from loaded to first install completion.
  let totalMilliseconds: Double
}

/// Shares schedule/render clocks between the detached renderer and a lead callback.
final class PresentationStageBox: @unchecked Sendable {
  private let lock = NSLock()
  private var scheduleMs = 0.0
  private var renderStarted: ContinuousClock.Instant?
  private var renderFinishedMs: Double?

  var scheduleMilliseconds: Double {
    get { lock.withLock { scheduleMs } }
    set { lock.withLock { scheduleMs = newValue } }
  }

  func beginRender() {
    lock.withLock { renderStarted = .now }
  }

  func renderMillisecondsSoFar() -> Double {
    lock.withLock {
      guard let renderStarted else { return 0 }
      return Self.milliseconds(renderStarted.duration(to: .now))
    }
  }

  func finishRender() -> Double {
    lock.withLock {
      guard let renderStarted else { return 0 }
      let value = Self.milliseconds(renderStarted.duration(to: .now))
      renderFinishedMs = value
      return value
    }
  }

  static func milliseconds(_ duration: Duration) -> Double {
    let parts = duration.components
    return Double(parts.seconds) * 1000 + Double(parts.attoseconds) / 1e15
  }
}
