# CORE-FIX milestone review

- **Date:** 2026-10-29
- **Reviewer:** Claude (Opus 5.5), for the owner's first milestone review
  (D-030)
- **Baseline:** `develop` at `ff625e7`, after the PR #27 merge, frozen as
  `review/core-fix` (project state: no active passes, CORE-001 next)
- **Scope:** everything merged since the shared-core reviews (PR #16,
  `1aa2b57`): the CORE-FIX phase (DOC-001, CORE-002B, CORE-003C, CORE-003D,
  CORE-005C, CORE-005D, CORE-003E), the tooling tasks TOOL-002 and TOOL-003,
  and the workflow changes D-028 to D-032. `main` already holds PRs #17 to
  #23; this review brings PRs #24 to #27 onto it. Platform work, the
  CORE-001 to CORE-006 aggregates and ADV work are out of scope.

## Verdict

CORE-FIX is done and can go to `main`. Each finding the 2026-10-28 reviews
assigned to this phase is fixed or decided, with tests, specs and a decision
record. No correctness, memory-safety or file-access defect was found in the
new code. The boundaries in `AGENTS.md` still hold: the core does no I/O,
stays `#![forbid(unsafe_code)]`, and the C ABI grew by one constant only.

Two findings below want an owner decision, neither blocking the merge: M1
(joiners in front matter) and M4 (requiring the status check on `main`).

## What was merged

