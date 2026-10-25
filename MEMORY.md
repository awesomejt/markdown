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
- Status: FEAS-001 through FEAS-005 are evaluated. FEAS-006 has completed
  Windows and macOS checkpoints but remains partial for native Linux integration;
  resume it before selecting other work. Inspect `project.yaml` for the live
  pass/handoff state. No production baseline or ABI is accepted.
- The required real Windows and macOS renderer evidence is now recorded.
  Shared-core integration and the capability matrix can proceed; GATE-001
  still waits for their results and the owner's architecture decision.
- Mac host setup: the project-state skill/backend and workflow contract have
  been restored from the owner's Linux user-level Codex directory. Supported
  inspection works. Xcode 27.1 is installed and selected. Use the normal pass
  lifecycle and re-inspect rather than relying on an earlier revision. The
  reviewed portability patch is now applied to this installed Mac backend,
  with its original backup; native coordination and fixture/path checks passed.
- Windows host setup: Codex roles/skill were copied from Mac, and the native
  build tools and fixture Python dependencies are installed. State coordination
  is restored: the backend detects native Windows versus POSIX Python, and
  `tools/patch_project_state_backend.py` reproduces the reviewed patch on each
  host. Windows and isolated native Mac regression suites passed on 2026-10-25;
  Linux rerun is outstanding. Use `--platform-info` to verify the selected path.

## Implementation workflow decision (2026-10-25)

D-007: after GATE-001, use small platform-focused delivery phases, each with its
own feature branch and PR. Batch dependency-ready phases on the current host.
Post a separate `/grok review` comment when ready, resolve actual feedback and
wait for owner merge approval. Scoped phase pushes/PR/review actions are already
authorized. See `docs/Delivery-Workflow.md` and `docs/Implementation-Plan.md`.
Materialize child tasks at prerequisite gates through the helper; early native
foundation tasks belong in the portable-core bucket. Today's FEAS-006 state is
unchanged; Linux integration remains the next execution work.

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
  Windows and macOS checks passed on 2026-10-25; Linux rerun remains outstanding.
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
- FEAS-006 Windows proves an experimental bulk C ABI with Rust-owned immutable
  snapshots, C# SafeHandle/C++ RAII cleanup and off-thread WinUI consumption.
  Virtualized 1 MiB/5 MiB prose rendered at about 381/459 ms authored-entry
  callbacks; Rust parse/model was about 9.5/42.6 ms. Continuous controls still
  stall. Handles return to zero through lifecycle/cancellation/panic tests;
  native Rust source/PDB debugging and a normal-DLL package passed. The model
  has snapshot-local IDs and incomplete semantic coverage; native Linux host/package
  evidence remains required. See the shared-core Windows report.
- FEAS-006 macOS imports the same C header into Swift, bulk-copies values and
  releases native snapshots/tokens before AppKit construction. All eight outline
  contracts pass. The final 54 GUI trials include 51 presentations, two intended
  cancellations and one retained TextKit 1/5 MiB timeout. TextKit 2 entry-to-draw
  medians were about 217/498 ms for 1/5 MiB prose; 5 MiB core/model and copy/decode
  were about 14.6/5.5 ms in GUI runs. Main-actor/strict concurrency checks,
  live Swift-to-Rust source debugging and relocated normal-library bundles passed.
  This is not a cold-launch or equivalent-renderer language comparison.
- Repeated Mac native-only 5 MiB parses grew RSS to about 583 MiB after twenty
  loads despite zero native owners and about 5.1 MiB live malloc usage. A leaks
  scan found zero leaked bytes; vmmap classified most growth as empty large
  malloc regions. Swift copying shows similar retention. Allocator pressure
  relief released zero bytes. Keep this production memory risk open; no fix or
  acceptable repeated-session budget is established by those bounded diagnostics.
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

### 2026-10-25 — FEAS-006 macOS checkpoint

Resumed the Windows partial handoff with the normal state lifecycle. Added native
Swift value/lifetime and synchronized cancellation adapters, strict concurrency
checks, off-thread AppKit consumption, timing/error/concurrency tests and
relocatable probe/normal dylib builds. Final ownership tests passed 1,000 sequential
plus 400 concurrent loads and ten cancellations after observed native entry.
All native counts returned to zero; host conversion failure and panic recovery
passed. Preserved layout failures and investigated rising RSS as empty-region
retention rather than claiming a universal leak proof. Live LLDB source and
mixed Swift/Rust frames passed after owner-approved/admin debugging setup;
the agent status query still reported disabled, so global authorization state
is not inferred. No further permission changes were made. Fresh normal-library
ZIP relocation, console/GUI/cancellation smokes, Rust gates, fixture/path checks
and the original FEAS-004 regression passed. See the Mac integration report.
Closed the Mac pass as partial at revision 98, with no active/stale passes
or warnings. FEAS-006 intentionally remains working without a lease and carries
an exact Linux handoff. No production ABI/model accepted.

### 2026-10-25 — FEAS-006 Windows checkpoint

Built Rust/pulldown-cmark 0.13.5 behind a versioned value-only C ABI and consumed
it from C# and C++ on native Windows. Validated byte/structure negotiation,
status errors, allocator ownership, 1,000 sequential and 400 concurrent loads,
ten in-flight cancellations, panic containment and native source/PDB resolution.
The 41 GUI trials include 37 completions, one intended cancellation and three
retained continuous-control timeouts. Virtualized 5 MiB entry-to-render was
about 459 ms, with 42.6 ms core work and 19.9 ms managed copy/decode; this is
not a cold-launch/presentation or equivalent-frontend language comparison.
Fixed inline-math source preservation after an outline mismatch; all eight
final outline contracts pass. Fresh unsigned packages use the normal DLL,
exclude probe exports/Markdig, and launch C#/C++ callers locally. Overall
FEAS-006 remains partial pending native macOS/Swift and Linux/GTK evidence.
Closed the Windows pass as partial at revision 96: no active/stale passes or
warnings. FEAS-006 intentionally remains working without a lease, with an exact
handoff; resume/recover it before selecting new work.

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
