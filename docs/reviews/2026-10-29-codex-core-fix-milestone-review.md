# CORE-FIX milestone review — Codex

- **Date:** 2026-10-29.
- **Reviewer:** Codex, independent milestone assessment under D-030.
- **Reviewed branch:** `review/core-fix` at
  `f54031886308a25ebfd46ca7f099300f8e0d5aeb`.
- **Implementation baseline:** `ff625e7561c30a2f1da9d6a035c76afe97f87331`,
  after PR #27. The subsequent two commits add Claude's and Grok's reviews
  and their index entries; they do not change implementation.
- **Scope:** CORE-FIX, TOOL-002/003 and D-028 through D-032, compared with
  the shared-core review integration at `1aa2b57`. The milestone's new
  implementation relative to `main` is PRs #24–#27; earlier task fixes are
  assessed as part of closing the original review findings.
- **Companions:** [Claude](2026-10-29-core-fix-milestone-review.md) and
  [Grok](2026-10-29-grok-core-fix-milestone-review.md).

## Verdict

**Ready for the owner's milestone merge into `main`.** No new blocking
correctness, memory-safety or capability-boundary defect was identified in
the inspected changes. The earlier Codex findings assigned to CORE-FIX have
concrete implementations, regression coverage and corresponding contracts.
This is approval advice for this milestone, not acceptance of the open CORE
aggregates, native applications or release hardening.

Keep the front-matter joiner question visible, require native memory and ABI
evidence at each 001B, and retain the existing release gates. None requires
expanding this review into implementation or changing an accepted decision.

## Evidence and limitations

Read the repository instructions, required project context, relevant decisions,
task acceptance criteria, prior shared-core Codex findings and both milestone
reviews. Inspected the production Rust diff, relevant Rust/FFI regressions,
header changes, new autolink goldens, conformance assertions, CI selection and
whitespace tooling. Particular attention went to shared producer/import guards,
autolink text buffering and provenance, parse-budget derivation, and Rust/C
default parity.

