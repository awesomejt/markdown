# Reviews

Point-in-time reviews of phases, aggregates and cross-cutting work, written by
the owner or by agents. Each review names its baseline commit, scope, evidence
and findings. Reviews are dated records, not live status: `project.yaml`,
`TODO.md` and `agents/Decisions.md` remain the sources of truth. Accepted
outcomes move into those files.

Name files `YYYY-MM-DD-<scope>-review.md`. Add one line per review below,
newest first.

| Date | Review | Scope | Baseline |
| --- | --- | --- | --- |
| 2026-10-29 | [Codex CORE-FIX milestone review](2026-10-29-codex-core-fix-milestone-review.md) | Independent milestone review: earlier Codex findings reconciled, ready for owner merge | `f540318` |
| 2026-10-29 | [Grok CORE-FIX milestone review](2026-10-29-grok-core-fix-milestone-review.md) | Independent milestone review of review/core-fix: ready for main | `ffc0ba4` |
| 2026-10-29 | [CORE-FIX milestone review](2026-10-29-core-fix-milestone-review.md) | First milestone (D-030): CORE-FIX, TOOL-002, TOOL-003, PRs #17 to #27 | `ff625e7` |
| 2026-10-28 | [Grok shared CORE review](2026-10-28-grok-shared-core-review.md) | Shared CORE delivery phases CORE-001A to CORE-006A | `146ea51` |
| 2026-10-28 | [Codex shared CORE review](2026-10-28-codex-shared-core-review.md) | Shared CORE delivery phases CORE-001A to CORE-006A | `c330090` |
| 2026-10-28 | [Holistic CORE review](2026-10-28-core-holistic-review.md) | Shared CORE delivery phases CORE-001A to CORE-006A | `39d1a56` |

The owner's reconciliation of the three 2026-10-28 reviews is D-026 and D-027
in [Decisions.md](../../agents/Decisions.md). The three 2026-10-29 CORE-FIX
milestone reviews were merged into `main` with PR #28 (`m-core-fix`); their
agreed follow-ups are in
[TODO.md](../../TODO.md#core-fix-milestone-follow-ups) and in state, starting
with CORE-003F.

The earlier feasibility review (REV-001) is
[Feasibility-Review.md](../Feasibility-Review.md).
