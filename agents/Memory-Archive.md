# Memory Archive

History moved out of the required-reading files by DOC-001 (2026-10-28). It
holds run logs, per-phase status notes, superseded handoffs and feasibility
host notes. The text is moved verbatim; only headings were demoted and
relative links rebased to this folder. It is history, not a required read.
Current posture lives in [MEMORY.md](../MEMORY.md), the current handoff in
[HANDOFF.md](../HANDOFF.md) and decisions in [Decisions.md](Decisions.md).
Where a status line below conflicts with those files, they win.

## MEMORY.md phase status history

From MEMORY.md "Current posture", newest first, as it stood before DOC-001.

- Status 2026-10-28 (latest, PR #16): Codex and Grok reviewed the shared
  core too. The owner accepted D-026 (the CORE-FIX phase: DOC-001, CORE-002B,
  CORE-003C, CORE-003D bare autolinks, CORE-005C, CORE-005D, plus HARD-001
  before DESK-002) and D-027. Under D-027 the graph is linear: the shared
  track ends with the CORE-001 to CORE-006 aggregates in `portable-core`,
  then each host is an independent chain in `desktop-apps` from CORE-006. The
  native CI and package-consumer evidence moved to each 001A/001B. The state
  wrapper has `remove_dependency` and `update_task`. PR #16 merged with
  owner approval (head `2b06e88`, CI green on all three OSes). Next: DOC-001.
  See HANDOFF.md.
- Status 2026-10-28: every shared CORE delivery phase through
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

## MEMORY.md implementation workflow decision (2026-10-25)

From MEMORY.md. The decision itself is D-007.

D-007: after GATE-001, use small platform-focused delivery phases, each with its
own feature branch and PR. Batch dependency-ready phases on the current host.
Post a separate `/grok review` comment when ready, resolve actual feedback and
wait for owner merge approval. Scoped phase pushes/PR/review actions are already
authorized. See `docs/Delivery-Workflow.md` and `docs/Implementation-Plan.md`.
Materialize child tasks at prerequisite gates through the helper; early native
foundation tasks belong in the portable-core bucket. FEAS-006 is evaluated;
FEAS-007 and GATE-001 are complete; D-014 moves FEAS-009's unrun work to
ADV-005 after the desktop MVP.

## MEMORY.md feasibility and evaluation summaries

From MEMORY.md "Durable context". The full reports are in `docs/`.

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

## MEMORY.md FEAS-009 best-effort checkpoint (2026-10-26)

From MEMORY.md. The decision itself is D-014.

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

## MEMORY.md accepted baseline and implementation start

From MEMORY.md. The decisions are D-008 to D-018.

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

## MEMORY.md run log

From MEMORY.md, newest first.

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

## HANDOFF.md phase handoff history

From HANDOFF.md, newest first. Each "Former resume point" was the live
handoff at its time and is superseded.

### Former resume point — CORE-FIX phase, through the milestone review

PR #16 merged the three shared-core reviews
([docs/reviews/](../docs/reviews/README.md)). The owner accepted D-025, D-026
and D-027 ([Decisions](Decisions.md)). CORE-FIX runs seven shared tasks
in order before any platform phase. Their scope is in
[TODO.md](../TODO.md#core-fix-phase-shared-core-review-follow-ups).

- **Done:** DOC-001, the agent-facing docs cleanup,
  [PR #17](https://github.com/awesomejt/markdown/pull/17). Its Grok review
  added CORE-003E (front-matter invisible-character parity), which the owner
  placed after CORE-005D as a separate task.
- **Done:** CORE-002B, import-policy parity and FFI host guidance,
  [PR #18](https://github.com/awesomejt/markdown/pull/18). V1 and v2 imports
  now re-check intents and metadata text against the producer's policy. One
  hand-written v1 golden intent became denied, with owner approval.
- **Done:** TOOL-002, CI speed-up,
  [PR #19](https://github.com/awesomejt/markdown/pull/19) (D-028). PRs run
  `tools/check.py core`; `main` and manual runs add `core-extended`. Run
  `tools/check.py all` locally before each review request.
- **Done:** CORE-003C, v2 corpus conformance assertions,
  [PR #21](https://github.com/awesomejt/markdown/pull/21). Every corpus check
  token runs on v1 and v2, with explicit per-intent pairing rules documented
  in the [conformance contract](../specs/parsing/conformance/README.md).
- **Done:** PR #22 (`grok` CLI reviews, D-029 review-gated merges) and the
  D-030 milestone workflow. Task PRs now target `develop`; `main` takes only
  owner-approved milestone reviews.
- **Done:** TOOL-003, first-push whitespace check,
  [PR #23](https://github.com/awesomejt/markdown/pull/23). Two deferred
  findings are in TODO.md.
- **Done:** CORE-003D, bare GFM autolinks (D-031),
  [PR #24](https://github.com/awesomejt/markdown/pull/24), merged into
  `develop` after the three-review cap with the owner's approval.
- **Done:** CORE-005C, the content-search memory guard,
  [PR #25](https://github.com/awesomejt/markdown/pull/25), merged into
  `develop` on a clean Grok review (D-029). The default file budget is
  2 MiB, and the node allowance follows it; see the
  [workspace-search spec](../specs/workspace-search/README.md#memory-guard-core-005c).
- **Done:** CORE-005D, wider invisible-filename warnings,
  [PR #26](https://github.com/awesomejt/markdown/pull/26), merged into
  `develop` on a clean Grok review (D-029). File names now flag Cf plus
  Default_Ignorable and U+2800, except ZWJ/ZWNJ and display modifiers; the
  shared tables live in `metadata.rs`.
- **Done:** CORE-003E, front-matter invisible-character parity,
  [PR #27](https://github.com/awesomejt/markdown/pull/27), merged into
  `develop` on a clean Grok review (D-029). Front-matter text now rejects
  the blank default-ignorable values and U+2800 too, keeping the display
  modifiers (D-032). `metadata::hidden_character` is the one shared guard.
- **Current:** CORE-FIX is complete, so the first milestone review is due
  (D-030): confirm `develop` is green with no task PR waiting, cut
  `review/core-fix` from it, and open the review PR into `main` with the
  review packet. Only the owner merges it. Work continues on `develop`
  meanwhile with CORE-001, the first CORE-001 to CORE-006 aggregate. Each
  host's chain in `desktop-apps` starts after CORE-006 (D-027). Do not select
  platform tasks before then.

### Former resume point — CORE-FIX phase, starting with DOC-001

PR #16 merged after owner approval. It holds three shared-core reviews (Claude, Codex,
Grok; see [docs/reviews/](../docs/reviews/README.md)) and the owner's
dispositions:

- D-025: native hosts build and request model v2.
- D-026: the CORE-FIX phase, six tasks in order: DOC-001, CORE-002B,
  CORE-003C (v2 conformance), CORE-003D (bare GFM autolinks), CORE-005C
  (content-search memory guard; content search is a model-independent text
  index) and CORE-005D (wider invisible-filename warnings). HARD-001 (release
  hardening) is before DESK-002. The scope is in [TODO.md](../TODO.md).
- D-027: the task graph is linear. The shared track runs CORE-FIX, then the
  CORE-001 to CORE-006 aggregates, in `portable-core`. Each host is one chain
  from CORE-006 in `desktop-apps`. `tools/project_state.py` gained
  `remove_dependency` and `update_task`.

State is at revision 498 with no active pass, and only DOC-001 is eligible.
PR #16 belonged to no state task, so its closeout changed no task status.
Next: start a DOC-001 pass from `main` on branch
`docs/doc-001-agent-docs-cleanup`, following AGENT_WORKFLOW.md:

```sh
python3 tools/project_state.py --file project.yaml inspect
```

### Previous step — owner holistic CORE review (done)

PR #13 (CORE-003B4) merged as `29f7334` from reviewed head `9b6bd46`; D-024 is
accepted. CORE-003B4 and the CORE-003B aggregate are completed in
`project.yaml` (revision 370, no live lease) on `chore/core-003b-closeout`.
The next step is the owner's holistic review of the shared CORE work. Do not
select MAC/WIN/LNX tasks until that review is done, even though state lists
MAC-001A, LNX-001A and WIN-001A as eligible.

### CORE-003B4 parser conformance hardening and budgets (merged in PR #13)

PR #12 (CORE-003B3) merged as `66c2162`; its state closeout is commit `3f73eb7`
on this branch. CORE-003B4 adds the following
([Parser-Budgets.md](../docs/Parser-Budgets.md)):

- **Conformance.** `tools/generate_conformance.py` turns the fixture
  expectations into `specs/parsing/conformance/corpus.json`. Each expectation
  becomes a check token or a reasoned deferral; unknown ones fail. The common
  suite runs `--check`, and `tests/conformance.rs` runs the tokens. The Rust
  profile replica (`tests/support/profiles.rs`) is pinned to the profile
  SHA-256 values.
- **Hostile input.** `tests/hostile.rs` covers corpus-seeded mutation (4,000
  cases per configuration; `MD_FUZZ_CASES`), 33 pathological constructs on v1
  and v2 with a five-second release guard, all profiles under default limits,
  and source-ceiling stops.
- **Budgets.** `tests/budgets.rs` enforces counted heap budgets per profile,
  release time guards and a retained-snapshot ceiling. The
  `examples/parse_budget.rs` example produced the named-host table.
- **Limits (D-024, proposed).** The 5 MiB prose profile hit `NodeLimit`, so
  the defaults are now 1,000,000 nodes and 2,000,000 events. No ABI change.
- **Speed.** Validation uses a hash ID index and a visit bitmap. Output is
  unchanged, and parse time halved.
- **Fallback.** A source-offset conflict replaces only the affected
  top-level block. The whole-body path remains as a safety net, and mutation
  tests assert it never fires.

- **Review fixes.** Grok's first review of PR #13 found that dense
  delimiter storms peak at about 1,152 MiB at the 8 MiB ceiling (the same on
  base). The worst case is now documented, with a 510 MiB budget on a 2 MiB
  `*a` storm. Rejoined `$` text events are refunded from the event limit, so
  dollar-dense prose parses at the ceiling.

Merged in PR #13 as `29f7334`; closeout on `chore/core-003b-closeout`.

### CORE-003B3 extension semantics (merged in PR #12)

PR #11 (CORE-005B2) merged as `28e33bc`; its state closeout is commit
`a1e3688` on this branch. CORE-003B3 enables pulldown-cmark GFM footnotes,
math and `ENABLE_GFM` (alert tags only) through one shared `PARSER_OPTIONS`.
V2 provenance reparsing uses the same set. See
[the extension semantics](../docs/Parsing.md#extension-semantics-core-003b3).

- Alerts become `alert` nodes; look-alikes stay quotes.
- Footnotes become definition and reference nodes. References name the first
  definition's exact label, matched with the parser's own `unicase` folding.
  Later folded duplicates are inert `unsupported_block` fallbacks, and
  undefined references stay text.
- Inline and display math are `unsupported_inline` fallbacks over the exact
  lexeme, each with `math_unavailable`.
- Diagram fences are unchanged.
- Front-matter `mdv: script` adds an info `voice_script_unavailable`; voice
  syntax reads as ordinary Markdown.

The model, schemas, goldens and ABI do not change. unicase 2.10.0 becomes a
direct dependency, with no new package. Reading-stat goldens moved towards
markdown-viewer parity (gfm-tour 215→192 words), as the search contract
predicted. The FFI corpus now produces all 24 node kinds. Evidence:
`tools/check.py all` passed on macOS (`.build/core003b3-all.log`). It covered
six B3 parser regressions, a v2 provenance regression, and a 20,000-case
extension-token generator run on v1 and v2. Next: phase PR, `/grok review`,
owner-authorized merge. Then CORE-003B4 with the CORE-003B closeout, then the
owner's holistic review before any platform phase.

### CORE-005B2 workspace search C ABI (merged in PR #11)

CORE-005B2 adds four D-022 functions per
[the C ABI contract](../specs/ffi/README.md#workspace-search-core-005b2):
classify path, find names over an entry array, and content admit/search with
a caller-owned in/out progress record written back only on `MD_OK`. Hit
`unit` indexes a v1 view only; v2 hosts navigate by span or anchor. The owner
merged PR #11 as `28e33bc`.

### CORE-005B workspace search primitives (merged in PR #10)

PR #9 (CORE-005A2) merged as `ec58c84`; its state closeout travels on this
branch. CORE-005B adds `markdown_core::workspace` per
[the workspace search contract](../specs/workspace-search/README.md). Adapters
feed `WorkspaceEntry` values: an opaque `u64` ID plus a validated
`WorkspacePath`, which is relative and `/`-separated with no dot components,
controls or bidi formatting characters. The core never resolves paths. `find_names`
ranks name-prefix, name and path hits. `ContentQuery::admit` and `search`
work file by file, with a caller-held `ContentProgress` holding file, byte,
per-file and total match budgets and permanent stops. Hits equal find's
matches and add a visible-text snippet, a line number, an anchor and the
file's `SourceInfo` for navigation. Cancellation returns nothing and leaves
progress unchanged. There is no new dependency, model or ABI change. The
owner approved splitting the C ABI exposure into CORE-005B2 (task created,
blocks CORE-005; each 002C needs it). The owner merged PR #10 as `0bf8eb7`
from reviewed head `2a47b80`.

### CORE-005A2 C ABI find and reading statistics (merged in PR #9)

PR #8 (CORE-005A) merged as `237cd88`; its state closeout travels on this
branch. CORE-005A2 adds two exports under D-022 per
[the C ABI contract](../specs/ffi/README.md#find-and-reading-statistics-core-005a2).
`md_snapshot_find` fills `md_find_match` and `md_match_segment` arrays and an
`md_find_result`. `md_snapshot_read_stats` fills `md_read_section` records and
an `md_read_stats`. Both use two-call `MD_ERR_BUFFER_TOO_SMALL` sizing, and no
records are written on failure. Each new struct has an `MD_*_SIZE` header
constant. The options carry the record element sizes, which are checked on
every call. There is no new handle type, no change to existing items and no
cancel handle. Evidence: Rust parity with `markdown_core::search` over the
corpus (v1/v2), argument, size, panic and concurrency tests, layout and
constant drift checks and the macOS C smoke. The Grok review nit was fixed,
and the owner merged PR #9 as `ec58c84` from reviewed head `f05cbb4`.

### CORE-005A find, word count and section timing (merged in PR #8)

PR #7 (CORE-006A C ABI) merged as `a751f48`; its state closeout is owned by the
orchestrator's project-state pass. CORE-005A adds `markdown_core::search` per
[the search contract](../specs/search/README.md): literal find over visible
semantic text (leaf units in reading order, inline boundaries transparent),
exact/atomic source mapping with per-node segments, simple case folding without
locale tailoring or NFC/NFD normalization, approximate grapheme/whole-word
boundaries, KMP matching with a `max_matches` budget, `truncated` flag and
cancellation; word count by a simplified UAX #29 rule (one word per CJK
ideograph); intro plus per-heading own/cumulative timing at 160 wpm (60–600),
matching markdown-viewer `b881034` defaults/exclusions. No new dependency, no
model/ABI change. The owner split FFI exposure into CORE-005A2 under D-022
(ABI v1 grows by new functions only, filling caller-owned arrays; no new
handle types or changes to existing items) and froze the PR #7 flat view as
an ABI v1 representation (D-023). Review findings were fixed before the merge.

### CORE-006A C ABI facade (merged in PR #7)

PR #6 (CORE-004A sessions) merged as `3b1eb4e`; its state closeout travels on
this branch. The owner accepted snapshot-local node IDs with block anchors for
the MVP (D-021). CORE-006A adds crate `core/crates/markdown-ffi` and the
hand-written header `core/include/markdown_core.h` per
[the C ABI contract](../specs/ffi/README.md): `md_abi_negotiate` with exact
struct sizes, per-session v1/v2 model choice, nonzero registry handles with one
release each (double release detected), typed `md_status` codes and static
names, `catch_unwind` on every entry point, session/job/snapshot functions,
anchor value copies and a bulk read-only snapshot view (fixed-size records
plus one string table) that round-trips the full v1/v2 model. No new package.
`tools/check.py core` runs the extern tests and a host-`cc` C smoke with layout
comparison; it is required on macOS/Linux and skipped on Windows.
Next: phase PR, `/grok review`, owner-authorized merge; then each host's 001B
consumer (Swift/AppKit, C# Native AOT, gtk-rs) validates CORE-006 on its host.

### CORE-004A sessions (merged in PR #6)

`markdown_core::session` per [the session contract](../specs/session/README.md):
`Arc` snapshots, strictly increasing generations, single-point latest-wins
publication, cooperative `CancellationToken` polling, adapter-reported
`Modified`/`Deleted`/`Unreadable` state with opaque revision-token and
exact-byte deduplication, and block anchors resolved as
exact/relocated/nearest/lost. Node IDs stay snapshot-local (D-021).

### PR #5 review (historical)

The owner asked for the [Grok review](https://github.com/awesomejt/markdown/pull/5#pullrequestreview-5671367164)
findings to be fixed on the PR #5 branch before the authorized merge, so the
**CORE-003B2F** scope landed in PR #5:

- V2 URI targets also deny Unicode 16.0 Default_Ignorable_Code_Point values and
  U+2800, literal or percent-encoded. Metadata text keeps its Cf/control guard,
  so that emoji variation selectors, Mongolian FVS and CGJ remain usable
  ([Front-Matter.md](../docs/Front-Matter.md), [URI-Policy.md](../docs/URI-Policy.md)).
- The v1 span correction for blockquote-continuation destinations is now
  documented, and `tests/parser.rs` has a regression for it.
- V2 emits an info `reference_definition_shadowed` diagnostic for each ignored
  duplicate definition. It is bounded and fails closed, and first-definition
  precedence is unchanged ([Parsing.md](../docs/Parsing.md)).

After the merge is verified, carry the CORE-003B2 and CORE-003B2F state
closeout in the next feature PR. Both are satisfied by the PR #5 merge, and
closing them unblocks the B4 and B aggregate gates. Then select the next
eligible task.

### Former resume point — CORE-003B2 references and URI targets

CORE-003B1 completed via owner-authorized [PR #4](https://github.com/awesomejt/markdown/pull/4)
merge `0ce353f`, containing corrected head `22146cc`. All four concerns/two nits
were addressed; required CI passed. The owner requested fixes/checks/merge and
cleanup as before, not another bot-response wait; no extra approval is inferred.
Its supported state closeout travels here. The four B children and aggregate/
native gates remain intact; B1 metadata does not complete CORE-003B.

Selected: **CORE-003B2**, branch `feat/core-003b2-references-uri`.
The owner chose additive opt-in model v2 with explicit reference provenance,
preserving v1 API/decoder/schema/goldens (D-020). Read
[the v2 contract](../specs/semantic-model/v2/README.md), [URI policy](../docs/URI-Policy.md),
[Parsing.md](../docs/Parsing.md), [development checks](../docs/Development.md) and
[dependency inventory](../docs/Dependencies.md). V2 is under phase-PR review and the owner authorized merge with the
review follow-up above. No default model/facade/native format switch.

Enable feature `model-v2` for `parse_markdown_v2`/`v2::Document`. The v1 parser
remains default with reference fallbacks/conservative URI subset. V2 separates
reference definition/destination spans from occurrence spans; supports full,
collapsed, shortcut and image references with parser case-fold/duplicate rules.
All targets are source-proven through a bounded expected-intent reparse, not
merely span geometry: forged destinations and unrelated definition bindings
are rejected. General display text/metadata fidelity is not a source signature.
Both decoders reject the other's wire shape/version. Explicit upgrade retains
legacy denials; old reference fallback nodes need source reparse to become links.

Typed v2 targets separate internal fragments, decoded-once relative capability
paths, canonical external URLs and static denials. Unicode/NFD/encoded spaces,
IDNs and visible parent-relative paths are supported with guards for malformed/
encoded control/Cf values, schemes, authority/userinfo, path separators/dot
obfuscation and Windows device variants. Adapters MUST NOT decode file targets
again, pass them to URL launchers or expand shell/tilde/env syntax. Native file
APIs must enforce approved-root containment through symlinks/reparse points and
revocation/type checks; targets grant no I/O. Remote images remain denied.
Mailto/query/URI edge cases remain intentionally limited; see the policy.

Source mapping now excludes quote continuation markers for multiline inline/
reference link/image destinations. Linked images keep both intents; nested links
remain inert. Known upstream tab-offset cases retain exact whole-body source
fallback and metadata, dropping targets. Proof scan budgets are independent of
published node/event counts; source depth/fact count and fixed 64 MiB fact-string
ceiling bound its extra work. It is not a deadline/allocator quota. Primary
parsing charges retained targets to the copy budget. B4 must measure costs.

Optional url 2.5.8/IDNA/ICU adds 28 packages to the full graph (60 including
schema tooling). V1 plus schema remains 32 and default v1 does not enable URL.
Eighteen package/version Unicode-3.0 exceptions were verified against local
manifests/LICENSE and scoped in policy; source restrictions remain unchanged.
No new installed software beyond normal Cargo fetching is needed. No UI,
advanced engine, FFI, stable identity/session/cancellation or I/O is implemented.

Regressions cover version/migration compatibility, exact source/schema/golden,
forged targets/definitions, references/linked images/quote continuations,
Unicode/percent/IDN/mail/fragment routing, hostile URIs/devices, budget separation
and 8,192 generated v2 inputs, plus existing v1/parser/metadata suites. Independent
read-only review found source-proof, quote mapping, linked-image, colon/authority,
device and budget issues; fixes/regressions passed its isolated re-review with
no remaining blocking correctness/security finding. Full conformance/fuzz/perf
remains B4; required current-head hosted common/Linux/macOS/Windows checks and
the owner authorized merge after the PR #5 findings were fixed on-branch (B2F scope).

Finish this phase partial without an active lease while awaiting verified merge.
The exact `/grok review` request and actual review are on PR #5; authenticated
`gh` is the fallback for connector write limitations. The owner explicitly
authorized PR #5 merge after recording its review findings.
After verified merge, carry state closeout in the next feature PR, not directly
to main. B3/B4 and CORE-004A/native A phases keep their dependencies. Preserve
Linux LNX-001D1 and ADV-005 gates.

## HANDOFF.md feasibility host notes

From HANDOFF.md. Reference only; the open risks are carried in MEMORY.md
and HANDOFF.md.

### Feasibility evidence retained for implementation

[Decisions](../agents/Decisions.md) and [the baseline](../docs/Implementation-Baseline.md)
carry the accepted stack and conditions. All prototype reports remain dated
evidence, not production contracts. Preserve Mac repeated-session RSS retention,
Windows Narrator/UIA defects, Linux screen-reader proof and the unrun native
packaging/accessibility/performance cases. FEAS-009 closed by owner-approved
deferral, not by passing its unrun checks. The engine report and ADV-005 retain
them. The remaining historical host notes below are reference only.

### FEAS-006 Linux checkpoint

The Linux report is
[Shared-Core-Linux-Evaluation.md](../docs/Shared-Core-Linux-Evaluation.md), with
[durable measurements](../spikes/feas006/results-linux-2026-10-25.yaml) and
[reproduction commands](../spikes/feas006/README.md). The native host is a separate
gtk-rs binary: it has no Rust dependency on the core crate and runtime-loads the
exported C symbols from an adjacent library. It negotiates layouts rather than
assuming other hosts' values; the C header, host and core agree on 40-byte
options, 48-byte blocks and 56-byte snapshots on x86-64 Linux.

The host copies and validates values, explicitly releases core snapshots and
tokens, then constructs GTK objects on the main thread. It writes a
`host-copy-released` checkpoint before synchronous `GtkTextBuffer` work.
The final corrected self-test passed 1,000 sequential plus 400 concurrent loads,
ten cancellations after observed native entry, typed errors, conversion-failure
cleanup and panic recovery, with zero live owners. All eight outline contracts,
twelve GTK presentations and two intended cancellations passed without timeout.

Warm console 1/5 MiB prose medians were about 5.5/29.8 ms core/model and
0.6/3.3 ms copy/decode. Fresh GTK processes reached a first useful main-loop
tick in about 277/498 ms; that callback is not GPU presentation or cold launch.
The 1 MiB logical-line case took about 11.4 seconds despite roughly 2 ms of
core/copy work. This reinforces FEAS-005: a monolithic `GtkTextView` is not the
production unbounded renderer. LNX-001 must prove virtualized semantic blocks,
document-wide selection and AT-SPI reading order.

GDB stopped in Rust `md_parse_v1`, resolved source plus gtk-rs host/Rust-core
frames, and continued normally. A fresh normal-library package passed a
100-load lifecycle check, console parse and GTK smoke after external-directory
relocation with `LD_LIBRARY_PATH` removed. It contains licenses for 72 locked
Rust dependency records, no probe exports and no core `DT_NEEDED`. Packaged
ELFs have debug sections removed and are scanned for checkout-root bytes;
separate ignored debug artifacts retain source information. This is same-host
relocation, not clean-machine, distro-floor, installer, sandbox, signing or
publication proof.

Review initially found checkout paths in packaged DWARF, permissive
cancel-on-entry classification, indefinite polling after worker disconnect and
incomplete loader-environment sanitization. All four were fixed and covered by
targeted tests; fresh re-review found no remaining code issue. GTK still emits
nonfatal theme slider-size warnings during automation, and GDB reported an
optional auto-load/debuginfod warning without losing source or frames.

Linux environment: CachyOS, kernel 7.3.0-4-cachyos, glibc 2.44, x86-64,
Ryzen 9 5950X, 64 GiB RAM, RTX 4080, KDE Wayland, Rust/Cargo 1.99.1,
GCC 16.2.1, GDB 18.1, GTK 4.24.1 and Pango 1.58.2.

### Windows and macOS evidence to carry

Windows evidence is in
[Shared-Core-Windows-Evaluation.md](../docs/Shared-Core-Windows-Evaluation.md) and
[results-windows-2026-10-25.yaml](../spikes/feas006/results-windows-2026-10-25.yaml).
The Rust-owned bulk C ABI passed C# SafeHandle/C++ RAII cleanup, 1,000 sequential
and 400 concurrent loads, ten observed-entry cancellations, panic containment,
all eight outlines, native Rust source/PDB debugging and a normal-DLL package.
Virtualized 1/5 MiB authored-entry callbacks were about 381/459 ms. Continuous
controls still stall. FEAS-008 Windows has since shown document-wide selection,
UIA reading order and Narrator navigation over virtualized rows; see its
report.

macOS evidence is in
[Shared-Core-macOS-Evaluation.md](../docs/Shared-Core-macOS-Evaluation.md) and
[results-macos-2026-10-25.yaml](../spikes/feas006/results-macos-2026-10-25.yaml).
Swift imports the C header, copies and frees values before AppKit work, and
passes strict concurrency, lifecycle/cancellation, all eight outlines, mixed
Swift/Rust source debugging and relocated normal-dylib bundles. TextKit 2
1/5 MiB entry-to-draw medians were about 217/498 ms. A retained TextKit 1
timeout and synchronous conversion/layout remain.

**Mac memory risk:** twenty native-only 5 MiB parses grew RSS to about 583 MiB
with zero native owners and about 5.1 MiB live malloc usage. `leaks` reported
zero leaked bytes and `vmmap` attributed most growth to empty large malloc
regions; Swift-copy runs retained similar RSS. This is neither a leak proof nor
an acceptable repeated-session budget.

Across all hosts, the flat spike model still has snapshot-local IDs and lacks
complete inline/source mappings, typed links/images, metadata, diagnostics,
search, incremental updates and production resource policy. Cancellation polls
at event boundaries. Unsafe FFI calls rely on the documented pointer, alignment
and lifetime contract. Preserve these limits for GATE-001.

## TODO.md session handoff history

From TODO.md "Immediate session handoff".

The shared-core review PR #16 is merged. The current handoff is the
[CORE-FIX phase](../TODO.md#core-fix-phase-shared-core-review-follow-ups), starting with
DOC-001. The history that follows is kept until DOC-001
trims it.

PR #7 (CORE-006A C ABI facade, [contract](../specs/ffi/README.md)) merged as
`a751f48`; it unblocks each host's 001B adapter phase, and CORE-006 stays open
until the Swift, C# and gtk-rs consumers pass there. PR #8 (**CORE-005A**
source-accurate find, word count and section timing;
[contract](../specs/search/README.md)) merged as `237cd88`, and PR #9
(**CORE-005A2**, `md_snapshot_find`/`md_snapshot_read_stats` under D-022)
merged as `ec58c84`. **CORE-005B** adds capability-fed
[workspace search primitives](../specs/workspace-search/README.md) in Rust,
merged in PR #10 as `0bf8eb7`. Its C ABI exposure, the owner-approved split
**CORE-005B2** required by each platform's 002C, merged in PR #11 as
`28e33bc`. **CORE-003B3** maps GFM alerts and footnotes to existing v1/v2
nodes. Math becomes explicit source fallbacks, and voice-script opt-in gets
a diagnostic ([extension semantics](../docs/Parsing.md#extension-semantics-core-003b3)).
It merged in PR #12 as `66c2162`. **CORE-003B4** reconciles the corpus through a
generated [conformance contract](../specs/parsing/conformance/README.md), adds
mutation and pathological-input coverage, and records named-host parser
time/memory with enforced heap budgets ([Parser-Budgets.md](../docs/Parser-Budgets.md)).
It raises the node/event defaults (D-024) and scopes the source-offset
fallback to the affected top-level block. It merged in PR #13 as `29f7334`,
and D-024 is accepted. CORE-003B was refined into four
tracked children: B1 metadata; B2 reference representation/URI compatibility;
B3 remaining extension semantics; B4 conformance/fuzz/property and parser
budgets. See [the catalog](../docs/Implementation-Plan.md) for scope/dependencies.
The parent retains all original acceptance and native packaging gates.
Inspect state and `HANDOFF.md` before any pass. Scoped commits/pushes/PR/review are authorized; merge requires
owner authorization; concurrency remains one.

- **CORE-003B2F — PR 5 URI and source-span follow-up (merged in PR #5).** Harden the Unicode
  visibility policy for URI targets and shared front-matter text, considering
  compatibility with legitimate text and pinned Unicode provenance. Test
  literal and percent-encoded invisible characters in paths and external URIs.
  Document and regression-test the v1 correction for blockquote continuation
  destination spans. Evaluate a bounded diagnostic for shadowed duplicate
  reference definitions; retain CommonMark first-definition precedence and
  document the decision. This follows B2 and gates B4 and the B aggregate.

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

## Superseded posture before CORE-001 closeout

After CORE-003F (PR #32, D-034), the handoff named CORE-001 as next, including
both TOOL-003 deferred findings from PR #23. CORE-001's shared workspace/CI
acceptance and those findings are now closed in PR #33; CORE-002 is next.
The earlier handoff's claim that `main` and `develop` were in sync applied to
the milestone sync, before the subsequent task PRs on `develop`.

## Superseded CORE-001 handoff before CORE-002 closeout

- **Done:** CORE-001 in [PR #33](https://github.com/awesomejt/markdown/pull/33):
  shared workspace/CI acceptance, both TOOL-003 findings and both review nits.
  Local `tools/check.py all` passed on macOS 27.0.1 with Rust 1.99.1;
  [hosted CI](https://github.com/awesomejt/markdown/actions/runs/40991275826)
  passed on macOS, Linux and Windows at `29b87d4`.
  [Grok review](https://github.com/awesomejt/markdown/pull/33#pullrequestreview-5707355475)
  at `0440908` had no serious finding; both nits were fixed at `29b87d4`,
  exempt from re-review under step 7. No live pass or lease remains.

The next task was CORE-002; its acceptance is now closed in PR #34.

## Superseded CORE-002 handoff before CORE-003 closeout

- **Done:** CORE-002 in [PR #34](https://github.com/awesomejt/markdown/pull/34):
  [aggregate model acceptance](../docs/Core-Model-Acceptance.md), with frozen
  schemas/goldens unchanged and D-021's snapshot-local MVP identity explicit.
  Local `tools/check.py all` passed on macOS 27.0.1 with Rust 1.99.1 at
  source baseline `0270e6b`; common checks passed for the acceptance docs.
  [PR CI](https://github.com/awesomejt/markdown/actions/runs/40993369885)
  passed at `453f183`.
  [Grok review](https://github.com/awesomejt/markdown/pull/34#pullrequestreview-5707501199)
  at that head had no serious finding; the sole link-anchor nit is fixed in
  the closeout commit and exempt from re-review under step 7. A second review
  submission only corrects the suggestion's line range; no re-review was
  requested. No live pass or lease remains.

The next task was CORE-003; its parsing/safety acceptance is now closed in PR #35.

## Superseded CORE-003 handoff before CORE-004 closeout

- **Done:** CORE-003 in [PR #35](https://github.com/awesomejt/markdown/pull/35):
  [parsing/safety acceptance](../docs/Core-Parser-Acceptance.md), covering corpus,
  malformed/safety, generated/hostile inputs and parser budgets, with no
  runtime/policy/limit/schema/golden change. Full local `tools/check.py all`
  passed on Apple M5 Max/macOS 27.0.1 with Rust 1.99.1 at baseline `cfe3e31`.
  Fresh 11-run release benchmarks and common documentation checks passed.
  [PR CI](https://github.com/awesomejt/markdown/actions/runs/40994659002)
  passed at `97d8888`.
  [Grok review](https://github.com/awesomejt/markdown/pull/35#pullrequestreview-5707583058)
  at that head had no serious finding. Both documentation nits are fixed in
  the closeout: explicit v1 credential allowance and the v2 link-heavy
  benchmark cost. They need no re-review under step 7; no findings deferred.
  No live pass or lease remains.

The next task was CORE-004; its session acceptance is now closed in PR #36.

## Superseded CORE-004 handoff before CORE-005 closeout

- **Done:** CORE-004 in [PR #36](https://github.com/awesomejt/markdown/pull/36):
  [session acceptance](../docs/Core-Session-Acceptance.md), covering generations,
  cancellation, immutable snapshots, change handling, deduplication and anchors.
  Focused session tests passed: 28 default, 29 in each v2/all-feature debug and
  all-feature release run, plus generation exhaustion. Full local
  `tools/check.py all` and common documentation checks passed on macOS 27.0.1
  with Rust 1.99.1 at baseline `b958f6e`.
  [PR CI](https://github.com/awesomejt/markdown/actions/runs/41010966470)
  passed at `261e587`.
  [Grok review](https://github.com/awesomejt/markdown/pull/36#pullrequestreview-5708589037)
  at that head had 0 blockers, 0 concerns and 0 nits. One review, no re-review
  requested, no findings deferred. No live pass or lease remains.

The next task was CORE-005; its search/read acceptance is now closed in PR #37.

## Superseded shared-core posture before CORE-005 closeout

  defect. Follow-ups: CORE-003F (front-matter joiners and subdivision flags,
  D-034 amending D-032) merged in PR #32; CORE-001 closed in PR #33 with
  hosted three-OS CI and both TOOL-003 findings fixed. CORE-002 closed in
  PR #34 on [model acceptance](../docs/Core-Model-Acceptance.md), with the
  frozen contracts unchanged and D-021's identity policy explicit. CORE-003
  closed in PR #35 with [parser/safety acceptance](../docs/Core-Parser-Acceptance.md)
  and fresh named-Mac benchmarks; runtime behavior is unchanged. CORE-004
  closed in PR #36 with [session acceptance](../docs/Core-Session-Acceptance.md),
  including explicit race-coverage and adapter-ordering limits. CORE-005 is next;

## CORE-005 closeout handoff (2026-10-30)

- **Done:** CORE-005 in [PR #37](https://github.com/awesomejt/markdown/pull/37):
  [search/read acceptance](../docs/Core-Search-Acceptance.md), with a regression
  for full/collapsed/shortcut/image references and corrected parity wording.
  The accepted v1 workspace index stays unchanged; v2 find need not reproduce
  a reference-syntax hit. No runtime/limit/ABI/schema/golden change.
  Focused regression, final `tools/check.py all` and common checks passed on
  macOS 27.0.1/Rust 1.99.1 after the review fixes.
  [Fixed-head three-OS CI](https://github.com/awesomejt/markdown/actions/runs/41013505189)
  passed at `6bd6cd5`.
  [Grok review](https://github.com/awesomejt/markdown/pull/37#pullrequestreview-5708692468)
  at `68cb794` had 0 blockers, 1 low wording concern and 2 nits, all fixed at
  `6bd6cd5` and exempt from re-review under step 7. One review, no re-review
  requested, all threads resolved, no findings deferred. No live lease remains.

## CORE-006 closeout handoff (2026-10-30)

- **Done:** CORE-006 in [PR #38](https://github.com/awesomejt/markdown/pull/38):
  [facade acceptance](../docs/Core-Facade-Acceptance.md), existing ABI v1 zero
  content-budget change from 8 to 2 MiB documented, shared/native evidence
  boundaries reconciled. No runtime/header/schema/golden/dependency change.
  Local `tools/check.py all` and staged common checks passed on macOS
  27.0.1/Rust 1.99.1. Fresh
  [three-OS debug and all-feature release CI](https://github.com/awesomejt/markdown/actions/runs/41024924816)
  passed at `4339669265c139fa493d4e73623ad9a9c278e503`; macOS/Linux compiled
  C smoke passed, Windows MSVC smoke remains WIN-001B.
  [Grok review](https://github.com/awesomejt/markdown/pull/38#pullrequestreview-5709806212)
  at that head had 0 blockers, 0 concerns and 2 nits. Both are addressed in
  the closeout: explicit all-feature-only release scope and committed run/head
  evidence. One review, no re-review needed, no findings deferred.

## MAC-001A posture superseded by MAC-001B (2026-10-30)

- MAC-001A (PR #39) adds the AppKit window/menu/lifecycle shell, SwiftPM
  development bundle and required path-filtered macOS 27 native CI smoke.
  The development bundle ID is `io.github.awesomejt.markdown`; no external
  Swift packages or core link yet. MAC-001B is next: model-v2 C ABI package,
  native lifecycle/errors/cancellation and measured encode/session/RSS bounds.

- Repeated native-only 5 MiB parses on the Mac grew RSS to about 583 MiB
  after twenty loads with zero native owners and no leaked bytes. It looks
  like macOS allocator retention; Linux/glibc stays flat. No acceptable
  repeated-session budget is established.

## MAC-001A closeout handoff archived after MAC-001B (2026-10-30)

- **Done:** MAC-001A in [PR #39](https://github.com/awesomejt/markdown/pull/39):
  [native AppKit shell](../apps/macos/README.md), standard window/menu/keyboard
  lifecycle, SwiftPM development bundle and required `macos-native` CI on
  `xcode-27`. Bundle ID: `io.github.awesomejt.markdown`. No core link yet.
  Local native debug/release smoke, full `tools/check.py all`, final common/
  gate checks and visible native UI checks passed. Fixed-head
  [CI run 41087967326](https://github.com/awesomejt/markdown/actions/runs/41087967326)
  at `1d818e4eee73862e7b8d50ed1c5780c9b45bb37b` includes the actual macOS 27
  native debug/release job; exact results are in the completion pass.
  [Grok review](https://github.com/awesomejt/markdown/pull/39#pullrequestreview-5715121608)
  at `d57667a` had 0 blockers, 2 soft CI concerns and 3 nits, all addressed at
  `1d818e4`: runner proof/maintenance, bounded AppKit transition polling,
  timeout diagnostics, plain gate checks and the GitHub-owned development ID.
  One review, no re-review required for those non-serious fixes, all threads
  resolved, no findings deferred.
