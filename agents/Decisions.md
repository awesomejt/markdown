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

## D-008 — Rust core with pulldown-cmark

**Date:** 2026-10-25

**Status:** accepted by owner at GATE-001 (gate still open)

The portable semantic core is Rust, using pulldown-cmark as the parser. Comrak
remains the fallback and cross-check oracle; cmark-gfm remains the GitHub
Flavored Markdown reference. The core must support front matter, math, GitHub
alerts and the advanced and custom syntax from `markdown-viewer`. FEAS-010
proves what that takes on pulldown-cmark; if it can't be done well, CORE-003 may
switch to Comrak behind the same interface.

## D-009 — Hand-written versioned C interface

**Date:** 2026-10-25

**Status:** accepted by owner at GATE-001 (gate still open)

Apps call the core through a narrow, versioned, hand-written C ABI that hands
over immutable values in bulk copies, as proven by FEAS-006 on all three
desktops. Generated bindings such as UniFFI are not used. The FEAS-006 flat
data model is not the production contract; CORE-002 and CORE-006 replace it.

## D-010 — Native platform stacks

**Date:** 2026-10-25

**Status:** accepted by owner at GATE-001 (gate still open)

- macOS: AppKit with TextKit 2 for the document; SwiftUI only for shell
  surfaces such as settings.
- Linux: GTK 4 with gtk-rs; libadwaita optional, not required.
- Windows: WinUI 3. The host language (C# or C++/WinRT) is decided from the
  FEAS-008 comparison.

The document renderer on Windows and Linux depends on FEAS-008.

## D-011 — MVP platform baselines and distribution

**Date:** 2026-10-25

**Status:** accepted by owner at GATE-001 (gate still open)

| Platform | MVP target | Distribution | After the MVP |
| --- | --- | --- | --- |
| Windows | Windows 11, x64 | MSIX | ARM64 |
| macOS | macOS 27, Apple Silicon | Developer ID, signed and notarized | Earlier macOS versions; Mac App Store optional |
| Linux | x86-64, KDE Plasma Wayland | Flatpak plus an Arch package | Debian and RPM packages; other desktops; ARM64 |

Windows 10 is not supported. Signing, notarization and publishing remain
separate tasks that need owner authorization.

## D-012 — Shared performance metric

**Date:** 2026-10-25

**Status:** accepted by owner at GATE-001 (gate still open)

Every platform budget uses launch to presented content: from process launch
to the first frame of document content on screen. Report cold and warm starts,
at least five runs with spread, on named hardware. Budget numbers are set from
FEAS-008 measurements, not from the earlier per-platform metrics.

## D-013 — v1 scope and delivery order

**Date:** 2026-10-25

**Status:** accepted by owner at GATE-001 (gate still open)

- v1 (the MVP) contains the core Markdown features only. It opens files and
  folders through the system; it doesn't read `*.mdv.yaml` project files.
- After the MVP, advanced features follow in this order: voice-over and
  expanded syntax, teleprompter, Mermaid, math, then D2.
- After all three walking skeletons exist, the MVPs are deepened in this order:
  macOS, Linux, Windows.

## Open at GATE-001

GATE-001 stays open until these are decided from spike evidence:

- **Main document surface.** The owner prefers native. FEAS-008 measures a
  WebView document baseline against native virtualized blocks for performance
  and implementation cost. The native-only guardrail in AGENTS.md stays in
  force unless a later decision changes it.
- **Windows host language,** from the FEAS-008 C#, Native AOT and C++/WinRT
  comparison.
- **Diagram and math engines.** FEAS-009 compares an offscreen web engine,
  existing engines (D2 in Go) and custom Rust implementations, aiming for the
  best-performing safe path. A D2 port must stay MPL-2.0 file by file; Mermaid
  and KaTeX are MIT. Go and web-engine use are not accepted yet.
- **Core extension approach,** from FEAS-010.
- **Budget numbers,** from FEAS-008 using the D-012 metric.
