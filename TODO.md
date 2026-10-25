# Project Roadmap

`project.yaml` is authoritative for live status, dependencies, blockers,
leases, and passes. This document defines task scope and acceptance criteria.

## Current gate: requirements and feasibility

No production application or shared core is scaffolded before GATE-001. Small
throwaway prototypes under `spikes/` are expected.

### Requirements and feasibility

- **DEC-001 — Accept native monorepo strategy.** Record owner approval of D-004
  and align durable project documentation. Completed 2026-10-24.
- **REQ-001 — Rebaseline requirements and repository strategy.** Extract the
  web project's functionality into phased native requirements; make Windows,
  macOS, and Linux first-class; document iOS constraints; recommend repository
  boundaries. Acceptance: requirements, platform, architecture, repository,
  implementation, memory, decisions, and task state agree.
- **FEAS-001 — Freeze semantic and performance corpora.** Copy representative,
  licensed inputs and semantic expectations from `markdown-viewer` at a pinned
  commit. Add large/hostile performance fixtures and a provenance manifest.
- **FEAS-002 — Compare core/parser candidates.** Measure Rust/Comrak,
  cmark-gfm/C or C++, and justified platform alternatives. Compare semantics,
  source spans, extension cost, safety, performance, memory, package impact,
  and maintainability.
- **FEAS-003 — Prove Windows native rendering.** Prototype the basic corpus in
  the proposed Windows-native text/layout stack; measure launch, layout,
  scrolling, selection, accessibility, and high-DPI behavior.
- **FEAS-004 — Prove macOS native rendering.** Prototype the same corpus using
  AppKit/SwiftUI candidates; measure equivalent native behaviors.
- **FEAS-005 — Prove Linux native rendering.** Prototype the same corpus using
  GTK/native Linux candidates; measure equivalent native behaviors across the
  selected desktop environments.
- **FEAS-006 — Prove shared-core integration.** Package and consume a minimal
  candidate core on Windows, macOS, and Linux. Measure call/copy overhead,
  lifecycle, errors, cancellation, debugging, and release packaging.
- **FEAS-007 — Complete the platform capability matrix.** Test advanced
  rendering, file watching, network folders, background work, print/export,
  and later iOS/iPadOS constraints. Mark supported, deferred, alternative, or
  unavailable per platform.
- **GATE-001 — Accept implementation baseline.** Record core/parser choice,
  facade, desktop UI stacks, baseline OS/architectures, performance budgets,
  package formats, platform order, and advanced-feature scope.

### Portable contract and core

- **CORE-001 — Scaffold monorepo workspaces and CI.** Create only the accepted
  core and native app shells, path-filtered platform jobs, lint/test commands,
  dependency inventory, and license checks.
- **CORE-002 — Define semantic document model v1.** Version blocks, inlines,
  spans, stable identity, diagnostics, outline, properties, link/resource
  intents, capabilities, and unsupported-feature nodes with goldens.
- **CORE-003 — Implement parsing and safety policy.** Produce bounded semantic
  snapshots against the conformance corpus.
- **CORE-004 — Implement document session semantics.** Define generations,
  cancellation, immutable revisions, external changes, and scroll anchors
  without platform filesystem assumptions.
- **CORE-005 — Implement search and read metadata.** Add source-accurate find,
  workspace search primitives, word count, section timing, and budgets.
- **CORE-006 — Stabilize desktop facade and packages.** Publish coarse values,
  typed errors, cancellation/lifetime handles, and model negotiation to all
  three native desktop test hosts.

### Desktop walking skeletons

- **WIN-001 — Windows single-document skeleton.** Native file open, core call,
  basic GFM blocks, outline, theme/text controls, selection, accessibility,
  external refresh, and a development package.
- **MAC-001 — macOS single-document skeleton.** Equivalent native macOS slice
  with AppKit/SwiftUI responsibilities decided from measurements.
- **LNX-001 — Linux single-document skeleton.** Equivalent native Linux slice
  with GTK/desktop integration and supported environment assumptions recorded.
- **DESK-001 — Cross-desktop skeleton gate.** Compare semantics and performance,
  fix contract bias, document deviations, and choose the MVP deepening order.

### Desktop basic MVPs

- **WIN-002 — Windows basic MVP.** Folder workspace, recents, browse/find/search,
  preferences, watcher/restoration, native integration, and performance tuning.
- **MAC-002 — macOS basic MVP.** Equivalent basic functionality optimized for
  macOS permissions, filesystem, menus, text system, and packaging.
- **LNX-002 — Linux basic MVP.** Equivalent basic functionality optimized for
  Linux filesystems, portals, desktop environments, and packages.
- **DESK-002 — Desktop MVP conformance and release gate.** Validate all three
  basic products, budgets, capability documentation, privacy, and artifacts.
- **DESK-003 — Desktop refinement.** Prioritize R2 features and platform-specific
  polish after the basic MVP gate.

### Mobile

- **IOS-001 — Prove iOS/iPadOS integration.** Validate core packaging, UIKit/
  SwiftUI text rendering, memory, lifecycle, document providers, and capability
  deviations.
- **IOS-002 — Build iOS/iPadOS basic MVP.** Implement the mobile-appropriate R1
  subset with native adaptive UI, access recovery, restoration, share/print,
  performance, and real-device evidence.

### Advanced functionality

- **ADV-001 — Reading refinement parity.** Selected styles, properties, timing,
  focus, print/share/export, and project settings by platform.
- **ADV-002 — Math and diagrams.** Implement only platform/engine combinations
  accepted by FEAS-007 and GATE-001, with safe offline fallbacks.
- **ADV-003 — Projects.** Add compatible or migrated project files and
  cross-root behavior where permission models allow it.
- **ADV-004 — Voice-over and teleprompter.** Add selected script semantics,
  timing, presentation, wake behavior, and documented platform deviations.

## Dependency summary

```text
REQ-001 -> FEAS-001 -> FEAS-002 -------------------------+
       \-> FEAS-003 (Windows renderer) ------------------+
       \-> FEAS-004 (macOS renderer) --------------------+-> GATE-001
       \-> FEAS-005 (Linux renderer) --------------------+
       \-> FEAS-006 (shared-core integration) -----------+
       \-> FEAS-007 (capability matrix) -----------------+

GATE-001 -> CORE-001 -> CORE-002 -> CORE-003/CORE-004
CORE-003 + CORE-004 -> CORE-005 -> CORE-006
CORE-006 -> WIN-001 + MAC-001 + LNX-001 -> DESK-001
DESK-001 -> WIN-002 + MAC-002 + LNX-002 -> DESK-002 -> DESK-003
DESK-002 -> IOS-001 -> IOS-002
DESK-002 -> ADV-001/ADV-002/ADV-003/ADV-004
```
