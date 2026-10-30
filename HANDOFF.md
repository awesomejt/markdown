# Agent Session and Platform Handoff

Updated: 2026-10-30 (America/Denver), after MAC-001A (PR #39).
Source baseline: `develop` after PR #39; reviewed head `d57667a`, validated
review fixes `1d818e4`, followed by one state/docs closeout. Preserve intended source, lock/spec/golden
files, docs and state together. Never transfer caches, environments, build
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
  [native AppKit shell](apps/macos/README.md), standard window/menu/keyboard
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
- **Next:** `portable-core` is complete and `desktop-apps` is active.
  MAC-001B is next on this Mac: native model-v2 facade adapter, release-mode
  package consumption, lifecycle/error/cancellation/version checks and measured
  encode/retained/session memory with a host source bound. WIN-001A/LNX-001A
  start on their hosts; their chains remain independent.
  The shared-core milestone boundary is reached. Per MEMORY.md, Claude writes
  the combined packet for CORE-001–CORE-006 (plus CORE-003F since `m-core-fix`),
  including MAC-001A if freezing current `develop`; name the exact snapshot and
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

- Mac repeated-session RSS retention (about 583 MiB after twenty 5 MiB
  parses, with no leak found).
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