| Evidence | Result and scope |
| --- | --- |
| `python3 tools/check.py common` | Passed locally on Linux x86_64, Python 3.12.14 and Node 24.22.1 after installing the pinned development dependencies. Fixture/model/capability validation, 56 Python regression tests, conformance drift, 373 local link paths, Markdown lint and whitespace checks passed at the reviewed head. |
| `python3 tools/test_check_dependencies.py` | 11 local regression tests passed. This tests the checker; it does not substitute for running dependency inspection against Cargo metadata. |
| [Baseline hosted run 40901792982](https://github.com/awesomejt/markdown/actions/runs/40901792982) | Inspected job steps, not only summary conclusions: `common`, `core` and `core-extended` actually ran successfully on the applicable runners; both Rust tiers passed on Linux, Windows and macOS at `ff625e7`. |
| [Head run 40927345998](https://github.com/awesomejt/markdown/actions/runs/40927345998) and [40927355773](https://github.com/awesomejt/markdown/actions/runs/40927355773) | Successful at `f540318`; common checks ran, while both Rust tiers were skipped on every host. These review-only runs are not fresh Rust executions. The implementation is unchanged from the fully tested baseline. |
| Local full-suite attempt | `python3 tools/check.py all` initially stopped at missing `jsonschema`; installing the pinned requirements enabled the successful common run above. Rust/Cargo is absent, so no local Rust, C smoke or full-suite pass is claimed. `python3 tools/check_dependencies.py` also cannot run without Cargo. |
| Operational state | The supported inspect command failed because the project-state backend is absent. A read-only YAML inspection found revision 606, no working tasks or active task references, all nine reviewed execution tasks completed, and CORE-001 pending. This is not a successful backend validation. |

Claude's full macOS run and Grok's Linux Rust/sanitizer results remain evidence
reported by those reviewers, not executions reproduced here. No fresh peak-heap
measurement, retained-RSS measurement, native consumer, GUI/accessibility test,
fuzzing campaign or advisory-database scan was performed in this review.

This change adds a review and index entry only. Per D-030, operational state
stays on `develop`; no pass, task, priority, decision or implementation is changed.

## Closure of the earlier Codex findings

| Earlier finding | Assessment at this milestone |
| --- | --- |
| C1: content-search memory amplification | CORE-005C lowers the default file budget to 2 MiB and derives node/event allowances from it. Rust, C defaults and documentation agree. Counted-heap regressions cover ten hostile constructs and legitimate dense documents. Session and native memory acceptance remain open by design. |
| C2: imported intent/metadata policy | CORE-002B replaces the weaker v1 import checks with the producer's `classify_link`/`local` predicates. Shared structural validation covers title, keys, scalar strings and string lists for both models. Denied intents may remain denied. The deliberate v1 credential-URL behavior is preserved; native v2 selection remains D-025. |
| C3: bare GFM autolinks | CORE-003D and D-031 implement one shared detector for v1/v2, exact-source spans and existing URI policy. Goldens, exclusions, hostile runs, split-event cases and v2 forged-intent tests accompany it. Documented GFM differences remain explicit. |
| C4: invisible filename warnings | CORE-005D shares the invisible-character table, flags the additional blank characters, retains display modifiers and ZWNJ/ZWJ exemptions, and preserves exact names and opaque IDs. Rust/FFI tests cover the widened flag. |
| C5: v2 corpus assertion gap | CORE-003C applies check tokens through a shared version-neutral view, with explicit per-intent pairing rules. This replaces the former outline/link-count-only comparison. It strengthens the existing corpus without claiming requirement-complete or exhaustive Unicode coverage. |
| C6: stale docs and snippet invariant | DOC-001 reconciles current posture and archives history. Sanitized/clipped snippets no longer carry an unconditional normalized-text equality promise; source ranges and anchors remain authoritative. |
| C7: native/release validation | Correctly retained in platform 001B and HARD-001 rather than falsely closed by CORE-FIX. Windows MSVC C smoke, production package consumption, flat-view/RSS measurements and release hardening still need their named-host evidence. |

## Findings and carried decisions

### C-M1 — Medium, existing: front-matter joiners remain an explicit product limitation

`metadata::hidden_character` includes `unsafe_character`, whose Cf ranges include
U+200C/U+200D. `Context::own` rejects those values; the producer then drops the
entire metadata map, and import validation follows the same rule. Persian
`title: می‌خواهم` and a family emoji title therefore lose typed metadata, not
just one invisible character. The source and diagnostic remain available.

Agree with Claude and Grok that this deserves a bounded follow-up before native
metadata acceptance. Recommend allowing ZWNJ/ZWJ in **values** while preserving
the key grammar, rejecting dangerous bidi controls, and adding Persian, Indic,
ZWJ-emoji and v1/v2 import tests.

However, this is not an unimplemented CORE-003E acceptance condition: that task
explicitly asked for a decision, and accepted D-032 chose rejection. Changing
it now requires the owner's policy disposition. Grok's suggested tag-sequence
and LRM/RLM allowances are separate choices; do not infer their approval from
a joiner exception. No merge block is warranted for this documented, existing
limitation.

### C-M2 — Low, compatibility note: content-search defaults change observable behavior

The zero-initialized C options path now selects 2 MiB rather than 8 MiB for
`max_file_bytes`. A previously searchable 3 MiB file is consequently skipped
as `TooLarge` unless the host opts into a larger budget. Source/header and FFI
specs document the new default; tests check zero-default and explicit-limit
behavior. This is intentional CORE-005C behavior with no shipping host yet.

Agree with Grok that integration notes should call out the before/after behavior.
It is a behavioral compatibility change, but not by itself a demonstrated
D-022 binary-ABI violation: no existing function signature or struct layout
changed. The new header macro is not an exported runtime symbol. No additional
wire version or ABI redesign is justified by this change alone.

### C-M3 — Risk, existing: 420 MiB is a measured test budget, not an allocator ceiling

`parse_limits(max_file_bytes)` enforces source, node and event counts; it does
not install a production allocator quota. `content_budgets.rs` checks counted
live heap against 420 MiB for its selected workloads. That is useful regression
evidence, not proof that every possible input stays below exactly 420 MiB or
that process RSS does. Explicitly raising the file budget restores the reader's
larger cost; concurrent searches multiply retained/in-flight work.

The workspace and FFI specs already say the measurements are per call and tell
hosts to cap concurrency. Grok's request to add that guidance is therefore
already satisfied at this baseline. Keep 001B resident/flat-view measurements,
worker scheduling, concurrency limits and session budgets as acceptance items.
No new content-search defect was reproduced here.

### C-M4 — Low, existing: finish the deferred whitespace-tool robustness fixes

`tools/check_git.py` still captures strict text output and parses headers
without disabling diff color. The two deferred PR #23 findings remain valid
under non-default Git configuration or non-UTF-8 offending content. They are
already recorded in TODO.md; address them in the common-tooling work rather
than reopening the parser milestone.

### C-M5 — Owner decision: require the milestone completion check

Recommend requiring `Checks / complete` on `main`, as the other reviewers do.
The inspected workflow has an aggregate check that rejects failed prerequisites,
and the implementation baseline has actual successful full-matrix evidence.
This review does not inspect or modify repository protection settings, so it
does not independently assert their current configuration. Settings changes and
the milestone merge remain with the owner.

## Reconciliation and next steps

Agree with both companion reviews on readiness and the known memory/native
accessibility risks. Claude's ASCII-letter autolink boundary observation is
consistent with the detector's deliberate rule; no change is proposed. Grok's
review-index nit is already resolved in `f540318` and needs no new task.

For the owner's milestone decision:

1. Merge the milestone when satisfied with the packet and applicable CI;
   only the owner merges into `main` under D-030.
2. Disposition the joiner follow-up and the completion-check requirement.
   Neither is silently accepted or implemented by this report.
3. Follow the existing `m-core-fix` tag and sync-back workflow after approval.
   On `develop`, CORE-001 is the next recorded shared task, followed by the
   remaining aggregates before any native shell begins.

There is no application to launch at this milestone. Native reading,
selection, accessibility, permissions, packaging and process-memory checks
belong to the later platform gates; do not waive them on the strength of
portable-core CI.
