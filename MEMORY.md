# Project Memory

Durable, concise context for future humans and agents. Detailed research and
design belong in `docs/` or `agents/`; runtime transcripts do not belong here.

## Current posture

- Phase: feasibility.
- Product: local-first, read-only native Markdown reader.
- First platforms: Windows, macOS, and Linux desktop applications.
- Later platforms: iOS and iPadOS after the desktop contracts are stable.
- Organization: one native monorepo with independent native UIs,
  shared specifications/fixtures/benchmarks, and a candidate portable core.
- Status: cross-machine handoff. FEAS-001, FEAS-002, and Linux FEAS-005 are
  complete. FEAS-003 on Windows and FEAS-004 on macOS are the only eligible
  next tasks; there is no active pass.
- Blockers: no technical blocker. Progress now requires real Windows and macOS
  hosts. Shared-core integration, the capability matrix, and GATE-001 are
  intentionally ordered after both native rendering evaluations.

## Durable context

- The related `markdown-viewer` repository was reviewed at commit `0ab59c0`
  on 2026-10-24. Its worktree was clean and its `main` was 258 commits ahead of
  `origin/main`; the review therefore describes local behavior that may not be
  published upstream.
- `markdown-viewer` is primarily a requirements and functionality source for
  this project. Existing code or dependencies are reused only when they fit the
  native performance and platform goals.
- The web application remains independently useful for LAN/iPad access. The
  native project does not replace it and must not regress it.
- “Native” currently means a platform-owned application and document UI, not a
  wrapper around the complete React app. Diagram compilation may use a bundled
  non-UI engine if security and App Store constraints allow it.
- Mermaid and D2 are the largest parity risks. D2 is rendered by a Go library
  in the web project; iOS cannot depend on spawning that renderer. Diagram
  support is gated and excluded from the first vertical slice.
- Desktop applications should establish thin native walking skeletons on all
  three operating systems early, before deepening one implementation enough to
  bias the shared contract.
- Moderate platform deviation is acceptable. Limitations must be documented in
  a capability matrix and surfaced clearly to users.
- `project.yaml` is stored as readable block-style YAML. State mutations use
  `tools/project_state.py`, which preserves YAML while delegating the atomic
  project-state contract to the installed backend.
- The independent corpus under `fixtures/` is pinned to `markdown-viewer`
  commit `0ab59c0f7bf91730bf9683b47a5eb2658f331b00`: eight semantic cases,
  four generated performance profiles, provenance/hashes, JSON Schemas in
  YAML, and a validator that does not access the sibling repository.
- FEAS-002 recommends Rust with pulldown-cmark as the lead core/parser spike:
  it had the lowest parse/walk time and RSS and exposes byte ranges for every
  event. Comrak remains the feature-rich fallback/conformance oracle, while
  cmark-gfm remains the reference GFM oracle. GATE-001 has not yet accepted a
  production parser or FFI mechanism.
- FEAS-005 validates GTK 4/gtk-rs for the Linux native shell and small-document
  text surface on KDE Wayland/XWayland. One monolithic `GtkTextView` is rejected
  for unbounded production documents: far-scroll did not converge for the
  1 MiB/5 MiB profiles, a 1 MiB line blocked over ten seconds, and peak process
  RSS was roughly 254–289 MiB. LNX-001 must prove virtualized semantic blocks
  while preserving cross-block selection and AT-SPI reading order.
- The current architecture recommendation is a Rust semantic core using
  pulldown-cmark, with Comrak as fallback/oracle and cmark-gfm as the reference
  GFM oracle. C++ is not the lead shared-core choice, but C++/WinRT remains a
  valid Windows-host candidate. A narrow C ABI or generated bindings remain
  the likely interop boundary. None of these recommendations becomes an
  accepted production baseline until FEAS-006 and GATE-001.
- Platform UI code is intentionally native rather than forced into one common
  language: GTK 4/gtk-rs is the Linux recommendation; Windows must compare the
  suitable WinUI 3 host options; macOS should measure AppKit with SwiftUI only
  where it helps. iOS/iPadOS remain later Swift/UIKit/SwiftUI work.
