# Project Roadmap

`project.yaml` is authoritative for live status, dependencies, blockers,
leases, and passes. This document defines task scope and acceptance criteria.

## Implementation delivery policy (after GATE-001)

CORE/WIN/MAC/LNX/IOS entries below are aggregate milestones. Execute the small,
platform-focused delivery phases in [the catalog](docs/Implementation-Plan.md),
batching ready work on the current OS. Each delivery phase uses one feature
branch and PR. Once ready, post a separate PR comment exactly `/grok review`,
address the feedback, and obtain owner merge approval. Follow
[Delivery-Workflow.md](docs/Delivery-Workflow.md) for branch, review and handoff
rules. GATE-001 is approved (D-018). CORE-001A, CORE-002A and CORE-003A are merged; CORE-003B1 is selected;
ADV-005 carries deferred engine work after DESK-002.

## Current phase: portable core and early native foundations

The gate is complete; the small child tasks are materialized in project state.
CORE-001A infrastructure, CORE-002A semantic v1 and CORE-003A basic parsing are
merged. CORE-003B1 adds [bounded typed front matter](docs/Front-Matter.md).
Native app shells remain separate; no working reader UI is delivered by the
core alone.

## Immediate session handoff

Work on `feat/core-003b1-front-matter` through its PR/check/Grok lifecycle.
CORE-003A merged state closeout travels here. CORE-003B was refined into four
tracked children: B1 metadata; B2 reference representation/URI compatibility;
B3 remaining extension semantics; B4 conformance/fuzz/property and parser
budgets. See [the catalog](docs/Implementation-Plan.md) for scope/dependencies.
The parent retains all original acceptance and native packaging gates.
CORE-004A remains independently ready. Inspect state and `HANDOFF.md` before
any pass. Scoped commits/pushes/PR/review are authorized; earlier review/merge
exceptions do not apply to this phase. Merge requires owner authorization;
concurrency remains one.

The numbered steps below are the earlier handoff for GATE-001 preparation,
kept as a record; steps 1–3 are done.

1. **Carry forward Windows FEAS-003.** The report in
   `docs/Windows-Rendering-Evaluation.md` records 60 native trials, continuous
   text versus virtualized blocks, C#/C++ host smokes, owner-confirmed Narrator,
   clipboard, same-scale multi-display checks, source/PDB debugging and a local
   development package. Preserve the 12 timeouts and unverified mixed-DPI,
   external UIA, managed-debugging and profiling behaviors. No production stack
   was accepted.
2. **Carry forward the completed macOS FEAS-004 evidence.** The report in
   `docs/macOS-Rendering-Evaluation.md` compares TextKit 1/2, reusable rows and
   SwiftUI hosting, with named-host measurements, native selection/copy,
   owner-confirmed VoiceOver reading, scale-2 display inspection, debugger,
   profiler and local development bundles. Preserve its unverified production
   behaviors and signing/distribution limits; do not treat them as passes.
3. **Reconcile the platform reports in the monorepo.** Preserve deviations;
   do not force identical UI implementations. Confirm each pass is complete
   and `project.yaml` has no stale lease before selecting more work.
4. **Prepare GATE-001.** The capability matrix is in
   `docs/Platform-Capability-Evaluation.md`; its “Implications for GATE-001”
   section lists the decisions the owner must make or defer. Ask the owner to
   accept or revise GATE-001; do not accept it on the owner's behalf.

### Requirements and feasibility

- **TOOL-001 — Restore portable project-state coordination.** Detect the
  Python runtime host, preserve the existing state contract, and provide a
  guarded portable backend patch. Acceptance: Windows native locking/atomic
  writes, POSIX compatibility, disposable-state concurrency/conflict/idempotency
  and failure checks, real-project inspection and supported pass closeout.
  Platform evidence and unrun host checks are recorded in `AGENT_WORKFLOW.md`.

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
  Completed 2026-10-24: eight semantic cases, four deterministic performance
  profiles, three schemas, copied Apache-2.0 license, and offline validation.
- **FEAS-002 — Compare core/parser candidates.** Measure Rust/Comrak,
  cmark-gfm/C or C++, and justified platform alternatives. Compare semantics,
  source spans, extension cost, safety, performance, memory, package impact,
  and maintainability. Completed 2026-10-24: pulldown-cmark is the lead Rust
  candidate for the next core/integration spikes; Comrak remains the semantic
  fallback/oracle and cmark-gfm the GFM conformance oracle.
  Supplemental macOS measurements, including Swift Markdown, are recorded in
  `docs/Parser-Evaluation-macOS.md`. They retain the lead recommendation and
  require investigation of repeated-parser RSS before production acceptance.
- **FEAS-003 — Prove Windows native rendering.** Evaluated 2026-10-25 on
  Windows 11/Ryzen 9 5950X: WinUI 3 shell viable; rich edit is a bounded-document
  candidate and block virtualization needs global selection/accessibility.
  See `docs/Windows-Rendering-Evaluation.md` for failures and remaining checks.
