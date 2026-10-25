# Implementation Delivery Workflow

## Scope and units

Effective from the first core implementation work after GATE-001. The current
feasibility work and its partial cross-machine handoffs keep their existing
lifecycle. This policy does not approve a production stack or bypass a gate.

A **delivery phase** is a bounded, testable increment with one feature branch
and one pull request. It can take several agent sessions on the same branch.
The larger CORE/WIN/MAC/LNX/IOS roadmap entries are milestones, not instructions
to build an entire application in one branch. Split a delivery phase further
before coding when it cannot be reviewed as one coherent change.

[Implementation-Plan.md](Implementation-Plan.md) defines the initial phase
catalog and dependencies. Each delivery phase records its ID, host, scope,
acceptance criteria, prerequisite merged PRs, native validation and exclusions.
One implementation pass runs at a time; cloud review does not raise that limit.

## Work in batches on the current host

Choose a dependency-ready phase for the current OS, or shared work that can be
validated there. Complete a useful sequence of that platform's phases before
switching machines. There is no required Mac/Linux/Windows round-robin order.
Shared contract work creates the prerequisites for each independent platform
sequence; a Mac phase must not depend on the Windows counterpart merely to
enforce symmetry.

Preserve real dependencies: all three thin desktop skeletons still precede
DESK-001 and MVP deepening. Mobile still follows its desktop gate. If the current
host has no ready phase, record the reason and next required host; do not waive
the gate or invent target-host evidence to stay busy.

Shared implementation/specification changes belong in a shared phase unless
they are a small compatible fix necessary for the selected platform phase.
Keep such fixes explicit and run the affected checks. Introduce versioned,
compatible contracts so the next platform can catch up; a breaking change needs
an explicit migration plan and affected-host validation.

## Feature branch and pull request lifecycle

1. Inspect state and Git, then select the phase and verify prerequisites. Create
   `feat/<phase-id-lowercase>-<short-description>` from the current integration
   branch (normally `main`); use `fix/` or `chore/` for those scopes. Reuse that
   branch and PR across sessions. Avoid implementation commits on `main`.
2. Start/resume the phase's execution task through the normal state helper.
   Implement only its acceptance criteria and keep unrelated changes out.
   A draft PR is allowed when useful; do not trigger review of unfinished work.
3. Run proportionate tests and the phase's native checks. Record exact commands,
   host/tool versions, results, limitations and evidence. An unavailable required
   runner/check keeps the PR in draft; an unrun check is not a pass.
4. Prepare focused commits and push the phase branch to this repository's
   configured remote. Create or update the phase PR, using the repository
   template. Describe the resulting behavior, acceptance criteria, validation,
   compatibility changes and residual risks. Set the PR ready for review only
   once the phase is reviewable and required checks pass.
5. Post a **new PR conversation comment whose complete body is exactly**:

   ```text
   /grok review
   ```

   This must be a comment on the correct phase PR, not text only in its body,
   a commit message or an inline code comment. Prefer the connected GitHub tool
   when it supports the operation. The equivalent CLI is:

   ```sh
   gh pr comment "$PR_URL" --body '/grok review'
   ```

6. Record the PR URL, requested head SHA and trigger-comment URL in the PR's
   review notes or session handoff. Verify that posting succeeded. One request
   per ready head is enough; inspect existing comments before retrying to avoid
   duplicates. A posted trigger proves a request, not that Grok ran or approved.
7. Inspect Grok's actual feedback and applicable CI results. Assess every
   finding, fix valid issues, and explain any rejected finding with evidence in
   the PR. Re-run relevant checks. After pushing review fixes or other new
   phase commits, request `/grok review` again for the new ready head. Feedback
   on an older head does not establish review of new changes.
8. Report readiness to the owner with the PR, checks, review disposition and
   remaining risks. Merge only when explicitly authorized or performed by the
   owner. Verify that the expected reviewed head was merged before releasing
   dependent phases. A cloud review is not an automatic merge authorization.

The owner has authorized the scoped commits, phase-branch pushes, PR creation/
updates and `/grok review` comments needed for this workflow. Do not ask again
for these routine steps. This does not authorize unrelated communication,
repository exposure changes, merging, releases, signing or production deployment.
This documentation change does not itself open a PR or trigger a review.

## Waiting, closeout and switching machines

Release the execution lease before a long review or owner-merge wait. Finish
the pass as `partial`, with an exact PR/branch/review handoff; do not mark the
delivery task complete just because code is ready. The current concurrency-one
backend intentionally requires resuming that working task before new work.
Do not route around its lease/recovery rules to start another task.

Review-request receipts and subsequent review-only discussion may be recorded
in the PR without generating another source commit. This avoids repeatedly
changing the head merely to record that it was reviewed. Source changes still
require checks and a new trigger. If Grok does not respond or the integration
is unavailable, retain the pending-review handoff and report it; never invent
an approval or silently waive the review.

After merge, resume/reconcile the task through the state helper, record the
merged PR/review evidence, and finish it `completed`. Include state/documentation
closeout in the next feature branch/PR, or a small housekeeping branch/PR if
there is no next phase. Do not push closeout commits directly to `main` or
reset project history. On another boot, reconcile the linked PR's actual state
before selecting work; a merged PR can legitimately precede its state closeout.

Before switching OS, update `HANDOFF.md` with the phase/task, branch, PR URL,
review status, last validated code/head reference, completed checks, unrun
native checks, blockers and exact resume command. Commit and push the intended
phase source and handoff, inspect state, and leave no live lease abandoned.
Do not transfer environments, caches, signing identities or private runtime
requests. Re-inspect the local state after pulling on the destination host.

## Mapping delivery phases into project state

`project.yaml.phases` currently contains broad operational buckets. These are
distinct from the smaller delivery phases above. Keep historical records intact.
Represent each delivery phase as an execution task, for example `MAC-001B`;
record its host/acceptance in its description and use explicit dependency edges.
The unsuffixed roadmap tasks remain aggregate acceptance milestones.

During GATE-001 closeout, before implementation selection:

- Materialize the initial core and desktop-foundation execution tasks with
  `create_task` and their edges with `add_dependency`; re-inspect after writes.
- Put core work **and the early native foundation tasks** in the forthcoming
  `portable-core` bucket. The backend only selects tasks in its active bucket;
  putting early host tasks in the later desktop bucket would recreate the
  cross-platform scheduling bottleneck.
- Make each aggregate milestone depend on its delivery tasks. Delivery tasks
  depend on specific predecessor delivery tasks, not on their own aggregate
  milestone. Aggregate closeout records already-merged PR evidence, not another
  monolithic implementation. Validate that the graph is acyclic and preserves
  gate requirements before closing GATE-001.
- Add MVP delivery tasks in `desktop-apps` during DESK-001 gate closeout, and
  similarly expand mobile/advanced milestones at their prerequisite gates.
  Keep future scope out of the current phase's branch.

This is a planned execution graph, not a change to today's live queue. Use the
helper's supported operations; do not hand-edit phases/tasks or reset the state
file. Test graph expansion on disposable state before applying it. Required
native CI and parent-milestone acceptance must still be satisfied before closing
an operational bucket; host batching does not remove those requirements.
