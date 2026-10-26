# Agent Session and Platform Handoff

Updated: 2026-10-26, FEAS-008 Windows part. Source baseline: `5bc81c4`
(`FEAS-080 - Linux part`) plus the uncommitted Windows half of
`spikes/feas008`, its report and state/doc updates. Preserve the checked-in
source, locks, measurements and reports; never transfer build products or
environments between platforms.

This file carries concise, commit-safe context between sessions. It does not
replace [project.yaml](project.yaml) for operational state,
[TODO.md](TODO.md) for acceptance criteria, [MEMORY.md](MEMORY.md) for durable
context, or [agents/Decisions.md](agents/Decisions.md) for accepted decisions.
Always inspect current state and Git status rather than relying on the revision
observed while this handoff was written.

## Resume here — GATE-001 spikes

GATE-001 is partially decided: D-008 to D-013 in
[Decisions.md](agents/Decisions.md) are accepted. Its remaining items wait on
three owner-added spikes, which `project.yaml` makes block the gate:

- **FEAS-010** — done: keep pulldown-cmark with shared extension passes; see
  [Core-Extension-Evaluation.md](docs/Core-Extension-Evaluation.md).
- **FEAS-008** — done on Linux and Windows.
  - Linux: [Virtualized-Renderer-Linux-Evaluation.md](docs/Virtualized-Renderer-Linux-Evaluation.md)
    recommends a native `GtkListView`.
  - Windows: [Virtualized-Renderer-Windows-Evaluation.md](docs/Virtualized-Renderer-Windows-Evaluation.md)
    and `spikes/feas008/results-windows-2026-10-26.yaml` recommend a native
    WinUI `ListView` in C# Native AOT, with C++/WinRT as the fallback. AOT and
    C++ were 240–283 ms warm and 309–356 ms cold; JIT missed the proposed
    400 ms warm budget; WebView2 was 2.4–12.4× slower at up to 1.4 GiB.
  - Open for WIN-001: Narrator reads each row twice, and whole-tree UIA
    searches time out at 5 MiB.
  - Recommendations only; GATE-001 decides.
- **FEAS-009** — next. Mermaid, D2 and math: an offscreen web engine,
  existing engines and custom Rust implementations. Proposed host plan (not
  yet confirmed by the owner):
  1. **macOS first, as the main pass.** The iOS-fit criterion can only be
     measured there: Xcode, the iOS simulator or a device, JavaScriptCore
     without a view, `WKWebView`, and building D2's Go for iOS. The
     platform-neutral comparisons also belong in this pass: fidelity against
     reference renders, sanitization, licensing, port effort and Rust/Go
     latency.
  2. **Short Windows and Linux passes after it.** The "offscreen web engine"
     option is a different engine on each OS (WebView2, WebKitGTK and
     WebKit/JavaScriptCore), as in FEAS-008. Measure compile latency, memory
     and package cost of each OS's engine and the in-process D2 Go library
     there; don't infer them from the Mac.
  Close the Mac pass as `partial` with that handoff, as FEAS-008 did.

Their acceptance criteria are in `TODO.md`. Measure with the D-012 metric:
launch to presented content, cold and warm, at least five runs with spread.
On Windows, cold runs purge the standby list and so need the owner to run the
harness from an elevated terminal. The native-only document guardrail stays
in force. GATE-001 is `blocked` in
state with a dependency blocker. When all three spikes finish, clear it with
`resolve_blocker` and their evidence, then resume GATE-001: record the open
decisions, set budgets, then rehearse and materialize the delivery-task graph.

## Completed foundation

FEAS-001 through FEAS-006 have native evidence. FEAS-006 now covers the same
experimental Rust C facade on Windows/C#/C++, macOS/Swift and Linux/gtk-rs,
including ownership, typed errors, cooperative cancellation, source debugging,
native UI consumption and local normal-library packages. The evidence task is
complete once the primary finishes its active closeout pass in `project.yaml`.

FEAS-007 has since reconciled these results into the capability matrix. Do not
begin production scaffolding or accept the spike ABI/model: GATE-001 still owns
the core language/parser, semantic contract, interop, native stacks, budgets and
platform-scope decision, with explicit owner acceptance required.

1. Read the repository-required documents and inspect Git status.
2. Inspect `project.yaml` through `python3 tools/project_state.py`; recover only
   genuinely stale work before starting one pass.
3. Confirm FEAS-007 is completed and no active/stale pass remains. If its
   closeout was interrupted, resume that same task before GATE-001.
4. Select GATE-001 only with the owner present to decide, and retain every
   measured platform deviation and failure.

## FEAS-006 Linux checkpoint

The Linux report is
[Shared-Core-Linux-Evaluation.md](docs/Shared-Core-Linux-Evaluation.md), with
[durable measurements](spikes/feas006/results-linux-2026-10-25.yaml) and
[reproduction commands](spikes/feas006/README.md). The native host is a separate
gtk-rs binary: it has no Rust dependency on the core crate and runtime-loads the
exported C symbols from an adjacent library. It negotiates layouts rather than
assuming other hosts' values; the C header, host and core agree on 40-byte
options, 48-byte blocks and 56-byte snapshots on x86-64 Linux.

