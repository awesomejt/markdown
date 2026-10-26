# Agent Session and Platform Handoff

Updated: 2026-10-25, FEAS-007 completion. Source baseline: `6847fb4`
(`FEAS-006 - Linux feasibility done`) plus the uncommitted FEAS-007 matrix,
validator, documentation and project-state closeout. Preserve the checked-in
FEAS-006 source, locks, measurements and reports; never transfer build products
or environments between platforms.

This file carries concise, commit-safe context between sessions. It does not
replace [project.yaml](project.yaml) for operational state,
[TODO.md](TODO.md) for acceptance criteria, [MEMORY.md](MEMORY.md) for durable
context, or [agents/Decisions.md](agents/Decisions.md) for accepted decisions.
Always inspect current state and Git status rather than relying on the revision
observed while this handoff was written.

## Resume here — GATE-001

FEAS-007 is complete. The matrix is
[Platform-Capability-Evaluation.md](docs/Platform-Capability-Evaluation.md):
30 atomic capabilities for Windows, macOS, Linux and iOS/iPadOS. Each cell has
a status, evidence level, fallback and named next check, backed by a source
catalog of measured reports, official HTTPS references and explicit gaps.
“Supported” means a feasible platform mechanism, never implemented product
support. Validate any edit with `python3 tools/validate_capabilities.py` and
`python3 tools/test_validate_capabilities.py`.

The next dependency-ready task is **GATE-001**. It is an owner decision, so
don't accept it on the owner's behalf. Present the matrix's “Implications for
GATE-001” section: permission for a non-UI web engine (Mermaid, KaTeX); Go as
a second language for D2; the Windows baseline, since `PrintManager` is Windows
11 only; Mac sandboxed versus Developer ID scope; and Linux package formats.
Together with the open questions in `docs/Product.md`, these drive the baseline.

Open evidence gaps that remain Deferred: macOS network-volume FSEvents, App Nap
throttling, the iOS child-process limit, Apple native SVG decoding, Windows PDF
export, Flatpak helper behavior, mixed/fractional scaling everywhere and every
iOS/iPadOS measurement.

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
controls still stall, and cross-row selection/UIA reading order remain open.

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
