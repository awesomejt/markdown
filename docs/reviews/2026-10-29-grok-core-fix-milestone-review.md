# CORE-FIX milestone review (Grok)

- **Date:** 2026-10-29
- **Reviewer:** Grok (independent second review, D-030)
- **Branch:** `review/core-fix` at `ffc0ba4` (code baseline `ff625e7`, the
  merge before Claude's review commit), 22 commits ahead of `main`
- **Scope:** PRs #17–#27 and the D-030 / review-cap commits. Diff against
  `main`: 41 files, +6,984/−1,597. `main` already holds #17–#23, so the new
  code since `main` is #24 (CORE-003D), #25 (CORE-005C), #26 (CORE-005D) and
  #27 (CORE-003E).
- **Companion:** [Claude's review](2026-10-29-core-fix-milestone-review.md)

## Verdict

**Ready for `main`. Fix one carried finding first or right after.** The
milestone delivers what it planned. Checks are green on Linux, including the
C ABI smoke under ASan, UBSan and LSan. One earlier Grok finding is still
only half fixed: front matter still rejects ZWNJ and ZWJ (M1 below). I rate
it higher than Claude does because it rejects ordinary Persian, Hindi and
emoji text. It is not a regression, though, so it does not block the merge.

## What the milestone delivered

| PR | Task | Result |
|---|---|---|
| #17 | DOC-001 | Agent docs cleanup; both Grok nits fixed (HANDOFF state is current; AGENTS.md says per-pass detail goes in the `project.yaml` pass record and the PR) |
| #18 | CORE-002B | Import-policy parity: v1/v2 imports re-check intents and metadata text; FFI host guidance |
| #19, #20 | TOOL-002 | Tooling |
| #21 | CORE-003C | Shared-core follow-up |
| #22 | D-029 workflow | Grok CLI review gate, at most 3 reviews per PR |
| #23 | TOOL-003 | `tools/check_git.py`; two deferred findings still open (Claude M3) |
| #24 | CORE-003D | Bare GFM autolinks for v1 and v2 (D-031) |
| #25 | CORE-005C | Content-search memory guard: 2 MiB file budget and a node limit scaled by bytes |
| #26 | CORE-005D | Workspace file-name hidden-character flags (the shared tables) |
| #27 | CORE-003E | Front matter rejects blank default-ignorables (D-032) |

## Evidence (Linux x86_64, Rust 1.99.1, `LD_LIBRARY_PATH` unset, at `ffc0ba4`)

| Check | Result |
|---|---|
| `cargo test --locked` (run before build) | 214 passed, 0 failed, 18 suites |
| `cargo test --locked --all-features` | 242 passed, 0 failed |
| `cargo clippy --all-targets -- -D warnings` (default and all-features) | clean |
| `cargo fmt --check` | clean |
| `cargo build --locked` | ok |
| `header_constants_match_rust`, `c_smoke` (in both test runs) | pass |
| C smoke, `-std=c11 -Wall -Wextra -Werror -pedantic`, ASan+UBSan, `-fno-sanitize-recover=all`, `detect_leaks=1` | `layout` ok; `run 2` ok on the default lib ("models 0x2"); `run 6` ok on a `model-v2` lib ("models 0x6"); no sanitizer reports |
| Exported `md_*` symbols / header prototypes, `main` vs head | 24 / 24 / 24, unchanged |

Claude ran `tools/check.py all` on macOS (938 tests, 72 suites). The two
counts measure different things and do not conflict. I did not rerun the
Python suites.

A direct probe with `examples/parse_markdown` confirmed M1: a Persian title
with ZWNJ (`می‌خواهم`), `a‍b` with ZWJ, and the England flag emoji
(tag characters) each give `title: None` and `front_matter_unsupported`.
`plain` parses cleanly.

## Earlier Grok findings

| Finding | Status |
|---|---|
| PR #11, #12 items | Resolved |
| PR #13 Concern (content search inherits the 1,152 MiB storm) | Resolved **for content search**: 2 MiB default and scaled node limit; worst measured peak 334.4 MiB against a 420 MiB budget (`tests/content_budgets.rs`, specs/workspace-search). Sessions still allow 8 MiB and about 1,152 MiB (see Risks). |
| PR #13 Nit | Resolved (15daf3e) |
| PR #17: front-matter hidden-character table should share the v2/ignorable table **and keep ZWJ/ZWNJ allowed** | **Partly resolved.** `metadata::hidden_character` now shares `invisible_character` (Default_Ignorable plus U+2800) with URI targets and file names, and keeps the display modifiers. But `unsafe_character` (General_Category=Cf) still covers U+200B..=U+200F and U+E0020..=U+E007F, so ZWNJ, ZWJ, LRM/RLM and emoji tag sequences are still rejected. Only file names (`workspace/path.rs`) exempt ZWNJ/ZWJ. |
| PR #17 Nits (stale HANDOFF line; where run logs go) | Resolved |

## Agreement with Claude

- **M1 (ZWNJ/ZWJ): agree on the fix, disagree on the priority.** Option 2
  (allow them in values) is right. But this was the second half of an
  existing Grok finding, not a new owner question. It drops the whole map
  for ordinary text in Persian, Arabic, Urdu and the Indic scripts, and for
  ZWJ emoji such as 👨‍👩‍👧. It should be the first post-milestone task (P1),
  not an open-ended decision. I would also extend it beyond Claude's scope:
  allow emoji tag sequences (U+E0020..=U+E007F after a base such as
  U+1F3F4) and consider LRM/RLM (U+200E/F), which RTL authors use. The
  embedding and override controls (U+202A..=U+202E, U+2066..=U+2069) should
  stay rejected.
- **M2 (autolink ASCII-letter boundary matches cmark-gfm): agree.** No change.
- **M3 (TOOL-003 deferred findings open): agree.** Low.
- **M4 (require `Checks / complete` on `main`): agree.**
- **Ready for `main`: agree.**
- **"The C ABI grew by one constant only": agree, with one addition.**
  `MD_CONTENT_DEFAULT_MAX_FILE_BYTES` (2,097,152) is the only new symbol.
  But zero in `max_file_bytes` now means 2 MiB instead of 8 MiB. That
  changes an existing default under ABI v1, where D-022 says the ABI grows
  only by additions. The risk is low because no host ships yet. It should be
  recorded in a decision or in the ABI changelog (P3).
- **Risks (macOS RSS ~583 MiB, the 1,152 MiB session storm, Windows/Linux
  accessibility): agree.** The storm figure matches `docs/Parser-Budgets.md`
  (`*a` at 8 MiB, `NodeLimit`, 1,152.0 MiB).

## Additional findings

1. **The review index is not updated (Nit).** `docs/reviews/README.md` asks
   that each review be added to the index. This review was committed as a
   single file by instruction, so the index needs a one-line entry in a
   follow-up.
2. **Sessions keep the 8 MiB / ~1,152 MiB worst case (Risk, known).** Only
   content search is bounded. A host that opens an untrusted 8 MiB file
   through `md_session_*` can still reach about 1.15 GiB. The guidance is
   documented. Before the first mobile host, either lower the session default
   or require hosts to set `max_source_bytes`.
3. **Content-search budgets are per call.** The 420 MiB budget bounds one
   search. Hosts that run searches in parallel multiply it. Worth a sentence
   in the host guidance.

## Risks

- Session dense-delimiter storm: ~1,152 MiB at the 8 MiB ceiling; 407.7 MiB
  at 2 MiB.
- macOS RSS ~583 MiB (Claude's measurement; not reproduced on Linux).
- Accessibility on Windows and Linux hosts is untested (no hosts yet).
- Front-matter false rejections for joiner scripts (M1) until it is fixed.

## Manual checks per host

None possible yet: no platform hosts exist. Linux covered the Rust checks
and the sanitized C smoke.

## Prioritized recommendations

1. **P1:** Allow ZWNJ/ZWJ in front-matter values. Add tests with Persian,
   Devanagari and ZWJ emoji, and keep the bidi controls rejected. Decide
   whether to allow tag sequences and LRM/RLM at the same time.
2. **P2:** Require `Checks / complete` on `main` (Claude M4), then tag
   `m-core-fix`.
3. **P2:** Bound sessions, or require hosts to set limits, before the first
   mobile host.
4. **P3:** Record the 8 → 2 MiB zero-default change under ABI v1. Add this
   review to `docs/reviews/README.md`. Close the TOOL-003 deferred findings.

## Owner decisions

- Merge `review/core-fix` into `main` now, with M1 as the first task, or
  fix M1 on the branch first. Grok recommends merging now.
- How far M1 goes: joiners only, or also tag sequences and LRM/RLM.
