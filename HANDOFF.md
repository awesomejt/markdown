# Agent Session and Platform Handoff

Updated: 2026-10-30 (America/Denver), after MAC-001E (PR #43).
Source baseline: `develop` after PR #43; reviewed head `9a0cb5f`,
validated fix head `dbced29`, followed by one state/docs closeout.
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
- **Next:** `portable-core` is complete and `desktop-apps` is active.
  MAC-001F is next on this Mac: document find, theme and text controls, with
  Unicode find and native command/settings checks. The Mac 001A–001C walking skeleton is a D-030 milestone
  boundary (rule 3). WIN-001A/LNX-001A start on their hosts; their chains
  remain independent.
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

- Mac repeated-session RSS retention: current release consumer reaches
  1,698.1 MiB after twenty 5 MiB loads with zero native handles and no leak
  found. MAC-001G must resolve or obtain owner disposition; the source/queue
  limits do not establish a whole-app RSS budget.
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
