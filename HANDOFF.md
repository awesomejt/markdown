# Agent Session and Platform Handoff

Updated: 2026-10-28, CORE-005B workspace search on `feat/core-005b-workspace-search`.
Source baseline: `ec58c84` on main (PR #9 merge) plus this phase's changes. Preserve intended
source, lock/spec/golden files, docs and state together. Never transfer caches,
environments, build products, private runtime state or user documents.

## Resume here — CORE-005B workspace search primitives

PR #9 (CORE-005A2) merged as `ec58c84`; its state closeout travels on this
branch. CORE-005B adds `markdown_core::workspace` per
[the workspace search contract](specs/workspace-search/README.md). Adapters
feed `WorkspaceEntry` values: an opaque `u64` ID plus a validated
`WorkspacePath`, which is relative and `/`-separated with no dot components,
controls or bidi formatting characters. The core never resolves paths. `find_names`
ranks name-prefix, name and path hits. `ContentQuery::admit` and `search`
work file by file, with a caller-held `ContentProgress` holding file, byte,
per-file and total match budgets and permanent stops. Hits equal find's
matches and add a visible-text snippet, a line number, an anchor and the
file's `SourceInfo` for navigation. Cancellation returns nothing and leaves
progress unchanged. There is no new dependency, model or ABI change. The
owner approved splitting the C ABI exposure into CORE-005B2 (task created,
blocks CORE-005; each 002C needs it). Next: phase PR, `/grok review`,
owner-authorized merge. Then CORE-005B2, CORE-003B3, CORE-003B4 (with the
CORE-003B closeout), then the owner's holistic review before any platform
phase.

## CORE-005A2 C ABI find and reading statistics (merged in PR #9)

PR #8 (CORE-005A) merged as `237cd88`; its state closeout travels on this
branch. CORE-005A2 adds two exports under D-022 per
[the C ABI contract](specs/ffi/README.md#find-and-reading-statistics-core-005a2).
`md_snapshot_find` fills `md_find_match` and `md_match_segment` arrays and an
`md_find_result`. `md_snapshot_read_stats` fills `md_read_section` records and
an `md_read_stats`. Both use two-call `MD_ERR_BUFFER_TOO_SMALL` sizing, and no
records are written on failure. Each new struct has an `MD_*_SIZE` header
constant. The options carry the record element sizes, which are checked on
every call. There is no new handle type, no change to existing items and no
cancel handle. Evidence: Rust parity with `markdown_core::search` over the
corpus (v1/v2), argument, size, panic and concurrency tests, layout and
constant drift checks and the macOS C smoke. The Grok review nit was fixed,
and the owner merged PR #9 as `ec58c84` from reviewed head `f05cbb4`.

## CORE-005A find, word count and section timing (merged in PR #8)

PR #7 (CORE-006A C ABI) merged as `a751f48`; its state closeout is owned by the
orchestrator's project-state pass. CORE-005A adds `markdown_core::search` per
[the search contract](specs/search/README.md): literal find over visible
semantic text (leaf units in reading order, inline boundaries transparent),
exact/atomic source mapping with per-node segments, simple case folding without
locale tailoring or NFC/NFD normalization, approximate grapheme/whole-word
boundaries, KMP matching with a `max_matches` budget, `truncated` flag and
cancellation; word count by a simplified UAX #29 rule (one word per CJK
ideograph); intro plus per-heading own/cumulative timing at 160 wpm (60–600),
matching markdown-viewer `b881034` defaults/exclusions. No new dependency, no
model/ABI change. The owner split FFI exposure into CORE-005A2 under D-022
(ABI v1 grows by new functions only, filling caller-owned arrays; no new
handle types or changes to existing items) and froze the PR #7 flat view as
an ABI v1 representation (D-023). Review findings were fixed before the merge.

## CORE-006A C ABI facade (merged in PR #7)

PR #6 (CORE-004A sessions) merged as `3b1eb4e`; its state closeout travels on
this branch. The owner accepted snapshot-local node IDs with block anchors for
the MVP (D-021). CORE-006A adds crate `core/crates/markdown-ffi` and the
hand-written header `core/include/markdown_core.h` per
[the C ABI contract](specs/ffi/README.md): `md_abi_negotiate` with exact
struct sizes, per-session v1/v2 model choice, nonzero registry handles with one
release each (double release detected), typed `md_status` codes and static
names, `catch_unwind` on every entry point, session/job/snapshot functions,
anchor value copies and a bulk read-only snapshot view (fixed-size records
plus one string table) that round-trips the full v1/v2 model. No new package.
`tools/check.py core` runs the extern tests and a host-`cc` C smoke with layout
comparison; it is required on macOS/Linux and skipped on Windows.
Next: phase PR, `/grok review`, owner-authorized merge; then each host's 001B
consumer (Swift/AppKit, C# Native AOT, gtk-rs) validates CORE-006 on its host.

## CORE-004A sessions (merged in PR #6)

`markdown_core::session` per [the session contract](specs/session/README.md):
`Arc` snapshots, strictly increasing generations, single-point latest-wins
publication, cooperative `CancellationToken` polling, adapter-reported
`Modified`/`Deleted`/`Unreadable` state with opaque revision-token and
exact-byte deduplication, and block anchors resolved as
exact/relocated/nearest/lost. Node IDs stay snapshot-local (D-021).

## PR #5 review (historical)

The owner asked for the [Grok review](https://github.com/awesomejt/markdown/pull/5#pullrequestreview-5671367164)
findings to be fixed on the PR #5 branch before the authorized merge, so the
**CORE-003B2F** scope landed in PR #5:

- V2 URI targets also deny Unicode 16.0 Default_Ignorable_Code_Point values and
  U+2800, literal or percent-encoded. Metadata text keeps its Cf/control guard,
  so that emoji variation selectors, Mongolian FVS and CGJ remain usable
  ([Front-Matter.md](docs/Front-Matter.md), [URI-Policy.md](docs/URI-Policy.md)).
- The v1 span correction for blockquote-continuation destinations is now
  documented, and `tests/parser.rs` has a regression for it.
- V2 emits an info `reference_definition_shadowed` diagnostic for each ignored
  duplicate definition. It is bounded and fails closed, and first-definition
  precedence is unchanged ([Parsing.md](docs/Parsing.md)).

After the merge is verified, carry the CORE-003B2 and CORE-003B2F state
closeout in the next feature PR. Both are satisfied by the PR #5 merge, and
closing them unblocks the B4 and B aggregate gates. Then select the next
eligible task.

## Resume here — CORE-003B2 references and URI targets

CORE-003B1 completed via owner-authorized [PR #4](https://github.com/awesomejt/markdown/pull/4)
merge `0ce353f`, containing corrected head `22146cc`. All four concerns/two nits
were addressed; required CI passed. The owner requested fixes/checks/merge and
cleanup as before, not another bot-response wait; no extra approval is inferred.
Its supported state closeout travels here. The four B children and aggregate/
native gates remain intact; B1 metadata does not complete CORE-003B.

Selected: **CORE-003B2**, branch `feat/core-003b2-references-uri`.
The owner chose additive opt-in model v2 with explicit reference provenance,
preserving v1 API/decoder/schema/goldens (D-020). Read
[the v2 contract](specs/semantic-model/v2/README.md), [URI policy](docs/URI-Policy.md),
[Parsing.md](docs/Parsing.md), [development checks](docs/Development.md) and
[dependency inventory](docs/Dependencies.md). V2 is under phase-PR review and the owner authorized merge with the
review follow-up above. No default model/facade/native format switch.

Enable feature `model-v2` for `parse_markdown_v2`/`v2::Document`. The v1 parser
remains default with reference fallbacks/conservative URI subset. V2 separates
reference definition/destination spans from occurrence spans; supports full,
collapsed, shortcut and image references with parser case-fold/duplicate rules.
All targets are source-proven through a bounded expected-intent reparse, not
merely span geometry: forged destinations and unrelated definition bindings
are rejected. General display text/metadata fidelity is not a source signature.
Both decoders reject the other's wire shape/version. Explicit upgrade retains
legacy denials; old reference fallback nodes need source reparse to become links.

Typed v2 targets separate internal fragments, decoded-once relative capability
paths, canonical external URLs and static denials. Unicode/NFD/encoded spaces,
IDNs and visible parent-relative paths are supported with guards for malformed/
encoded control/Cf values, schemes, authority/userinfo, path separators/dot
obfuscation and Windows device variants. Adapters MUST NOT decode file targets
again, pass them to URL launchers or expand shell/tilde/env syntax. Native file
APIs must enforce approved-root containment through symlinks/reparse points and
revocation/type checks; targets grant no I/O. Remote images remain denied.
Mailto/query/URI edge cases remain intentionally limited; see the policy.

Source mapping now excludes quote continuation markers for multiline inline/
reference link/image destinations. Linked images keep both intents; nested links
remain inert. Known upstream tab-offset cases retain exact whole-body source
fallback and metadata, dropping targets. Proof scan budgets are independent of
published node/event counts; source depth/fact count and fixed 64 MiB fact-string
ceiling bound its extra work. It is not a deadline/allocator quota. Primary
parsing charges retained targets to the copy budget. B4 must measure costs.

Optional url 2.5.8/IDNA/ICU adds 28 packages to the full graph (60 including
schema tooling). V1 plus schema remains 32 and default v1 does not enable URL.
Eighteen package/version Unicode-3.0 exceptions were verified against local
manifests/LICENSE and scoped in policy; source restrictions remain unchanged.
No new installed software beyond normal Cargo fetching is needed. No UI,
advanced engine, FFI, stable identity/session/cancellation or I/O is implemented.

Regressions cover version/migration compatibility, exact source/schema/golden,
forged targets/definitions, references/linked images/quote continuations,
Unicode/percent/IDN/mail/fragment routing, hostile URIs/devices, budget separation
and 8,192 generated v2 inputs, plus existing v1/parser/metadata suites. Independent
read-only review found source-proof, quote mapping, linked-image, colon/authority,
device and budget issues; fixes/regressions passed its isolated re-review with
no remaining blocking correctness/security finding. Full conformance/fuzz/perf
remains B4; required current-head hosted common/Linux/macOS/Windows checks and
the owner authorized merge after the PR #5 findings were fixed on-branch (B2F scope).

Finish this phase partial without an active lease while awaiting verified merge.
The exact `/grok review` request and actual review are on PR #5; authenticated
`gh` is the fallback for connector write limitations. The owner explicitly
authorized PR #5 merge after recording its review findings.
After verified merge, carry state closeout in the next feature PR, not directly
to main. B3/B4 and CORE-004A/native A phases keep their dependencies. Preserve
Linux LNX-001D1 and ADV-005 gates.

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
