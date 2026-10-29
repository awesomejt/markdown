# Implementation Plan

## Planning principles

- Follow [Delivery-Workflow.md](Delivery-Workflow.md): one small delivery phase,
  one feature branch, one PR, and a Grok review through the `grok` CLI when ready.
- Batch dependency-ready work on the current OS before switching machines.
- Extract and prioritize requirements before selecting implementation reuse.
- Prove native performance on every desktop before deepening one platform.
- Deliver thin vertical slices, then coherent basic MVPs, then refinements and
  advanced functionality.
- Share semantics and fixtures by default; share code only with evidence.
- Record moderate platform deviations instead of forcing lowest-common-
  denominator behavior.

## Phase 1 — requirements and feasibility

Tasks: REQ-001, FEAS-001 through FEAS-010, REV-001, and GATE-001. FEAS-009
closes by the owner's D-014 deferral; ADV-005 carries its unrun post-MVP work.

Outputs:

- Prioritized requirements inventory extracted from `markdown-viewer`.
- Versioned semantic and performance corpus with provenance.
- Parser/core comparison covering Rust, C/C++, and platform alternatives.
- Native long-document rendering prototypes on Windows, macOS, and Linux.
- Shared-core facade and packaging proof on all three desktops.
- Capability matrix for advanced features and later iOS/iPadOS constraints.
- Accepted implementation baseline or an explicit platform-specific pivot.

Exit criteria:

- Every desktop can display and navigate the representative basic corpus using
  its proposed native text/layout surface.
- Performance measurements name hardware, OS, compiler, build mode, corpus,
  and metric definitions.
- A candidate shared core demonstrates acceptable call/copy/debug/package cost
  on every desktop, or shared implementation is rejected with evidence.
- Basic and advanced requirement tiers are agreed.
- Platform deviations and diagram constraints are explicit.

## Delivery phase catalog

The numbered sections below are roadmap milestones. The execution units in this
catalog are smaller delivery phases, each with its own feature branch and PR.
IDs with suffixes are child delivery tasks, materialized during GATE-001 closeout.
GATE-001 must reconcile this catalog with the accepted stack and materialize the
initial dependency graph as described in [Delivery-Workflow.md](Delivery-Workflow.md).
Refine an oversized row before coding, preserving its parent acceptance criteria.

### Shared foundation (any supported development host)

