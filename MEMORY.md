# Project Memory

Durable, concise context for future humans and agents. Detailed research and
design belong in `docs/` or `agents/`; runtime transcripts do not belong here.

## Current posture

- Phase: portable-core (GATE-001 approved by owner, D-018).
- Product: local-first, read-only native Markdown reader.
- First platforms: Windows, macOS, and Linux desktop applications.
- Later platforms: iOS and iPadOS after the desktop contracts are stable.
- Organization: one native monorepo with independent native UIs,
  shared specifications/fixtures/benchmarks, and a candidate portable core.
- Status 2026-10-28 (latest): every shared CORE delivery phase through
  CORE-006A is merged. The owner's holistic CORE review
  (docs/reviews/2026-10-28-core-holistic-review.md, PR #16) found no
  correctness or memory-safety defect. The owner accepted D-025: native 001B
  packages build `markdown-ffi` with `model-v2` and request v2, because v1
  denies `../`, non-ASCII and percent-encoded links. Follow-ups in state:
  CORE-002B (v1 `from_json` re-checks producer intent policy; FFI off-thread,
  single-call guidance), CORE-005C (content-search memory guard; blocks each
  002C) and DOC-001 (trim this file and HANDOFF/TODO). Each 001B also measures
  flat-view and retained RSS, and WIN-001B makes the MSVC C smoke required.
  Grok and Codex review PR #16; reconcile their reviews before its closeout.
  Status lines below are older history.
- CORE-003B retains B1 metadata, B2 references/URI, B3 other extension semantics
  and B4 conformance/fuzz/performance with original aggregate/native gates.
- Status 2026-10-28: CORE-003B4 merged (PR #13 `29f7334`, reviewed head
  `9b6bd46`) and the CORE-003B aggregate is closed (docs/Parser-Budgets.md). The generated
  `specs/parsing/conformance/corpus.json` maps every fixture expectation to a
  Rust check or a reasoned deferral (`tools/generate_conformance.py --check`
  is in the common suite). There are mutation and pathological-input tests
  and counted heap budgets. The 5 MiB prose profile hit `NodeLimit`, so D-024
  (accepted) raises the defaults to 1,000,000 nodes and 2,000,000 events.
  Faster validation (hash index, visit bitmap) halved parse time: 5 MiB prose
  takes 143 ms with a 145 MiB peak on the M5 Max. The source-offset fallback
  now replaces only the affected top-level block. Grok (PR #13) found dense
  delimiter storms peak at about 1,152 MiB at the 8 MiB ceiling (unchanged
  from base); now documented and a 2 MiB storm is budgeted. `$` rejoins are
  refunded from the event limit. Next: the owner's holistic CORE review,
  before any platform phase.
- CORE-003B3 (merged, PR #12 `66c2162`). One shared `PARSER_OPTIONS` (also
  used by v2 provenance reparsing) enables GFM footnotes, math and alert tags.
  Alerts and footnotes map to existing v1/v2 nodes. Footnote references
  carry the first definition's exact label (direct `unicase` 2.10.0, the
  parser's own folding), and later folded duplicates are inert fallbacks.
  Math is an `unsupported_inline` source fallback, and `mdv: script` front
  matter adds an info `voice_script_unavailable`. There is no model, schema,
  golden or ABI change. Reading stats now match markdown-viewer for
  alerts, math and footnotes.
- CORE-005B2 (merged, PR #11 `28e33bc`): four D-022 workspace search
  functions (24 exports). Classify path, find names over an entry array, and
  content admit/search with packed snippets/anchors and a caller-owned in/out
  progress written back only on success. Hit `unit` indexes a v1 view only.
- CORE-005B (merged, PR #10 `0bf8eb7`): `markdown_core::workspace`
  (specs/workspace-search/README.md) takes adapter entries (opaque `u64` ID
  plus validated `/`-relative display path; the core never resolves paths)
  and bytes. It provides a shared Markdown/hidden filter (md/markdown/mdx,
  dot components; viewer parity), ranked file-name search and per-file
  content search via find with visible-text snippets, line numbers, anchors
  and `SourceInfo`. Budgets live in a caller-held plain `ContentProgress`
  value with permanent stops, which suits D-022. The owner split the FFI
  exposure into CORE-005B2 (blocks CORE-005; each 002C needs it).
- CORE-005A2 (merged, PR #9 `ec58c84`): `md_snapshot_find` and
  `md_snapshot_read_stats` (specs/ffi/README.md) are the first D-022
  additions (20 exports). They fill caller-owned record arrays with two-call
  `MD_ERR_BUFFER_TOO_SMALL` sizing. Options carry the record element sizes,
  so every new struct size is checked on each call. There is no cancel
  handle. Rust parity tests and the C smoke cover them.
- CORE-005A (merged, PR #8 `237cd88`): `markdown_core::search`
  (specs/search/README.md) adds literal source-mapped find (simple case fold,
  no NFC/NFD normalization, approximate cluster/whole-word boundaries, bounded
  and cancellable), simplified-UAX #29 word count (one word per CJK
  ideograph) and intro/heading section timing at 160 wpm (markdown-viewer
  D-32 parity). Core semantics only; the owner split FFI exposure into
  CORE-005A2 (required by each 001F) under D-022: ABI v1 grows only by new
  functions filling caller-owned arrays. D-023 freezes the PR #7 flat view.
- CORE-006A (merged, PR #7 `a751f48`): crate `markdown-ffi` and the
  hand-written `core/include/markdown_core.h` (specs/ffi/README.md) expose
  registry handles, exact struct-size/ABI/model negotiation, typed status
  codes, contained panics, sessions/jobs/anchors and a bulk flat snapshot
  view. A host-`cc` C smoke runs in core checks (macOS/Linux; Windows skips).
  Native consumers remain the platform 001B phases; CORE-006 stays open.
- CORE-004A (merged): `markdown_core::session` adds `Arc`
  snapshots, generations, latest-wins publication, cooperative cancellation,
  no-I/O external-change state and block anchors (specs/session/README.md).
  Node IDs remain snapshot-local; anchors carry cross-generation identity
  (owner-accepted for the MVP in D-021; persistent IDs deferred).
- Previously selected: CORE-003B2 on `feat/core-003b2-references-uri`. Owner selected
  additive opt-in model v2 (D-020), preserving v1 APIs/decoder/schema/goldens.
  Feature `model-v2` exposes `parse_markdown_v2` and `v2::Document`: explicit
  reference/inline source provenance, source-backed validation and typed URI
  targets. V1 remains default; explicit upgrades retain denials; no lossy
  downgrade or facade/native format switch is implied. See the v2 spec and
  docs/URI-Policy.md for decode-once/capability rules and residual limits.
  URL/IDNA is optional; 60 all-feature packages with reviewed scoped Unicode
  exceptions. Metadata/engines/FFI/UI/session ownership are not expanded here.
  PR #5 Grok review found invisible non-Cf Unicode can pass URI and shared
  metadata guards; v1 blockquote-continuation destination spans also changed
  as a correction without a dedicated v1 regression. These were fixed in PR #5
  (B2F scope): URI targets deny Default_Ignorable_Code_Point values and U+2800,
  while metadata keeps Cf only; there is a v1 span regression; and v2 has a
  bounded `reference_definition_shadowed` info diagnostic. After verifying the
  merge, carry the B2 and B2F state closeout in the next feature PR. Node identity/
  cancellation remains CORE-004A; preserve Linux LNX-001D1 and ADV-005 conditions.
- License: owner selected Apache-2.0 for implementation source (D-019).
- The required real Windows, macOS, and Linux renderer and shared-core
  integration evidence is recorded, and the capability matrix reconciles it.
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
  host. Windows, isolated native Mac, and native Linux regression suites passed
  on 2026-10-25. Use `--platform-info` to verify the selected path.
- Windows host facts from FEAS-008 (2026-10-26):
  - Agent shells are not elevated. Cold-cache runs (standby purge) need the
    owner to run the harness from an elevated terminal.
  - Developer Mode is on, so Python `os.symlink` works; PowerShell 5.1
    `New-Item -ItemType SymbolicLink` still needs admin.
  - The feasibility venv has no pip, and markdownlint-cli2 is not installed.
  - Native AOT linking needs `vswhere` on `PATH`; `build_windows.py` adds it.
  - Displays are three 2560×1440 at 150%.
  - Use the native UIA3 client (`UiaProbe.exe`), not Windows PowerShell's
    managed UIA, which lacks `HeadingLevel`.
  - Use forward slashes in Git Bash commands given to the owner.

## Implementation workflow decision (2026-10-25)

D-007: after GATE-001, use small platform-focused delivery phases, each with its
own feature branch and PR. Batch dependency-ready phases on the current host.
Post a separate `/grok review` comment when ready, resolve actual feedback and
wait for owner merge approval. Scoped phase pushes/PR/review actions are already
authorized. See `docs/Delivery-Workflow.md` and `docs/Implementation-Plan.md`.
Materialize child tasks at prerequisite gates through the helper; early native
foundation tasks belong in the portable-core bucket. FEAS-006 is evaluated;
FEAS-007 and GATE-001 are complete; D-014 moves FEAS-009's unrun work to
ADV-005 after the desktop MVP.

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
  Windows, macOS, and Linux checks passed on 2026-10-25.
- FEAS-002 recommends Rust with pulldown-cmark as the lead core/parser spike:
  it had the lowest parse/walk time and RSS and exposes byte ranges for every
  event. Comrak remains the feature-rich fallback/conformance oracle, while
  cmark-gfm remains the reference GFM oracle. D-008/D-009 later accepted the
  Rust/pulldown-cmark core and hand-written C ABI; the spike model stays disposable.
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
  accepted production baseline until GATE-001, now approved by D-018.
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
  has snapshot-local IDs and incomplete semantic coverage. See the shared-core
  Windows report.
- FEAS-006 macOS imports the same C header into Swift, bulk-copies values and
  releases native snapshots/tokens before AppKit construction. All eight outline
  contracts pass. The final 54 GUI trials include 51 presentations, two intended
  cancellations and one retained TextKit 1/5 MiB timeout. TextKit 2 entry-to-draw
  medians were about 217/498 ms for 1/5 MiB prose; 5 MiB core/model and copy/decode
  were about 14.6/5.5 ms in GUI runs. Main-actor/strict concurrency checks,
  live Swift-to-Rust source debugging and relocated normal-library bundles passed.
  This is not a cold-launch or equivalent-renderer language comparison.
- FEAS-006 Linux runtime-loads the exported C ABI from a separate gtk-rs host,
  negotiates 40/48/56-byte layouts, copies and releases values before GTK, and
  passes the same lifecycle/error/panic/cancellation pattern. All eight outlines,
  twelve GTK presentations and two intended cancellations passed. Warm console
  1/5 MiB core/model medians were about 5.5/29.8 ms with 0.6/3.3 ms copy cost;
  GTK first-useful ticks were about 277/498 ms. A 1 MiB logical line still took
  about 11.4 seconds, reinforcing the virtualized-renderer requirement. GDB
  source frames and a stripped/path-scanned adjacent-library package passed on
  the named host. See the shared-core Linux report.
- FEAS-010 (2026-10-25) recommends keeping pulldown-cmark. All product syntax
  works as shared passes with exact spans: 8 of 8 cases (Comrak 7 of 8), and
  39 ms at 5 MiB with every pass versus 104 ms for Comrak. Never enable its YAML
  metadata option: it accepts `---` blocks mid-document and hides a setext
  heading. Detect front matter on line 1 in shared code. Unbalanced inline math
  stays text. Keep voice scans bounded; the first scanner was quadratic.
- FEAS-008 Windows (2026-10-26) recommends a native WinUI `ListView` over
  the same bounded fragments, hosted in C# Native AOT with C++/WinRT as the
  fallback. Warm/cold medians on 1 MiB, 5 MiB and long lines were 240–283 ms
  and 309–356 ms for AOT and C++. JIT was 401–445 ms warm, missing the
  400 ms budget proposed on Linux. WebView2 was 582–3381 ms warm, using up to
  1.4 GiB. `ListView` realizes about 15 rows; GTK realizes about 205.
  Pitfalls found: the heading level must go on the row's text element, not
  the `ListViewItem`, or Narrator H finds nothing; recycled containers must
  keep template content; DataPackage stream formats fail after Flush, so use
  Win32 clipboard formats; Native AOT needs QueryInterface casts for template
  parts. Narrator reads each row twice, and whole-tree UIA searches time out
  at 5 MiB. Both are WIN-001 defects. See
  `docs/Virtualized-Renderer-Windows-Evaluation.md`.
- REV-001 (2026-10-25) found the feasibility evidence sound: Linux reruns and
  raw-data recomputation matched every checked report value. On Linux/glibc,
  repeated parses keep RSS flat (pulldown-cmark 40 MiB from 1 to 20 parses), so
  the Mac growth is macOS-allocator-specific. It adds a constant ~27.8 MiB per
  5 MiB load with no plateau observed. Key gap: Windows and Linux haven't proven
  virtualized blocks with document-wide selection and reading order. Cross-
  platform timing definitions differ. See `docs/Feasibility-Review.md`.
- Repeated Mac native-only 5 MiB parses grew RSS to about 583 MiB after twenty
  loads despite zero native owners and about 5.1 MiB live malloc usage. A leaks
  scan found zero leaked bytes; vmmap classified most growth as empty large
  malloc regions. Swift copying shows similar retention. Allocator pressure
  relief released zero bytes. Keep this production memory risk open; no fix or
  acceptable repeated-session budget is established by those bounded diagnostics.
- FEAS-007 classifies 30 atomic capabilities per platform with a fallback,
  named next check and source catalog. `tools/validate_capabilities.py` enforces
  the evidence rules. Key deviations: sandboxed macOS, Flatpak and iOS can't read
  images beside a lone file; Linux has no share portal; Linux network watching
  needs polling; iOS suspends background work; `PrintManager` is Windows 11 only.
  Mermaid needs a browser engine, and KaTeX output has no native presenter, so
  both are deferred. D2 v0.9.0 is pure Go, so it is feasible in-process,
  including on iOS, but Go would be a second language. Every iOS cell is
  authoritative or unverified until IOS-001.
- The owner requires real target-host evidence from Windows and macOS before
  further architecture decisions. `project.yaml` therefore makes FEAS-003 and
  FEAS-004 prerequisites of FEAS-006 and FEAS-007 as well as GATE-001.

## FEAS-009 best-effort checkpoint (2026-10-26)

Owner requested bounded feasibility and deferral of deeper post-MVP work.
Mac compiler screen: KaTeX/JavaScriptCore, Mermaid/WKWebView and direct D2 Go;
15 fresh processes, 90 case outcomes, 25 timed samples per case. Basic valid
compiles work; D2 raw SVG retains remote-icon/JavaScript-link references, so
resource/output policy is still required. This is not visual fidelity, native
presentation, full sanitizer or D-012 evidence. iOS frameworks compile/link;
engine/device runtime and Rust alternatives remain untested. No extra Homebrew
install needed. Report: `docs/Diagram-Math-Engine-Evaluation.md`.

Owner subsequently accepted D-014: close FEAS-009 by explicit deferral and
track the unrun work in ADV-005 after DESK-002, blocking ADV-002. Its desktop-apps
bucket avoids waiting for the whole mobile phase. GATE-001's spike blocker can
be resolved; remaining baseline choices and gate closure still need the owner.
No engine is accepted and no deferred check is marked passed.

## Accepted baseline and implementation start

Owner accepted all component choices D-008–D-017 and final gate closeout D-018.
GATE-001 is complete. Portable-core is active with 36 materialized child tasks
and 92 edges, including the early LNX-001D1 accessibility proof. Windows uses
C# Native AOT; C++/WinRT is only a fallback. macOS uses AppKit/TextKit 2. Shared
Rust passes and initial 400/700/150 ms targets remain subject to real native
MVP evidence, including the unresolved Mac memory risk. ADV-005 retains
post-MVP engine validation. See `docs/Implementation-Baseline.md`.

CORE-001A merged in PR #1; its operational closeout travels on CORE-002A's
branch. CORE-002A defines safe owned values, opaque IDs, source length/hash and
UTF-8 byte spans, strict graph/intent validation and explicit unsupported
fallbacks. See `specs/semantic-model/v1/README.md`. V1 is under PR review and
freezes at owner-authorized merge. Identity matching is CORE-004A, not done by
these types. Resource intents grant no filesystem access and are not full URI
sanitization. Parser accuracy, native UI and C ABI remain unproven here.

The core now uses serde/serde_json and sha2. Optional `schema` adds schemars;
source/license review and all-feature inventory are in `docs/Dependencies.md`.
Default/all-feature tests, generated schema snapshot and five source-bound
24-kind goldens run through common/core checks. New crates.io dependencies
remain locked; no spike crate is linked into the production workspace.

Items below that these supersede are kept for history.

- Confirm the order in which the three desktop MVPs are deepened after their
  walking skeletons exist.
- Accept or reject the recommended Rust/pulldown-cmark semantic core after
  cross-platform packaging and host-integration evidence from FEAS-006.
- Select WinUI 3 with C# or C++/WinRT, the Windows document layout strategy,
  and the stable Rust boundary using measurements from a real Windows host.
- Confirm the AppKit/TextKit 2 lead renderer recommendation, selective SwiftUI
  shell responsibilities and the stable Rust/Swift boundary at the accepted
  gates. macOS FEAS-004 measured native layout and FEAS-006 measured core
  interop; GATE-001 still owns acceptance.
- Confirm Linux virtualized-block selection and AT-SPI behavior during LNX-001;
  a monolithic `GtkTextView` is already rejected for unbounded documents.
- Confirm whether “native” allows narrowly scoped non-UI engines for diagrams
  while retaining platform-native document presentation.
- Decide how much voice-over/script behavior belongs in the first public
  native release.

## Known risks

- A rich native attributed-text document can become slow or inaccessible if
  represented as one monolithic view.
- Virtualized document lists make the app own accessibility structure. Open
  gaps: Orca H heading navigation on Linux; on Windows, Narrator reading each
  row twice, and whole-tree UIA searches exceeding the provider timeout at
  5 MiB.
- Parser source ranges may be insufficient for every web behavior; the core
  model needs stable block identities and explicit source mapping.
- iOS security-scoped bookmarks can become stale and require user recovery.
- Raw HTML, remote resources, local links, and SVG each need separate policy.
- Feasibility FFI and local package consumption pass on Windows, macOS, and
  Linux, but production ABI stability, clean deployment, platform packages and
  Apple XCFramework concerns remain open.

## Run log

### 2026-10-26 — FEAS-008 Windows part

Added a disposable C ABI over the FEAS-008 model. Built the same WinUI 3
virtualized `ListView` surface in C# (one source, JIT and Native AOT builds)
and C++/WinRT, plus a WebView2 baseline. All four hosts share the model and
policy. In an elevated D-012 sweep (5 cold and 5 warm runs per input, cold
after a standby purge), every run passed the automated checks.

- C# AOT and C++/WinRT presented in 240–283 ms warm and 309–356 ms cold.
- C# JIT took 401–445 ms warm and WebView2 582–3381 ms.
- The owner verified drag and keyboard selection, a 5 MiB Ctrl+A copy
  (byte-exact from a separate process), find, outline, links and images.
- With Narrator, the owner confirmed in-order reading, H heading navigation
  and continuous reading past the screen.
- UIA3 inspection found contiguous model order at the top, middle and end.
- Four owner-found defects were fixed. Duplicated Narrator reading and
  whole-tree UIA timeouts at 5 MiB remain.
- The first pass lease expired during manual testing. It was recovered and
  the work finished in a new pass.

Recommendation only. Next: FEAS-009.

### 2026-10-25 — FEAS-008 Linux part

Built `spikes/feas008`: an owned-value block model, a gtk-rs `GtkListView`
renderer and a WebKitGTK baseline over the same model and policy. Native warm
launch to presented content was 289/302/373 ms (1 MiB, 5 MiB, long lines) and
cold 528–623 ms; WebKitGTK was 952/2898/1291 ms warm at 2.1–3.1× the memory.
GTK keeps about 205 rows realized, so fragments are bounded (1 KiB, 512 B
unbreakable) and labels need `width-chars`; without it a 1 MiB line timed out.
The owner verified drag/keyboard selection, 5 MiB Ctrl+A copy (byte-exact via
`wl-paste`), find, outline, links, images and Orca row reading. Six defects the
owner found were fixed. Orca H navigation fails; Say All past the screen is
unconfirmed. Recommendation only. Next: FEAS-008 on Windows, or FEAS-009.

### 2026-10-25 — FEAS-010 core extensions

Built `spikes/feas010`: pulldown-cmark and Comrak adapters into one
intermediate form, with shared passes for front matter, slugs, link policy,
alerts, fences and VO0 voice-over syntax. pulldown-cmark passed all 8 frozen
cases and Comrak 7, with 0 span failures; 13 misparse tests pass on both.
Found that pulldown-cmark's metadata option hides mid-document content,
replaced it with a shared first-line detector, and fixed a quadratic scanner.
Benchmarked five trials per mode. Recommendation recorded, not accepted:
GATE-001 decides. Next: FEAS-008 Linux part or FEAS-009.

### 2026-10-25 — GATE-001 decisions, part 1

Collected the owner's answers through structured prompts. Accepted D-008 to
D-013. The owner asked for spikes before deciding the remaining items, so
FEAS-008, FEAS-009 and FEAS-010 were created through the state helper, each
blocking GATE-001. The native-only document guardrail stays in force while
FEAS-008 measures a WebView baseline. A D2 port would stay MPL-2.0 file by file.
GATE-001 remains open; no task graph was materialized.

### 2026-10-25 — REV-001 feasibility review

At the owner's request, reviewed FEAS-001–007 through a new REV-001 task that
blocks GATE-001. Recomputed FEAS-003 and FEAS-006 Windows tables exactly; reran
the FEAS-002 benchmark (within about 12%), the FEAS-006 Rust tests and the Linux
self-test. Found that `nfd-filename` is generated upstream by
`testdata/mkfixtures.sh`, not tracked. Its hash equals the script's fixed
output, so the manifest now records an optional `generator` that schema and
validator accept. Recorded the gaps and four GATE-001 recommendations in
`docs/Feasibility-Review.md`. Next: GATE-001 with the owner.

### 2026-10-25 — FEAS-007 capability matrix

Resumed the paused planning pass. Revised the schema into 30 atomic rows: split
SVG/pan-zoom/export, voice-over/teleprompter/display sleep, share/export and the
three packaging tiers, moved iOS constraints into a synthesis section and
dropped state restoration as out of scope. Read official desktop, iOS/iPadOS and
engine sources; all 82 external links returned HTTP 200. Measured cells cite the
FEAS-003–006 reports. Cells without an official statement are Deferred and
Unverified, never inferred: macOS network FSEvents, App Nap, the iOS
child-process limit, Apple native SVG, Windows PDF and Flatpak helpers. Added a
standard-library validator and 19 mutation tests over the checked-in document.
Requirements, Architecture, README, Validation, TODO and the proposed decisions
now point to the matrix. GATE-001 is not accepted. Next: GATE-001 with the owner.

### 2026-10-25 — FEAS-006 Linux checkpoint

Completed the all-desktop shared-core feasibility evidence with a separate
gtk-rs host that runtime-loads the existing C facade rather than calling Rust
internals. The final self-test passed 1,000 sequential and 400 concurrent loads,
ten observed-entry cancellations, typed errors, conversion failure cleanup and
panic recovery with zero owners. All eight outline contracts, twelve GTK
presentations and two intended cancellations passed without timeout. GDB stopped
in Rust with host/core frames and continued normally. A relocated normal package
passed lifecycle, console and GTK smokes with loader overrides removed; packaged
ELFs have no probe exports, debug sections or checkout-path bytes, while separate
debug artifacts retain symbols. Review found and resolved path-privacy,
cancellation-classification, worker-disconnect and loader-environment defects;
fresh re-review found no remaining code issue. The monolithic long-line case
still took about 11.4 seconds, and no production ABI/model/package is accepted.
See `docs/Shared-Core-Linux-Evaluation.md`. Next: FEAS-007.

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
