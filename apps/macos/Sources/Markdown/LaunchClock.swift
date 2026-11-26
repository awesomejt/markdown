import Darwin
import Foundation

/// Process-relative startup marks for MAC-003C attribution. Times are
/// milliseconds since the kernel process start, matching `Measurement`.
@MainActor
enum LaunchClock {
  private static var marks: [String: Double] = [:]
  private static var processStart: Date?

  /// Record `name` once, relative to the process start time.
  static func mark(_ name: String, at date: Date = Date()) {
    guard marks[name] == nil else { return }
    let start: Date
    if let processStart {
      start = processStart
    } else if let resolved = try? Self.kernelProcessStart() {
      processStart = resolved
      start = resolved
    } else {
      return
    }
    marks[name] = date.timeIntervalSince(start) * 1000
  }

  /// Snapshot of recorded marks for the launch measurement row.
  static func snapshot() -> [String: Double] { marks }

  private static func kernelProcessStart() throws -> Date {
    var info = kinfo_proc()
    var size = MemoryLayout<kinfo_proc>.stride
    var name: [Int32] = [CTL_KERN, KERN_PROC, KERN_PROC_PID, getpid()]
    guard sysctl(&name, 4, &info, &size, nil, 0) == 0 else {
      throw LaunchClockError.processStartUnavailable
    }
    let start = info.kp_proc.p_un.__p_starttime
    return Date(timeIntervalSince1970: Double(start.tv_sec) + Double(start.tv_usec) / 1e6)
  }

  private enum LaunchClockError: Error {
    case processStartUnavailable
  }
}
