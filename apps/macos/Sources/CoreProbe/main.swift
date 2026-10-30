import CMarkdown
import Darwin
import Foundation
import MarkdownCore

@main
enum CoreProbe {
  static func main() async {
    do { try await run() } catch {
      let message = (error as? CoreError)?.description ?? "Invalid probe input"
      FileHandle.standardError.write(Data("Core probe failed: \(message)\n".utf8))
      exit(1)
    }
  }

  static func run() async throws {
    try CoreRuntime.validate()
    guard CoreRuntime.packageConfiguration == "release" else { throw CoreError.incompatibleABI }
    let mode = CommandLine.arguments.dropFirst().first ?? "single"
    guard ["single", "repeated", "dense-default", "dense-host"].contains(mode) else {
      throw CoreError.invalidSnapshot
    }
    let initial = try CoreRuntime.liveHandles()
    if mode == "dense-default" {
      try denseDefault()
    } else {
      let count = mode == "repeated" ? 20 : 1
      let dense = mode == "dense-host"
      let size = Int(CoreRuntime.maxSourceBytes)
      let unit = dense ? "*a" : "A sentence with readable words and an ordinary paragraph.\n\n"
      let source: Data
      if CommandLine.arguments.count > 2 {
        source = try Data(contentsOf: URL(fileURLWithPath: CommandLine.arguments[2]))
      } else {
        source = Data(String(repeating: unit, count: size / unit.utf8.count + 1).utf8.prefix(size))
      }
      let baseline = rss()
      for iteration in 1...count {
        // Each iteration drops the session, native snapshot and owned Swift copy.
        let row = try await trial(source, iteration)
        var result = row
        result["mode"] = mode
        result["baseline_rss"] = baseline
        result["retained_rss"] = rss()
        result["retained_heap"] = heap()
        let handles = try CoreRuntime.liveHandles()
        result["native_handles"] = handles.sessions + handles.jobs + handles.snapshots
        try emit(result)
      }
    }
    guard try CoreRuntime.liveHandles() == initial else { throw CoreError.invalidSnapshot }
  }

  static func trial(_ source: Data, _ iteration: Int) async throws -> [String: Any] {
    let session = try CoreSession(documentID: "probe")
    let start = ContinuousClock.now
    let result = try await session.parse(source)
    let end = ContinuousClock.now
    var row: [String: Any] = [
      "iteration": iteration, "source_bytes": source.count,
      "outcome": result.outcome.rawValue, "failure": result.failure?.rawValue ?? 0,
      "parse_ms": result.timings.parseMilliseconds, "encode_ms": result.timings.encodeMilliseconds,
      "call_ms": milliseconds(start, end),
      "copy_ms": result.timings.copyMilliseconds, "copied_bytes": result.document?.copiedBytes ?? 0,
      "nodes": result.document?.nodes.count ?? 0, "live_rss": rss(),
    ]
    let closing = ContinuousClock.now
    try await session.close()
    row["close_ms"] = milliseconds(closing, .now)
    row["peak_rss"] = peakRSS()
    return row
  }

  static func denseDefault() throws {
    let size = 8 * 1024 * 1024
    let source = Array(String(repeating: "*a", count: size / 2 + 1).utf8.prefix(size))
    var options = md_session_options()
    options.struct_size = UInt32(MemoryLayout<md_session_options>.size)
    options.model_version = UInt32(MD_MODEL_V2)
    // Baseline probe deliberately selects core defaults, bypassing host policy.
    var session: md_session = 0
    var job: md_job = 0
    var generation: UInt64 = 0
    try Array("probe".utf8).withUnsafeBufferPointer {
      try status(md_session_create($0.baseAddress, $0.count, &options, &session))
    }
    defer { _ = md_session_release(session) }
    try source.withUnsafeBufferPointer {
      try status(md_session_submit(session, $0.baseAddress, $0.count, nil, 0, &job, &generation))
    }
    defer { _ = md_job_release(job) }
    var result = md_job_result()
    result.struct_size = UInt32(MemoryLayout<md_job_result>.size)
    let before = rss()
    try status(md_job_run(job, &result))
    defer { if result.snapshot != 0 { _ = md_snapshot_release(result.snapshot) } }
    try emit([
      "mode": "dense-default", "source_bytes": size, "outcome": result.outcome,
      "failure": result.failure, "baseline_rss": before, "live_rss": rss(), "peak_rss": peakRSS(),
    ])
  }

  static func status(_ code: md_status) throws {
    if code != MD_OK { throw CoreError.status(code) }
  }

  static func rss() -> UInt64 {
    var info = mach_task_basic_info()
    var count = mach_msg_type_number_t(
      MemoryLayout<mach_task_basic_info>.size / MemoryLayout<integer_t>.size)
    let result = withUnsafeMutablePointer(to: &info) {
      $0.withMemoryRebound(to: integer_t.self, capacity: Int(count)) {
        task_info(mach_task_self_, task_flavor_t(MACH_TASK_BASIC_INFO), $0, &count)
      }
    }
    precondition(result == KERN_SUCCESS)
    return info.resident_size
  }

  static func peakRSS() -> Int64 {
    var usage = rusage()
    precondition(getrusage(RUSAGE_SELF, &usage) == 0)
    return Int64(usage.ru_maxrss)
  }

  static func heap() -> UInt64 {
    var statistics = malloc_statistics_t()
    malloc_zone_statistics(nil, &statistics)
    return UInt64(statistics.size_in_use)
  }

  static func milliseconds(_ start: ContinuousClock.Instant, _ end: ContinuousClock.Instant)
    -> Double
  {
    let parts = start.duration(to: end).components
    return Double(parts.seconds) * 1000 + Double(parts.attoseconds) / 1e15
  }

  static func emit(_ row: [String: Any]) throws {
    var row = row
    row["core_configuration"] = CoreRuntime.packageConfiguration
    let bytes = try JSONSerialization.data(withJSONObject: row, options: [.sortedKeys])
    print(String(decoding: bytes, as: UTF8.self))
  }
}
