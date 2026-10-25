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
- Status: FEAS-001 through FEAS-005 are evaluated on their named hosts,
  including Windows FEAS-003. FEAS-006/FEAS-007 follow; inspect `project.yaml`
  for pass state and dependency readiness. No production baseline is accepted.
- The required real Windows and macOS renderer evidence is now recorded.
  Shared-core integration and the capability matrix can proceed; GATE-001
  still waits for their results and the owner's architecture decision.
- Mac host setup: the project-state skill/backend and workflow contract have
  been restored from the owner's Linux user-level Codex directory. Supported
  inspection works. Xcode 27.1 is installed and selected. Use the normal pass
  lifecycle and re-inspect rather than relying on an earlier revision.
- Windows host setup: Codex roles/skill were copied from Mac, and the native
  build tools and fixture Python dependencies are installed. State coordination
  is restored: the backend detects native Windows versus POSIX Python, and
  `tools/patch_project_state_backend.py` reproduces the reviewed patch on each
  host. Windows and isolated native Mac regression suites passed on 2026-10-25;
  Linux rerun is outstanding. Use `--platform-info` to verify the selected path.

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
- Unicode fixture contents now use ASCII filenames in Git. The manifest's
  `runtime_path` generates Unicode/NFC/NFD names in separate ignored run
  directories; `tools/validate_fixtures.py` checks three generated paths.
  `.gitattributes` preserves exact corpus bytes across Windows checkouts.
  Windows checks passed on 2026-10-25; Linux/macOS reruns remain outstanding.
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
  language: GTK 4/gtk-rs is the Linux recommendation; Windows FEAS-003 supports
  WinUI with C# or C++/WinRT hosts, and macOS FEAS-004 favors AppKit/TextKit 2.
  iOS/iPadOS remain later Swift/UIKit/SwiftUI work.
- Windows FEAS-003 compared rich text, read-only rich edit and virtualized
  rows in 60 native trials. Twelve timed out; 5 MiB continuous controls did
  not reach a render callback within 45 seconds. Rich edit navigates bounded
  text better, while blocks realize few views but lack document-wide selection.
  Markdig is only a slow synchronous spike adapter, not a production choice.
  Native clipboard, owner-confirmed Narrator, same-scale three-display sweep,
  source/PDB debugging and development-folder launches passed. Mixed DPI,
  full external UIA, managed/FFI debugging and clean deployment remain unverified;
  WPR capture was blocked by profiling policy. See the Windows evaluation.
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
- Confirm the AppKit/TextKit 2 lead renderer recommendation, selective SwiftUI
  shell responsibilities and the stable Rust/Swift boundary at the accepted
  gates. macOS FEAS-004 measured native layout; FEAS-006 still owns core interop.
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

### 2026-10-25 — Windows FEAS-003 rendering

On Ryzen 9 5950X/64 GiB/RTX 4080/Windows 11 26300.9711, the WinUI spike
completed 48 of 60 primary trials, preserving 12 timeouts. At 1 MiB prose,
entry-to-render medians were 3.61 s rich text, 3.94 s rich edit and 1.20 s
virtualized rows. At 5 MiB, block mode rendered in about 16.15 s, dominated
by roughly 15.68 s in the provisional Markdig adapter. All 34 reached
continuous selections matched; blocks have no global selection. The owner
confirmed Narrator audio and turned it off afterward; clipboard comparison
and a corrected three-display 150% sweep passed. C++ literal host and native
source/PDB debug smokes passed without establishing a host-language winner.
No signing/publishing was performed. Reports, lockfile, source and durable
measurements are in `spikes/feas003/` and `docs/Windows-Rendering-Evaluation.md`.
FEAS-003 closed at revision 94 with no active/stale passes, no unleased working
tasks and no warnings. Next: FEAS-006; FEAS-007 is also dependency-ready.
GATE-001 remains unaccepted.

### 2026-10-25 — portable state coordination

