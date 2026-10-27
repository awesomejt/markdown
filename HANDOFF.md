# Agent Session and Platform Handoff

Updated: 2026-10-27, CORE-002A PR #2 review fixes and authorized merge preparation.
Source baseline: `8a164ad` on main plus CORE-002A changes.
Preserve intended source, locks, specs/goldens, docs and state together. Do not
copy environments, build products, private runtime state or user documents.

## Resume here — CORE-002A semantic model

CORE-001A is completed via owner merge of [PR #1](https://github.com/awesomejt/markdown/pull/1)
as `8a164ad`, containing corrected head `24994fc`. Six known Grok findings were
addressed and all checks passed; no fresh bot approval is inferred from its
review-request comment. The merged operational closeout travels in this phase.
Aggregate CORE-001 still waits for the native shell CI milestones.

Selected task: **CORE-002A**, branch `feat/core-002a-semantic-model`.
Scope: owned model v1, strict source/graph/intent validation, schema and goldens.
No Markdown parser, identity matching, sessions, FFI or native UI implementation.
Read [the contract](specs/semantic-model/v1/README.md),
[development checks](docs/Development.md) and [dependency inventory](docs/Dependencies.md).

The immutable Document boundary requires matching exact UTF-8 source length/
SHA-256 and caller limits. DTOs remain unchecked until validation. Golden
fixtures cover all 24 node kinds, NFC/NFD and CRLF, tables/tasks/footnotes,
metadata/intents/diagnostics and unsupported source fallbacks. Source bytes are
pinned with `.gitattributes`. Identity stability is a lineage contract; its
algorithm remains CORE-004A. Source hashes are not signatures, and resource
intents are not grants or complete sanitization.

The default build uses serde/serde_json and sha2; optional feature `schema`
adds schemars. Four exact transitive license declarations were reviewed and
added to the policy, including unicode-ident's additional Unicode notice.
All resolved packages and pins are in Cargo.lock and the inventory; no new
implementation language or parser dependency is introduced.

[PR #2](https://github.com/awesomejt/markdown/pull/2) carries the model.
Grok's first review found one concern and three nits; all four are addressed:
strict empty-struct link/resource variants reject extra fields without changing
wire shape/schema; model JSON reads explicitly use UTF-8; validation error
paths are built lazily; additional license declarations are scoped to reviewed
package versions. Regressions cover strict `invalid_json` failures, lazy paths
and rejection of unreviewed packages/versions/expressions. The owner authorized
pushing fixes, merging PR #2, syncing main and deleting the feature branch.

Mac full checks and an EncodingWarning-as-error model validation pass. Required
hosted checks and a fresh `/grok review` still must be inspected for the final
fix head. Review request receipts and disposition belong in PR discussion.
The GitHub connector lacks write permission; authenticated `gh` is the fallback.
After the authorized merge, reconcile live state through the helper and carry
post-merge state/documentation closeout in the next feature PR or housekeeping
PR, never a direct main push. CORE-003A is the next shared parser phase only
after verified merge. Native foundation phases are also independently ready.

The review-fix pass finishes partial before push/review; inspect live state and
Git before resuming. No runtime/private artifacts, user source, transcripts or
credentials belong in commits. Preserve early Linux LNX-001D1 and post-MVP
ADV-005 conditions.

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
