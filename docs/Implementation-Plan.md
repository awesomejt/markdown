# Implementation Plan

## Planning principles

- Extract and prioritize requirements before selecting implementation reuse.
- Prove native performance on every desktop before deepening one platform.
- Deliver thin vertical slices, then coherent basic MVPs, then refinements and
  advanced functionality.
- Share semantics and fixtures by default; share code only with evidence.
- Record moderate platform deviations instead of forcing lowest-common-
  denominator behavior.

## Phase 1 — requirements and feasibility

Tasks: REQ-001, FEAS-001 through FEAS-006, and GATE-001.

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
