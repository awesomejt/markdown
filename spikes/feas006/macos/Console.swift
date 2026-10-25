import Foundation
import Darwin

func require(_ condition: @autoclosure () -> Bool, _ message: String) throws {
    if !condition() { throw NSError(domain: "FEAS006", code: 1, userInfo: [NSLocalizedDescriptionKey: message]) }
}
func rss() -> UInt64 {
    var info = mach_task_basic_info()
    var count = mach_msg_type_number_t(MemoryLayout<mach_task_basic_info>.size / MemoryLayout<natural_t>.size)
    let status = withUnsafeMutablePointer(to: &info) {
        $0.withMemoryRebound(to: integer_t.self, capacity: Int(count)) {
            task_info(mach_task_self_, task_flavor_t(MACH_TASK_BASIC_INFO), $0, &count)
        }
    }
    return status == KERN_SUCCESS ? info.resident_size : 0
}
final class Results: @unchecked Sendable {
    private let lock = NSLock()
    private var values: [UInt32] = []
    func add(_ value: UInt32) { lock.lock(); defer { lock.unlock() }; values.append(value) }
    var all: [UInt32] { lock.lock(); defer { lock.unlock() }; return values }
}
func status(_ data: Data, _ options: md_options) -> UInt32 {
    var options = options
    var snapshot = md_snapshot()
    let value = data.withUnsafeBytes {
        md_parse_v1($0.bindMemory(to: UInt8.self).baseAddress, UInt64($0.count), &options, &snapshot)
    }
    if value == 0 { precondition(md_snapshot_free_v1(snapshot.handle) == 0) }
    return value
}
func checkZero() throws {
    guard let (snapshots, tokens) = CoreProbe.live() else { throw CoreFailure.invalidSnapshot }
    try require(snapshots == 0 && tokens == 0, "Native owners did not return to zero")
}
func selfTest(_ large: Data) throws -> [String: Any] {
    let negotiated = try Core.abi()
    try require(negotiated.probe_hooks == 1, "Self-test requires probe build")
    try checkZero()
    let small = Data("# café\n\n**bold** and `code`\n".utf8)
    var abi = md_abi_info()
    try require(md_get_abi_v1(2, &abi) == 2 && md_get_abi_v1(1, nil) == 1, "ABI errors")
    var options = Core.options(), snapshot = md_snapshot()
    try require(md_parse_v1(nil, 1, &options, &snapshot) == 1, "Null input")
    try require(md_parse_v1(nil, 0, nil, &snapshot) == 1 && md_parse_v1(nil, 0, &options, nil) == 1, "Null controls")
    try require(status(Data([0xff]), options) == 3, "Invalid UTF-8")
    options.version = 2; try require(status(small, options) == 2, "Version mismatch")
    options = Core.options(); options.struct_bytes = 1; try require(status(small, options) == 2, "Size mismatch")
    options = Core.options(); options.max_input_bytes = 1; try require(status(small, options) == 4, "Input limit")
    options = Core.options(); options.max_output_bytes = 1; try require(status(small, options) == 8, "Output limit")
    options = Core.options(); options.max_blocks = 1; try require(status(small, options) == 8, "Block limit")
    options = Core.options(); options.max_input_bytes = 0; try require(status(small, options) == 1, "Invalid limits")
    var token: UInt64 = 0
    try ensureCore(md_cancel_create_v1(&token))
    try require(md_snapshot_free_v1(token) == 7, "Wrong handle class")
    try ensureCore(md_cancel_request_v1(token))
    try require(status(small, Core.options(cancel: token)) == 5, "Pre-cancellation")
    try ensureCore(md_cancel_free_v1(token))
    try require(md_cancel_free_v1(token) == 7 && md_cancel_request_v1(token) == 7, "Stale cancellation")
    try require(status(small, Core.options(cancel: token)) == 7, "Stale parse token")
    options = Core.options()
    var borrowed = small
    try ensureCore(borrowed.withUnsafeBytes { md_parse_v1($0.bindMemory(to: UInt8.self).baseAddress, UInt64($0.count), &options, &snapshot) })
    borrowed.resetBytes(in: 0..<borrowed.count)
    let native = String(bytes: UnsafeBufferPointer(start: snapshot.text, count: Int(snapshot.text_len)), encoding: .utf8)
    try ensureCore(md_snapshot_free_v1(snapshot.handle))
    try require(native?.contains("café") == true && md_snapshot_free_v1(snapshot.handle) == 7, "Borrowed input/double free")
    do { _ = try Core.parseAndCopy(small, injectHostFailure: true); throw CoreFailure.invalidSnapshot }
    catch CoreFailure.injectedHostFailure {}
    try checkZero()
    let retained = try Core.parseAndCopy(small)
    try checkZero()
    try require(retained.blocks.first?.text == "café", "Copied values after native release")
    var memory: [UInt64] = []
    for index in 0..<1000 {
        try autoreleasepool { _ = try Core.parseAndCopy(index % 100 == 0 ? large : small) }
        if index % 100 == 99 { try checkZero(); memory.append(rss()) }
    }
    let concurrent = Results()
    DispatchQueue.concurrentPerform(iterations: 4) { _ in
        for _ in 0..<100 {
            do { _ = try Core.parseAndCopy(small); concurrent.add(0) }
            catch { concurrent.add(99) }
        }
    }
    try require(concurrent.all.count == 400 && concurrent.all.allSatisfy { $0 == 0 }, "Concurrent copies")
    try checkZero()
    var cancellationMS: [Double] = []
    for _ in 0..<10 {
        try autoreleasepool {
            let cancel = try CoreCancellation(), result = Results(), group = DispatchGroup()
            group.enter()
            DispatchQueue.global(qos: .userInitiated).async {
                defer { group.leave() }
                do { _ = try Core.parseAndCopy(large, cancellation: cancel); result.add(0) }
                catch CoreFailure.status(let value) { result.add(value) }
                catch { result.add(99) }
            }
            let deadline = coreTime() + 5
            while CoreProbe.active() == 0 && coreTime() < deadline { usleep(100) }
            try require(CoreProbe.active() == 1, "Native parse entry not observed")
            let start = coreTime(); try ensureCore(cancel.request())
            try require(group.wait(timeout: .now() + 5) == .success, "Cancellation deadline")
            cancellationMS.append((coreTime() - start) * 1000)
            try require(result.all == [5], "In-flight cancellation result")
            cancel.close()
        }
        try checkZero()
    }
    try require(CoreProbe.panic() == 6, "Panic containment")
    _ = try Core.parseAndCopy(small)
    try checkZero()
    return ["passed": true, "sequential_loads": 1000, "large_loads": 10, "concurrent_loads": 400,
            "cancellation_after_entry_ms": cancellationMS, "resident_bytes_after_batches": memory,
            "live_snapshots": 0, "live_cancellations": 0, "error_cases": "ABI, null controls, UTF-8, limits, stale/wrong/double handles",
            "host_exception_cleanup": true, "copied_values_survive_free": true, "input_not_borrowed": true,
            "panic_contained_and_next_call_passed": true]
}

