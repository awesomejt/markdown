import Foundation
import Darwin

enum CoreFailure: Error { case status(UInt32), invalidSnapshot, injectedHostFailure }
func ensureCore(_ status: UInt32) throws { if status != 0 { throw CoreFailure.status(status) } }
func coreTime() -> Double { ProcessInfo.processInfo.systemUptime }

struct CoreBlock: Sendable {
    let id: UInt64
    let kind: UInt32
    let headingLevel: UInt32
    let sourceStart: UInt64
    let sourceEnd: UInt64
    let text: String
}
struct CoreDocument: Sendable {
    let blocks: [CoreBlock]
    let coreMS: Double
    let nativeCallMS: Double
    let copyDecodeMS: Double
    let textBytes: UInt64
    let tableBytes: UInt64
}

// Only immutable handle identity crosses threads. The core supports concurrent
// request calls; a running worker retains this owner until its parse returns.
final class CoreCancellation: @unchecked Sendable {
    let id: UInt64
    private let lock = NSLock()
    private var released = false
    init() throws { var value: UInt64 = 0; try ensureCore(md_cancel_create_v1(&value)); id = value }
    func request() -> UInt32 {
        lock.lock(); defer { lock.unlock() }
        return released ? 7 : md_cancel_request_v1(id)
    }
    // Call only after the associated parse returns; lock also makes late UI
    // requests harmless. Destruction remains the fallback for thrown paths.
    func close() {
        lock.lock(); defer { lock.unlock() }
        if !released { precondition(md_cancel_free_v1(id) == 0); released = true }
    }
    deinit { close() }
}

enum Core {
    static func options(cancel: UInt64 = 0) -> md_options {
        md_options(version: 1, struct_bytes: UInt32(MemoryLayout<md_options>.size),
                   max_input_bytes: 16 * 1024 * 1024, max_output_bytes: 64 * 1024 * 1024,
                   max_blocks: 1_000_000, cancel_handle: cancel)
    }
    static func abi() throws -> md_abi_info {
        var value = md_abi_info()
        try ensureCore(md_get_abi_v1(1, &value))
        guard value.version == 1, value.pointer_bytes == MemoryLayout<UnsafeRawPointer>.size,
              value.options_bytes == MemoryLayout<md_options>.size,
              value.block_bytes == MemoryLayout<md_block>.size,
              value.snapshot_bytes == MemoryLayout<md_snapshot>.size else { throw CoreFailure.invalidSnapshot }
        return value
    }
    static func parseAndCopy(_ data: Data, cancellation: CoreCancellation? = nil,
                             injectHostFailure: Bool = false) throws -> CoreDocument {
        _ = try abi()
        var options = options(cancel: cancellation?.id ?? 0)
        var snapshot = md_snapshot()
        let start = coreTime()
        let status: UInt32 = data.withUnsafeBytes { bytes in
            md_parse_v1(bytes.bindMemory(to: UInt8.self).baseAddress, UInt64(bytes.count), &options, &snapshot)
        }
        withExtendedLifetime(cancellation) {}
        let callMS = (coreTime() - start) * 1000
        try ensureCore(status)
        // Exactly the library that created the snapshot frees it, on every path.
        // No imported pointer or parser lifetime escapes this function.
        defer { precondition(md_snapshot_free_v1(snapshot.handle) == 0) }
        guard snapshot.handle != 0, snapshot.version == 1,
              snapshot.struct_bytes == MemoryLayout<md_snapshot>.size,
              snapshot.text_len <= options.max_output_bytes, snapshot.block_count <= options.max_blocks,
              snapshot.text_len == 0 || snapshot.text != nil,
              snapshot.block_count == 0 || snapshot.blocks != nil else { throw CoreFailure.invalidSnapshot }
        let copyStart = coreTime()
        let payload = Array(UnsafeBufferPointer(start: snapshot.text, count: Int(snapshot.text_len)))
        let table = Array(UnsafeBufferPointer(start: snapshot.blocks, count: Int(snapshot.block_count)))
        if injectHostFailure { throw CoreFailure.injectedHostFailure }
        var blocks: [CoreBlock] = []
        blocks.reserveCapacity(table.count)
        for (index, block) in table.enumerated() {
            guard block.id == UInt64(index + 1), block.kind <= 5, block.heading_level <= 6,
                  block.source_start <= block.source_end, block.source_end <= data.count,
                  block.text_offset <= payload.count, block.text_len <= UInt64(payload.count) - block.text_offset,
                  let text = String(bytes: payload[Int(block.text_offset)..<Int(block.text_offset + block.text_len)], encoding: .utf8)
            else { throw CoreFailure.invalidSnapshot }
            blocks.append(CoreBlock(id: block.id, kind: block.kind, headingLevel: block.heading_level,
                                    sourceStart: block.source_start, sourceEnd: block.source_end, text: text))
        }
        return CoreDocument(blocks: blocks, coreMS: Double(snapshot.core_ns) / 1e6,
                            nativeCallMS: callMS, copyDecodeMS: (coreTime() - copyStart) * 1000,
                            textBytes: snapshot.text_len,
                            tableBytes: snapshot.block_count * UInt64(MemoryLayout<md_block>.size))
    }
}

// Optional instrumentation is found dynamically. Normal package binaries do
// not import or require any md_probe_* export. Never accept arbitrary DLL paths.
enum CoreProbe {
    typealias Live = @convention(c) (UnsafeMutablePointer<UInt64>?, UnsafeMutablePointer<UInt64>?) -> UInt32
    typealias Active = @convention(c) () -> UInt64
    typealias Panic = @convention(c) () -> UInt32
    private static func lookup(_ name: String) -> UnsafeMutableRawPointer? {
        guard let process = dlopen(nil, RTLD_NOW) else { return nil }
        defer { dlclose(process) }
        // The Rust dylib is a strong link dependency and stays loaded for the
        // process lifetime. Only the temporary lookup handle is closed here;
        // no non-Sendable global pointer is shared between worker threads.
        return dlsym(process, name)
    }
    static func live() -> (UInt64, UInt64)? {
        guard let symbol = lookup("md_probe_live_v1") else { return nil }
        var snapshots: UInt64 = 0, cancellations: UInt64 = 0
        precondition(unsafeBitCast(symbol, to: Live.self)(&snapshots, &cancellations) == 0)
        return (snapshots, cancellations)
    }
    static func active() -> UInt64? {
        guard let symbol = lookup("md_probe_active_v1") else { return nil }
        return unsafeBitCast(symbol, to: Active.self)()
    }
    static func panic() -> UInt32? {
        guard let symbol = lookup("md_probe_panic_v1") else { return nil }
        return unsafeBitCast(symbol, to: Panic.self)()
    }
}
