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
hosts. Each phase uses a feature branch and PR. Once ready, post a separate PR
comment exactly `/grok review`, assess the actual review and address findings;
owner merge authorization remains required. Scoped phase commits, pushes, PR
updates and review comments are authorized as routine steps of this workflow.

Use `docs/Delivery-Workflow.md` and the catalog in `docs/Implementation-Plan.md`.
Retain broad state buckets and aggregate milestones; materialize their smaller
execution tasks through supported helper operations at prerequisite gates.
Early native foundation tasks share the portable-core bucket so host batching
works with active-bucket selection. Preserve all-platform gates, native evidence
and concurrency one. This decision changes future workflow, not today's partial
FEAS-006 handoff or the unaccepted production baseline.

## Proposed decisions for GATE-001

The following are not yet accepted:

- Rust semantic core, initially spiked with pulldown-cmark and retaining
  Comrak as the parser fallback/oracle, with platform-native presentation.
- The experimental FEAS-006 bulk C ABI: Windows and macOS ownership/cancellation
  and native UI evidence are positive, but the native Linux binding/package and the
  production semantic model must be reconciled before accepting the interface.
- No general-purpose WebView for the main document surface.
- GTK 4/gtk-rs for the Linux shell, with a virtualized semantic-block document
  renderer rather than one unbounded `GtkTextView`.
- Windows and macOS framework/language choices after their measured spikes.
- WinUI 3 shell with bounded read-only rich edit and a virtualized semantic
  block strategy to be proven for Windows; FEAS-003 does not select between
  C# and C++/WinRT or accept cross-row selection/accessibility limitations.
- AppKit/TextKit 2 as the lead macOS document-surface candidate following
  FEAS-004; selective SwiftUI hosting remains available, while synchronous
  model construction, memory and pathological blocks need further work.
- Tier 1 excludes Mermaid and D2. The FEAS-007
  [capability matrix](../docs/Platform-Capability-Evaluation.md) found no safe
  offline native path for Mermaid without a browser engine, and none for KaTeX
  output without a presenter. D2 is feasible as a pure Go library.
- Whether a bundled offscreen web engine may act as a non-UI diagram or math
  compiler, and whether Go is accepted as a second implementation language for
  D2. Both need owner decisions recorded here before ADV-002.
