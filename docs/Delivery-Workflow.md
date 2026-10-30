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

## Branches and milestone reviews (D-030)

The owner reviews milestones, not every task PR:

| Branch | Holds | Changes by |
| --- | --- | --- |
| `main` | Owner-approved milestones only, each tagged `m-<milestone>` | Owner merge of a milestone review PR |
| `develop` | Integration of finished tasks; the default base for work | Task PRs merged under D-029 |
| `review/<milestone>` | Frozen snapshot of `develop` under owner review | Owner-requested review fixes |
| `feat/`, `fix/`, `chore/`, `docs/`, `test/` | One task each, reviewed by Grok | The pass runner |
| `patch/` | Unreviewed housekeeping (below); Grok ignores it | The pass runner |

Never commit directly to `main` or `develop`. (On 2026-10-28 the owner allowed
two one-time direct commits to `main`, to install this workflow and then the
review cap and branch settings; both allowances have ended.) `develop` is the
GitHub default branch, and `main` is protected: changes arrive only by pull
request, the `complete` job of the `Checks` workflow must pass (added
2026-10-29 after the CORE-FIX milestone review), and force pushes and
deletion are blocked. The rules apply to admins too, so a stuck check is
re-run, not bypassed; only the owner may relax the rule.

**Milestones.** Cut a review at the first of these boundaries:

1. The end of CORE-FIX (CORE-003D, CORE-005C, CORE-005D, CORE-003E).
2. The CORE-001 to CORE-006 aggregates closed: the shared core is done.
3. Per host, the 001A–001C walking skeleton, then the 001D–001I MVP.
4. HARD-001, before DESK-002.
5. A cap: about 8 task PRs or 2 weeks since the last review branch, whichever
   comes first.

**Milestone review.**

1. Confirm `develop` is green and has no task PR waiting to merge, then create
   `review/<milestone>` from it and push it.