Owner requested Windows/POSIX backend repair after the fixture-only exception.
Minimal bootstrap restored inspection at revision 89 without changing history;
subsequent work used the normal TOOL-001 pass lifecycle. The installed backend
is backed up beside its source. A hash-guarded repository installer preserves
unknown/local edits and makes the patch transferable without vendoring another
state engine. Windows uses a bounded exclusive byte-range sidecar lock for
readers and writers; POSIX keeps shared/exclusive flock. The YAML wrapper and
JSON backend share synced temporary-file replacement. Native Windows and isolated
Mac suites each passed eight checks with one opposite-platform skip, covering
concurrency, idempotency, failure cleanup and lifecycle. No Mac repository or
installed skill was changed. TOOL-001 completed at revision 92 with no active
or stale passes, no unleased working tasks, and no warnings; only FEAS-003 was
eligible. Linux native testing remains outstanding.

### 2026-10-25 — portable Unicode filesystem fixtures

Owner authorized a fixture-only exception while the Windows project-state
backend remains blocked by its Unix-only `fcntl` import. No pass was opened
and `project.yaml` was left unchanged. Replaced tracked Unicode filenames
and the redundant NFC alias with two ASCII-named inputs, retaining the frozen
content hashes, origin commit and license. Runtime generation isolates NFC/NFD
spellings, reports actual directory entries and canonical lookup behavior,
and never overwrites existing output. Windows validation passed for eight
semantic cases, four performance profiles and three generated paths; four
generator regression tests passed. Native adapter normalization fallback and
Linux/macOS filesystem behavior are not established by these checks.

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

### 2026-10-24 — macOS FEAS-002 supplement

On Apple M5 Max/macOS 27.0.1, release probes plus Swift Markdown 0.9.0
completed 120 native runs. For 5 MiB prose, median parse/walk was 11.94 ms
pulldown-cmark, 34.05 ms Comrak, 42.85 ms cmark-gfm, and 377.02 ms Swift
Markdown. Keep pulldown-cmark as the lead feasibility candidate, but investigate
repeated-parser RSS: its single-iteration peak was 63.63 MiB while repeated
timing trials reached about 119 MiB. Swift Markdown is not Apple-only; the
earlier platform exclusion was corrected. Details and limitations are in
`docs/Parser-Evaluation-macOS.md`. Owner authorized this supplement without
the absent state backend; no pass or task status changed. FEAS-004 is next
on this Mac once normal pass coordination is restored.

### 2026-10-24 — Mac project-state backend restored

Restored the existing `project-yaml-state-management` skill and referenced
contract from the owner's Linux home over authorized SSH. Skill file hashes
match; no Linux project files or local task-state fields were changed.
The repository wrapper now inspects state successfully. The FEAS-004 workflow
blocker is resolved; full Xcode 27.1 is also installed and selected. See
`HANDOFF.md` for resume instructions.

### 2026-10-24 — FEAS-004 macOS rendering

On Apple M5 Max/macOS 27.0.1/Xcode 27.1, compared AppKit TextKit 1, TextKit 2,
reusable text rows and a SwiftUI-hosted TextKit 2 surface in 80 successful
native trials. All 60 continuous-text runs exported matching selections;
all variants reached the end. Recommend TextKit 2 as the lead surface: 1 MiB
first-draw median was 491 ms and 5 MiB end request about 0.8 ms, versus 2.55 s
for TextKit 1. First draw is not full launch/presentation. Reusable rows save
views but lack cross-row selection/navigation. Synchronous attributed-model
construction, 5 MiB memory near 388 MiB, and long-line timer delays remain
production risks. Owner confirmed audible VoiceOver reading; its original off
state was restored. LLDB, Instruments and local development bundles passed
their smoke checks; identity signing/notarization were not performed. Details
are in `docs/macOS-Rendering-Evaluation.md`. Next: real Windows FEAS-003;
shared-core and capability work still wait for Windows evidence.
