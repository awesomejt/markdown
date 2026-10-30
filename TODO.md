# Project Roadmap

`project.yaml` is authoritative for live status, dependencies, blockers,
leases, and passes. This document defines task scope and acceptance criteria.

## Implementation delivery policy (after GATE-001)

CORE/WIN/MAC/LNX/IOS entries below are aggregate milestones. Execute the small,
platform-focused delivery phases in [the catalog](docs/Implementation-Plan.md),
batching ready work on the current OS. Each delivery phase uses one feature
branch and PR into `develop`. Opening the PR when ready starts the Grok review;
address the feedback, and merge into `develop` once a review has no serious
finding (D-029). Unreviewed housekeeping uses `patch/` branches (D-033). The owner reviews and merges milestones from
`review/<milestone>` into `main` (D-030). Follow
[Delivery-Workflow.md](docs/Delivery-Workflow.md) for branch, review and handoff
rules. GATE-001 is approved (D-018). CORE-001A through CORE-003A, CORE-003B1
through CORE-003B4 (with B2F), CORE-004A, CORE-005A/A2, CORE-005B/B2 and
CORE-006A are merged, and the CORE-003B aggregate is closed. The owner's
[holistic CORE review](docs/reviews/2026-10-28-core-holistic-review.md)
(PR #16, merged) accepted D-025 (native hosts use model v2). Reconciled with the
[Grok](docs/reviews/2026-10-28-grok-shared-core-review.md) and
[Codex](docs/reviews/2026-10-28-codex-shared-core-review.md) reviews, its
follow-ups form the [CORE-FIX phase](#core-fix-phase-shared-core-review-follow-ups)
(D-026), which completes before any platform phase starts. Work runs as one
linear shared track, then one independent chain per host (D-027; see the
[dependency summary](#dependency-summary)). ADV-005 carries deferred engine
work after DESK-002.

## Current phase: portable core (shared track)

The gate is complete; the small child tasks are materialized in project state.
`portable-core` now holds only shared work. It completes when CORE-006 closes,
which activates `desktop-apps` and the platform chains.
CORE-001A infrastructure, CORE-002A semantic v1 and CORE-003A basic parsing are
merged. CORE-003B1 adds [bounded typed front matter](docs/Front-Matter.md).
CORE-003B2 adds [opt-in v2 provenance/URI targets](specs/semantic-model/v2/README.md)
while preserving default v1 (owner-selected D-020).
Native app shells remain separate; no working reader UI is delivered by the
core alone.

## Immediate session handoff

- The current task, branch and exact next step are in [HANDOFF.md](HANDOFF.md).
- The current posture, host notes and risks are in [MEMORY.md](MEMORY.md).
- The merge-by-merge history of the completed shared phases, the CORE-003B2F
  scope and the GATE-001 preparation steps are in
  [the memory archive](agents/Memory-Archive.md#todomd-session-handoff-history).

The shared-core reviews (PR #16) are merged, and so is the
[CORE-FIX phase](#core-fix-phase-shared-core-review-follow-ups): DOC-001,
CORE-002B, TOOL-002 and CORE-003C in PRs #17 to #21, the D-029 and D-030
workflow changes in PR #22 and on `main`, TOOL-003 in PR #23, CORE-003D in
PR #24, CORE-005C in PR #25, CORE-005D in PR #26 and CORE-003E in PR #27
(D-032). The owner merged the first milestone review (PR #28) into `main`,
tagged `m-core-fix`. Its first [follow-up](#core-fix-milestone-follow-ups),
CORE-003F, merged in PR #32 (D-034); CORE-001 closed in PR #33 with both
TOOL-003 findings fixed and hosted three-OS CI green. CORE-002 is next on `develop`. Inspect
state and `HANDOFF.md` before any pass. Scoped commits, pushes, PR updates and
review requests are authorized; a Grok review with no serious finding approves
a merge into `develop` (D-029); only the owner merges into `main` (D-030);
concurrency remains one.

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

The CORE aggregates close in order (CORE-001 to CORE-006) at the end of the
shared track, after CORE-FIX, on shared acceptance only. Native evidence
belongs to each platform's 001A and 001B (D-027).

- **CORE-001 — Scaffold monorepo workspaces and CI.** The accepted core
  workspace, platform-isolated CI definitions, lint/test commands, dependency
  inventory and license checks, passing in hosted CI on all three OSes. Each
  native shell's CI job is that platform's 001A. Includes the two TOOL-003
  deferred findings: first-push whitespace checks must handle forced Git color
  and non-UTF-8 problem content, with regression tests that still reject new
  whitespace violations.
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
  typed errors, cancellation/lifetime handles and model negotiation through the
  C ABI. Release-mode Rust/FFI tests pass in hosted CI on all three OSes. Each
  native package consumer is that platform's 001B. Closing CORE-006 completes
  `portable-core`.

### CORE-FIX phase: shared-core review follow-ups

Three reviews of the shared core at the same implementation baseline (`39d1a56`):
[Claude holistic](docs/reviews/2026-10-28-core-holistic-review.md) (F1–F7),
[Codex](docs/reviews/2026-10-28-codex-shared-core-review.md) (C1–C7) and
[Grok](docs/reviews/2026-10-28-grok-shared-core-review.md) (G1–G6). None
found a correctness, memory-safety or file-access defect. The owner
dispositioned them on 2026-10-28 (D-026). CORE-FIX starts the shared track
and runs before any platform phase, including the 001A shells. State chains
the seven tasks in the order below, so only the next one is eligible. Each task
is one feature branch and PR under
[Delivery-Workflow.md](docs/Delivery-Workflow.md).

1. **DOC-001 — Agent-facing docs cleanup (F5, G5, C6).** Docs only. It goes
   first because every later session reads these files.
   - Cut MEMORY.md's posture to a current summary of about 30 lines. Move the
     run logs and per-phase notes to an archive file, without deleting any
     durable fact.
   - Remove the stale CORE-003B2 "Resume here" section from HANDOFF.md.
   - Fix stale status text: D-020 still says it awaits merge, HANDOFF.md
     calls D-024 "proposed", and some contract headers still say "under
     review".
   - Trim this file's "Immediate session handoff" to pointers.
   - Qualify the snippet invariant in the
     [workspace-search spec](specs/workspace-search/README.md#snippets).
     `normalize(match) == normalize(query)` does not hold for sanitized
     (U+FFFD) or clipped display text. Source ranges and anchors stay
     authoritative. The tested display behavior does not change.
   - Acceptance: Markdown lint and local link checks pass, and the
     required-reading files agree with `project.yaml` and Decisions.md.
2. **CORE-002B — Import-policy parity and FFI host guidance (F3, F4, G3, C2).**
   - V1 validation, including `from_json`, re-runs the parser's own
     link/resource policy (`classify_link`/`local`) for intents that are not
     denied, as v2 does.
   - Shared structural validation (`validate_structure`, which v1 and v2 both
     use) applies the producer's control/Cf/bidi guards to the metadata title
     and to scalar and list string properties. It keeps the deliberate
     allowance for variation selectors and CGJ in display text.
   - Scope is parity with the producer, not a stricter policy. The v1 parser
     itself emits `https://user:pw@…` as `External` (Codex's correction to
     F3), so v1 `from_json` keeps accepting it. Native hosts get credential
     denial from v2 (D-025).
   - Add host guidance to the [C ABI spec](specs/ffi/README.md): call the
     find, read-stats and workspace functions off the UI thread, and size
     record arrays from `max_matches` so that one call is enough.
     Cancellable variants are added only if 001F measures a need (D-022
     additive rule).
   - Acceptance: regression tests for the review probes on both models.
     Traversal, encoded dot segments, whitespace and a U+202E destination are
     rejected; a U+202E title or property is rejected under v1 and v2. No
     wire, schema or parser-golden change. One owner-approved exception
     (2026-10-28): the hand-written v1 model golden `resources.json` claimed
     `requires_capability` for `../café.md#part`, which the parser denies,
     so that one intent is now denied, with the source and spans unchanged.
3. **CORE-003C — V2 corpus conformance assertions (C5).** Test-only. After
   D-025, v2 is the shipped model, but the generated conformance suite
   compares only the outline and the link count for v2.
   - Run the supported corpus check tokens on v2 as well, with explicit
     version-specific expectations for intents rather than identical wire
     shapes.
   - Cover resources, metadata, diagnostics, capability flags and denied
     schemes. Keep the coverage for references that v1 deliberately leaves
     inert.
   - It goes before CORE-003D so that the autolink change is checked against
     the strengthened v2 assertions.
4. **CORE-003D — Bare GFM autolinks (C3).** R1.2 requires GFM autolinks, and
   the owner decided on 2026-10-28 to implement them in the shared core.
   Today only angle autolinks link.
   - One bounded shared pass detects `http(s)://`, `www.` and email
     autolinks, following the GFM extended-autolink rules for trailing
     punctuation, balanced parentheses and entity-like suffixes.
   - It produces exact source spans across split text events. It never
     applies inside code spans, code blocks, raw HTML or existing links.
   - Targets pass through the normal URI policy. Under v2, the provenance
     re-parse proves them like any other link.
   - It respects parse limits, cancellation and the counted heap budgets.
   - Record in Decisions.md whether v1 output also gains autolinks. This is
     a change to parser output, not to the wire format.
   - Add conformance fixtures, mutation and pathological cases (long
     URL-like runs, many `www.` prefixes), and regenerate the goldens on
     purpose with a reviewed diff.
   - Update [Parsing.md](docs/Parsing.md).
5. **CORE-005C — Workspace content-search memory guard (F2, G1, C1, G4).**
   - Lower the per-file default for content search, for example to 2 MiB,
     and/or add a delimiter-density admission check that skips with a typed
     reason.
   - Any density heuristic gets false-positive tests on legitimate documents
     (the corpus prose and table profiles).
   - Prove the bound with a counted heap budget, with Rust/FFI parity. Do not
     present it as an application memory bound: hosts still cap concurrency,
     and each 001B measures its own cost.
   - Document in the [workspace-search spec](specs/workspace-search/README.md)
     that content search is a text index that does not depend on the model
     version, so its v1 parse is deliberate (owner decision, 2026-10-28).
     Richer semantics come from the document session.
   - Each platform's 002C needs it.
6. **CORE-005D — Wider invisible-filename warnings (C4).**
   `WorkspacePath::has_invisible()` flags only Cf characters.
   - Also flag blank characters that are not Cf: the Hangul fillers U+115F,
     U+1160, U+3164 and U+FFA0, and U+2800 BRAILLE PATTERN BLANK. Reuse the
     v2 URI table of invisible characters where it fits.
   - Keep the ZWJ/ZWNJ exemptions. Never reject or rewrite a name, and keep
     the exact name and the opaque entry ID.
   - Add Rust/FFI flag parity tests and update the workspace-search spec.
     Each platform's 002B needs it.
7. **CORE-003E — Front-matter invisible-character parity (Grok, PR #17).**
   The owner added it on 2026-10-28 as its own follow-up, without widening
   CORE-002B or CORE-005D. `metadata::unsafe_character` covers only Cf, while
   v2 URI targets deny Default_Ignorable_Code_Point and U+2800 (B2F) and
   CORE-005D flags Hangul fillers and U+2800 in filenames.
   - Decide and record whether the metadata title and scalar and list string
     properties also reject or diagnose blank or invisible characters that
     are not Cf: the Hangul fillers, U+2800 and other default-ignorable
     values. Keep the variation-selector and CGJ display allowance.
   - Reuse the shared tables with provenance: since CORE-005D,
     `metadata::invisible_character` (Default_Ignorable plus U+2800) and
     `metadata::display_modifier` sit beside `unsafe_character`. Apply the rule in the
     producer and in shared `validate_structure`, so v1, v2 and `from_json`
     keep the parity CORE-002B establishes.
   - Acceptance: v1 and v2 tests for each new range and for the allowance,
     goldens unchanged or regenerated with a reviewed diff, and
     docs/Front-Matter.md and the model specs updated.

**TOOL-002 — CI speed-up (owner request after PR #18, before CORE-003C).**
Cache Rust builds in the core job, split `tools/check.py` into a PR tier and a
`core-extended` tier that runs on `main` and manual runs, and skip the host
matrix for PR pushes that add only state or docs after a green head (D-028).
Acceptance: regression tests for the skip rules, `tools/check.py all` passes,
green hosted CI with a measured warm-cache time and a state-only push that
skips the host steps.

Not in CORE-FIX, kept where they belong:

- **D-025 enforcement (F1, G2), flat-view/RSS measurement (F2) and the
  required MSVC C smoke (F6, C7).** These belong to each platform's 001B. It
  builds the core with `model-v2`, requests v2, measures the flat-view and
  retained-RSS cost for one document and for repeated documents on its host,
  and on Windows makes the MSVC C smoke a required CI step. If 001F measures
  real latency from the search calls that can't be cancelled (F4), add
  cancellable variants under D-022.
- **HARD-001 — Release hardening (F7, G6, C7).** Follows DESK-001 and blocks
  DESK-002. Coverage-guided fuzzing of the parser, the model decoder and the
  safe FFI inputs (with valid backing buffers), sanitizer runs where
  supported, and a dependency advisory scan wired into the checks.
- **Maintenance (F7, C7), when a concrete change touches the code.** Split
  `parse.rs` by responsibility, replace ordinal parsing of IDs with a side
  index, and stop charging footnote labels to the copy budget twice. Any
  upgrade of the `pulldown-cmark` pin is its own gated task that re-runs the
  conformance, mutation and pathological suites. No broad refactor.

### CORE-FIX milestone follow-ups

The [CORE-FIX milestone review](docs/reviews/README.md) (PR #28, tag
`m-core-fix`) had three independent reviewers: Claude, Grok and Codex. None
found a blocking defect. The items they agreed on, or that should be
addressed later, are recorded here and in state:

- **CORE-003F — Front-matter joiner allowance (M1, all three).** Done in
  PR #32 (D-034): joiners between visible characters and subdivision flags
  are allowed in titles and values, not keys; LRM, RLM and bidi controls
  stay rejected. A new task
  between CORE-003E and CORE-001. Front matter rejects ZWNJ and ZWJ (Cf), so a
  title or value holding either discards the whole map: Persian,
  Arabic-script, Indic and emoji ZWJ titles lose their metadata. File names
  already exempt both.
  - Allow ZWNJ and ZWJ in the title and string and list values, not in keys,
    in the producer and in shared validation (v1, v2, `from_json`).
  - Keep the bidi embedding, override and isolate controls rejected. LRM and
    RLM stay rejected unless the owner decides otherwise (Grok suggested
    allowing them; Codex treats it as a separate choice).
  - Decide and record whether emoji tag characters are allowed only inside a
    well-formed tag sequence (U+1F3F4 … U+E007F), as Grok suggests.
  - It amends D-032, so record a new decision. Acceptance: v1 and v2 tests
    for Persian, Devanagari and ZWJ emoji, keys still rejecting joiners, bidi
    controls still rejected, goldens unchanged or regenerated with a reviewed
    diff.
- **TOOL-003 deferred findings (M3, C-M4).** Resolved by CORE-001 in PR #33:
  color-free diffs, lossless decoding and safe filename diagnostics, with
  real-Git regressions; new whitespace violations still fail.
- **2 MiB content-search default (Grok, C-M2).** A zero `max_file_bytes` now
  means 2 MiB instead of 8 MiB. Reviewers differ on whether that strains
  D-022; it changes no layout or signature and no host had shipped. CORE-006
  records the before/after behavior in the FFI spec.
- **Session memory bound (Grok, C-M3).** Content search is bounded, but a
  session can still reach about 1,152 MiB on a dense 8 MiB input. Each
  host's 001B now measures that and sets the host's session
  `max_source_bytes`, or records why the default is acceptable, before
  IOS-001.
- **No change:** the ASCII-letter autolink boundary matches cmark-gfm (M2).
  The specs already tell hosts to cap concurrent content searches.
- **Done:** `main` requires `Checks / complete` (M4, C-M5), approved by the
  owner and applied 2026-10-29.

### Desktop walking skeletons

Each host is one chain in `desktop-apps`, from CORE-006 through 001A–001I to
its 001 aggregate (D-027). The 001 aggregate also confirms the native CI job
(001A) and the release-mode core package consumer (001B) that CORE-001 and
CORE-006 no longer carry. The three host chains are independent.

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
  continuous reading on KDE Wayland. LNX-001E and everything after it depend
  on this early proof; failure returns to the owner before deeper renderer work.
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
  Needs HARD-001 (release hardening) as well as the three 002 aggregates.
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
- **Follow-up:** Revisit persistent cross-revision node identity (see D-021).

## Deferred review findings

Low findings and nits from Grok reviews that were merged under D-029 without
being fixed. A later pass fixes them or records why not; remove each entry
when it is resolved.

None open after CORE-001 (PR #33). The two TOOL-003 findings from PR #23,
Grok review 5692910610, are resolved: forced color and non-UTF-8 problem
content no longer cause false failures. Both PR #33 review nits are also fixed.

## Process options under consideration

Harder gates the owner deferred on 2026-10-28 to see how the D-030 workflow
works first, revisited at the first milestone review (PR #28).

- None open. Requiring the `Checks / complete` status on `main` was applied
  on 2026-10-29 after all three CORE-FIX milestone reviewers recommended it.

## Dependency summary

```text
REQ-001 -> FEAS-001 -> FEAS-002 ---------------------------+
       \-> FEAS-003 (Windows renderer) --+                  |
       \-> FEAS-004 (macOS renderer) ----+-> FEAS-006 ------+-> GATE-001
                                         +-> FEAS-007 ------+
       \-> FEAS-005 (Linux renderer) -----------------------+

Completed shared foundation (history):
GATE-001 -> CORE-001A -> CORE-002A -> CORE-003A -> CORE-004A -> CORE-006A
CORE-003A -> CORE-003B1..B4 (with B2F) -> CORE-003B
CORE-004A -> CORE-005A -> CORE-005A2;  CORE-005A -> CORE-005B -> CORE-005B2

Shared track (portable-core, one chain; D-026, D-027):
DOC-001 -> CORE-002B -> TOOL-002 -> CORE-003C -> CORE-003D -> CORE-005C -> CORE-005D
  -> CORE-003E -> CORE-003F -> CORE-001 -> CORE-002 -> CORE-003 -> CORE-004 -> CORE-005 -> CORE-006
  -> portable-core complete, desktop-apps active

Each host (desktop-apps; MAC, LNX and WIN chains are independent):
CORE-006 -> 001A -> 001B -> 001C -> 001D [-> LNX-001D1] -> 001E -> 001F
  -> 001G -> 001H -> 001I -> MAC-001 / LNX-001 / WIN-001

Joins:
MAC-001 + LNX-001 + WIN-001 -> DESK-001
DESK-001 -> HARD-001
DESK-001 -> each host 002A -> ... -> 002F -> platform 002 aggregate
  (002B also needs CORE-005D; 002C also needs CORE-005B, CORE-005B2, CORE-005C)
HARD-001 + all platform 002 aggregates -> DESK-002 -> DESK-003
DESK-002 -> IOS-001 -> IOS-002
DESK-002 -> ADV-001/ADV-003/ADV-004
DESK-002 -> ADV-005 -> ADV-002
```

Live state in `project.yaml` matches these chains: every open task has one
open predecessor, except the DESK-001 and DESK-002 joins. The catalog in
[Implementation-Plan.md](docs/Implementation-Plan.md) defines each child's
scope. The platform 002 children are created in state at DESK-001 closeout.
