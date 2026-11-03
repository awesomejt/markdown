# Windows core package and adapter (WIN-001B)

Baseline: `203b67c` (`develop` after PR #65).
This phase adds a native Windows consumer of the production
[C ABI](../specs/ffi/README.md) to the [Windows 11 WinUI 3 Native AOT shell](../apps/windows/README.md),
and enables the MSVC C package smoke (`tests/c/smoke.c`) on Windows in `tools/check.py`.
Rust core source, the public header, schemas, goldens and the core dependency
graph are unchanged. The app validates the linked ABI and package identity at
startup and its `--smoke` mode parses through the adapter; the window remains
the WIN-001A shell. Native file opening and reload are WIN-001C, and document
rendering starts in WIN-001D.

## Package and linking

`apps/windows/tools/package_core.py --configuration <debug|release>` builds
`markdown-ffi` with `model-v2`, `panic=unwind`, `/PDBALTPATH:markdown_ffi.pdb`
and the pinned Rust 1.99.1 for `x86_64-pc-windows-msvc`, into
`build/windows/rust`. It stages, under the ignored
`build/windows/core-package/<configuration>/`, the shared library
`bin/markdown_ffi.dll` (`3,554,304` bytes debug, `1,294,848` bytes release),
the import library `lib/markdown_ffi.dll.lib` (`7,146` bytes), the exact public
header `include/markdown_core.h`, the Apache license, a `manifest.json`
(target, models, profile, toolchain, commit, header/library/import-library/lock
hashes and exports), a two-line `identity` file and a generated C# constant
file `PackageIdentity.g.cs`. The package directory is replaced atomically
from `.staging-*`, so a failed run never leaves a partial package. No binary or
generated package is committed.

The PE32+ export directory audit parses `bin/markdown_ffi.dll` directly in
Python and requires the DLL to export the exact 24 `md_` functions declared in
`core/include/markdown_core.h` and nothing else. The build remaps checkout,
Cargo home and Rust sysroot prefixes and sets a relative PDB path
(`/PDBALTPATH:markdown_ffi.pdb`), and the audit verifies that neither the
checkout path nor the Cargo home path leaks into the DLL bytes.

The C# Native AOT host (`apps/windows/Markdown.Windows.csproj`) maps its
MSBuild `$(Configuration)` (`Debug` -> `debug`, `Release` -> `release`),
compiles `PackageIdentity.g.cs` from the matching package directory and copies
`bin/markdown_ffi.dll` alongside `Markdown.Windows.exe` in the output and
publish directories. At startup, `CoreRuntime` registers a `DllImportResolver`
that loads `markdown_ffi.dll` exclusively from `AppContext.BaseDirectory`,
resolves the loaded module handle in the process via `GetModuleHandleExW` and
`GetModuleFileNameW`, verifies that the loaded module path matches
`AppContext.BaseDirectory\markdown_ffi.dll`, and compares its on-disk SHA-256
and build configuration against the compiled-in `CorePackageIdentity` constants
before calling `md_abi_negotiate`. A debug host cannot load a release DLL, a
release host cannot load a debug DLL, and a DLL on `PATH` or the working
directory cannot shadow the application bundle's copy.

In parallel, `core/crates/markdown-ffi/tests/c_abi.rs` now locates MSVC
(`vcvars64.bat` via `vswhere.exe`, or `cl.exe` on `PATH`), compiles
`core/crates/markdown-ffi/tests/c/smoke.c` with `/std:c11 /W4 /WX /O1`, links
it against the staged `markdown_ffi.dll.lib`, and runs both `smoke layout` and
`smoke run` against the staged `markdown_ffi.dll` (verified via
`GetModuleHandleExW` on `md_status_name(MD_OK)`). `tools/check.py` sets
`MD_REQUIRE_C_SMOKE=1` on all platforms, including Windows.

## Adapter and ownership

`apps/windows/Core/` (`CoreFfi.cs`, `CoreRuntime.cs`, `CoreDocument.cs`,
`CoreSession.cs`) is a UI-free namespace inside the Windows host with no
`Microsoft.UI` or `Windows` dependencies. `CoreFfi` hand-writes the C ABI
records (`[StructLayout(LayoutKind.Sequential)]`) and P/Invoke declarations
(`CallingConvention.Cdecl, ExactSpelling = true`) under D-009, including the
D-022 find, reading-stats and workspace-search record layouts and size
constants. `CoreRuntime.Validate()` negotiates ABI v1 and requires model v2,
anchor version 1, the 1,024-byte anchor bound, 8-byte pointers, every
negotiated record size (10 structs) and every D-022 size constant (16 structs)
to match the C# `sizeof(...)` layouts. `Program.Main` runs `CoreRuntime.Validate()`
before initializing WinUI and exits with an error on any mismatch.

Every `CoreSession` requests `MD_MODEL_V2` (D-025) with the host limits below.
API status failures throw `CoreException` (`CoreErrorKind.Status` or
`InvalidArgument`); job outcomes (`CoreOutcome`) and parse failures
(`CoreParseFailure`) are separate typed values, and an unknown outcome or a
`Failed` outcome without a failure kind fails closed with
`CoreErrorKind.InvalidSnapshot`.

`CoreDocument` copies the UTF-8 source buffer, string table and every flat
record and index array out of `md_snapshot_view` while the native snapshot
handle is live. Under WIN-001H, it also retains an owned `CoreSnapshot` wrapper (`apps/windows/Core/CoreSnapshot.cs`) around the native `md_snapshot` handle, implementing `IDisposable` to deterministically release the native snapshot. Snapshot disposal and finalization enqueue `md_snapshot_release` non-blockingly to the `markdown-core` worker thread via `CoreRuntime.EnqueueWork`. `CoreSnapshot` and `CoreDocument` expose model-v2 anchor creation (`AnchorAt`, `AnchorForNode`) and resolution (`ResolveAnchor`), mapping native `md_anchor` byte buffers and `md_anchor_resolution` records to managed types for reading position preservation and cross-session restoration. Before allocating any managed array, `SnapshotBudget` prices the
Before allocating any managed array, `SnapshotBudget` prices the
total byte count from the view's counts using checked arithmetic and rejects
any view exceeding `MaxSnapshotCopyBytes` (128 MiB) with
`CoreErrorKind.SnapshotTooLarge`. Non-empty view tables must have non-null,
aligned pointers (`CopySlice<T>`). After copying, `CheckIndices` validates
UTF-8 across `Source` and `Strings` and bounds-checks every cross-table index,
parent/child link, source byte span and string-table range (including UTF-8
character boundary checks), throwing `CoreErrorKind.InvalidSnapshot` on any
violation so a malformed snapshot cannot trigger an out-of-bounds index
exception in presentation code. `CoreDocument` holds only managed arrays and
strings and outlives its `CoreSession` and every native handle. Spans remain
UTF-8 byte offsets and node IDs stay snapshot-local (D-021).

## Worker, admission, cancellation and close

One process-wide background thread (`markdown-core`) runs submission, parse,
flat-view encode, managed copy and every `md_session_release`, never the WinUI
dispatcher thread. At most two parse calls are admitted **process-wide** across
all windows and sessions (`CoreRuntime.MaxAdmitted = 2`: one running, one
queued); a third concurrent call fails immediately with `CoreErrorKind.Busy`
rather than queuing unbounded input buffers.

`CoreSession.Parse` takes a completion callback that runs exactly once: on the
worker when the call finishes, or immediately on the cancelling thread when a
queued call is cancelled. Cancelling a queued `ParseRequest` reports
`CoreErrorKind.Cancelled` at once, drops its source and revision-token buffers,
and releases its admission slot before the worker reaches it so a replacement
parse is admitted immediately. Cancelling a running request calls
`md_job_cancel` under the request lock (which also guards `md_job_release`) and
completes with `CoreErrorKind.Cancelled`. `ParseBlocking` waits for the
callback and is refused on the worker thread
(`CoreErrorKind.BlockingWaitOnWorker`), as is `Close()`.

`Close()` detaches the session handle and cancels in-flight work immediately,
then waits for `md_session_release` on the shared worker; concurrent and
repeated callers share one completion. Because the release queues behind any
parse currently running on the shared worker, UI callers use `CloseThen` or
`Dispose()`, both of which detach and cancel immediately without blocking the
calling thread. Queued work that reaches the worker after a close or dispose
sees the detached handle and completes with `CoreErrorKind.SessionClosed`.

## Host limits

| Resource | Limit |
| --- | ---: |
| Session source (`MaxSourceBytes`) | 5 MiB |
| Nodes (`MaxNodes`) | 500,000 |
| Events (`MaxEvents`) | 1,000,000 |
| Copied parser text (`MaxTextBytes`) | 16 MiB |
| Owned snapshot copy (`MaxSnapshotCopyBytes`) | 128 MiB |
| Running heavy operations | 1 |
| Admitted parse calls, including running | 2 |

These match the macOS and Linux hosts. The 5 MiB source ceiling admits the
accepted `prose-5mib.md` profile (461,177 nodes) and all frozen profiles while
bounding dense inputs (`*a` repeated) well below the core's 8 MiB default.
These are per-call and queue limits, **not a whole-app memory bound**.

## Measurements

Measured 2026-11-01 on Windows 11 Pro 26H2 build 26300.9711 (x64), AMD Ryzen 9
5950X (16 cores / 32 threads, 64 GiB RAM), Rust 1.99.1, MSVC 14.51.36231, .NET
SDK 10.0.402 / Native AOT runtime 10.0.13, release core package
(`build/windows/core-package/release/bin/markdown_ffi.dll`, SHA-256
`7cb119bddbce9ab15559a096da7835c151447dcb32fbcf3f34f085172d698b95`) and
release Native AOT host (`build/windows/Release/Markdown.Windows.exe`), warm
filesystem cache and no concurrent build. Five fresh processes parsed the
frozen `prose-5mib.md` (5,242,880 bytes, SHA-256
`43086da373aa098b0a03817236f52c82d7b5383d7564ee41098e75c58246c715`) through
`--core-probe single`. Working set (RSS), peak working set and private bytes
come from `K32GetProcessMemoryInfo` (`PROCESS_MEMORY_COUNTERS_EX`); managed
heap bytes come from `GC.GetTotalMemory` after compacting collection. These are
adapter operation costs, not D-012 launch-to-content measurements.

| Operation, 5 MiB prose (`prose-5mib.md`) | Min ms | Median ms | Max ms |
| --- | ---: | ---: | ---: |
| Native job run (`md_job_run`: parse + v2 model) | 519.4 | 528.5 | 547.9 |
| First flat-view encode (`md_snapshot_get_view`) | 166.4 | 169.0 | 182.2 |
| Owned C# copy + cross-table index/UTF-8 check | 17.7 | 18.2 | 18.7 |
| Total parse + encode + copy | 704.3 | 716.0 | 748.9 |

The owned copy totals `50,086,591` bytes (`47.77 MiB`) across `461,177` nodes,
`24,273` outline entries, `5,242,880` source bytes and `10,247,559` string-table
bytes. Across the five fresh-process `single` runs:

- Process working set (RSS) is `12.1–12.4 MiB` before loading the fixture,
  `17.2–17.4 MiB` after loading the 5 MiB input array, and `184.6–184.7 MiB`
  with both the live native session and the owned `CoreDocument` held (managed
  GC heap `52.89 MiB`, private bytes `189.4–189.5 MiB`).
- Per-process peak working set is `226.6–226.7 MiB` (`237,568,000–237,678,592`
  bytes).
- Closing the native session while retaining the owned `CoreDocument` drops
  working set to `67.4–67.5 MiB` and private bytes to `59.9–60.6 MiB` (releasing
  ~`117 MiB` of native Rust model arenas).
- Dropping the owned `CoreDocument` and compacting the managed heap drops the
  managed GC heap to `5.13 MiB` (`5,375,352` bytes, holding only the 5 MiB
  input buffer and baseline runtime objects), working set to `51.4–51.6 MiB`
  (`53,907,456–54,079,488` bytes) and private bytes to `43.9–44.6 MiB`. All
  native handle counts return to `0,0,0`.

Twenty sequential 5 MiB sessions in one process (`--core-probe repeated
build/fixtures/performance/prose-5mib.md 20`) keep zero native handles after
every close (`0,0,0`):

- Median timings across the 20 iterations are `533.5 ms` parse (`596.4 ms` max),
  `163.3 ms` encode (`190.1 ms` max) and `17.7 ms` copy (`19.7 ms` max).
- Retained working set after session close and document drop is `51.6 MiB`
  after iteration 1 (`54,108,160` bytes), `52.5 MiB` after iteration 10
  (`55,037,952` bytes) and `52.4 MiB` after iteration 20 (`54,996,992` bytes;
  private bytes `43.9 MiB` -> `45.3 MiB` -> `45.9 MiB`).
- The managed GC heap after collection is identical after iteration 1 and
  iteration 20 (`5,378,080` bytes / `5.13 MiB`), confirming zero managed or
  native object accumulation across repeated document lifecycles.

| Fresh-process dense probe, three runs | Result | Duration ms | Peak working set MiB | Peak delta MiB |
| --- | --- | ---: | ---: | ---: |
| 8 MiB `*a` repeated, core defaults (`dense-default`) | `Failed(NodeLimit)` | 883.5–938.8 | 796.9 | 776.5–776.8 |
| 5 MiB `*a` repeated, host limits (`dense-host`) | `Failed(NodeLimit)` | 497.7–520.4 | 502.9 | 485.5–485.8 |

`dense-default` passes zero limits to measure the core's 8 MiB / 1,000,000-node
ceiling; `dense-host` applies `HostLimits.Default` (5 MiB / 500,000 nodes /
1,000,000 events), reducing peak working set by ~`294 MiB` (`796.9 MiB` ->
`502.9 MiB`) and returning working set to `13.4–13.5 MiB` (`private_after_close`
`5.0–5.1 MiB`, handles `0,0,0`) once the session closes.

## Reproduce

From the repository root on Windows 11 x64:

```powershell
python tools/generate_performance_fixtures.py
python apps/windows/tools/check.py
1..5 | ForEach-Object { & ./build/windows/Release/Markdown.Windows.exe --core-probe single build/fixtures/performance/prose-5mib.md | Out-String -Stream }
& ./build/windows/Release/Markdown.Windows.exe --core-probe repeated build/fixtures/performance/prose-5mib.md 20 | Out-String -Stream
1..3 | ForEach-Object { & ./build/windows/Release/Markdown.Windows.exe --core-probe dense-default | Out-String -Stream }
1..3 | ForEach-Object { & ./build/windows/Release/Markdown.Windows.exe --core-probe dense-host | Out-String -Stream }
```