- **FEAS-004 — Prove macOS native rendering.** Prototype the same corpus using
  AppKit/SwiftUI candidates; measure equivalent native behaviors. Evaluated
  2026-10-24 on Apple M5 Max/macOS 27.0.1: TextKit 2 is the lead document-surface
  candidate; TextKit 1 far navigation stalls and reusable rows lose continuous
  selection. See `docs/macOS-Rendering-Evaluation.md` for measured limits and
  deferred production checks. All three desktop renderer evaluations are now recorded.
- **FEAS-005 — Prove Linux native rendering.** Prototype the same corpus using
  GTK/native Linux candidates; measure equivalent native behaviors across the
  selected desktop environments. Completed 2026-10-24 on KDE Wayland with an
  XWayland smoke: GTK 4/gtk-rs is viable, but a monolithic `GtkTextView` is not
  the production large-document strategy; LNX-001 must prove virtualized
  blocks without losing selection or accessibility.
- **FEAS-006 — Prove shared-core integration.** Starts only after FEAS-003 and
  FEAS-004. Package and consume a minimal
  candidate core on Windows, macOS, and Linux. Measure call/copy overhead,
  lifecycle, errors, cancellation, debugging, and release packaging.
  Windows checkpoint evaluated 2026-10-25: Rust C ABI, C#/C++ ownership,
  errors/cancellation, WinUI integration and local package passed; continuous
  controls retain layout failures. macOS checkpoint evaluated 2026-10-25:
  Swift/AppKit ownership, cancellation, concurrency, live source debugging and
  relocated normal-library bundles passed. Repeated native RSS grows in empty
  malloc regions despite zero handles; no production mitigation is accepted.
  Linux checkpoint evaluated 2026-10-25: a separate gtk-rs host consumed the C
  facade with negotiated layouts, copied ownership, lifecycle/error/panic/
  cancellation coverage, GTK timing, Rust source debugging and a stripped,
  path-scanned relocated normal-library package. The monolithic long-line case
  retained an 11.4-second layout delay; no production ABI/model or renderer is
  accepted. See the three `docs/Shared-Core-*-Evaluation.md` reports.
- **FEAS-007 — Complete the platform capability matrix.** Starts only after
  FEAS-003 and FEAS-004. Test advanced
  rendering, file watching, network folders, background work, print/export,
  and later iOS/iPadOS constraints. Mark supported, deferred, alternative, or
  unavailable per platform. Evaluated 2026-10-25: 30 atomic capabilities in
  `docs/Platform-Capability-Evaluation.md`, each with evidence, fallback and a
  named next check, mechanically validated by `tools/validate_capabilities.py`.
  iOS/iPadOS cells remain authoritative or unverified until IOS-001.
- **REV-001 — Review feasibility studies.** Owner-requested quality review of
  FEAS-001–007 before GATE-001. Acceptance: reports checked against raw data,
  host-reproducible results rerun, factual errors corrected and findings in
  `docs/Feasibility-Review.md`. Completed 2026-10-25; it blocks GATE-001 in state.
- **FEAS-010 — Prove core syntax extensions on pulldown-cmark.** Owner-added at
  GATE-001. Prove front matter, math, GitHub alerts, voice-over syntax, diagram
  fences and other custom syntax with exact spans, and compare with Comrak.
  Acceptance: each extension passes its semantic expectation with span checks,
  costs are measured on the performance corpus, and a recommendation keeps or
  replaces pulldown-cmark. Evaluated 2026-10-25: keep pulldown-cmark; see
  `docs/Core-Extension-Evaluation.md`.
- **FEAS-008 — Prove virtualized native renderer and Windows host language.**
  Owner-added at GATE-001; runs on Linux and Windows. Acceptance: whole-document
  selection and Ctrl+A copy of 5 MiB; Orca and Narrator reading order
  (owner-confirmed plus AT-SPI/UIA inspection); find and heading jump to
  off-screen blocks; local images and links under resource policy; D-012
  launch-to-presented-content timings. On Windows, the same view in C# (JIT
  and Native AOT) and C++/WinRT; on both, a WebView document baseline comparing
  performance with implementation cost. Linux part evaluated 2026-10-25:
  recommend native `GtkListView`; see
  `docs/Virtualized-Renderer-Linux-Evaluation.md`. Windows part evaluated
  2026-10-26: recommend a native WinUI `ListView` in C# Native AOT, with
  C++/WinRT as the fallback; see
  `docs/Virtualized-Renderer-Windows-Evaluation.md`.
