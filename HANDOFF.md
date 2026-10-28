# Agent Session and Platform Handoff

Updated: 2026-10-28, DOC-001 in progress. Source baseline: `main` at
`1aa2b57`, the PR #16 merge. Preserve intended source, lock/spec/golden
files, docs and state together. Never transfer caches, environments, build
products, private runtime state or user documents.

Older per-phase handoffs and the feasibility host notes are in
[the memory archive](agents/Memory-Archive.md).

## Resume here — CORE-FIX phase

PR #16 merged the three shared-core reviews
([docs/reviews/](docs/reviews/README.md)). The owner accepted D-025, D-026
and D-027 ([Decisions](agents/Decisions.md)). CORE-FIX runs six shared tasks
in order before any platform phase. Their scope is in
[TODO.md](TODO.md#core-fix-phase-shared-core-review-follow-ups).

- **Current:** DOC-001, the agent-facing docs cleanup, is in progress on
  branch `docs/doc-001-agent-docs-cleanup`. It is docs only.
- **Next:** CORE-002B (import-policy parity and FFI host guidance). Then
  CORE-003C, CORE-003D, CORE-005C and CORE-005D, then the CORE-001 to
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
branch and PR, followed by a separate `/grok review` comment. Scoped phase
commits, pushes, PR updates and review comments are authorized; merge, tag,
signing, publishing and production deployment require explicit owner approval.

Keep `markdown-viewer` read-only. Do not add a sibling build/runtime dependency
or shared UI toolkit. No previous fixture, accessibility or debugging exception
broadens current authorization. Never commit environments, binaries, credentials,
signing material, transcripts, raw private paths or user document contents.

Before switching sessions, refresh this handoff with the source commit, live
task/pass state, evidence links, unresolved risks and exact next action. Ignored
build output and `.agents/` handoffs are not substitutes for committed evidence.
