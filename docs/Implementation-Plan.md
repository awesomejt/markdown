# Implementation Plan

## Planning principles

- Follow [Delivery-Workflow.md](Delivery-Workflow.md): one small delivery phase,
  one feature branch, one PR, and a separate `/grok review` comment when ready.
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
| CORE-003B | Extension/conformance gaps, hostile inputs, fuzz/property and parser performance budgets pass | CORE-003A |
| CORE-004A | Immutable sessions, generations, cancellation, external changes and anchors; race/lifetime checks pass | CORE-003A |
| CORE-005A | Source-accurate document find, word count and section timing; Unicode/range checks pass | CORE-004A |
| CORE-005B | Capability-fed workspace search primitives, cancellation and resource budgets; bounded-search checks pass | CORE-005A |
| CORE-006A | Bulk facade, value ownership, typed errors and version negotiation; ABI/lifetime tests and current-host package smoke pass | CORE-004A |

CORE-006A enables native adapters without waiting for all workspace-search work.
It does not close CORE-006: all three native package consumers must pass in their
platform B phases below. Shared changes require affected-host CI according to
Repository-Strategy.md; a local pass alone does not establish all-host support.
CORE-001 closes only after native build jobs have run as the shells are added.

### Desktop foundation (one independent sequence per host)

Instantiate every row for `MAC`, `LNX`, and `WIN`: for example `MAC-001A`.
In the prerequisites column, “own” means that same platform. Each row requires
native build and relevant interaction evidence on its named OS. These children
live alongside shared foundation work in the `portable-core` state bucket.

| Suffix | Scope and acceptance evidence | Merged prerequisites |
| --- | --- | --- |
| 001A | Native launch/window/commands, build instructions and native CI smoke | CORE-001A |
| 001B | Native facade adapter and local core package; ownership/errors/cancellation and release-build smoke | own 001A, CORE-006A |
| 001C | File open/association, permission capability and errors; generation-safe off-thread load and revoked/unavailable-file checks | own 001B |
| 001D | Headings/prose/lists/links/code, outline and basic selection; semantic corpus/navigation checks | own 001C |
| 001E | Tables/task lists/images and resource policy; rich-block and denied/unsafe-resource checks | own 001D |
| 001F | Document find, theme and text controls; Unicode find and native command/settings checks | own 001D, CORE-005A |
| 001G | Large-document layout, global selection and accessibility; native reader/copy, scaling and performance budgets | own 001E, own 001F |
| 001H | External refresh, anchors and reading restoration; replacement/deletion/race and relaunch checks | own 001G |
| 001I | Local development artifact, license inventory and relocated launch; full skeleton acceptance on this host | own 001H, CORE-003B |

Platform-specific early gate under D-017:

| Delivery phase | Scope and acceptance evidence | Merged prerequisites |
| --- | --- | --- |
| LNX-001D1 | Prove native document/heading semantics, off-screen heading navigation and continuous reading beyond visible rows with AT-SPI inspection and owner-confirmed Orca behavior on the KDE Wayland baseline; retain correct focus, reading order and basic selection | LNX-001D |

LNX-001E and LNX-001F additionally depend on LNX-001D1. Do not deepen the
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
| 002B | Native workspace browser and navigation; large-folder and keyboard checks | own 002A |
| 002C | Workspace search UI; bounded indexing, cancellation and result navigation checks | own 002B, CORE-005B |
| 002D | Persistent preferences/native settings; migration and relaunch checks | own 002A |
| 002E | Workspace watching/restoration and performance; change storms, access loss and session recovery | own 002C, own 002D |
| 002F | Development packaging and complete R1 conformance; install/run evidence and host budgets | own 002E |

Close each platform's 002 milestone from these merged phases; DESK-002 still
requires all three products. Production packaging/signing/publishing requires
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

On the current host, complete ready shared prerequisites and its 001A–001I
sequence, with one PR/review/owner merge per phase. Then boot another host and
complete its ready foundation sequence. Host order is flexible; no host needs
the counterpart's 001B–001I merely for symmetry. Finish outstanding shared and
all-host evidence, close DESK-001, then batch each platform's 002 phases in the
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
