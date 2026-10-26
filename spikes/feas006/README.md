# FEAS-006 shared-core boundary spike

Windows, macOS, and Linux portions are evaluated. This is an experimental
ABI/model and feasibility package set, not the production semantic contract or
an accepted stack. See the platform reports and checked-in result summaries for
durable findings; FEAS-007 and GATE-001 remain separate work.

## Build and verify on macOS

Use native arm64 macOS with Xcode/Swift, Rust and the fixture Python
dependencies. Source is under `macos/`; the AppKit surface reuses FEAS-004
through `-D FEAS006`, excluding Swift Markdown from this build.

```sh
python3 spikes/feas006/build_macos.py
python3 spikes/feas006/measure_macos.py
python3 spikes/feas006/package_macos.py > build/feas006/macos/package.json
python3 spikes/feas006/verify_package_macos.py build/feas006/macos/package.json --output build/feas006/macos/package-checks
```

The builder uses Cargo from PATH, or this checkout's earlier isolated
`build/feas002-macos/toolchains/` when PATH has no Cargo. No global toolchain
configuration is changed. Release Rust has unwind containment and debug info;
the builder sets a relocatable `@rpath/libfeas006_core.dylib` identity at link
time. Probe and normal libraries are separate. Swift hosts use language mode 5
with Swift 6.4, complete concurrency diagnostics and warnings-as-errors. This
does not establish a full Swift 6 migration. AppKit uses explicit main-actor
isolation, including assertions for timers scheduled on its main run loop.

`macos/CoreInterop.swift` imports the C header rather than duplicating struct
layouts. It negotiates sizes, validates values, bulk-copies text/table buffers,
materializes Swift strings and uses `defer` for same-library snapshot release
on success or conversion failure. A synchronized cancellation owner stays alive
through parsing; native snapshots/tokens are released before UI construction.
The normal binary locates optional probe exports dynamically and does not
require them. Handles/pointers never escape into the Swift value model.

The AppKit host opens a loading window, waits for its first main-loop heartbeat,
then reads/parses/copies on a worker. Cancel and close-during-load request
cooperative cancellation; closed-window results are discarded. Attributed
conversion and native layout still run on the main thread and are not made
cancellable by the core boundary. TextKit 1/2 and bounded-row limitations remain
those of the native surfaces; the flattened Rust projection differs from the
old Swift Markdown projection, so timing is not an equivalent-renderer race.

Useful single runs, from the root:

```sh
build/feas006/macos/probe/SwiftCoreProbe --self-test build/fixtures/performance/prose-5mib.md
build/feas006/macos/probe/MacCoreProbe tk2 fixtures/markdown/foundation/gfm-tour.md \
  --automated build/feas006/macos/example.json
build/feas006/macos/probe/MacCoreProbe tk2 build/fixtures/performance/prose-5mib.md \
  --automated build/feas006/macos/cancel.json --cancel-on-entry
```

The matrix requires a new output directory (`--output` selects another). It
checks all eight outlines, records five console iterations per semantic/performance
input, and retains GUI failures/timeouts and partial checkpoints. Timings are
authored-entry/draw callbacks, not dyld-inclusive cold launch or GPU presentation.

Memory diagnosis is separate from normal timing runs:

```sh
build/feas006/macos/probe/MemoryDiagnostics native build/fixtures/performance/prose-5mib.md
build/feas006/macos/probe/MemoryDiagnostics copy build/fixtures/performance/prose-5mib.md
leaks --noContent --atExit -- build/feas006/macos/probe/SwiftCoreProbe \
  --memory-native build/fixtures/performance/prose-5mib.md
```

The diagnostic records unmodified RSS/malloc samples, then requests allocator
pressure relief as a diagnostic only. The pressure-relief call is not used by
the UI or benchmark path and is not an accepted production remedy. A `--pause`
argument after the input lets `vmmap --summary PID` inspect that owned process.
Zero counters and a leak scan are bounded evidence, not a universal leak proof.

The packager creates fresh local bundles/ZIP with the **normal dylib**, header,
Swift console, fixture provenance and locked Rust dependency licenses. Console
rpaths resolve an adjacent library; `.app` rpaths resolve `Contents/Frameworks`.
No build-directory library path or Swift Markdown runtime is needed. Only default
linker ad-hoc signatures are present; identity signing, resource sealing,
notarization, installation and publishing are not performed. The declared
macOS 13 floor is unverified; native evidence names the tested OS/architecture.

The macOS checkpoint is complete. Do not run Windows builders here or substitute
Mac source for another platform's native evidence.

## Build and verify on Linux

Use native x86-64 Linux with Rust, a C compiler, GTK 4 development files,
GDB, ELF tools and the fixture Python dependencies. The gtk-rs host loads the
exported C ABI at runtime; it does not link to or call the Rust crate directly.

```sh
python3 spikes/feas006/build_linux.py
build/feas006/linux/probe/feas006-linux-host \
  --self-test build/fixtures/performance/prose-5mib.md
python3 spikes/feas006/measure_linux.py
python3 spikes/feas006/package_linux.py > build/feas006/linux/package.json
```

The host defaults to `libfeas006_core.so` beside its own executable. `--core`
accepts an explicit diagnostic path for repository tooling only; packages and
relocation checks do not use it or a global loader override. Probe and normal
libraries are built separately, and only the normal library is packaged. The
package script rejects probe exports, copies dependency licenses from both
locked Rust graphs, checks ELF dependencies, relocates to a fresh temporary
directory, and runs console plus GTK smokes there. Packaged ELF copies have
DWARF removed and are scanned for checkout-path bytes; unstripped debug builds
remain separately under ignored `build/feas006/linux/debug/`. Every relocated
command runs without `LD_LIBRARY_PATH`. System GTK remains an
external runtime dependency; no distro floor, installation, signing, sandbox,
or publishing behavior is established.

GTK parsing/copying runs on a worker. The checkpoint records a
`host-copy-released` phase before synchronous `GtkTextBuffer` construction, so
an externally retained layout timeout does not hide successful ownership
release. A `first-useful-tick` is a GTK main-loop callback, not GPU presentation
or cold launch. The monolithic text surface retains FEAS-005's known limits and
is not the LNX-001 virtualized renderer. The in-flight cancellation case passes
only when probe counters observe native entry and the parse returns `Cancelled`.

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

## Evaluation status and next gate

Carry source, locks, header, checked-in summaries and reports through Git; do
not transfer binaries or environments between platforms. Native checkpoints
are recorded in the
[Windows](../../docs/Shared-Core-Windows-Evaluation.md),
[macOS](../../docs/Shared-Core-macOS-Evaluation.md), and
[Linux](../../docs/Shared-Core-Linux-Evaluation.md) reports. FEAS-006 establishes
feasibility on the named hosts only. FEAS-007 must reconcile the platform
capability matrix, and GATE-001 must accept or reject the production language,
model, facade, native stacks, budgets and platform scope.

References: [Rust FFI](https://doc.rust-lang.org/nomicon/ffi.html),
[.NET native interop](https://learn.microsoft.com/en-us/dotnet/standard/native-interop/best-practices).
