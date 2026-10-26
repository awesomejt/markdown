# Agent Session and Platform Handoff

Updated: 2026-10-26, GATE-001 approved and CORE-001A started.
Source baseline: `b95351f` plus approved gate closeout and CORE-001A changes.
Preserve intended source, locks, docs and project state together. Do not copy
environments, caches, binaries, signing identities or private runtime state.

## Resume here — CORE-001A branch and PR

The owner explicitly approved the assembled baseline and advancement in D-018.
GATE-001 is complete and portable-core active. All 36 initial child tasks and
92 edges are materialized. Do not rerun graph expansion or reopen gate choices.
D-019 selects Apache-2.0 for implementation; fixtures keep their original
license/provenance. The early LNX-001D1 proof remains required before
LNX-001E/001F. Advanced engine work stays in ADV-005 after DESK-002.

Selected task: **CORE-001A**, branch `feat/core-001a-workspace-ci`.
PR: [PR #1](https://github.com/awesomejt/markdown/pull/1) (created draft; inspect live status).
Scope: Rust workspace/common checks/dependency-license policy and host-isolated
CI definitions. No product parser/model, native shell, signing or publishing.
Gate documentation/state closeout travels in this feature PR per workflow.

See [development checks](docs/Development.md),
[dependency policy](docs/Dependencies.md),
[accepted baseline](docs/Implementation-Baseline.md) and
[delivery workflow](docs/Delivery-Workflow.md).

Mac local common/core checks pass with the verified isolated Rust 1.99.1 toolchain;
see the documented environment override. The crate has no behavior tests yet;
empty Cargo harnesses are not product validation. Fixture/capability/path checks,
host-selection regressions, license-policy regressions and workflow lint passed.
Hosted runner checks are in progress; Grok has not yet been requested at this
checkpoint. Inspect the latest PR head/checks, make ready only after they pass,
then post `/grok review` once for that head. Review receipts live in PR discussion.
The connected GitHub integration could not create a PR (403); authenticated
`gh` created PR #1 successfully. Use that authorized fallback for write actions.

Closeout snapshot: revision **277**, partial with no active/stale passes
or warnings. CORE-001A intentionally remains working without a lease.
Inspect live state, Git and the PR before resuming. Use
`gh pr view feat/core-001a-workspace-ci --json url,state,headRefOid`.
Before review, keep the PR draft until required checks pass, then post a new
conversation comment exactly `/grok review`. Record receipts/review discussion
in the PR to avoid changing the source head solely to log a review. Leave a
partial pass without a live lease during long waits; task completion requires
an owner-authorized merge. No dependent work starts merely because code is ready.

## Feasibility evidence retained for implementation

[Decisions](agents/Decisions.md) and [the baseline](docs/Implementation-Baseline.md)
carry the accepted stack and conditions. All prototype reports remain dated
evidence, not production contracts. Preserve Mac repeated-session RSS retention,
Windows Narrator/UIA defects, Linux screen-reader proof and the unrun native
packaging/accessibility/performance cases. FEAS-009 closed by owner-approved
deferral, not by passing its unrun checks. The engine report and ADV-005 retain
them. The remaining historical host notes below are reference only.

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