- The owner requires real target-host evidence from Windows and macOS before
  further architecture decisions. `project.yaml` therefore makes FEAS-003 and
  FEAS-004 prerequisites of FEAS-006 and FEAS-007 as well as GATE-001.

## Open decisions

- Confirm the order in which the three desktop MVPs are deepened after their
  walking skeletons exist.
- Accept or reject the recommended Rust/pulldown-cmark semantic core after
  cross-platform packaging and host-integration evidence from FEAS-006.
- Select WinUI 3 with C# or C++/WinRT, the Windows document layout strategy,
  and the stable Rust boundary using measurements from a real Windows host.
- Select AppKit/SwiftUI responsibilities, the macOS document layout strategy,
  and the stable Rust/Swift boundary using measurements from a real Mac.
- Confirm Linux virtualized-block selection and AT-SPI behavior during LNX-001;
  a monolithic `GtkTextView` is already rejected for unbounded documents.
- Confirm whether “native” allows narrowly scoped non-UI engines for diagrams
  while retaining platform-native document presentation.
- Decide how much voice-over/script behavior belongs in the first public
  native release.

## Known risks

- A rich native attributed-text document can become slow or inaccessible if
  represented as one monolithic view.
- Parser source ranges may be insufficient for every web behavior; the core
  model needs stable block identities and explicit source mapping.
- iOS security-scoped bookmarks can become stale and require user recovery.
- Raw HTML, remote resources, local links, and SVG each need separate policy.
- FFI and packaging must be proven on Windows, macOS, and Linux before the
  shared core expands; Apple XCFramework concerns are one part of that matrix.

## Run log

### 2026-10-24 — planning baseline

Reviewed `markdown-viewer`, documented reuse boundaries, proposed an
Apple-first/Rust-core architecture, defined phased feasibility gates, and
created the repository workflow, memory, validation, and task-state system.

### 2026-10-24 — desktop-first clarification

Reframed the project around extracted requirements and performance rather than
code reuse. Windows, macOS, and Linux are first-class desktop targets; iOS and
iPadOS follow. Recommended one monorepo with platform-native UIs, shared
contracts and fixtures, and documented capability differences.

### 2026-10-24 — monorepo accepted

The owner accepted D-004: the native Windows, macOS, Linux, and later iOS/
iPadOS applications will live in one repository while keeping their UI,
platform adapters, builds, and release artifacts independent.

### 2026-10-24 — FEAS-001 corpus frozen

Added byte-identical foundation and advanced Markdown cases with Apache-2.0
provenance, parser-independent semantic expectations, Unicode/NFD path cases,
and deterministic 1 MiB/5 MiB prose plus hostile delimiter/long-line profiles.
`tools/validate_fixtures.py` verifies schemas, completeness, hashes, and
generated content entirely within this repository.

### 2026-10-24 — FEAS-002 parser comparison

Pinned and measured Comrak 0.57.0, pulldown-cmark 0.13.5, and cmark-gfm
0.29.0.gfm.13 against the frozen performance and semantic inputs. The event
parser was materially faster and lighter on the named Linux workstation, but
its semantic-model construction cost is still unmeasured. Recorded Rust with
pulldown-cmark as the lead feasibility direction, with Comrak as the fallback;
the production choice remains gated on model and cross-platform integration
evidence.

### 2026-10-24 — FEAS-005 Linux rendering

Built a native GTK 4/gtk-rs `GtkTextView` spike over the frozen corpus and
measured real frame-clock, selection, scroll, AT-SPI, Wayland/XWayland, memory,
and dynamic-package behavior. The 1 MiB prose case first-framed in about 299 ms
on the named workstation, but far-scroll and pathological long-line behavior
rule out a monolithic text buffer as the universal renderer. GTK remains the
recommended Linux shell; virtualized blocks are the next renderer hypothesis.

### 2026-10-24 — native desktop handoff

Recorded the owner-required sequencing stop after Linux evaluation. The next
sessions run FEAS-003 on a real Windows system and FEAS-004 on a real Mac. No
shared-core integration, platform capability conclusion, or implementation
baseline decision should proceed until both results are recorded. The task
graph was tightened so only those two native evaluations are eligible.
