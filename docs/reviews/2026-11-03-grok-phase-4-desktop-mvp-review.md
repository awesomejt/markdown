# Phase 4 desktop basic MVPs review (Grok)

- **Date:** 2026-11-03
- **Reviewer:** Grok (independent review)
- **Branch:** `review/phase-4` created from `develop` at `9e1566eda1a292643c1711bd13fd37180d514777` (merge of PR #64 LNX-001E)
- **Scope:** Phase 4 — desktop basic MVPs (WIN-002, MAC-002, LNX-002, DESK-002) as defined in [docs/Implementation-Plan.md](../Implementation-Plan.md). Review of current readiness based on Phase 3 progress (desktop walking skeletons) and the planned 002 delivery phases. No Phase 4 implementation code exists yet.
- **Companion:** None yet; this is the first Phase 4 review.

## Verdict

**Phase 4 has not started; skeletons (Phase 3) are nearly complete on all three hosts.** MAC-001 and WIN-001 aggregates are closed (PRs #48 and #62). LNX-001 is through LNX-001E (PR #64); LNX-001F is next. Milestone reviews under D-030 for the Windows and macOS skeletons are the immediate next steps on those hosts. Phase 4 (002A–002F per host + DESK-002) is correctly gated on DESK-001 after the three 001 aggregates. The plan is clear and the dependency graph is linear per host. No blockers for finishing Phase 3; Phase 4 work should not begin until the skeleton milestone reviews land and DESK-001 reconciles contracts.

## What Phase 4 is planned to deliver

From Implementation-Plan.md:

| Delivery phase | Scope and acceptance evidence | Prerequisites |
|---|---|---|
| 002A | Folder capability/access recovery and recents; denied/revoked/moved-folder checks | DESK-001 |
| 002B | Native workspace browser and navigation; large-folder and keyboard checks | own 002A, CORE-005D |
| 002C | Workspace search UI; bounded indexing, cancellation and result navigation checks | own 002B, CORE-005B/B2, CORE-005C |
| 002D | Persistent preferences/native settings; migration and relaunch checks | own 002A |
| 002E | Workspace watching/restoration and performance; change storms, access loss and session recovery | own 002C, own 002D |
| 002F | Development packaging and complete R1 conformance; install/run evidence and host budgets | own 002E |

Exit criteria (Phase 4):

- A user can install/run each app, open a file or folder, navigate/search, read representative documents offline, quit/relaunch, and recover unavailable files or permissions.
- Named baseline hardware meets accepted launch, parse, memory, layout, scroll, and search budgets.
- Capability deviations are documented and surfaced in the app.

DESK-002 joins the three 002 aggregates plus HARD-001.

## Evidence

Inspected at `9e1566e` on `develop`:

- [docs/Implementation-Plan.md](../Implementation-Plan.md) — Phase 3 and Phase 4 definitions, delivery catalog, linear tracks (D-027).
- [HANDOFF.md](../../HANDOFF.md) — Current posture: LNX-001F next; Windows and macOS skeleton milestone reviews next under D-030.
- [TODO.md](../../TODO.md) — Confirms MAC-001 aggregate closed, WIN-001 aggregate closed, LNX-001E closed; next are milestone reviews and LNX-001F.
- [docs/reviews/README.md](README.md) — Naming convention `YYYY-MM-DD-<scope>-review.md` and index requirement.
- Branch list: only `develop` and `main` existed before this branch; no prior `review/phase-4`.
- Recent PRs: #64 (LNX-001E) just merged; #62 (WIN-001 aggregate); #48 (MAC-001 aggregate).

No builds or tests were run for this review (review of plan and status only; no Phase 4 code). Platform hosts are in progress on their respective machines; this review is from the shared repository state.

## Findings

1. **Phase 4 work has not begun (Informational).** All 002 tasks are future. State and HANDOFF correctly show the active work is still the remaining 001F/001G/001H/001I and aggregates on Linux, plus D-030 milestone reviews on Windows and macOS.
2. **Skeleton milestone reviews are the correct gate before Phase 4 (Agree with plan).** D-030 requires owner review of `review/<milestone>` into `main` before the next phase. MAC and WIN skeletons have aggregates closed; their milestone PRs should be prepared next.
3. **Dependency graph remains clean (Positive).** D-027 linear tracks per host plus the DESK-001 join are correctly reflected. CORE-005B/B2/C/D are already available for the later 002C workspace search.
4. **Open budgets from Phase 3 should be tracked into Phase 4 (Concern, low).** TODO.md lists open native budgets (macOS RSS, Windows/Linux accessibility, large-document timings). These should be explicitly carried or dispositioned before 002F packaging acceptance.
5. **Review index needs the new entry (Nit).** Per docs/reviews/README.md, add a line for this review (newest first).

## Risks

- Premature start of 002 work before skeleton milestone reviews and DESK-001 could create contract drift between hosts.
- Unclosed open budgets from 001G/001H could surprise 002E/002F performance acceptance.
- Linux is still one or two delivery phases behind the other hosts; concurrency limit of one means finishing LNX-001 before starting any 002 is the right sequence.

## Prioritized recommendations

1. **P1:** Finish remaining Phase 3 work (LNX-001F through aggregate) and open the Windows and macOS skeleton milestone review branches/PRs under D-030.
2. **P2:** In DESK-001, explicitly disposition the open native budgets listed in TODO.md so they are either closed or accepted for Phase 4.
3. **P3:** Add this review to the index in docs/reviews/README.md. Update HANDOFF.md once the next milestone review branch is created.

## Owner decisions

- Confirm that Phase 4 review at this point is useful as a readiness checkpoint (or wait until after DESK-001).
- Sequence: complete all three 001 aggregates + their D-030 reviews before any 002 work.
