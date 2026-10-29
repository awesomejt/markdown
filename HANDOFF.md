# Agent Session and Platform Handoff

Updated: 2026-10-28, review cap and branch settings on `main` (after PR #23).
Source baseline: `develop`, fast-forwarded to that `main` commit. Preserve intended source, lock/spec/golden
files, docs and state together. Never transfer caches, environments, build
products, private runtime state or user documents.

Older per-phase handoffs and the feasibility host notes are in
[the memory archive](agents/Memory-Archive.md).

## Resume here — CORE-FIX phase

PR #16 merged the three shared-core reviews
([docs/reviews/](docs/reviews/README.md)). The owner accepted D-025, D-026
and D-027 ([Decisions](agents/Decisions.md)). CORE-FIX runs seven shared tasks
in order before any platform phase. Their scope is in
[TODO.md](TODO.md#core-fix-phase-shared-core-review-follow-ups).

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
  in the [conformance contract](specs/parsing/conformance/README.md).
- **Done:** PR #22 (`grok` CLI reviews, D-029 review-gated merges) and the
  D-030 milestone workflow. Task PRs now target `develop`; `main` takes only
  owner-approved milestone reviews.
- **Done:** TOOL-003, first-push whitespace check,
  [PR #23](https://github.com/awesomejt/markdown/pull/23). Two deferred
  findings are in TODO.md.
- **Done:** CORE-003D, bare GFM autolinks (D-031),
  [PR #24](https://github.com/awesomejt/markdown/pull/24), merged into
  `develop` after the three-review cap with the owner's approval.
- **Current:** CORE-005C, the only eligible task. Start its pass on a branch
  from `develop`, and target its PR at `develop`.
- **Then:** CORE-005D and CORE-003E. Their end is the first
  milestone review (`review/core-fix` into `main`). Then the CORE-001 to
  CORE-006 aggregates. Each host's chain in `desktop-apps` starts after
  CORE-006 (D-027). Do not select platform tasks before then.

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
  retained RSS, and on Windows makes the MSVC C smoke required.

## Workflow and authorization boundaries

The project-state backend portability patch passed native Windows, isolated Mac
and native Linux regression suites on 2026-10-25. On another host, run the
guarded repository patch/check and `--platform-info` before coordination. Do not
hand-edit `project.yaml` or improvise a backend.

After GATE-001, use the small platform-focused phase catalog in
[Implementation-Plan.md](docs/Implementation-Plan.md) and the branch/PR lifecycle
in [Delivery-Workflow.md](docs/Delivery-Workflow.md). Each phase gets a feature
branch and PR, followed by a Grok review requested through the `grok` CLI.
Scoped phase commits, pushes, PR updates and review requests are authorized.
A Grok review with no serious finding approves the merge (D-029); other
merges, tags, signing, publishing and production deployment require explicit
owner approval.

Keep `markdown-viewer` read-only. Do not add a sibling build/runtime dependency
or shared UI toolkit. No previous fixture, accessibility or debugging exception
broadens current authorization. Never commit environments, binaries, credentials,
signing material, transcripts, raw private paths or user document contents.

Before switching sessions, refresh this handoff with the source commit, live
task/pass state, evidence links, unresolved risks and exact next action. Ignored
build output and `.agents/` handoffs are not substitutes for committed evidence.