| Delivery phase | Scope and acceptance evidence | Merged prerequisites |
| --- | --- | --- |
| CORE-001A | Accepted workspace, common checks, dependency/license inventory and platform-isolated CI definitions; common checks run | GATE-001 |
| CORE-002A | Versioned model/capabilities, identity/spans/resource intents and golden fixtures; schema/compatibility checks pass | CORE-001A |
| CORE-003A | Bounded basic parsing and safety policy; basic/malformed/unsafe corpus passes | CORE-002A |
| CORE-003B | Aggregate acceptance retained: all B1–B4 merged, supported conformance/hostile inputs/fuzz/property and measured parser budgets pass | CORE-003A, CORE-003B1–B4 |
| CORE-003B1 | Bounded typed first-position metadata, atomic duplicate/unsupported rejection, exact source and copy-budget/privacy tests | CORE-003A |
| CORE-003B2 | Reference representation/activation under version/compatibility rules; URI refinement including Unicode without obfuscation; exact spans and safety corpus | CORE-003A |
| CORE-003B3 | Remaining bounded MVP extension semantics and explicit advanced fallbacks; malformed/undefined and source-span checks, no advanced engines | CORE-003B1 |
| CORE-003B4 | Supported semantic corpus reconciliation, hostile/fuzz/property checks and measured named-host parser time/memory budgets; remaining offset fallbacks | CORE-003B2, CORE-003B3 |
| CORE-004A | Immutable sessions, generations, cancellation, external changes and anchors; race/lifetime checks pass | CORE-003A |
| CORE-005A | Source-accurate document find, word count and section timing; Unicode/range checks pass | CORE-004A |
| CORE-005A2 | C ABI exposure of find, word count and section timing under D-022 (new functions only, caller-owned arrays, header size constants checked via `struct_size`); header constant/layout checks and C smoke pass | CORE-005A |
| CORE-005B | Capability-fed workspace search primitives, cancellation and resource budgets; bounded-search checks pass | CORE-005A |
| CORE-005B2 | C ABI exposure of the workspace search primitives under D-022 (new functions only, caller-owned progress struct and record arrays, header size constants); header constant/layout checks and C smoke pass | CORE-005B |
| CORE-006A | Bulk facade, value ownership, typed errors and version negotiation; ABI/lifetime tests and current-host package smoke pass | CORE-004A |
| DOC-001 | CORE-FIX 1. Agent-facing docs cleanup: concise MEMORY.md posture with an archive, stale HANDOFF.md/TODO.md handoff and decision-status text removed, snippet invariant qualified; lint and link checks pass | none |
| CORE-002B | CORE-FIX 2. V1 validation (including `from_json`) re-runs the producer link/resource policy; shared validation applies the producer metadata string guards under v1 and v2; FFI host guidance for off-thread, single-call search sizing; regression and golden checks pass | CORE-002A, DOC-001 |
| CORE-003C | CORE-FIX 3. Supported corpus check tokens run on v2 with version-specific expectations; test-only, no golden change | CORE-002B |
| CORE-003D | CORE-FIX 4. Bare GFM `http(s)://`, `www.` and email autolinks in one bounded shared pass with exact spans, URI policy and v2 provenance; conformance, hostile and budget checks pass; goldens regenerated with a reviewed diff | CORE-003C |
| CORE-005C | CORE-FIX 5. Workspace content-search memory guard: lower per-file default and/or density admission check with a typed skip and false-positive tests; content search documented as a model-independent text index; counted heap budget and Rust/FFI parity pass | CORE-005B2, CORE-003D |
| CORE-005D | CORE-FIX 6. Invisible-filename flag also covers blank characters that are not Cf (Hangul fillers, Braille blank), never rejecting names; Rust/FFI parity pass | CORE-005C |
| CORE-003E | CORE-FIX 7 (owner-added from the PR #17 Grok review). Front-matter strings: decide and apply treatment of invisible characters that are not Cf (Hangul fillers, U+2800, default-ignorables), keeping the variation-selector/CGJ allowance; producer and shared validation at v1/v2 parity; decision recorded | CORE-005D |

CORE-003B was refined after CORE-003A review because metadata, reference/model
compatibility, extension semantics and fuzz/performance acceptance were too
large for one review. The four children and nine new dependency edges were
rehearsed on disposable state, then materialized through the state helper.
The original CORE-003B prerequisites/dependents, native gates and aggregate
acceptance remain intact. CORE-004A remains independently ready after CORE-003A;
metadata does not add a new prerequisite for sessions or native A phases.

CORE-005A was split by owner approval: CORE-005A delivers the Rust find and
reading-metadata semantics, and CORE-005A2 exposes them through the C ABI
under D-022. Each platform's 001F needs CORE-005A2, because native find UI
calls the core through the C ABI. CORE-005 also depends on CORE-005A2.
CORE-005B was split the same way on 2026-10-28 (owner-approved): CORE-005B
delivers the Rust [workspace search primitives](../specs/workspace-search/README.md),
and CORE-005B2 exposes them under D-022. Each platform's 002C needs both, and
CORE-005 depends on CORE-005B2.

The CORE-FIX rows come from the three
[shared-core reviews](reviews/README.md) of 2026-10-28, which the owner
reconciled in D-026. They run in the numbered order, and state encodes it as
dependency edges. CORE-003E (added after D-026) then leads into the CORE aggregates (below), so
no platform phase starts before CORE-FIX completes. When the 002 tasks are
created, each platform's 002B needs CORE-005D and each 002C needs CORE-005C.

#### Linear tracks (D-027)

State keeps each track a single chain, so every open task has one open
predecessor. Only DESK-001 and DESK-002 join tracks:

```text
Shared (portable-core):  DOC-001 -> CORE-002B -> CORE-003C -> CORE-003D -> CORE-005C
                         -> CORE-005D -> CORE-003E -> CORE-001 -> CORE-002 -> CORE-003
                         -> CORE-004 -> CORE-005 -> CORE-006 -> phase complete
Each host (desktop-apps): CORE-006 -> 001A -> 001B -> 001C -> 001D [-> LNX-001D1]
                         -> 001E -> 001F -> 001G -> 001H -> 001I -> 001 aggregate
Joins:                   MAC-001 + LNX-001 + WIN-001 -> DESK-001 -> HARD-001,
                         each 002 chain; HARD-001 + all 002 aggregates -> DESK-002
```

The CORE aggregates close on shared acceptance. The native evidence they
used to wait for belongs to each platform: CORE-001's native shell CI job is
part of 001A, and CORE-006's native package consumer is part of 001B. Each
platform's 001 aggregate checks that evidence. The three host chains stay
independent, because they run on different machines; the concurrency limit
of one already serializes the work. The catalog's prerequisite columns below
list each row's direct predecessor plus the completed shared tasks it uses.
Transitive and completed prerequisites are not repeated as state edges.

CORE-006A delivered the C ABI facade. CORE-006 closes on its shared
acceptance: release-mode Rust/FFI tests in hosted CI on all three OSes. Each
platform's 001B proves its native consumer (D-027). Shared changes require
affected-host CI according to Repository-Strategy.md; a local pass alone does
not establish all-host support.

### Desktop foundation (one independent sequence per host)

Instantiate every row for `MAC`, `LNX`, and `WIN`: for example `MAC-001A`.
In the prerequisites column, “own” means that same platform. Each row requires
native build and relevant interaction evidence on its named OS. These children
live in the `desktop-apps` state bucket, which becomes active when
`portable-core` completes (D-027).

| Suffix | Scope and acceptance evidence | Merged prerequisites |
| --- | --- | --- |
| 001A | Native launch/window/commands, build instructions and a native CI job in the path-filtered workflow (from CORE-001) | CORE-001A, CORE-006 (shared track complete) |
| 001B | Native facade adapter and local core package built with `model-v2`, sessions requesting v2 (D-025); release-mode consumer passes lifecycle/error/cancellation/version tests (from CORE-006); flat-view encode cost and single/repeated-document retained RSS measured on this host; on Windows the MSVC C smoke becomes a required CI step | own 001A, CORE-006A |
| 001C | File open/association, permission capability and errors; generation-safe off-thread load and revoked/unavailable-file checks | own 001B |
| 001D | Headings/prose/lists/links/code, outline and basic selection; semantic corpus/navigation checks | own 001C |
| 001E | Tables/task lists/images and resource policy; rich-block and denied/unsafe-resource checks | own 001D |
| 001F | Document find, theme and text controls; Unicode find and native command/settings checks | own 001E, CORE-005A, CORE-005A2 |
| 001G | Large-document layout, global selection and accessibility; native reader/copy, scaling and performance budgets | own 001F |
| 001H | External refresh, anchors and reading restoration; replacement/deletion/race and relaunch checks | own 001G |
| 001I | Local development artifact, license inventory and relocated launch; full skeleton acceptance on this host | own 001H, CORE-003B |

Platform-specific early gate under D-017:

| Delivery phase | Scope and acceptance evidence | Merged prerequisites |
| --- | --- | --- |
| LNX-001D1 | Prove native document/heading semantics, off-screen heading navigation and continuous reading beyond visible rows with AT-SPI inspection and owner-confirmed Orca behavior on the KDE Wayland baseline; retain correct focus, reading order and basic selection | LNX-001D |

On Linux the chain is LNX-001D, LNX-001D1, then LNX-001E. Do not deepen the
Linux renderer before that proof. If it fails, stop those dependent phases,
record the failing behavior and return to the owner for a native accessibility
design revision or explicit renderer decision. App-level shortcuts alone do
not establish screen-reader heading navigation or continuous reading. Other
desktop environments remain later coverage under the accepted MVP baseline.

Use measured platform strategies: Windows must prove global selection/UIA and
mixed-DPI behavior; Linux must prove cross-block selection/AT-SPI on supported
environments; macOS must resolve measured memory/layout risks. A failed native
budget stays open. Split 001G further if layout and accessibility cannot form
one reviewable increment. No signing or distribution is included.

Close each CORE and platform aggregate only after its children and original
acceptance criteria pass. All CORE milestones and WIN/MAC/LNX-001 precede
DESK-001. That gate reconciles contracts/deviations before any MVP deepening.

### Desktop MVP (after DESK-001)

Instantiate for each host as `MAC-002A`, `LNX-002A`, `WIN-002A`, etc. Expand these
children in `desktop-apps` during DESK-001 closeout. Each row has its own PR and
requires native validation; the counterparts on other hosts are independent.

| Suffix | Scope and acceptance evidence | Merged prerequisites |
| --- | --- | --- |
| 002A | Folder capability/access recovery and recents; denied/revoked/moved-folder checks | DESK-001 |
| 002B | Native workspace browser and navigation; large-folder and keyboard checks | own 002A, CORE-005D |
| 002C | Workspace search UI; bounded indexing, cancellation and result navigation checks | own 002B, CORE-005B, CORE-005B2, CORE-005C |
| 002D | Persistent preferences/native settings; migration and relaunch checks | own 002A |
| 002E | Workspace watching/restoration and performance; change storms, access loss and session recovery | own 002C, own 002D |
| 002F | Development packaging and complete R1 conformance; install/run evidence and host budgets | own 002E |

Close each platform's 002 milestone from these merged phases; DESK-002 still
requires all three products and HARD-001 (release hardening: coverage-guided
fuzzing of the parser, model decoder and safe FFI inputs, sanitizer runs where
supported and a dependency advisory scan; D-026). HARD-001 follows DESK-001. Production packaging/signing/publishing requires
separate owner authorization.

### Later delivery phases

After DESK-002, ADV-005 resumes deferred FEAS-009 engine validation using real
MVP documents and bounded platform/engine phases. It blocks ADV-002, not the
core MVP or GATE-001. Its desktop-apps state bucket allows execution before
the later mobile phase. Final engine adoption remains open under D-014.

At DESK-002, split refinements by platform and individual behavior: for example,
macOS print/export and Linux large-folder navigation are separate PRs. Do not
combine all R2 work into DESK-003's branch.

Split IOS-001 into packaging/ABI, native rendering, and memory/lifecycle/provider
proof phases. Split IOS-002 into access/navigation, reading/find, restoration,
share/print, and real-device conformance phases. Each depends on the relevant
merged mobile predecessor; mobile code builds on macOS and device evidence
requires a named device. Materialize concrete IDs/criteria before selection.

For ADV milestones, separate the shared contract/engine from each platform's
integration. Math, Mermaid and D2 are separate feature phases, each gated on its
accepted engine/policy. Projects, script semantics and teleprompter presentation
likewise need bounded shared/platform phases rather than a single all-OS PR.
Record exact dependencies and capability fallbacks when those gates open.

### Example boot schedule

After the shared track closes with CORE-006, complete the current host's
001A–001I sequence, with one PR/review/owner merge per phase. Then boot another host and
complete its ready foundation sequence. Host order is flexible; no host needs
the counterpart's 001B–001I merely for symmetry. Close each host's 001
aggregate, then DESK-001, then batch each platform's 002 phases in the
same way. If a review, shared prerequisite or gate blocks work, record the
handoff rather than bypassing it. The concurrency limit remains one.

## Phase 2 — portable contract and core

Tasks: CORE-001 through CORE-006.

1. Scaffold the accepted monorepo workspaces and platform-isolated CI.
2. Freeze semantic model v1 and capability schema through golden fixtures.
3. Implement parsing and safety policy against the corpus.
4. Implement generation/cancellation/change semantics.
5. Implement search and reading metadata.
6. Stabilize the facade and native packages for all desktop hosts.

Exit criteria:

- Semantic, malformed-input, safety, fuzz/property, and performance checks pass.
- No UI or ambient platform filesystem authority leaks into portable code.
- Windows, macOS, and Linux test hosts consume release-mode artifacts through
  the accepted facade.
- The core can be changed atomically with adapters and fixtures in the monorepo.

## Phase 3 — desktop walking skeletons

Tasks: WIN-001, MAC-001, LNX-001, and DESK-001.

Each platform implements the same narrow vertical slice:

- Native launch, file open/association, and basic error handling.
- Off-UI-thread parsing and generation-safe result publication.
- Native headings, prose, lists, links, code, tables, task lists, and images.
- Outline navigation, theme/text-size controls, selection, and accessibility.
- External file change refresh and reading-position restoration.
- Local package/install artifact for development validation.

The three skeletons should stay shallow until all pass the semantic and
performance gate. DESK-001 records platform-specific architecture choices and
selects the order for deepening the MVPs.

## Phase 4 — desktop basic MVPs

Tasks: WIN-002, MAC-002, LNX-002, and DESK-002.

Add folder workspaces, native persistent access/recents, browsing, find,
workspace search, preferences, robust file watching, state restoration,
packaging, and platform performance tuning.

Exit criteria:

- A user can install/run each app, open a file or folder, navigate/search, read
  representative documents offline, quit/relaunch, and recover unavailable
  files or permissions.
- Named baseline hardware meets accepted launch, parse, memory, layout, scroll,
  and search budgets.
- Capability deviations are documented and surfaced in the app.

## Phase 5 — desktop refinement

Task: DESK-003 plus scoped platform follow-ups.

Prioritize recent/workspace polish, large-folder virtualization, styles,
properties, reading time, focus mode, print/share/export, and other R2 features.
Implement platform behavior independently where that provides a better or
faster experience.

## Phase 6 — iOS/iPadOS

Tasks: IOS-001 and IOS-002.

First validate core packaging and native document rendering under mobile memory
and lifecycle constraints. Then build the basic mobile MVP around document
pickers, security-scoped/provider access, adaptive navigation, foreground
refresh, restoration, share, and print.

iOS does not block desktop releases and is not required to reproduce desktop
folder watching, background indexing, subprocess-based diagrams, or windowing.
Every deviation belongs in the capability matrix.

## Phase 7 — advanced features

Tasks: ADV-001 through ADV-004.

Add advanced requirements in user-value order: math, accepted diagram engines,
projects, voice-over/script features, teleprompter, and advanced export. Each
task names supported platforms, performance/security cost, fallback behavior,
and acceptance evidence.

## Release posture

Each OS produces and versions its own artifact. Signing, notarization, stores,
package repositories, update feeds, and production publishing remain separate
owner-authorized tasks. A coordinated source release does not require every
platform artifact to have identical features when the capability matrix
documents the difference.
