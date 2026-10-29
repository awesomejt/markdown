# Decisions

Accepted decisions only. Proposed ideas remain in design documents until an
owner or the named project gate accepts them.

## D-001 — Separate native repository

**Date:** 2026-10-24  
**Status:** accepted by repository creation

The native application lives in `markdown`. `markdown-viewer` remains an
independent LAN web application and behavior reference. The native project may
copy versioned fixtures/contracts with provenance but has no sibling-path build
or runtime dependency.

## D-002 — Recovery-safe local project state

**Date:** 2026-10-24  
**Status:** accepted for planning

Operational task/pass state uses the local `project-state/v1` file
`project.yaml`, stored as readable block YAML and mutated atomically through
`tools/project_state.py`. `TODO.md` explains the roadmap; `MEMORY.md` and this
file store durable context and decisions. Concurrency begins at one active
pass.

## D-003 — Desktop-first platform scope

**Date:** 2026-10-24  
**Status:** accepted from owner clarification

Windows, macOS, and Linux desktop applications are the first delivery family.
iOS and iPadOS follow after the desktop core and behavior contracts mature.
Each platform is optimized independently and may have documented capability
differences. Requirements and functionality from `markdown-viewer` are the
starting point; existing implementation code is not a reuse requirement.

## D-004 — One native monorepo

**Date:** 2026-10-24  
**Status:** accepted by owner

Keep the native platform family in one repository with independent application
trees for Windows, macOS, Linux, and iOS/iPadOS. Share requirements, semantic
contracts, fixtures, benchmarks, and only the core code proven useful across
platforms. Do not share the UI layer. Reconsider a split only when ownership,
release governance, licensing, repository scale, or platform divergence makes
atomic cross-platform changes a liability.

## D-005 — Require target-host desktop evidence before the baseline gate

**Date:** 2026-10-24

**Status:** accepted by owner

Complete FEAS-003 on a real Windows host and FEAS-004 on a real macOS host
before shared-core integration, the final capability matrix, or GATE-001.
Cross-compilation, compatibility layers, and simulators may support those
evaluations but do not replace native launch, layout, selection, accessibility,
scaling, lifecycle, debugging, and packaging evidence. The project task graph
enforces this order.

## D-006 — Generate Unicode filesystem test paths

**Date:** 2026-10-25

**Status:** accepted by owner request

Keep committed fixture filenames ASCII and generate Unicode names from manifest
recipes when running filesystem tests. Create NFC and NFD spellings in separate
directories so a checkout never requires normalization-equivalent names to
coexist. Preserve original input bytes, hashes, origin paths and licensing.
Record actual on-disk spelling and canonical-equivalent lookup results on each
host; filesystem observations do not replace native adapter conformance tests.
Disable Git line-ending conversion for the frozen fixture corpus.

## D-007 — Small platform phases with branch and PR review

**Date:** 2026-10-25

**Status:** accepted by owner request

Starting after GATE-001, divide core and application implementation into bounded
delivery phases, grouped by platform where possible to support booting between
hosts. Each phase uses a feature branch and PR. Once ready, request the Grok
review by running the `grok` CLI (amended 2026-10-28 at the owner's request;
it was a `/grok review` PR comment), assess the actual review and address
findings; merge needs owner authorization, which a clean Grok review now
gives under D-029. Scoped phase commits,
pushes, PR updates and review requests are authorized as routine steps of this
workflow.

Use `docs/Delivery-Workflow.md` and the catalog in `docs/Implementation-Plan.md`.
Retain broad state buckets and aggregate milestones; materialize their smaller
execution tasks through supported helper operations at prerequisite gates.
Early native foundation tasks share the portable-core bucket so host batching
works with active-bucket selection. Preserve all-platform gates, native evidence
and concurrency one. This decision changes future workflow, not today's partial
FEAS-006 handoff or the unaccepted production baseline.

## D-008 — Rust core with pulldown-cmark

**Date:** 2026-10-25

**Status:** accepted by owner at GATE-001

The portable semantic core is Rust, using pulldown-cmark as the parser. Comrak
remains the fallback and cross-check oracle; cmark-gfm remains the GitHub
Flavored Markdown reference. The core must support front matter, math, GitHub
alerts and the advanced and custom syntax from `markdown-viewer`. FEAS-010
proves what that takes on pulldown-cmark; if it can't be done well, CORE-003 may
switch to Comrak behind the same interface.