The host copies and validates values, explicitly releases core snapshots and
tokens, then constructs GTK objects on the main thread. It writes a
`host-copy-released` checkpoint before synchronous `GtkTextBuffer` work.
The final corrected self-test passed 1,000 sequential plus 400 concurrent loads,
ten cancellations after observed native entry, typed errors, conversion-failure
cleanup and panic recovery, with zero live owners. All eight outline contracts,
twelve GTK presentations and two intended cancellations passed without timeout.

Warm console 1/5 MiB prose medians were about 5.5/29.8 ms core/model and
0.6/3.3 ms copy/decode. Fresh GTK processes reached a first useful main-loop
tick in about 277/498 ms; that callback is not GPU presentation or cold launch.
The 1 MiB logical-line case took about 11.4 seconds despite roughly 2 ms of
core/copy work. This reinforces FEAS-005: a monolithic `GtkTextView` is not the
production unbounded renderer. LNX-001 must prove virtualized semantic blocks,
document-wide selection and AT-SPI reading order.

GDB stopped in Rust `md_parse_v1`, resolved source plus gtk-rs host/Rust-core
frames, and continued normally. A fresh normal-library package passed a
100-load lifecycle check, console parse and GTK smoke after external-directory
relocation with `LD_LIBRARY_PATH` removed. It contains licenses for 72 locked
Rust dependency records, no probe exports and no core `DT_NEEDED`. Packaged
ELFs have debug sections removed and are scanned for checkout-root bytes;
separate ignored debug artifacts retain source information. This is same-host
relocation, not clean-machine, distro-floor, installer, sandbox, signing or
publication proof.

Review initially found checkout paths in packaged DWARF, permissive
cancel-on-entry classification, indefinite polling after worker disconnect and
incomplete loader-environment sanitization. All four were fixed and covered by
targeted tests; fresh re-review found no remaining code issue. GTK still emits
nonfatal theme slider-size warnings during automation, and GDB reported an
optional auto-load/debuginfod warning without losing source or frames.

Linux environment: CachyOS, kernel 7.3.0-4-cachyos, glibc 2.44, x86-64,
Ryzen 9 5950X, 64 GiB RAM, RTX 4080, KDE Wayland, Rust/Cargo 1.99.1,
GCC 16.2.1, GDB 18.1, GTK 4.24.1 and Pango 1.58.2.

## Windows and macOS evidence to carry

Windows evidence is in
[Shared-Core-Windows-Evaluation.md](docs/Shared-Core-Windows-Evaluation.md) and
[results-windows-2026-10-25.yaml](spikes/feas006/results-windows-2026-10-25.yaml).
The Rust-owned bulk C ABI passed C# SafeHandle/C++ RAII cleanup, 1,000 sequential
and 400 concurrent loads, ten observed-entry cancellations, panic containment,
all eight outlines, native Rust source/PDB debugging and a normal-DLL package.
Virtualized 1/5 MiB authored-entry callbacks were about 381/459 ms. Continuous
controls still stall. FEAS-008 Windows has since shown document-wide selection,
UIA reading order and Narrator navigation over virtualized rows; see its
report.

macOS evidence is in
[Shared-Core-macOS-Evaluation.md](docs/Shared-Core-macOS-Evaluation.md) and
[results-macos-2026-10-25.yaml](spikes/feas006/results-macos-2026-10-25.yaml).
Swift imports the C header, copies and frees values before AppKit work, and
passes strict concurrency, lifecycle/cancellation, all eight outlines, mixed
Swift/Rust source debugging and relocated normal-dylib bundles. TextKit 2
1/5 MiB entry-to-draw medians were about 217/498 ms. A retained TextKit 1
timeout and synchronous conversion/layout remain.

**Mac memory risk:** twenty native-only 5 MiB parses grew RSS to about 583 MiB
with zero native owners and about 5.1 MiB live malloc usage. `leaks` reported
zero leaked bytes and `vmmap` attributed most growth to empty large malloc
regions; Swift-copy runs retained similar RSS. This is neither a leak proof nor
an acceptable repeated-session budget.

Across all hosts, the flat spike model still has snapshot-local IDs and lacks
complete inline/source mappings, typed links/images, metadata, diagnostics,
search, incremental updates and production resource policy. Cancellation polls
at event boundaries. Unsafe FFI calls rely on the documented pointer, alignment
and lifetime contract. Preserve these limits for GATE-001.

## Workflow and authorization boundaries

The project-state backend portability patch passed native Windows, isolated Mac
and native Linux regression suites on 2026-10-25. On another host, run the
guarded repository patch/check and `--platform-info` before coordination. Do not
hand-edit `project.yaml` or improvise a backend.

After GATE-001, use the small platform-focused phase catalog in
[Implementation-Plan.md](docs/Implementation-Plan.md) and the branch/PR lifecycle
in [Delivery-Workflow.md](docs/Delivery-Workflow.md). Each phase gets a feature
branch and PR, followed by a separate `/grok review` comment. Scoped phase
commits, pushes, PR updates and review comments are authorized; merge, tag,
signing, publishing and production deployment require explicit owner approval.

Keep `markdown-viewer` read-only. Do not add a sibling build/runtime dependency
or shared UI toolkit. No previous fixture, accessibility or debugging exception
broadens current authorization. Never commit environments, binaries, credentials,
signing material, transcripts, raw private paths or user document contents.

Before switching sessions, refresh this handoff with the source commit, live
task/pass state, evidence links, unresolved risks and exact next action. Ignored
build output and `.agents/` handoffs are not substitutes for committed evidence.