2. Open a PR from `review/<milestone>` into `main` with a review packet:
   the tasks and PRs merged, decisions taken (including D-029 merges), the
   deferred findings in [TODO.md](../TODO.md#deferred-review-findings), CI
   and local check evidence, the manual checks the owner should run on each
   named host (launch, reading, accessibility, performance), known risks,
   and anything needing an owner decision.
3. Keep working on `develop` meanwhile; the review branch does not move with
   it. Make owner-requested fixes on the review branch with checks, and keep
   `project.yaml` changes on `develop` so the two do not conflict. A fix that
   needs a Grok review gets one, per step 7 below. Grok ignores `review/*`,
   so such a fix goes on a reviewed branch with a PR into the review branch.
4. Only the owner merges the review PR, with a merge commit. Then tag the
   merge `m-<milestone>` and push the tag, delete the review branch, and merge
   `main` back into `develop` through a PR from a `patch/` branch. That sync
   PR adds nothing new, so it needs CI but no Grok review (D-033).

Things that need the owner still stop work at once, not at the milestone:
gates, golden exceptions, new architecture decisions, a missing or unclear
Grok review, and failing CI on `develop`.

## Feature branch and pull request lifecycle

1. Inspect state and Git, then select the phase and verify prerequisites. Create
   `feat/<phase-id-lowercase>-<short-description>` from `develop`; use `fix/`,
   `chore/`, `docs/` or `test/` for those scopes, or `patch/` for the
   unreviewed housekeeping [below](#unreviewed-patch-branches). Reuse that
   branch and PR across sessions. Task PRs target `develop`, never `main`.
2. Start/resume the phase's execution task through the normal state helper.
   Implement only its acceptance criteria and keep unrelated changes out.
   Do not open the PR before the work is reviewable: creating it starts the
   Grok review (step 5), and that review counts toward the cap.
3. Run proportionate tests and the phase's native checks, including a local
   `tools/check.py all` for any core change, because hosted PR runs use only the
   faster core tier (D-028). Record exact commands,
   host/tool versions, results, limitations and evidence. An unavailable required
   runner/check keeps the PR unopened; an unrun check is not a pass.
4. Prepare focused commits and push the phase branch to this repository's
   configured remote. Create the phase PR, using the repository template,
   once the phase is reviewable. Describe the resulting behavior, acceptance
   criteria, validation, compatibility changes and residual risks.
5. Creating the PR starts the review: the Grok GitHub integration (through
   Cursor) reviews every new PR whose branch is not `review/*` or `patch/*`.
   Nothing needs to be run. To request a later review, post a separate PR
   conversation comment that is exactly `/grok review`. If no review starts
   after creation, post that comment once.
6. Monitor the PR until Grok's review arrives, for example by polling
   `gh pr view <PR> --json reviews,comments` every minute or two. Check
   reviews, review comments and conversation comments, and identify Grok's
   by its app account, not by the owner's. Record the PR URL, reviewed head
   SHA and review URL in the handoff. A trigger proves a request, not that
   Grok reviewed or approved. If nothing arrives within about 30 minutes,
   post `/grok review` once more; if that also stays silent, keep the pass
   partial with a pending-review handoff and report it.
7. Inspect Grok's actual feedback and applicable CI results. A finding is
   **serious** when Grok rates it high or critical or lists it to fix now;
   Grok's `bug` label usually lands there. Low findings, `suggestion` and
   `nit` are not serious. Fix serious findings on this branch, re-run the
   checks, push, then post `/grok review` for the new head and monitor again,
   repeating until a review has no serious finding. Fix or defer the others
   with a stated reason, and explain any rejected finding with evidence in
   the PR. Do not request another review for commits that only fix
   non-serious findings, make minor tweaks, update documentation or record
   project status.

   **Review cap.** A PR gets at most three Grok reviews, the automatic one
   included. When the third still has a serious finding, stop: post the
   disposition, report the open findings and wait for the owner, who merges,
   defers the findings or asks for more fixes. Do not request a fourth
   review or merge without that answer. If Grok reviews a push on its own,
   that review counts too.
8. Decide the merge (D-029). When the latest Grok review has no serious
   finding, it is the owner's standing approval to merge the PR into
   `develop`. The approved head is the reviewed commit plus any later
   commits of the kinds step 7 exempts from review. Record each finding left unfixed in
   [TODO.md](../TODO.md#deferred-review-findings) for a later pass, then
   follow [Closing out an approved PR](#closing-out-an-approved-pr) and report
   the outcome. Stop and ask the owner instead when the review is missing or
   its severity is unclear, when required CI fails, or when the PR needs an
   owner decision the review cannot give, such as a golden exception or a
   gate.

### Unreviewed patch branches

Grok ignores PRs from `review/*` and `patch/*` branches (the owner's Grok
setting, D-033). Use a `patch/<short-description>` branch from `develop` for
a change that needs no PR review:

- workflow, process and other documentation changes;
- administrative changes: project-state records, roadmap and handoff updates,
  and the milestone sync from `main` into `develop`;
- nit fixes that change no behavior, such as typos, comments and formatting.

Anything that changes the behavior of code, tools, CI, specs, fixtures or
goldens uses a reviewed branch, as does any change that needs an owner
decision. Task closeouts stay on their task's branch. A `patch/` PR merges
into `develop` with a merge commit once its required CI passes; that green
CI is the approval (D-033). Run `tools/check.py common` first, and say in the
PR why it needs no review. If a `patch/` change turns out to need review,
move it to a reviewed branch rather than asking Grok on the patch PR.

The owner has authorized the scoped commits, phase-branch pushes, PR creation/
updates and `/grok review` comments needed for this workflow. Do not ask again
for these routine steps, nor for a merge into `develop` that step 8 or the
patch rule approves, nor for the milestone steps above other than the owner's
merge into `main`. This does not authorize unrelated communication,
repository exposure or settings changes, other merges, releases, signing or
production deployment.

## Waiting, closeout and switching machines

Release the execution lease before a long review wait, or heartbeat it: a
pass whose lease expires during review must be recovered before its closeout. Finish
the pass as `partial`, with an exact PR/branch/review handoff; do not mark the
delivery task complete just because code is ready. The current concurrency-one
backend intentionally requires resuming that working task before new work.
Do not route around its lease/recovery rules to start another task.

Review-request receipts and subsequent review-only discussion may be recorded
in the PR without generating another source commit. This avoids repeatedly
changing the head merely to record that it was reviewed. Every new commit
still needs its checks; step 7 says which commits also need a new review. If
Grok does not respond (step 6), retain the pending-review handoff and report
it; never invent an approval or silently waive the review.

On another boot, reconcile the linked PR's actual state before selecting
work. A PR merged without its closeout commit, for example one merged before
this rule, is closed out in a small housekeeping branch and PR.

## Closing out an approved PR

Once a task PR is approved for merge, through a Grok review with no serious
finding (step 8) or by the owner, the closeout travels in that PR, so
`develop` is never edited directly and never needs a follow-up state commit:

1. **Record the approval on the PR branch.** Resume the task through the state
   helper and finish its pass `completed`. The evidence names the PR, the
   reviewed head, the review and the approval; the merge commit is not
   known yet and is not invented. If the task is the last child of an
   aggregate, close the aggregate the same way. Mark decisions the owner
   accepted with the approval, and update `TODO.md`, `MEMORY.md` and
   `HANDOFF.md` so they describe the merged state and the next step. Re-inspect
   state: no live lease and no `working` task may remain.
2. **Push the closeout.** Commit only these state and documentation files as
   one `chore(state)` commit and push it to the PR branch. Run the common
   checks first. This commit needs no new Grok review. A later commit that
   step 7 does not exempt needs a new review, and with it a new approval.
3. **Merge and delete the remote branch.** Wait for the required CI on the new
   head, then merge into `develop` with a merge commit and delete the remote
   branch. Verify that the merged head is the approved head (step 8) followed
   by exactly one closeout commit, and nothing after it.
   If CI fails or the merge conflicts, stop and report instead of forcing it.
4. **Return the local repository to `develop`.** Switch to `develop`, pull the latest
   from GitHub with a fast-forward, and delete the old local branch. Confirm a
   clean working tree before selecting new work.

An approval authorizes these steps for that PR only.

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
  cross-platform scheduling bottleneck. *Superseded by D-027:* the owner runs
  the shared track first, so the native foundation tasks now live in
  `desktop-apps`, which activates when `portable-core` completes. Keep each
  track a single chain and give it no redundant edges.
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