| PR | Task | Outcome | Grok reviews |
| --- | --- | --- | --- |
| [#17](https://github.com/awesomejt/markdown/pull/17) | DOC-001 | Agent-facing docs cleaned up; its review added CORE-003E | 1 |
| [#18](https://github.com/awesomejt/markdown/pull/18) | CORE-002B | v1 and v2 imports re-run the producer's link, resource and metadata policy; FFI host guidance | 1 |
| [#19](https://github.com/awesomejt/markdown/pull/19), [#20](https://github.com/awesomejt/markdown/pull/20) | TOOL-002 | Rust cache, `core` and `core-extended` tiers, incremental PR host skip (D-028) | 1 + 1 |
| [#21](https://github.com/awesomejt/markdown/pull/21) | CORE-003C | Every corpus check runs on v1 and v2, with per-intent pairing rules | 1 |
| [#22](https://github.com/awesomejt/markdown/pull/22) | Workflow | `grok` CLI reviews and review-gated merges (D-029) | 4 |
| [#23](https://github.com/awesomejt/markdown/pull/23) | TOOL-003 | Whitespace check on a branch's first push; two low findings deferred | 5 (before the cap) |
| [#24](https://github.com/awesomejt/markdown/pull/24) | CORE-003D | Bare GFM autolinks in the shared parser, v1 and v2 (D-031) | 3 |
| [#25](https://github.com/awesomejt/markdown/pull/25) | CORE-005C | Content-search memory guard: 2 MiB default file budget, node allowance follows it | 1 |
| [#26](https://github.com/awesomejt/markdown/pull/26) | CORE-005D | File-name warnings cover blank default-ignorable characters | 1 |
| [#27](https://github.com/awesomejt/markdown/pull/27) | CORE-003E | Front matter rejects the same blank characters (D-032) | 1 |

All task PRs from #24 on merged into `develop` under D-029, each after a Grok
review with no high or critical finding. PRs #17 to #23 predate D-030 and
merged into `main` with owner approval. D-030 itself and the review cap were
the owner's two one-time direct commits to `main`.

**Decisions taken:** D-028 (CI tiers), D-029 (Grok severity gates the
merge), D-030 (milestone reviews on `main`), D-031 (bare autolinks in v1 and
v2) and D-032 (front matter rejects blank default-ignorable characters).
D-031 and D-032 were accepted through clean reviews; neither changed a
frozen golden. CORE-002B changed one hand-written v1 golden intent to denied,
with owner approval.

## Evidence

- **Local:** `tools/check.py all` on Apple M5 Max, macOS 27.0.1, Rust 1.99.1
  (project toolchain), Python from `.build/feas004-venv`, on `ff625e7`. Exit
  0 in 4 min 11 s: every tier (`common`, `core`, `core-extended`), 938 Rust
  tests in 72 suites with none failed, the C smoke, conformance drift,
  dependency/license policy and Markdown lint.
- **Hosted:** the `Checks` workflow passed on every `develop` push from
  PR #24 to PR #27 (`core` and `core-extended` on Linux, Windows and macOS).
  The push of `review/core-fix` and the milestone PR into `main` both run the
  full matrix with `core-extended`.
- **Source read:** the full CORE-FIX Rust diff (22 files, about 2,000 lines):
  `autolink.rs`, the `parse.rs` builder and provenance changes, `metadata.rs`,
  `validate.rs`, `v2.rs`, `workspace/content.rs`, `workspace/path.rs`, the FFI
  workspace defaults and header, and the new tests. Specs read: the
  workspace-search memory guard, the conformance contract, the model v1 and
  v2 READMEs, [Front-Matter.md](../Front-Matter.md) and
  [URI-Policy.md](../URI-Policy.md).
- **Not run:** native hosts (none exist yet), Windows and Linux local runs,
  and fuzzing. Hosted CI covers the three OSes for the Rust tiers only.

## What is working well

- **One autolink detector, proved twice.** The builder and the v2
  provenance pass both call `autolink::find` over the same exact-source text
  runs, so a forged span, destination or occurrence fails import (covered in
  `tests/autolinks.rs`). The detector resumes past every failed scan, so it
  stays linear; the hostile-run test pins that at 200,000 repetitions.
- **Held text keeps the old shape.** Plain text still produces one node per
  event. The `$`-split refunds are credited while text is held, so the event
  budget sees them at once, and replaced by the real merges on flush. Code
  blocks and image alt text are captures, so they never reach the detector.
- **Import parity is real now.** v1 import no longer has its own looser
  `relative` and `external` checks; it re-runs `classify_link` and `local`
  from the parser, so an imported document can narrow an intent to denied but
  never claim one the parser withheld.
- **One character table, three policies.** `invisible_character` moved from
  `v2.rs` to `metadata.rs` with its Unicode provenance. URI targets deny all of
  it, front matter rejects it minus the display modifiers, and file names flag
  it minus the display modifiers, ZWNJ and ZWJ. The differences are stated in
  code comments, D-032 and the docs.
- **The memory guard is measured.** Content search bounds one call to about
  334 MiB measured against a 420 MiB budget. `content_budgets.rs` measures
  peak memory for hostile and legitimate dense files at the default budget,
  and checks that explicit budgets keep the reader limits and a node floor.
  The FFI default and header moved with the Rust default.
- **The review gate worked.** PR #24 needed three Grok reviews, which found
  real autolink and budget edges: trailing dots hiding underscores, a failed
  domain or email scan hiding a later scheme, dots a path keeps, and a
  deferred `$`-split refund that could hit the event limit early. Each has a
  regression test.

## Findings

Severity is for this milestone. None blocks the merge into `main`.

### M1. Joiners discard Persian, Indic and emoji titles (medium, owner decision)

`metadata::hidden_character` includes ZWNJ (U+200C) and ZWJ (U+200D), because
both are Cf. So a front-matter title or value containing either discards the
whole map with `front_matter_unsupported`. ZWNJ is routine in Persian
spelling (`می‌خواهم`), and both appear in Indic scripts and in emoji sequences
such as family or profession emoji. File names already exempt both for that
reason (CORE-005D).

This predates CORE-FIX and [Front-Matter.md](../Front-Matter.md) documents it
("excludes legitimate format-dependent text such as joiner sequences"), so
it is not a regression. But the reader is meant for Unicode notes, and a
Persian user would lose every title. Options:

1. Keep it, as documented.
2. Allow ZWNJ and ZWJ in front-matter values (not keys), as file names do,
   and keep rejecting bidi controls and the rest. Titles stay readable;
   a joiner is invisible but does not reorder or hide text by itself.
3. Allow them only between two letters or marks, which is closer to
   legitimate use but adds a context rule.

Recommendation: option 2 as a small shared task early in the CORE-001 to
CORE-006 aggregates, with fixtures for Persian, Devanagari and an emoji ZWJ
sequence. It changes no golden. Tag sequences (U+E0020 to U+E007F, used by
subdivision flags) stay rejected either way; they are rarer and can be
reconsidered with it.

### M2. Autolink boundary uses ASCII letters only (low)

`autolink::url` refuses a scheme preceded by an ASCII letter (`xhttp://`), but
accepts one after a non-ASCII letter (`éhttp://example.com` becomes a link
from `http`). cmark-gfm also rewinds only over ASCII letters, so this matches
GitHub; noting it so a later parity pass does not "fix" it the other way.
No change recommended.

### M3. TOOL-003 deferred findings still open (low)

The two PR #23 findings in TODO.md remain: colored diff output and strict
UTF-8 decoding in `tools/check_git.py` can cause false failures under
non-default Git settings. Neither affects CI or default configurations. Fold
them into CORE-001, which owns the common checks.

### M4. `Checks / complete` is not required on `main` (owner decision)

The owner deferred this to the first milestone review. The D-030 flow worked:
every `develop` push was green, and the only path into `main` is this review
PR. Requiring the status check would stop an owner merge on red CI, at the
cost of a stuck check blocking a milestone. Recommendation: require it now.
The owner can still bypass it as an admin if a runner is stuck, and a red
check on a milestone PR should stop the merge anyway.

## Manual checks for the owner

No native host app exists yet, so there is no launch, reading or
accessibility check to run at this milestone. What remains is evidence the
hosted runners cannot give:

- **Windows host:** `python tools/check.py all` in a Git Bash shell, to catch
  path or line-ending issues outside the hosted image. Optional.
- **Linux host:** `python3 tools/check.py all`. Optional.
- **This Mac:** done above.
- **Performance:** none due. The content-search guard was measured on macOS
  in CORE-005C; Windows and Linux retained-RSS measurements are each 001B's
  job (D-025).

## Known risks carried forward

Unchanged by this milestone; listed in [MEMORY.md](../../MEMORY.md) and
[HANDOFF.md](../../HANDOFF.md):

- Mac repeated-session RSS retention (about 583 MiB after twenty 5 MiB parses,
  no leak found).
- Dense input at the 8 MiB ceiling peaks near 1,152 MiB before the node limit
  stops it; sessions inherit it. Content search is now bounded, sessions are
  not.
- Windows Narrator double reading and UIA search timeouts at 5 MiB; Linux Orca
  heading navigation and the LNX-001D1 gate.
- D-028 moved release time guards and the model-v2-only build off task PRs,
  so a one-OS regression in those tiers shows on the next `develop` push. None
  has happened yet.

## Owner decisions requested

1. Approve and merge `review/core-fix` into `main` (merge commit), to be
   tagged `m-core-fix`.
2. M1: keep the joiner rejection in front matter, or schedule option 2.
3. M4: require `Checks / complete` on `main`, or keep deferring.
