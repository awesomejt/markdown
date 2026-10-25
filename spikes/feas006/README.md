# FEAS-006 shared-core boundary spike

Windows portion evaluated; native macOS/Linux host bindings and packages remain
required before FEAS-006 closes. This is an experimental ABI/model, not the
production semantic contract or an accepted stack.

## Build and verify on Windows

Requires native x64 Windows, Rust/MSVC, .NET 10, Visual Studio C++ tools, the
FEAS-003 WinUI dependencies and the fixture Python environment. From Git Bash
at the repository root:

```sh
.build/feasibility-venv/Scripts/python.exe spikes/feas006/build_windows.py
cargo test --manifest-path spikes/feas006/rust/Cargo.toml --locked --features probe-hooks --target-dir build/feas006/rust
cargo clippy --manifest-path spikes/feas006/rust/Cargo.toml --locked --all-targets --features probe-hooks --target-dir build/feas006/rust -- -D warnings
.build/feasibility-venv/Scripts/python.exe spikes/feas006/measure_windows.py
.build/feasibility-venv/Scripts/python.exe spikes/feas006/package_windows.py
```

`measure_windows.py` requires a new output directory; use `--output` for another
run. Builds, raw metrics, PDBs, packages and panic-test stderr stay under ignored
`build/feas006/`. Rust dependencies and WinUI NuGet dependencies are locked.
No installed toolchains, global configurations, signing identities or other
repositories are changed by these commands.

The builder produces two Rust DLLs: `rust/release/` enables explicit
`probe-hooks`, while `rust-standard/release/` excludes those exports. The console
and WinUI test outputs use the probe DLL; the development ZIP gets the normal
DLL. Its packager performs a fresh framework-dependent publish, copies license
files from locked Cargo/NuGet dependencies, and never signs/registers/installs.

## Boundary and ownership

The [C header](include/md_core.h) and [Rust crate](rust/src/lib.rs) define ABI v1.
It is native-endian, naturally aligned POD within one process. Windows x64
layouts are options 40 bytes, block 48 bytes and snapshot 56 bytes; callers
negotiate version, pointer width and sizes before parsing. Other architectures
must verify their layouts natively instead of assuming those values.

Normal exports:

- `md_get_abi_v1`: version/layout negotiation.
- `md_parse_v1`: borrowed UTF-8 input capability to one owned immutable snapshot.
- `md_snapshot_free_v1`: release through the same DLL/allocator that created it.
- `md_cancel_create_v1`, `md_cancel_request_v1`, `md_cancel_free_v1`: opaque,
  thread-safe cooperative cancellation capabilities.

Snapshots contain a UTF-8 payload and block-value table with snapshot-local IDs,
kind, heading level, source byte range and text slice offsets. The C# caller
bulk-copies both buffers, validates ranges, decodes strings, then frees the
native snapshot through `SafeHandle` before constructing UI objects. The C++
caller uses RAII and its own copied buffers. No parser arena, borrowed input
reference, native filesystem path, UI object or allocator ownership crosses
the boundary. There are no per-block FFI calls.

Input remains immutable through the call; options/output must be valid,
aligned, non-overlapping buffers. Non-null invalid pointers, racing free against
reads and mutation through returned const pointers violate the C contract and
are not made safe by a panic handler. Handles are opaque library-instance-local
values, never persistent IDs. Zero is reserved; stale/double/type-mismatched
handle operations are rejected. Native input is capped at 16 MiB and logical
output at 64 MiB, with a caller block limit. These are not total process-memory
or CPU-time quotas. Allocation failure/abort/access violations are not covered
by Rust panic containment.

| Status | Value | Meaning |
| --- | ---: | --- |
| Ok | 0 | Snapshot created or operation completed |
| BadArgument | 1 | Null control pointer or invalid limits |
| VersionMismatch | 2 | Unsupported ABI or options size |
| InvalidUtf8 | 3 | Input is not UTF-8 |
| InputLimit | 4 | Input exceeds the negotiated cap |
| Cancelled | 5 | Cancellation observed before publication |
| Panic | 6 | Rust unwind caught inside the export |
| InvalidHandle | 7 | Unknown, released or wrong-class handle |
| OutputLimit | 8 | Logical payload/table or block cap exceeded |

