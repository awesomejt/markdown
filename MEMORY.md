# Project Memory

Durable, concise context for future humans and agents. Detailed research and
design belong in `docs/` or `agents/`; runtime transcripts do not belong here.

## Current posture

- Phase: feasibility.
- Product: local-first, read-only native Markdown reader.
- First platforms: Windows, macOS, and Linux desktop applications.
- Later platforms: iOS and iPadOS after the desktop contracts are stable.
- Proposed organization: one native monorepo with independent native UIs,
  shared specifications/fixtures/benchmarks, and a candidate portable core.
- Status: planning baseline created; feasibility tasks are ready.
- Blockers: none. Several design choices intentionally remain gated by spikes.

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

## Open decisions

- Confirm the order in which the three desktop MVPs are deepened after their
  walking skeletons exist.
- Select Rust, C++, platform libraries, or a narrow combination based on the
  conformance and performance corpus rather than code-reuse preference.
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
