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

**Status:** owner selected the additive versioning direction; implementation
contract awaits phase review and owner-authorized merge.

CORE-003B2 requires reference destinations outside their referring node, while
frozen v1 requires occurrence-contained destination spans. Add opt-in v2 with
explicit inline/reference provenance while preserving v1's API, strict decoder,
schema and fixtures. Do not silently reinterpret v1 fields or default formats.
New adapters explicitly choose supported versions; facade negotiation remains
CORE-006A. The v2 contract and explicit migration are in
`specs/semantic-model/v2/README.md`; targets grant no I/O or URI activation.