On success, pointers remain valid until release. Callers ignore output data on
errors. No user document contents are written to diagnostic messages. Parse/model
construction happens outside the registry lock; only handle publication/release
and token lookup are serialized. A running parser holds its own `Arc` reference
to the cancellation flag. Polling is cooperative at event boundaries and cannot
interrupt a parser's internal work before its next event.

The model flattens inline formatting and does not yet implement edit-stable IDs,
complete inline/source mappings, typed links/images, metadata properties,
diagnostics, search, incremental updates or production resource policy. HTML and
math are inert readable source; links/images are labels. The initial inline-math
outline mismatch was fixed by preserving its source delimiters, without changing
the frozen fixture expectations. Outline checks are a subset of semantic
conformance, not a full parser/model pass.

## Windows hosts and measurement

The WinUI project compiles the FEAS-003 control source with `FEAS006`; its normal
Markdig path is unchanged. This build excludes the Markdig adapter/dependency,
loads the Rust DLL from the application directory and runs parsing/copy/decode
on a worker. An optional `FEAS006_CORE_DLL` environment variable selects a
trusted explicit DLL path for diagnostic comparisons. No global DLL search path
is modified. A Cancel button/window-close hook requests cancellation.

The inherited surfaces and selection/navigation limitations are unchanged.
Worker heartbeat ticks and zero native live counts before UI construction
distinguish parsing from layout. `--cancel-before-parse` exercises the GUI's
typed cancellation path. It does not make synchronous WinUI layout cancellable.

The console host reports core parse+flat-model time, whole native-call time,
managed buffer-copy/decode/materialization and end-to-end return time. The core
timer includes parser traversal and snapshot assembly, not just parsing; native
call time additionally includes UTF-8 validation, publication and P/Invoke
overhead. End-to-end time includes release and first-use binding/JIT cost.
The native console copies UTF-8/table values without materializing C# strings,
so its copy time is not an equivalent-work language comparison.

`measure_windows.py` records five iterations per input in each console host,
eight outline contracts, and 41 GUI trials: 24 semantic, 12 virtualized performance,
four bounded/unbounded continuous-control comparisons and one pre-cancellation.
Fresh processes are not cold-cache launches; render callbacks are not GPU
presentation timestamps. Markdig/Rust projections differ, so compare separately
measured parser/model stages rather than claiming a controlled language race.

Probe-only exports expose live-handle/active-parse counters, a fixed-message
panic test and, on Windows x64, an intentional breakpoint. The self-test performs
1,000 load/free iterations (including ten 5 MiB documents), 400 concurrent small
loads, host-exception cleanup, retained managed-value checks, and ten cancellation
requests after observing native entry. Memory samples follow forced managed GC;
zero counters and stable samples are evidence, not proof against every leak.

`build/feas006/native/DebugFfiProbe.exe CLIENT_EXE CORE_DLL OUTPUT_JSON` uses the
FEAS-003 Win32/DbgHelp harness in Rust-DLL mode. It resolves a breakpoint in a
Rust export reached from C++, then terminates that owned debuggee. This proves
native Rust source/PDB availability, not mixed managed/Rust stepping in an IDE.

## Next native-host acceptance

Carry this source, locks, header, measurements and handoff through Git; do not
transfer Windows binaries or environments. On macOS build the crate natively,
add a Swift/AppKit value/lifetime adapter and local development package, and
measure conversion, cancellation, repeated sessions and source debugging. On
Linux prove the same facade from the native GTK host rather than assuming a
direct Rust call proves the C boundary. Recheck panic strategy, symbol visibility,
layout and allocator behavior on each host. Keep FEAS-006 partial until both
host reports and package evidence exist; GATE-001 remains unaccepted.

References: [Rust FFI](https://doc.rust-lang.org/nomicon/ffi.html),
[.NET native interop](https://learn.microsoft.com/en-us/dotnet/standard/native-interop/best-practices).