func normalCheck() throws -> [String: Any] {
    let abi = try Core.abi()
    try require(abi.probe_hooks == 0, "Normal package must disable probe hooks")
    let small = Data("# Package café\n\nbody\n".utf8)
    for _ in 0..<100 {
        let copy = try Core.parseAndCopy(small)
        try require(copy.blocks.first?.text == "Package café", "Normal copied values")
    }
    try require(status(Data([0xff]), Core.options()) == 3, "Normal UTF-8 status")
    var options = Core.options(); options.version = 2
    try require(status(small, options) == 2, "Normal version status")
    options = Core.options(); options.max_output_bytes = 1
    try require(status(small, options) == 8, "Normal output limit")
    let cancel = try CoreCancellation()
    try ensureCore(cancel.request())
    try require(status(small, Core.options(cancel: cancel.id)) == 5, "Normal cancellation")
    cancel.close()
    try require(cancel.request() == 7, "Closed host cancellation owner")
    do { _ = try Core.parseAndCopy(small, injectHostFailure: true); throw CoreFailure.invalidSnapshot }
    catch CoreFailure.injectedHostFailure {}
    let after = try Core.parseAndCopy(small)
    try require(after.blocks.first?.text == "Package café", "Normal post-error recovery")
    return ["passed": true, "loads": 100, "typed_errors": true, "pre_cancelled": true,
            "host_exception_recovery": true, "probe_hooks": 0]
}

@main struct ConsoleMain {
    static func main() {
        do {
            let args = CommandLine.arguments
            let abi = try Core.abi()
            var output: [String: Any] = ["abi": abi.version, "pointer_bytes": abi.pointer_bytes,
                "options_bytes": abi.options_bytes, "block_bytes": abi.block_bytes,
                "snapshot_bytes": abi.snapshot_bytes, "probe_hooks": abi.probe_hooks]
            if args.count > 1 && args[1] == "--normal-check" {
                output["normal_check"] = try normalCheck()
            } else if args.count > 2 && ["--memory-native", "--memory-copy"].contains(args[1]) {
                let data = try Data(contentsOf: URL(fileURLWithPath: args[2]))
                var memory: [UInt64] = []
                for _ in 0..<20 {
                    try autoreleasepool {
                        if args[1] == "--memory-native" { try require(status(data, Core.options()) == 0, "Native memory parse") }
                        else { _ = try Core.parseAndCopy(data) }
                    }
                    try checkZero(); memory.append(rss())
                }
                output["resident_bytes_after_large_loads"] = memory
                output["memory_mode"] = args[1]
            } else if args.count > 2 && args[1] == "--self-test" {
                output["self_test"] = try selfTest(Data(contentsOf: URL(fileURLWithPath: args[2])))
            } else if args.count > 2 && args[1] == "--contract" {
                let document = try Core.parseAndCopy(Data(contentsOf: URL(fileURLWithPath: args[2])))
                output["headings"] = document.blocks.filter { $0.kind == 1 }.map { ["level": $0.headingLevel, "text": $0.text] as [String: Any] }
            } else if args.count > 1 && args[1] != "--abi" {
                let data = try Data(contentsOf: URL(fileURLWithPath: args[1]))
                let iterations = args.count > 2 ? Int(args[2]) ?? 0 : 5
                try require(iterations > 0, "Invalid iterations")
                var rows: [[String: Any]] = []
                for _ in 0..<iterations {
                    let start = coreTime(), document = try Core.parseAndCopy(data)
                    rows.append(["core_parse_model_ms": document.coreMS, "native_call_ms": document.nativeCallMS,
                                 "copy_decode_ms": document.copyDecodeMS, "end_to_end_ms": (coreTime() - start) * 1000,
                                 "blocks": document.blocks.count, "text_bytes": document.textBytes,
                                 "table_bytes": document.tableBytes, "resident_bytes": rss()])
                }
                output["rows"] = rows
            }
            if let (snapshots, tokens) = CoreProbe.live() { output["live_snapshots"] = snapshots; output["live_cancellations"] = tokens }
            let bytes = try JSONSerialization.data(withJSONObject: output, options: [.sortedKeys])
            print(String(decoding: bytes, as: UTF8.self))
        } catch {
            fputs("Swift core probe failed: \(error)\n", stderr); exit(1)
        }
    }
}