## D-009 — Hand-written versioned C interface

**Date:** 2026-10-25

**Status:** accepted by owner at GATE-001

Apps call the core through a narrow, versioned, hand-written C ABI that hands
over immutable values in bulk copies, as proven by FEAS-006 on all three
desktops. Generated bindings such as UniFFI are not used. The FEAS-006 flat
data model is not the production contract; CORE-002 and CORE-006 replace it.

## D-010 — Native platform stacks

**Date:** 2026-10-25

**Status:** accepted by owner at GATE-001

- macOS: AppKit with TextKit 2 for the document; SwiftUI only for shell
  surfaces such as settings.
- Linux: GTK 4 with gtk-rs; libadwaita optional, not required.
- Windows: WinUI 3. The host language (C# or C++/WinRT) is decided from the
  FEAS-008 comparison.

The document renderer on Windows and Linux depends on FEAS-008.

## D-011 — MVP platform baselines and distribution

**Date:** 2026-10-25

**Status:** accepted by owner at GATE-001

| Platform | MVP target | Distribution | After the MVP |
| --- | --- | --- | --- |
| Windows | Windows 11, x64 | MSIX | ARM64 |
| macOS | macOS 27, Apple Silicon | Developer ID, signed and notarized | Earlier macOS versions; Mac App Store optional |
| Linux | x86-64, KDE Plasma Wayland | Flatpak plus an Arch package | Debian and RPM packages; other desktops; ARM64 |

Windows 10 is not supported. Signing, notarization and publishing remain
separate tasks that need owner authorization.

## D-012 — Shared performance metric

**Date:** 2026-10-25

**Status:** accepted by owner at GATE-001

Every platform budget uses launch to presented content: from process launch
to the first frame of document content on screen. Report cold and warm starts,
at least five runs with spread, on named hardware. Budget numbers are set from
FEAS-008 measurements, not from the earlier per-platform metrics.

## D-013 — v1 scope and delivery order

**Date:** 2026-10-25

**Status:** accepted by owner at GATE-001

- v1 (the MVP) contains the core Markdown features only. It opens files and
  folders through the system; it doesn't read `*.mdv.yaml` project files.
- After the MVP, advanced features follow in this order: voice-over and
  expanded syntax, teleprompter, Mermaid, math, then D2.
- After all three walking skeletons exist, the MVPs are deepened in this order:
  macOS, Linux, Windows.

## D-014 — Defer advanced engine validation until after the MVP

**Date:** 2026-10-26

**Status:** accepted by owner

Close FEAS-009 on its bounded Mac evidence and explicit deferral of the unrun
Windows/Linux checks and deeper engine comparisons. Preserve those checks as
unrun, not passed. Final Mermaid/math/D2 adoption, native presentation, fidelity,
resource/output safety, memory/package costs, Rust alternatives and iOS engine
fit move to ADV-005 after DESK-002. ADV-005 blocks ADV-002 engine adoption and
will be split into bounded platform/engine delivery tasks as needed.

ADV-005 uses the desktop-apps operational bucket so active-bucket selection
permits it after the desktop MVP without waiting for the whole mobile phase.
This closes the FEAS-009 prerequisite by owner-approved scope disposition;
it does not accept Go, a WebView document surface, any diagram/math engine or
the remainder of GATE-001. The core MVP continues to exclude these features.

## D-015 — Shared Rust extension passes

**Date:** 2026-10-26

**Status:** accepted by owner at GATE-001

Keep pulldown-cmark and implement product syntax through bounded shared Rust
passes as evaluated in FEAS-010. Disable the parser's YAML metadata option;
recognize front matter only at the beginning of the document. Preserve exact
source spans, malformed-input fallback and bounded scans. This accepts the
semantic approach, not post-MVP diagram/math presenters or a production model.
See `docs/Core-Extension-Evaluation.md`.

## D-016 — Initial desktop performance targets

**Date:** 2026-10-26

**Status:** accepted by owner at GATE-001

Use initial targets of at most 400 ms warm launch to presented content,
700 ms cold launch to presented content, and 150 ms off-screen jumps for the
1 MiB/5 MiB prose and 1 MiB long-line profiles on named baseline hosts. Use
D-012: at least five runs with spread, explicit cache treatment and metric
boundaries. These are implementation targets to validate with the MVP, not
claims that all products already pass or universal hardware guarantees.

FEAS-008 supports them on the tested Windows/Linux workstation. macOS still
requires equivalent D-012 evidence; earlier first-draw/core-entry results do
not establish a pass. Record package/runtime conditions and retained outliers.
Repeated-session memory, accessibility and responsiveness remain acceptance
work; no numeric RSS budget or waiver of the open Mac memory risk is implied.
Revisit targets explicitly using real MVP measurements, not by silently
changing the metric or excluding failures.

## D-017 — Windows Native AOT and conditional Linux native rendering

**Date:** 2026-10-26

**Status:** accepted by owner at GATE-001

Use native virtualized semantic blocks on Windows and Linux. Windows uses
WinUI 3 hosted in C# Native AOT. C++/WinRT remains a fallback for a concrete
blocker, not a second production host to maintain by default. FEAS-008 found
similar AOT/C++ startup and memory, so C++ has no demonstrated performance
advantage that warrants switching from the existing C# work. No C# JIT shipping
baseline is selected. Fix duplicate Narrator announcements and whole-tree UIA
timeouts before accepting the Windows renderer; packaged MSIX and mixed-DPI
behavior still require native implementation evidence.

Linux provisionally uses GTK 4/gtk-rs with a native virtualized list. Add
LNX-001D1 immediately after basic text/navigation and before LNX-001E/001F.
It must prove AT-SPI document/heading structure, off-screen heading navigation
and continuous reading beyond visible rows with owner-confirmed Orca behavior
on the accepted KDE Wayland baseline, while preserving focus, reading order and
basic selection. App shortcuts alone are not a screen-reader proof. A failure
stops deeper renderer work and returns to the owner for a revised native
accessibility design or explicit renderer decision. Do not silently accept
inaccessible output or substitute a WebView.

macOS keeps the accepted AppKit/TextKit 2 direction. Native resource policy,
large-document behavior, accessibility and memory risks still require their
platform acceptance checks. This selects implementation directions, not
production readiness. Evidence: the Windows/Linux FEAS-008 reports.

## D-018 — Accept implementation baseline and advance to core

**Date:** 2026-10-26

**Status:** accepted by owner

The owner approved `docs/Implementation-Baseline.md` and explicitly authorized
closing GATE-001 and advancing to CORE-001A. D-008–D-017, the 36 delivery tasks
and 92 dependency edges are the accepted starting baseline. Preserve the early
Linux accessibility condition, all recorded native risks, post-MVP engine
deferral, initial targets and branch/PR/Grok workflow. This approval permits
implementation; it does not approve a merge, signing, publishing a release or
production deployment.

## D-019 — Apache-2.0 for implementation source

**Date:** 2026-10-26

**Status:** accepted by owner

License the new implementation source under Apache-2.0 using the root LICENSE.
Imported fixtures retain their existing notices, license and pinned provenance.
Third-party dependency/font licenses remain independent. Publishing releases,
signing and deployment still require separate authorization.

## Accepted contract — CORE-002A semantic model v1

The implementation contract in `specs/semantic-model/v1/README.md` froze at
owner-authorized PR #2 merge `e5f1a17` on 2026-10-27. All four original Grok
findings were addressed; the owner explicitly waived a second review while
troubleshooting its integration. Required checks passed for head `75a8bc8`. It uses owned values,
flat ID-referenced typed nodes, exact UTF-8 spans/source fingerprinting,
diagnostics/metadata/outline, semantic capability flags and explicit unsupported
fallbacks. JSON shape is generated from Rust types; strict validation is still
required beyond schema. Stable ID reconciliation remains CORE-004A, not a new
algorithm chosen here. Default safety ceilings are caller-adjustable and are
not accepted performance budgets. This contract does not accept an advanced
engine, parser implementation, native presenter or C ABI layout.

## D-020 — Additive opt-in semantic model v2

**Date:** 2026-10-27

**Status:** accepted by owner. The owner selected the additive versioning
direction, and the v2 contract froze at the owner-authorized CORE-003B2
[PR #5](https://github.com/awesomejt/markdown/pull/5) merge (`752912a`,
2026-10-27), which also carried the B2F review fixes.

CORE-003B2 requires reference destinations outside their referring node, while
frozen v1 requires occurrence-contained destination spans. Add opt-in v2 with
explicit inline/reference provenance while preserving v1's API, strict decoder,
schema and fixtures. Do not silently reinterpret v1 fields or default formats.
New adapters explicitly choose supported versions; facade negotiation remains
CORE-006A. The v2 contract and explicit migration are in
`specs/semantic-model/v2/README.md`; targets grant no I/O or URI activation.

## D-021 — Snapshot-local node IDs with block anchors for the MVP

**Date:** 2026-10-27

**Status:** accepted by owner

**Context:** The frozen [v1 contract](../specs/semantic-model/v1/README.md)
says surviving semantic identities "should keep IDs across revisions", and
`docs/Parsing.md` previously expected adapters to need reconciled IDs before
relying on persistent node IDs. CORE-004A instead kept node IDs snapshot-local
and added block-level session anchors, recording the discrepancy for the owner.

**Decision:** For the MVP, node IDs stay snapshot-local, deterministic
ordinals: the same bytes always produce the same IDs. Block-level session
anchors (content fingerprint, neighbour context, intra-block offset) carry
reading position across generations. Persistent cross-revision node IDs are
deferred and will be revisited. This decision clarifies the v1/v2 "should"
wording for the MVP; the frozen model READMEs are not edited.

**Rationale:** Deterministic IDs keep session output identical to
`parse_markdown`/goldens, avoid whole-tree matching cost and mis-match risk,
and anchors are persistable values for relaunch restoration. The owner expects
small impact: most documents are small and only visible rows are realized.

**Consequences:** Native lists rebuild rows on reload; selection and
assistive-technology focus must be re-found through anchors; caches key by
content fingerprint, not node ID. Adapters must not persist node IDs.

**Revisit trigger:** A platform B-phase accessibility or interaction check
shows user impact, for example Narrator/VoiceOver/Orca focus jumping on
reload or lost multi-block selection. The likely cheaper path is an
anchor-based old-block to new-block mapping rather than full persistent IDs.
The follow-up requirements are listed in
[the session contract](../specs/session/README.md).

## D-022 — C ABI v1 grows by adding functions only

**Date:** 2026-10-27

**Status:** accepted by owner

**Context:** CORE-005A needed a way to expose find and reading statistics to
native hosts. ABI v1 has no extension point: `md_abi_info` cannot report new
struct sizes without changing its own size, and `md_live_handles` counts only
sessions, jobs and snapshots.

**Decision:** ABI v1 grows only by adding new functions. Each new function
fills caller-owned arrays or structs. No new handle types are added. No
existing function, struct, constant, `md_abi_info` or `md_live_handles`
changes. The size of each new struct is a header constant and is checked
through `struct_size` on every call (`MD_ERR_STRUCT_SIZE` otherwise). An
incompatible change still requires a new ABI version.

**Consequences:** Hosts negotiate as before, then compare each new struct's
header size constant with `sizeof` before calling the new functions. A host
built against a newer header fails to link or load against an older library;
there is no runtime feature probe. Leak checks keep the three handle counts.
New results that cannot be owned by a snapshot handle are sized through
`MD_ERR_BUFFER_TOO_SMALL` or a caller capacity. The first application is
CORE-005A2 (find, word count and section timing over a snapshot handle).

## D-023 — Flat-record snapshot view is a frozen ABI v1 representation

**Date:** 2026-10-27

**Status:** accepted by owner

The PR #7 (`a751f48`) bulk `md_snapshot_view`, made of fixed-size records plus
one string table and owned by the snapshot handle, is a frozen ABI v1
representation alongside the v1 and v2 JSON models. Its record layouts,
field meanings, constants and index conventions follow the
[C ABI contract](../specs/ffi/README.md) and change only under D-022's
additive rule or a new ABI version. The JSON models remain the semantic wire
contracts; the flat view must keep round-tripping them exactly.

## D-024 — Parser default limits admit representative prose at the source ceiling

**Date:** 2026-10-28

**Status:** accepted by the owner with the PR #13 merge approval (2026-10-28)

**Context:** CORE-003B4 measured the generated profiles. The accepted 5 MiB
prose profile, the upper-bound document for D-016, failed with `NodeLimit`
under the CORE-003A defaults of 250,000 nodes and 1,000,000 events. Prose
produces about 92,000 nodes and 116,000 events per MiB, so it needs about
738,000 nodes and 932,000 events at the 8 MiB source ceiling.

**Decision:** `ParseLimits::default()` uses 1,000,000 nodes, equal to the model
and C ABI node ceiling, and 2,000,000 events. Source (8 MiB), depth (128) and
copied text (32 MiB) are unchanged. They stay defensive ceilings, and dense
hostile markup still stops with typed limits. Counted heap budgets per profile
and coarse release time guards are enforced in CI. Named-host times are
recorded, not guaranteed ([Parser-Budgets.md](../docs/Parser-Budgets.md)).

**Consequences:** Sessions, workspace content search and the C ABI's
zero-selects-default limits inherit the new values without an ABI change. A
5 MiB prose snapshot retains about 102 MiB. Dense delimiter input at the
8 MiB source ceiling peaks at about 1,152 MiB (about 140 times the source)
before the node limit stops it, unchanged from the earlier defaults because
the cost is upstream inline work. A 2 MiB dense storm has a CI heap budget.
Reducing per-node memory and pre-screening delimiter density are tracked
residual risks. Hosts that need tighter ceilings pass explicit limits, such as
a lower source limit or content search file size. Text events the builder
rejoins across a `$` split are not charged to the event limit, so
dollar-dense prose at the ceiling also parses.

## D-025 — Native hosts build and request semantic model v2

**Date:** 2026-10-28

**Status:** accepted by the owner in the holistic CORE review
([PR #16](https://github.com/awesomejt/markdown/pull/16))

**Context:** The holistic CORE review
([F1](../docs/reviews/2026-10-28-core-holistic-review.md#f1--high-the-default-model-denies-most-real-relative-links))
found that model v1, the Rust default and the only model in the default
`markdown-ffi` build, denies parent-relative paths, non-ASCII file names and
any percent-encoding. That covers most relative links and images in real
documentation folders. V2 classifies them under the reviewed URI policy
(docs/URI-Policy.md). D-020 made v2 opt-in, but no decision said which model
the native packages use.

**Decision:** Each native 001B package builds `markdown-ffi` with the
`model-v2` feature and creates sessions with `MD_MODEL_V2`. V1 stays the Rust
default and remains supported for compatibility, goldens, tests and the
default library build. This changes no API, ABI, schema or default.

**Consequences:** All three desktop hosts consume one model and one URI
policy. Documents with link or image intents pay a second bounded parse for
source provenance (about 78 ms against 43 ms on the synthetic 40,960-link
input; prose is unchanged). Hosts navigate find and workspace hits by span or
anchor, because a v1 `unit` index does not apply to a v2 view
(specs/ffi/README.md). The v2 URL/IDNA dependencies ship in native packages,
so each 001I license inventory includes them.

## D-026 — CORE-FIX phase from the shared-core reviews

**Date:** 2026-10-28

**Status:** accepted by the owner on the shared-core review PR
([PR #16](https://github.com/awesomejt/markdown/pull/16))

**Context:** Three reviews covered the shared core at implementation baseline
`39d1a56`: the [Claude holistic review](../docs/reviews/2026-10-28-core-holistic-review.md)
(F1–F7), the [Codex review](../docs/reviews/2026-10-28-codex-shared-core-review.md)
(C1–C7) and the [Grok review](../docs/reviews/2026-10-28-grok-shared-core-review.md)
(G1–G6). None found a correctness, memory-safety or file-access defect. The
owner reconciled them and decided the open questions.

**Decision:**

- The follow-ups form a CORE-FIX phase, run in this order before any platform
  phase, including the 001A shells: DOC-001, CORE-002B, CORE-003C, CORE-003D,
  CORE-005C, CORE-005D. State encodes the order as dependency edges, and the
  shared track then closes the CORE aggregates before any 001A (D-027).
  [TODO.md](../TODO.md) holds the scope. On 2026-10-28 the owner appended
  CORE-003E (front-matter invisible-character parity, from the PR #17 Grok
  review) after CORE-005D as its own task rather than widening an existing
  one.
- **Import policy (F3, G3, C2).** CORE-002B makes decoding match the producer,
  not a stricter policy. V1 `from_json` re-runs the parser's own link and
  resource policy. The shared structural validation applies the producer's
  metadata string guards under v1 and v2. V1 keeps accepting the
  `https://user:pw@…` targets its parser emits; native hosts get credential
  denial from v2 (D-025).
- **Bare GFM autolinks (C3).** R1.2 stands. The shared core implements bare
  `http(s)://`, `www.` and email autolinks in one bounded pass (CORE-003D). No
  host adds its own detection. CORE-003D records whether v1 output gains them.
- **v2 conformance (C5).** The supported corpus checks run on v2 with
  version-specific expectations (CORE-003C), before the autolink change.
- **Workspace content search (G4).** Content search is a text index that does
  not depend on the model version. Its v1 parse is deliberate, and richer
  semantics come from the document session. CORE-005C documents this with
  the memory guard.
- **Filename warnings (C4).** The invisible-name flag widens to blank
  characters that are not Cf (Hangul fillers, Braille blank). Names are never
  rejected (CORE-005D).
- **Release hardening (F7, G6, C7).** HARD-001 adds coverage-guided fuzzing,
  sanitizer runs and a dependency advisory scan before DESK-002.
- **Not scheduled.** Each 001B keeps its accepted D-025, flat-view/RSS and
  MSVC requirements (F1, F2, F6). Maintenance items (splitting `parse.rs`,
  an ID side index, footnote label double charge) wait for a concrete change
  to that code. A `pulldown-cmark` upgrade is its own gated task.

**Consequences:** Platform 001A work waits for six small shared phases, but
the three hosts build on one validated import policy, one autolink rule set,
v2-checked conformance and one workspace memory bound. Each platform's 002B
needs CORE-005D when the 002 tasks are created at DESK-001.

## D-027 — Linear shared and per-host task tracks

**Date:** 2026-10-28

**Status:** accepted by the owner on the shared-core review PR
([PR #16](https://github.com/awesomejt/markdown/pull/16))

**Context:** The graph had no cycles, but the shared track and the platform
tracks waited on each other. CORE-001 waited on every host's 001A, and
CORE-002 to CORE-006 waited on CORE-001 in turn. CORE-006 waited on every
host's 001B, while each host's 001 aggregate waited on CORE-006. All 28
platform 001 children sat in `portable-core`, so the core phase could not
finish until three desktop skeletons existed. Many edges only repeated what
another path already enforced.

**Decision:**

- The shared track is one chain in `portable-core`: the CORE-FIX tasks
  (D-026), then the aggregates CORE-001 to CORE-006 in order. The aggregates
  close on shared acceptance.
- Native evidence moves to the platform tasks that produce it. 001A carries
  the native shell CI job (from CORE-001). 001B carries the release-mode
  native package consumer (from CORE-006), alongside its D-025 model-v2 and
  flat-view/RSS acceptance. Each host's 001 aggregate checks both.
- The platform 001 children move to `desktop-apps`. Each host is one chain
  from CORE-006: 001A to 001D, LNX-001D1 on Linux, then 001E to 001I and the
  001 aggregate. 001F now follows 001E, and 001G follows 001F.
- The host chains stay independent of each other, because they run on
  different machines; the concurrency limit of one serializes the work.
  DESK-001 and DESK-002 are the only joins. HARD-001 sits between them.
- State keeps no redundant edge between open tasks. Edges from completed
  tasks stay as history. `tools/project_state.py` gains `remove_dependency`
  and `update_task` (text, priority, role and phase of tasks without an
  active pass), which use the backend's lock, revision and idempotency rules,
  so roadmap changes no longer need hand edits.

**Consequences:** `portable-core` completes when CORE-006 closes, and that
activates `desktop-apps`. The graph went from 181 to 131 edges. The
eligible queue shows one shared task at a time, then one task per host. The
aggregate acceptance in Implementation-Baseline.md (GATE-001) is superseded
where it made CORE-001 and CORE-006 wait for native hosts.

## D-028 — Tiered core CI and incremental PR checks

**Date:** 2026-10-28

**Status:** accepted 2026-10-28 (owner merged TOOL-002, PR #19)

**Context:** The hosted core job compiled the workspace from scratch four
times per OS (default, model-v2, all-features and all-features release). The
macOS job took about 20 minutes cold. A `chore(state)` closeout commit reran
the whole matrix, because host selection compared the PR head with `main`.

**Decision:**

- Host jobs cache Rust builds with a SHA-pinned `Swatinem/rust-cache`.
- `tools/check.py core` is the PR tier: fmt, Clippy with warnings denied,
  default and all-features debug tests, and dependency policy.
  `core-extended` holds the model-v2-only build and the all-features release
  build with its release-only time guards. Hosted CI runs it on `main` pushes
  and manual runs. `tools/check.py all` runs every tier, and the pass runner
  runs it locally before each review request.
- A PR push skips the host steps when the previous head is an ancestor of the
  new head, its GitHub Actions `Checks / complete` run succeeded, and the
  commits in between touch only known prose/operational paths.

**Consequences:** Release time guards and the model-v2-only build gate a PR
on the runner's host, not on all three hosted OSes. A regression in those
tiers that only one other OS shows surfaces on the `main` run after merge and
is fixed forward. The skip adds no risk beyond merging an older green head
after `main` moved, which the repository already allows. Branch and PR sizing
(one task per PR) is unchanged.

## D-029 — Grok review severity gates the merge

**Date:** 2026-10-28

**Status:** accepted by owner request

**Context:** Every PR waited for an explicit owner merge approval even when
Grok's review found nothing serious. Grok reviews are now requested through
the `grok` CLI, which rates findings as fix-now (high or critical) or later
(low or nits).

**Decision:** A Grok review with no high or critical finding is the owner's
standing approval to merge that PR. High or critical findings are fixed on the
same branch and reviewed again until a review is clean. Commits that only fix
low findings, make minor tweaks, update documentation or record project status
need no re-review. Low findings and nits left unfixed are recorded in TODO.md
"Deferred review findings" for a later pass.
The usual closeout, CI, merge and branch cleanup follow. The runner stops and
asks the owner when the review is missing or its severity unclear, when
required CI fails, or when the PR needs an owner decision a review cannot
give, such as a golden exception or a gate.

**Consequences:** Delivery no longer waits on the owner for routine PRs, so
review quality carries more weight; the owner keeps the holistic phase review
and can stop or revert any merge. Tags, releases, signing and publishing still
need explicit approval.

D-030 narrows this approval to merges into `develop`. A PR gets at most three
Grok reviews: when the third still has a serious finding, the runner stops and
waits for the owner (added 2026-10-28 after PR #23 needed five).

## D-030 — Milestone reviews on `main`, task work on `develop`

**Date:** 2026-10-28

**Status:** accepted by owner request, installed by one owner-allowed direct
commit to `main`

**Context:** Under D-029 Grok reviews gate each task PR, but the owner wants to
stay involved through periodic reviews and manual checks without holding up
each task. Long-lived branches per phase or platform would conflict on
`project.yaml`, which every task changes.

**Decision:**

- `develop` integrates finished tasks. Task branches start from it, and task
  PRs target it and merge under D-029.
- At a milestone, the runner freezes `develop` as `review/<milestone>` and
  opens a PR into `main` with a review packet: tasks and PRs, decisions,
  deferred findings, check evidence, manual checks per host, risks and open
  owner decisions. Work continues on `develop` meanwhile.
- Only the owner merges into `main`. The runner then tags the merge
  `m-<milestone>`, deletes the review branch and syncs `main` into `develop`
  by PR.
- Milestones are the end of CORE-FIX, the closed CORE aggregates, each host's
  skeleton (001A–001C) and MVP (001D–001I), and HARD-001, capped at about
  8 task PRs or 2 weeks.
- Gates, golden exceptions, new architecture decisions, unclear reviews and
  failing CI still stop work for the owner at once.
- CI runs on pushes to `main`, `develop` and `review/*`, and runs
  `core-extended` on milestone PRs into `main` as well.
- No direct commits to `main` or `develop`. The owner allowed two on
  2026-10-28, to install this workflow and then the review cap and branch
  settings, so that no branch carried conflicting instructions; both
  allowances have ended.
- `develop` is the GitHub default branch, and `main` is protected (pull
  requests only, no force pushes or deletion), applied 2026-10-28.

**Consequences:** `main` is always owner-reviewed, and tasks never wait on
the owner. Problems found at a milestone cost more rework than per-PR review
would, which the milestone cap bounds. Review fixes are made on the review
branch and reach `develop` through the sync. Tags for milestones are authorized; release
tags, signing and publishing still need explicit approval.
