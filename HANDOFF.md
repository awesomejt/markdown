# Agent Session and Platform Handoff

Updated: 2026-10-29 (America/Denver), after CORE-003 (PR #35).
Source baseline: `develop` after PR #35; acceptance head `97d8888`, parser baseline `cfe3e31`. Preserve intended source, lock/spec/golden
files, docs and state together. Never transfer caches, environments, build
products, private runtime state or user documents.

Older per-phase handoffs and the feasibility host notes are in
[the memory archive](agents/Memory-Archive.md).

## Resume here — after the CORE-FIX milestone

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
  [parsing/safety acceptance](docs/Core-Parser-Acceptance.md), covering corpus,
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
- **Next:** CORE-004, then the CORE-005 and CORE-006 aggregates. CORE-006 also records the 8 to
  2 MiB content-search default change in the FFI spec. Each host's chain in
  `desktop-apps` starts after CORE-006 (D-027); do not select platform tasks
  before then.
- **Done (milestone M4):** `main` now requires the `complete` check from the
  `Checks` workflow (owner-approved 2026-10-29).
- The next milestone is the CORE-001 to CORE-006 aggregates closing, or the
  cap of about 8 task PRs or 2 weeks.

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
