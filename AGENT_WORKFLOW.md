# Agent Workflow

This repository uses a local `project-state/v1` file for recovery-safe task
coordination. The human-readable roadmap is `TODO.md`; live status is
`project.yaml`.

## State helper

Use the repository wrapper, which preserves normal block-style YAML while
delegating contract operations to the `project-yaml-state-management` skill:

```sh
python3 tools/project_state.py --file project.yaml inspect
```

The wrapper requires PyYAML and locates the skill backend at its normal Codex
location. Set `MARKDOWN_STATE_BACKEND` to an alternate backend path when
needed. If either dependency is unavailable, do not hand-edit task, pass,
lease, relationship, or revision fields. Continue read-only work if safe, and
report the missing dependency.

The wrapper uses the backend's exclusive lock, optimistic revisions,
idempotency, and atomic replacement, then emits readable block YAML.
`project.yaml.lock` and transient requests are ignored.

## Start or resume work

1. Read the required files in `AGENTS.md` and inspect Git state.
2. Inspect `project.yaml`.
3. If the snapshot reports stale passes or working tasks without leases,
   recover or resume them before selecting new work.
4. Select the highest-priority eligible task in the active phase.
5. Create a unique request envelope under `.agents/requests/` and invoke
   `start_pass` with the snapshot revision, selected task, concrete objective,
   and a bounded lease.
6. Re-inspect after every state mutation. Never guess a revision or lease
   version.

Only the primary runner owns pass lifecycle calls. Supporting reviewers or
testers return evidence to that runner; they do not start a second pass unless
the state explicitly permits it.

## During a pass

- Make only the selected task's changes.
- Heartbeat before the lease expires during long work.
- Save concise reset-safe handoffs under `.agents/handoffs/` when a pass cannot
  finish in one context.
- Put durable product or architecture decisions in `agents/Decisions.md`, not
  in transient handoffs.
- Add newly discovered work as a task with acceptance criteria and dependency,
  rather than silently expanding scope.

## Finish a pass

Run the checks defined by the task and `docs/Validation.md`, then call
`finish_pass`:

- `completed`: acceptance criteria and required validation passed.
- `partial`: useful work remains; include an exact handoff.
- `blocked`: include the reason, blocker type, and next human action.
- `failed`: record reproducible evidence and recovery guidance.
- `aborted`: work was intentionally stopped; explain why and how to resume.

After finishing, inspect state again, update durable memory if needed, and
leave the project in `active`, `blocked`, or completed phase state—not with an
unowned `working` task.

## Changing roadmap state

- Create tasks through `create_task`.
- Add ordering through `add_dependency`; dependencies must remain acyclic.
- Resolve blockers with evidence through `resolve_blocker`.
- Complete a phase only when all of its tasks are complete, or when the owner
  explicitly authorizes an override recorded in the transition.
- Keep `TODO.md` synchronized when task titles, scope, gates, or acceptance
  criteria change. Routine status and lease changes belong only in
  `project.yaml`.

## Git workflow

- Branch names: `<type>/<task-key-lowercase>-<slug>`.
- Prefer one task per branch and focused Conventional Commits.
- Never stage unrelated user changes.
- Run task checks before asking for review; record exact commands.
- Do not merge, tag, sign, publish, or push without owner authorization.
