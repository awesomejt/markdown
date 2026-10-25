import Foundation
import Darwin

func residentBytes() -> UInt64 {
    var info = mach_task_basic_info()
    var count = mach_msg_type_number_t(MemoryLayout<mach_task_basic_info>.size / MemoryLayout<natural_t>.size)
    let status = withUnsafeMutablePointer(to: &info) {
        $0.withMemoryRebound(to: integer_t.self, capacity: Int(count)) {
            task_info(mach_task_self_, task_flavor_t(MACH_TASK_BASIC_INFO), $0, &count)
        }
    }
    return status == KERN_SUCCESS ? info.resident_size : 0
}
func memorySample() -> [String: UInt64] {
    var stats = malloc_statistics_t()
    malloc_zone_statistics(nil, &stats)
    return ["resident_bytes": residentBytes(), "malloc_in_use_bytes": UInt64(stats.size_in_use),
            "malloc_allocated_bytes": UInt64(stats.size_allocated)]
}
@main struct MemoryMain {
    static func main() throws {
        let args = CommandLine.arguments
        guard (3...4).contains(args.count), ["native", "copy"].contains(args[1]) else { exit(2) }
        let data = try Data(contentsOf: URL(fileURLWithPath: args[2]))
        if args.contains("--debug-wait") { fputs("debug-ready\n", stderr); Thread.sleep(forTimeInterval: 10) }
        var samples: [[String: UInt64]] = []
        for _ in 0..<20 {
            try autoreleasepool {
                if args[1] == "copy" { _ = try Core.parseAndCopy(data) }
                else {
                    var options = Core.options(), snapshot = md_snapshot()
                    try ensureCore(data.withUnsafeBytes {
                        md_parse_v1($0.bindMemory(to: UInt8.self).baseAddress, UInt64($0.count), &options, &snapshot)
                    })
                    try ensureCore(md_snapshot_free_v1(snapshot.handle))
                }
            }
            guard let live = CoreProbe.live(), live.0 == 0, live.1 == 0 else { throw CoreFailure.invalidSnapshot }
            samples.append(memorySample())
        }
        if args.contains("--pause") { fputs("sampling complete\n", stderr); Thread.sleep(forTimeInterval: 10) }
        // Diagnostic only, after all unmodified samples; not a production fix
        // or a setting used for the measured UI/console runs.
        let relieved = malloc_zone_pressure_relief(nil, 0)
        let output: [String: Any] = ["mode": args[1], "samples": samples,
                                   "diagnostic_pressure_relief_bytes": relieved,
                                   "after_pressure_relief": memorySample(), "live_snapshots": 0, "live_tokens": 0]
        print(String(decoding: try JSONSerialization.data(withJSONObject: output, options: [.sortedKeys]), as: UTF8.self))
    }
}
