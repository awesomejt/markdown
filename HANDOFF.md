# Agent Session and Platform Handoff

Updated: 2026-11-13 (America/Denver), LNX-001H completed under D-029 (PR #67 closed out and merged into `develop`).
Active task: Next execution task `LNX-001I` (`5f484073-f083-404e-bb0d-47cdc3531582`) local development artifact, licenses, relocated launch on Linux, or Windows/macOS walking skeleton milestone review under D-030.
Pass `aa39b32d-c984-4deb-99b9-36392cba77d1` completed, approved PR #67 merged into `develop`.
`apps/linux/` delivers LNX-001H:

- Core snapshot anchors: `md_snapshot_anchor_at`, `md_snapshot_anchor_for_node`, and `md_snapshot_resolve_anchor` with `CoreSnapshot` thread-safe handle and non-blocking worker drop release, plus `CoreSession::external_change`.
- External file watching & refresh: inotify file monitoring with generation validation, preserving reading position via node anchor or fallback fragment scroll ratio; replacement/deletion/truncation/race resilience.
- Session & position restoration: atomic JSON restoration persistence under `$XDG_STATE_HOME/markdown/restoration.json`, multi-window and single-window lifecycle management (window count tracked, store preserved across clean exits), and virtualized layout transition scroll tracking to avoid layout race conditions.
- Test suites & verification: `apps/linux/tools/check.py` comprehensive test suite passes in Debug and Release (render mapping, JSON parser, relaunch checks with headless Xvfb, core probe with 0 leaked handles); `apps/linux/tools/measure.py refresh` passes performance budgets across all document profiles (`kept_place: true`).

PR #67 reviewed by Grok Bot across 3 reviews; review 3 reported 0 Blockers (clean approval under D-029).

Next Linux task: `LNX-001I` (local development artifact, licenses, relocated launch). Next Windows/macOS task: walking skeleton milestone review (D-030).

Previous Linux task: `LNX-001H` merged in [PR #67](https://github.com/awesomejt/markdown/pull/67). Previous Windows task: `WIN-001I` merged in [PR #61](https://github.com/awesomejt/markdown/pull/61).

Previous Linux handoff baseline: `develop` after PR #52; reviewed head `f12d2e5`, non-serious
review fixes at `27e84df`, followed by one state/docs closeout. The merge SHA is
not known at closeout time.
Preserve intended source, lock/spec/golden files, docs and state together. Never transfer caches, environments, build
products, private runtime state or user documents.

Older per-phase handoffs and the feasibility host notes are in
[the memory archive](agents/Memory-Archive.md).

## Resume here — desktop foundations

CORE-FIX (PRs #17 to #27) is merged into `main` through the first milestone
review, [PR #28](https://github.com/awesomejt/markdown/pull/28), tagged
`m-core-fix` (D-030). `develop` now includes the follow-up task PRs. Its three reviews
(Claude, Grok, Codex) are in [docs/reviews/](docs/reviews/README.md); none
found a blocking defect. The follow-ups are in
[TODO.md](TODO.md#core-fix-milestone-follow-ups) and in state. The CORE-FIX
done list is in [the memory archive](agents/Memory-Archive.md#handoffmd-phase-handoff-history).

- **Done:** CORE-003F (milestone M1) in PR #32, D-034: front-matter titles
  and values allow ZWNJ/ZWJ between visible characters and subdivision-flag
  tag sequences; keys, bidi controls, LRM and RLM stay rejected.
- **Done:** CORE-001 in [PR #33](https://github.com/awesomejt/markdown/pull/33):
  shared workspace/CI acceptance and both TOOL-003 findings.
- **Done:** CORE-002 in [PR #34](https://github.com/awesomejt/markdown/pull/34):
  [aggregate model acceptance](docs/Core-Model-Acceptance.md), frozen contracts
  unchanged and D-021's snapshot-local identity explicit.
- **Done:** CORE-003 in [PR #35](https://github.com/awesomejt/markdown/pull/35):
  [parsing/safety acceptance](docs/Core-Parser-Acceptance.md), with fresh
  named-Mac benchmarks and no runtime change.
- **Done:** CORE-004 in [PR #36](https://github.com/awesomejt/markdown/pull/36):
  [session acceptance](docs/Core-Session-Acceptance.md), with explicit race
  coverage, adapter-ordering and memory limits.
- **Done:** CORE-005 in [PR #37](https://github.com/awesomejt/markdown/pull/37):
  [search/read acceptance](docs/Core-Search-Acceptance.md), with explicit
  reference-syntax regressions and v1-workspace/v2-find boundaries. Detailed
  validation and review disposition are in the pass record and archive.
- **Done:** CORE-006 in [PR #38](https://github.com/awesomejt/markdown/pull/38):
  [facade acceptance](docs/Core-Facade-Acceptance.md), ABI v1 default-change
  history and fresh three-OS release checks. `portable-core` is complete.
- **Done:** MAC-001A in [PR #39](https://github.com/awesomejt/markdown/pull/39):
  native AppKit window/menu/lifecycle shell and required macOS 27 CI smoke.
  Detailed closeout evidence is in the memory archive and pass record.
- **Done:** MAC-001B in [PR #40](https://github.com/awesomejt/markdown/pull/40):
  [model-v2 static package and Swift adapter](docs/macOS-Core-Integration.md),
  ABI negotiation, owned snapshot copies, bounded worker/admission, prompt
  queued cancellation and shared async cleanup. Matching debug/release tests
  (10 each), relocated AppKit bundles/release consumer, local full portable
  suite and common checks passed on Apple M5 Max/arm64, macOS 27.0.1/Xcode 27.1.
  [Fixed-head native/three-OS CI](https://github.com/awesomejt/markdown/actions/runs/41103418470)
  passed at `eef0b48c1890886a2b501f24d0e0e789563ecd32`.
  [Grok review 2](https://github.com/awesomejt/markdown/pull/40#pullrequestreview-5716513463)
  at `be4010e9fc813db7eddd4cec4390ef264a4a3b20` confirms review-1 fixes, with no blocker/concern.
  Its sole test nit was fixed at `eef0b48` without re-review.
  Two reviews used; all findings addressed and threads resolved.
  Host source limit is 5 MiB; dense 8 MiB default/session and 5 MiB host
  peak RSS are 577.9/361.8 MiB. Repeated 5 MiB loads retain up to 1,698.1 MiB
  RSS after 20 sessions despite zero native handles and no leak found.
  MAC-001G must resolve or obtain owner disposition before aggregate acceptance.
- **Done:** MAC-001C in [PR #41](https://github.com/awesomejt/markdown/pull/41):
  [file open, association, typed access errors and generation-safe loading](apps/macos/README.md#opening-documents-mac-001c).
  Debug and release native checks each passed 7 document and 10 core tests,
  plus both relocated bundle smokes with document checks. Launch Services
  recorded the Viewer/Alternate claim. [Grok review 1](https://github.com/awesomejt/markdown/pull/41#pullrequestreview-5717458097)
  at `8b44d4f` found 0 blockers, 2 concerns and 2 nits. Both concerns and one
  nit were fixed at `b5da5b6` without re-review (none was high or critical).
  The dedup nit is deferred in TODO.md. One review was used.
- **Done:** MAC-001D in [PR #42](https://github.com/awesomejt/markdown/pull/42):
  [TextKit 2 rendering, outline sidebar and navigation](apps/macos/README.md#reading-documents-mac-001d).
  A new `MarkdownRender` target renders off the main thread (newest render
  wins, cancellable). Debug and release native checks each passed 8 render,
  7 document and 10 core tests, plus both relocated bundle smokes with reading
  checks. All 8 corpus outlines match their expectations. Release 5 MiB prose
  renders in about 540 ms off-main and installs in 15.6 ms; installing the
  1 MiB long-line profile blocks the main thread for about 246 ms (MAC-001G).
  [Grok review 1](https://github.com/awesomejt/markdown/pull/42#pullrequestreview-5718332317)
  at `f5cca9f` found 0 blockers, 2 concerns and 2 nits; all four were fixed at
  `5fcb25d` without re-review (none was high or critical). One review was used.
  Its review threads have replies but were left unresolved: the agent's
  permission classifier blocked resolving them. Fixed-head CI
  [41124510842](https://github.com/awesomejt/markdown/actions/runs/41124510842)
  passed on its third attempt. The first two hit a hosted-runner rustup
  `cargo-fmt` conflict, and dispatched `develop` runs flaked in MAC-001C
  checks; see the TODO.md hosted `macos-native` flakiness entry.
- **Done:** MAC-001E in [PR #43](https://github.com/awesomejt/markdown/pull/43):
  [tables, task lists and the image resource policy](apps/macos/README.md#tables-task-lists-and-images-mac-001e).
  Debug and release native checks each passed 13 render, 8 document and
  10 core tests, plus both relocated bundle smokes with the rich-block check.
  [Grok review 1](https://github.com/awesomejt/markdown/pull/43#pullrequestreview-5719371478)
  at `9a0cb5f` found 0 blockers, 1 concern (`realpath` on the main actor)
  and 1 nit (transient full-decode memory). Both were fixed at `dbced29`
  without re-review; one review was used. Release prose 5 MiB renders in
  about 580 ms (MAC-001D: 540 ms).
- **Done:** MAC-001F in [PR #44](https://github.com/awesomejt/markdown/pull/44):
  [find, text size and appearance](apps/macos/README.md#find-text-size-and-appearance-mac-001f).
  Debug and release native checks each passed 13 render, 6 find, 8 document
  and 11 core tests, plus both relocated bundle smokes with the find, text
  size, appearance and Settings checks.
  [Grok review 1](https://github.com/awesomejt/markdown/pull/44#pullrequestreview-5719953560)
  at `d7ba449` found 0 blockers, 1 concern and 3 nits. The concern and two
  nits were fixed at `71a7dd8` without re-review (none was high or critical).
  The footnote nit doesn't apply, because labels are not searchable. One
  review was used. A release find over 5 MiB with 10,000+ matches takes
  46–56 ms, so macOS needs no cancellable find variant. Manual check 8 is
  unrun.
- **Done:** MAC-001G in [PR #45](https://github.com/awesomejt/markdown/pull/45):
  [large documents, copy, VoiceOver rotors and budgets](apps/macos/README.md#large-documents-and-accessibility-mac-001g).
  It resolves the repeated-load RSS gate: macOS `realloc` growth in the
  large range left dirty "Malloc Large (empty)" regions, so the core's
  macOS-only global allocator now copies reallocations of 1 MiB and more.
  The app retains 227–581 MiB over twenty 5 MiB loads; it had grown about
  81 MiB per load. Bounded paragraphs, a lead shown first, clear-then-set
  text replacement, lazy RTF/RTFD copy and heading/level/table/image rotors
  are in. Debug and release native checks each passed 26 render, 8 document
  and 11 core tests plus both bundle smokes; `tools/check.py all` passed.
  [Grok review 1](https://github.com/awesomejt/markdown/pull/45#pullrequestreview-5720913417)
  at `ee4c8d5` found 0 blockers, 2 concerns and 3 nits. Both concerns and
  two nits were fixed at `34fabd1` without re-review, and one nit is
  deferred. One review was used. Open budgets are in
  [TODO.md](TODO.md#open-native-budgets): prose 5 MiB warm launch (median
  547 ms), unrun cold launches, the first VoiceOver line query on 5 MiB
  (3.6 s, then ~1 GiB) and mixed-scale displays.
- **Done:** MAC-001H in [PR #46](https://github.com/awesomejt/markdown/pull/46):
  [external refresh, anchored reading positions and restoration](apps/macos/README.md#external-refresh-and-restoration-mac-001h).
  A kqueue watcher (following atomic saves, deletion and reappearance) and
  a stat check on window activation feed the core session's change
  dispositions; refreshes load behind the shown revision; the top line and
  selection carry across revisions through core anchors (each
  `CoreDocument` now owns its snapshot handle); a failed refresh keeps the
  last version with a notice; AppKit state restoration saves a bookmark,
  anchors and the sidebar state. Debug and release native checks each
  passed 30 render, 11 document and 13 core tests, both bundle smokes and a
  new real two-launch relaunch check. `measure.py refresh` kept the place
  in 10 of 10 refreshes on each profile (5 MiB: about 1.05 s per refresh).
  [Grok review 1](https://github.com/awesomejt/markdown/pull/46) at
  `ba7f337` found 0 blockers, 3 concerns and 1 nit; all four were fixed at
  `394fb11` without re-review. One review was used. The MAC-001G text-size
  nit is resolved; the refresh footprint (5 MiB: flat 887–1,043 MiB) is a
  new open budget in [TODO.md](TODO.md#open-native-budgets). Manual check
  10 is unrun.
- **Done:** MAC-001I in [PR #47](https://github.com/awesomejt/markdown/pull/47):
  [development artifact, license inventory and relocated launch](apps/macos/README.md#development-artifact-and-licenses-mac-001i)
  and [skeleton acceptance](docs/macOS-Skeleton-Acceptance.md). An unsigned,
  stripped arm64 zip (about 1.1 MB) with notices for 43 linked packages and
  no build-machine paths; extracted outside the checkout it passed the full
  smoke directly and through Launch Services. Debug and release native
  checks passed (30 render, 11 document, 13 core tests, both smokes and
  relaunch checks). [Grok review 1](https://github.com/awesomejt/markdown/pull/47)
  at `e9a6dd5` found 0 blockers, 1 concern (Launch Services registration
  and a timeout orphan) and 1 nit (inventory is a superset); both fixed at
  `c837f23` without re-review. One review was used. A clean second Mac and
  a quarantined copy are untested (CAP-28).
- **Done:** MAC-001 aggregate in [PR #48](https://github.com/awesomejt/markdown/pull/48):
  [aggregate evidence](docs/macOS-Skeleton-Acceptance.md#mac-001-aggregate).
  No source change. Fresh debug/release `apps/macos/tools/check.py` passed
  at `96d8aea` (30 render, 11 document, 13 core tests, smokes, relaunch,
  artifact and release consumer), hosted `develop` run 41215648620 passed
  `macos-native`. AppKit throughout; SwiftUI stays allowed for later shell
  surfaces (D-010). [Grok review 1](https://github.com/awesomejt/markdown/pull/48#pullrequestreview-5723675753)
  at `679068c` found 0 blockers, 0 concerns and 2 nits; both fixed in the
  closeout without re-review. One review was used.
- **Done:** LNX-001A in [PR #49](https://github.com/awesomejt/markdown/pull/49):
  [GTK 4 shell, build/check instructions and `linux-native` CI](apps/linux/README.md).
  Separate `apps/linux` Cargo workspace; `tools/check_dependencies.py linux`
  checks its 65-package graph (target-lexicon 0.13.5 reviewed for it alone).
  `apps/linux/tools/check.py` passed on CachyOS/KDE Plasma 6.8.2 Wayland,
  GTK 4.24.1, Rust 1.99.1 (debug and release, both smoke modes; X11 via
  Xwayland too); AT-SPI inspection and second-launch activation checked.
  The first hosted `linux-native` run passed on `ubuntu-24.04`.
  [Grok review 1](https://github.com/awesomejt/markdown/pull/49) at
  `f4b9f36` found 0 blockers, 0 concerns and 2 nits; both fixed at `b84db0d`
  without re-review. One review was used. Physical keys, window controls and
  the About dialog's appearance are the README's unrun manual check.
- **Done:** LNX-001B in [PR #50](https://github.com/awesomejt/markdown/pull/50):
  [model-v2 core library package and Rust adapter](docs/Linux-Core-Integration.md).
  `package_core.py` stages the cdylib (24 exports exactly; the staticlib's
  private `std` breaks the LTO release link); `apps/linux/core` negotiates
  the ABI, requests model v2 under 5 MiB/500k/1M/128 MiB host limits, runs
  all heavy work on one worker with two process-wide admitted parses, and
  copies owned, index-checked values. 15 consumer tests passed in debug and
  release, plus both smokes with an in-app core parse, on Wayland and under
  `xvfb-run` X11. Release 5 MiB prose: parse 355 ms, encode 113 ms, copy
  8.7 ms (median); retained RSS 130 MiB is glibc caching (46 MiB after
  `malloc_trim`); dense 8 MiB default peak 652 MiB, 5 MiB host 379 MiB.
  [Grok review 1](https://github.com/awesomejt/markdown/pull/50) at
  `d4f20b1` found 0 blockers, 1 concern (blocking `close()` on the GTK main
  thread) and 2 nits (process-wide admission; cross-table validation); all
  three fixed at `0526c9b` without re-review. One review was used. Hosted
  `linux-native` then needed PyYAML for the fixture generator (`b8d7de0`);
  [run 41282215270](https://github.com/awesomejt/markdown/actions/runs/41282215270)
  passed every job.
- **Done:** LNX-001C in [PR #51](https://github.com/awesomejt/markdown/pull/51):
  [native file opening, Reload, capabilities and background loading](apps/linux/README.md#local-files-and-loading-lnx-001c).
  Native Wayland and X11 checks pass in debug/release: 11 document + 15 core
  tests per build, shell/picker and isolated desktop-entry smokes. Portable
  checks pass with ignored private agent Markdown excluded from lint only.
  [Grok review 1](https://github.com/awesomejt/markdown/pull/51#pullrequestreview-5727226665)
  at `d5f6b4b` found 0 blockers, 3 concerns and 4 nits, no high/critical finding.
  `aef0cd4` makes Open transactional, stops idle polling, cleans up programmatic
  destruction and sizes bounded buffers from metadata; it documents dispatcher
  delays. One disposal-performance nit is deferred in TODO.md, and the explicit
  x86-64 baseline gate is retained with rationale. One review used, all threads
  resolved; non-serious fixes need no re-review. [Fixed-head CI](https://github.com/awesomejt/markdown/actions/runs/41289847319)
  is green. Live AT-SPI inspection failed due to host bus/service errors;
  portal interaction/revocation and physical unplug remain unclaimed.
- **Done:** LNX-001D in [PR #52](https://github.com/awesomejt/markdown/pull/52):
  [native virtualized reader, outline and basic selection](apps/linux/README.md#native-reader-lnx-001d).
  Plans prepare off-main under cancellation and a 128 MiB accounted budget;
  escaped fragments stay within 512 UTF-8 bytes. Safe links use epoch-bound
  tokens; no relative-file grants or resource fetching. Global selection is G.
  [Grok review 1](https://github.com/awesomejt/markdown/pull/52#pullrequestreview-5729937327)
  at `f12d2e5` found 0 blockers, 2 concerns and 4 nits, no high/critical finding.
  `27e84df` fixes soft/hard breaks, repeated heading roles and small code nits;
  packaged-core tests prove source scheme canonicalization. Reload-position
  preservation is deferred to H in TODO.md. One review used, all threads
  resolved; [fixed-head CI](https://github.com/awesomejt/markdown/actions/runs/41344465909)
  is green, including GTK 4.14. Full Wayland/X11 debug/release checks pass
  12 loader + 8 renderer + 15 core tests per build and native/association smokes.
  Live AT-SPI Registry activation failed; no service/settings changes or Orca
  proof were attempted. Native role/navigation checks do not satisfy D1.
- **Done:** WIN-001A in [PR #53](https://github.com/awesomejt/markdown/pull/53):
  [WinUI 3 C# Native AOT shell, build/check instructions and `windows-native` CI](apps/windows/README.md#win-001a-local-evidence).
  Debug and Release Native AOT shell smokes pass; native menu/window/keyboard
  and UIA3 shell checks recorded. [Grok review 1](https://github.com/awesomejt/markdown/pull/53#pullrequestreview-5730825360)
  at `bc000d7` had no serious finding; `357a27d` fixed its Bash concern and
  shortcut-binding nit without re-review, and [all-host CI](https://github.com/awesomejt/markdown/actions/runs/41363512342)
  passed.
- **Done:** WIN-001B in [PR #54](https://github.com/awesomejt/markdown/pull/54):
  [model-v2 core DLL package and C# Native AOT adapter](docs/Windows-Core-Integration.md).
  17 consumer tests pass in Debug and Release, plus Debug/Release shell + core
  smokes, relocated Release smoke/probe and required MSVC `cl.exe` C smoke.
  Release 5 MiB prose: median parse 528.5 ms, encode 169.0 ms, copy 18.2 ms;
  retained working set across 20 sessions is flat at 51.6–52.5 MiB (GC heap
  5.13 MiB, handles 0,0,0); dense 8 MiB default peaks at 796.9 MiB and 5 MiB
  host at 502.9 MiB. [Grok review 1](https://github.com/awesomejt/markdown/pull/54#pullrequestreview-5731262483)
  at `11aecab` had 0 blockers, 3 concerns and 2 nits; all 5 findings were fixed at
  `059d18c`, all 4 threads resolved, and [all-host CI](https://github.com/awesomejt/markdown/actions/runs/41375068539)
  passed.
- **Done:** WIN-001C in [PR #55](https://github.com/awesomejt/markdown/pull/55):
  [native file open, file associations, Reload, and generation-safe loading](apps/windows/README.md#local-files-and-loading-win-001c).
  17 core consumer tests and 12 document tests pass in Debug, Release, and
  relocated Release, plus Debug/Release/relocated shell+picker+reload smokes and
  4-delivery multi-process association checks (`check_association.py`).
  [Grok review 1](https://github.com/awesomejt/markdown/pull/55#pullrequestreview-5731847930)
  at `3915231` had 0 blockers, 4 concerns and 1 nit; all 5 findings were fixed at
  `60f0970` without re-review, all 5 threads resolved, and
  [fixed-head CI](https://github.com/awesomejt/markdown/actions/runs/41385009602)
  passed.
- **Done:** LNX-001D1 in [PR #63](https://github.com/awesomejt/markdown/pull/63):
  [document accessibility inspection and MVP scope under D-035](docs/Linux-Accessibility-Gate.md).
  Automated AT-SPI probe confirms heading roles with level 1 in model order, paragraph roles,
  and outline navigation focus; off-screen Orca structural heading navigation and continuous
  reading are formally deferred to ADV-001 under owner decision D-035. Grok review 1 reported
  0 blockers, 3 concerns, 1 nit; all findings addressed at `22277a1` with green CI. LNX-001E is next on Linux.
  The Linux 001A–001C milestone review packet remains due under D-030; hand it
  to the owner along with the existing shared/Mac milestone work. This task
  authorizes no milestone merge into `main`.
- **Next:** `desktop-apps` is active. No Mac task is eligible until
  DESK-001 (it needs WIN-001 and LNX-001). On this Mac the next step is the
  D-030 milestone review: the macOS skeleton (rule 3) and the overdue
  8-PR cap since `m-core-fix`. Fix the hosted `macos-native` flakiness
  (TODO.md) first if it keeps failing `develop`; it recurred on PR #49.
  WIN-001D follows the approved WIN-001C merge; the host chains remain independent.
  The shared-core milestone boundary is reached. Per MEMORY.md, Claude writes
  the combined packet for CORE-001–CORE-006 (plus CORE-003F since `m-core-fix`),
  including MAC-001A/B if freezing current `develop`; name the exact snapshot and
  every task it contains. Grok/Codex then add independent reviews on
  `review/<milestone>`. Follow D-030
  to freeze green `develop` with no task PR waiting and open the milestone PR;
  only the owner merges into `main`. No milestone review branch/PR is created
  by this task closeout; hand off packet preparation to Claude.
- **Done (milestone M4):** `main` now requires the `complete` check from the
  `Checks` workflow (owner-approved 2026-10-29).
- The next milestone is ready for packet preparation: CORE-001 to CORE-006
  are closed. No live pass/lease or unowned working task remains.

Inspect state before any pass, following AGENT_WORKFLOW.md:

```sh
python3 tools/project_state.py --file project.yaml inspect
```

On the Mac, Homebrew `python3` lacks PyYAML, so use
`.build/feas004-venv/bin/python` instead (see the MEMORY.md host notes).

## Evidence and risks to carry

[Decisions](agents/Decisions.md) and [the baseline](docs/Implementation-Baseline.md)
carry the accepted stack and conditions. Prototype reports are dated
evidence, not production contracts. Preserve these open items:

- Mac repeated-session memory is fixed and bounded (MAC-001G), but no numeric
  whole-app budget is accepted. The open Mac budgets are in TODO.md.
- Windows Narrator reading each row twice, and whole-tree UIA searches that
  time out at 5 MiB.
- Linux Orca H heading navigation, and the LNX-001D1 accessibility gate.
- The unrun native packaging, accessibility and performance cases. FEAS-009
  closed by owner-approved deferral (D-014), not by passing; ADV-005 keeps
  its checks.
- Each 001B builds `model-v2` and requests v2 (D-025), measures flat-view and
  retained RSS, and on Windows makes the MSVC C smoke required. It also
  measures session peak memory on a dense 8 MiB input and sets the host's
  session `max_source_bytes`, before IOS-001 (milestone review).

## Workflow and authorization boundaries

The project-state backend portability patch passed native Windows, isolated Mac
and native Linux regression suites on 2026-10-25. On another host, run the
guarded repository patch/check and `--platform-info` before coordination. Do not
hand-edit `project.yaml` or improvise a backend.

After GATE-001, use the small platform-focused phase catalog in
[Implementation-Plan.md](docs/Implementation-Plan.md) and the branch/PR lifecycle
in [Delivery-Workflow.md](docs/Delivery-Workflow.md). Each phase gets a feature
branch and PR; creating the PR starts the Grok review, which the runner
monitors for, and `/grok review` requests another. Housekeeping goes on a
`patch/` branch that Grok ignores and green CI merges (D-033). Scoped phase
commits, pushes, PR updates and review requests are authorized. A Grok
review with no serious finding approves the merge (D-029); other
merges, tags, signing, publishing and production deployment require explicit
owner approval.

Keep `markdown-viewer` read-only. Do not add a sibling build/runtime dependency
or shared UI toolkit. No previous fixture, accessibility or debugging exception
broadens current authorization. Never commit environments, binaries, credentials,
signing material, transcripts, raw private paths or user document contents.

Before switching sessions, refresh this handoff with the source commit, live
task/pass state, evidence links, unresolved risks and exact next action. Ignored
build output and `.agents/` handoffs are not substitutes for committed evidence.