- **FEAS-009 — Compare diagram and math engines.** Owner-added at GATE-001.
  For Mermaid, D2 and KaTeX-syntax math, compare an offscreen web engine,
  existing engines (D2 in Go) and custom Rust implementations or ports.
  Acceptance: latency, memory, fidelity, sanitization, package size, iOS fit,
  port effort and licensing per engine, with a recommendation. Mac checkpoint
  2026-10-26: compiler-only KaTeX/JSC, Mermaid/WKWebView and D2/Go screen;
  see `docs/Diagram-Math-Engine-Evaluation.md`. Owner requests best-effort,
  timeboxed work. Closed by owner-approved scope disposition D-014: Windows/
  Linux checks and deeper fidelity, native presenter/sanitizer, Rust-candidate
  and iOS engine/runtime work move to ADV-005 after DESK-002. These original
  criteria remain unrun; closure is a deferral, not a full technical pass.
- **GATE-001 — Accept implementation baseline.** Completed by owner approval
  D-018 on 2026-10-26. Choices/conditions D-008–D-017 and the 36-task/92-edge
  initial graph are recorded in `docs/Implementation-Baseline.md` and live state.
  The rehearsal, native evidence, approved deferral and phase transition are
  recorded. CORE-001A starts implementation; no spike model becomes production
  merely through gate acceptance.

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

- **WIN-001 — Windows single-document skeleton.** WinUI 3/C# Native AOT
  (C++/WinRT fallback only for a concrete blocker). Native file open, core call,
  basic GFM blocks, outline, theme/text controls, selection, accessibility,
  external refresh, and a development package. Following FEAS-003, prove
  off-thread bounded parsing/cancellation and virtualized blocks with global
  selection and UIA reading order; exercise mixed DPI, font scaling and real
  input frame pacing before accepting a large-document strategy.
- **MAC-001 — macOS single-document skeleton.** Equivalent native macOS slice
  with AppKit/SwiftUI responsibilities decided from measurements.
- **LNX-001 — Linux single-document skeleton.** GTK/gtk-rs native virtualized
  blocks, conditional on LNX-001D1 after basic text/navigation: prove AT-SPI
  heading semantics, off-screen heading navigation and owner-confirmed Orca
  continuous reading on KDE Wayland. LNX-001E/001F depend on this early proof;
  failure returns to the owner before deeper renderer work.
- **DESK-001 — Cross-desktop skeleton gate.** Compare semantics and performance,
  fix contract bias, document deviations, and choose the MVP deepening order.
  Before closeout, materialize the platform MVP delivery tasks and dependencies.

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
- **ADV-002 — Math and diagrams.** After ADV-005, implement only explicitly
  accepted platform/engine combinations, with safe offline fallbacks.
- **ADV-003 — Projects.** Add compatible or migrated project files and
  cross-root behavior where permission models allow it.
- **ADV-004 — Voice-over and teleprompter.** Add selected script semantics,
  timing, presentation, wake behavior, and documented platform deviations.

- **ADV-005 — Validate post-MVP diagram and math engines.** Owner-deferred
  FEAS-009 remainder (D-014). Depends on DESK-002 and blocks ADV-002. Reuse the
  bounded Mac evidence, run short Windows/Linux compiler/resource checks, then
  compare shortlisted engines with real MVP documents. Acceptance: named-host
  latency distributions, total process memory/package costs, reference fidelity,
  native presentation/accessibility, SVG/resource safety, licensing/provenance,
  Rust alternatives/port effort and relevant iOS fit or explicit scoped deferral;
  recommend adoption or further deferral per engine. Split into bounded
  platform/engine delivery tasks before implementation. It lives in the
  desktop-apps state bucket so it need not wait for the entire mobile phase.

## Dependency summary

```text
REQ-001 -> FEAS-001 -> FEAS-002 ---------------------------+
       \-> FEAS-003 (Windows renderer) --+                  |
       \-> FEAS-004 (macOS renderer) ----+-> FEAS-006 ------+-> GATE-001
                                         +-> FEAS-007 ------+
       \-> FEAS-005 (Linux renderer) -----------------------+

GATE-001 -> CORE-001A -> CORE-002A -> CORE-003A -> CORE-004A -> CORE-006A
CORE-003A -> CORE-003B1, CORE-003B2
CORE-003B1 -> CORE-003B3; CORE-003B2 + CORE-003B3 -> CORE-003B4
CORE-003B1–B4 -> CORE-003B; CORE-004A -> CORE-005A -> CORE-005B
CORE-001A -> each host 001A; CORE-006A + own 001A -> own 001B -> ... -> 001I
CORE-005A -> own 001F; CORE-003B -> own 001I
LNX-001D -> LNX-001D1 (early accessibility proof) -> LNX-001E + LNX-001F
all CORE + WIN/MAC/LNX-001 aggregate acceptance -> DESK-001
DESK-001 -> each host 002A -> ... -> 002F -> platform 002 aggregate
CORE-005B -> own 002C; all platform 002 aggregates -> DESK-002 -> DESK-003
DESK-002 -> IOS-001 -> IOS-002
DESK-002 -> ADV-001/ADV-003/ADV-004
DESK-002 -> ADV-005 -> ADV-002
```

The abbreviated chains above are orientation only; the catalog defines precise
child dependencies (including independent branches). The live state still
represents the pre-GATE-001 queue until supported graph expansion at the gate.
