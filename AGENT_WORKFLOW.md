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

### Host detection and backend setup

Use the current host's Python, not the shell name, to select platform behavior:

```sh
python tools/patch_project_state_backend.py --check
python tools/patch_project_state_backend.py
python tools/project_state.py --platform-info
python tools/test_project_state.py
```

The patch command supports the original owner-provided backend at SHA-256
`ec6881a156f28825424a99f094db95e67944845aecd542a78e80bfa51497828e`.
It backs up that file beside the installed backend, rejects unknown/local edits,
and is a no-op when the reviewed patch is already installed. Apply it on each
host after installing the skill; it does not change `project.yaml`. Use
`--backend PATH` for an alternate installed copy, and set
`MARKDOWN_STATE_BACKEND` to that same path for the wrapper and tests.

For this Windows checkout, replace `python` above with
`.build/feasibility-venv/Scripts/python.exe` in Git Bash (or the corresponding
backslash path in PowerShell). Linux/macOS virtual environments use `bin/python`.

`--platform-info` reports `win32`/`nt` for native Windows Python, including in
Git Bash, `darwin`/`posix` on macOS, and `linux`/`posix` on Linux. Windows locks
byte zero of a stable sidecar with `msvcrt`, serializing both readers and writers;
contention times out after 30 seconds with a retryable `lease_conflict`.
POSIX retains shared/exclusive `fcntl.flock` locks. Locks release when handles
close; do not delete a sidecar to break a live lock.

Both platforms flush and sync a sibling temporary file, close it, then call
`os.replace`. POSIX also preserves permission bits and syncs the parent directory.
Windows does not emulate POSIX permissions or directory syncing. Atomic
replacement does not establish identical power-loss durability or network-share
locking guarantees on every filesystem. Use a local checkout and one active
machine at a time; coordinate handoffs through Git, not simultaneous state writes
on separate copies.

On 2026-10-25 the native Windows and macOS regression suites passed, including
concurrent writers, revision conflicts, idempotency, lock release, write-failure
cleanup and lifecycle checks. Windows timeout and POSIX permissions/directory
sync have host-specific tests. Linux uses the POSIX branch but still needs a
native rerun; do not infer a Linux pass from macOS results.

## Start or resume work

1. Read the required files in `AGENTS.md` and inspect Git state.
2. Inspect `project.yaml`.
3. If the snapshot reports stale passes or working tasks without leases,
   recover or resume them before selecting new work.
4. Select the highest-priority eligible task in the active phase that can be
   executed and validated on the current host. Record a host limitation rather
   than claiming native evidence from another OS.
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
- Remove an edge through `remove_dependency`, and change a task's title,
  description, validation expectations, priority, role or phase through
  `update_task`. Both are repository operations in `tools/project_state.py`
  (D-027). They use the backend's lock, revision and idempotency rules.
  `update_task` refuses a task with an active pass, and a completed task keeps
  its phase.
- Keep each track a single chain: no edge an existing path already implies
  between open tasks (D-027).
- Resolve blockers with evidence through `resolve_blocker`.
- Complete a phase only when all of its tasks are complete, or when the owner
  explicitly authorizes an override recorded in the transition.
- Keep `TODO.md` synchronized when task titles, scope, gates, or acceptance
  criteria change. Routine status and lease changes belong only in
  `project.yaml`.

## Git workflow

- Branch names: `<type>/<task-key-lowercase>-<slug>`.
- From core implementation onward, use one small delivery phase per feature
  branch and PR, following [Delivery-Workflow.md](docs/Delivery-Workflow.md).
  Use the platform-focused catalog in `docs/Implementation-Plan.md`, not one
  branch for an entire aggregate roadmap milestone.
- Use focused Conventional Commits. When the PR is ready, post a separate
  conversation comment exactly `/grok review`; verify the request, assess actual
  feedback, fix valid findings and request review again after source changes.
  Keep review/merge waits as partial passes with resumable handoffs.
- Expand future delivery-task graphs at their prerequisite gate through the
  helper; do not rewrite today's feasibility queue for a future policy.
- Never stage unrelated user changes.
- Run task checks before asking for review; record exact commands.
- The owner has authorized scoped implementation phase commits, branch pushes,
  PR creation/updates and the Grok review comments under Delivery-Workflow.md.
  Merge, tag, sign, publish and unrelated external actions still require owner
  authorization. Once a PR is approved, record the closeout on its branch,
  push, merge, delete the branch and return to `main`, as described in
  Delivery-Workflow.md "Closing out an approved PR"; never push closeout
  directly to `main`.
