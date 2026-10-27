# Agent Session and Platform Handoff

Updated: 2026-10-27, CORE-002A merged closeout and CORE-003A basic parser.
Source baseline: `e5f1a17` on main plus this phase's changes. Preserve intended
source, locks, authored specs/goldens, docs and state together. Do not copy
environments, build products, private runtime state or user documents.

## Resume here — CORE-003A basic parser

CORE-002A is completed via owner-authorized [PR #2](https://github.com/awesomejt/markdown/pull/2)
merge `e5f1a17`, containing corrected head `75a8bc8`. All four Grok findings
were addressed and all required hosted checks passed. The owner explicitly
waived another review while troubleshooting automation; no bot approval is
claimed. Its state closeout and frozen-model documentation travel in this phase.
CORE-001A was merged in PR #1; aggregate CORE-001 waits for native shell CI.

Selected task: **CORE-003A**, branch `feat/core-003a-basic-parser`.
Read [Parsing.md](docs/Parsing.md), [the model](specs/semantic-model/v1/README.md),
[development checks](docs/Development.md) and [dependencies](docs/Dependencies.md).
The core parses bounded UTF-8 into validated owned v1 snapshots with deterministic
snapshot-local IDs. No source/model schema change, session identity, cancellation,
search, FFI, native UI, I/O or advanced engine is implemented.

Implementation covers basic blocks/inlines, GFM tables/tasks/strike, outline,
exact source/destination spans, literal code and local/external/denied intents.
Raw HTML and advanced fences are inert source fallbacks. Reference links/images
use source fallbacks because definitions are outside v1 occurrence-contained
intent spans. First-position front matter retains its exact span and diagnostic
but has no typed properties yet. Presenters must keep that source readable.

CORE-003B owns reference representation/activation, typed metadata, other
extensions, URI refinement, full conformance/fuzz/property and measured budgets.
The current conservative URI subset denies percent encoding, whitespace,
backslashes, absolute paths and dot segments; native adapters must still enforce
capability containment and safe URI activation. Resource intents grant no I/O.
Parser construction can scan/allocate for bounded source before event limits
apply; these limits are not time/allocator quotas or native performance proof.

The parser dependency is pulldown-cmark 0.13.5 without default HTML/CLI features.
It adds bitflags/unicase to the graph; all 32 resolved packages pass existing
license/source policy. Frozen FEAS and model goldens/schema are unchanged.
An original parser source/JSON golden is in `specs/parsing/basic/`; its exact
source bytes are preserved by `.gitattributes`. An example regenerates output
from deliberately supplied bounded stdin; never commit user snapshots.

Mac full common/core checks pass, including parser/model debug/release/default/
schema builds, Python regressions, Markdown/links and dependency/source policy.
Inspect actual PR head and hosted Linux/macOS/Windows/common checks before
marking ready; then post one exact `/grok review` comment for that ready head.
Assess feedback and request owner authorization before merging. The previous
merge/review exception applied only to PR #2. The connector lacks write access;
authenticated `gh` is the authorized fallback. Keep review receipts in PR notes.

Finish CORE-003A partial with no active lease while waiting for CI/review/merge.
Inspect live state before resuming; partial working status without a lease is
intentional. After verified owner-authorized merge, carry supported state
closeout in the next feature PR; do not push closeout directly to main.
Next shared phases: CORE-003B or CORE-004A after this merge; native foundation
A phases remain independently ready. Preserve Linux LNX-001D1 and ADV-005 gates.

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
