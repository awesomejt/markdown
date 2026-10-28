# Shared core review — Codex

- Date: 2026-10-28.
- Reviewer: Codex, with independent read-only C ABI and search/workspace reviews.
- Reviewed commit: `c330090b56288557aeabf4df14c1886503e44fab` on
  `review/shared-core`. The implementation matches `main` at `39d1a56`;
  subsequent branch commits contain Claude's review and owner dispositions.
- Scope: CORE-001A, CORE-002A, CORE-003A/B1/B2/B2F/B3/B4, CORE-004A,
  CORE-005A/A2/B/B2 and CORE-006A: parser, models, URI policy, sessions,
  anchors, search, workspace primitives, C ABI, checks and contracts.
- Deliverable: review only. No implementation, task state, accepted decision,
  existing report or priority was changed. Proposed work below awaits comparison
  with Claude and Grok and the owner's agreement.

## Assessment

The core is a credible foundation for native integration. Its ownership and
authority boundaries are substantially stronger than a simple parser wrapper:
the safe Rust crate performs no file access, typed URI targets grant no access,
v2 checks intent provenance against source, and the C ABI isolates unsafe code
behind typed registry handles and explicit buffer/lifetime contracts.

The full local check suite passed. This review found no new memory-safety,
arbitrary-file-access or C ABI ownership defect. That is a bounded review
result, not proof that untrusted input or native integrations are safe in all
cases. No production native consumer has yet established those guarantees.

The largest practical risk remains memory amplification, already documented
and partly tracked by CORE-005C. The most useful addition to Claude's review
is that the metadata import-policy gap also affects **v2**, not just v1.
Other additions are a requirements decision needed for bare GFM autolinks,
the limited scope of filename spoof warnings, and incomplete v2 conformance
assertions now that D-025 selects v2 for every native host.

No finding here requires redesigning the shared-core architecture. Keep the
small follow-up phases and native evidence gates rather than combining these
items into a general refactor.

## Evidence and limits

Reviewed the required project context, decisions through D-025, task catalog,
live state, Claude's report, semantic/session/search/workspace/FFI contracts,
production Rust modules, header, relevant tests and common/CI tooling.
The state inspection reported revision 376, no active or stale passes and no
working task without a lease. It was left unchanged for this review-only pass.

Current-host validation:

| Check | Result |
| --- | --- |
| `CHECK_BASE=origin/main ... python tools/check.py all` | Passed on macOS 27.0.1, aarch64, Rust 1.99.1 and Python 3.14.7, using the isolated toolchain in Development.md |
| Rust suites | 837 passing test executions across default, model-v2, all-feature and release configurations; this is not 837 distinct tests |
| Python regressions | 52 passing tests across the five common/dependency suites |
| Other gates in the full run | Formatting, Clippy with warnings denied, schema/golden and conformance checks, fixtures, dependency/source/license policy, local links and Markdown lint passed |
| Independent C ABI run | 35 all-feature Rust tests and two C ABI tests passed, including compiled C smoke/layout checks on this Mac |
| Independent workspace run | 23 all-feature workspace tests passed |
| Targeted public-API probes | Reproduced import-policy behavior, bare/angle autolinks, v1 credential URLs and filename warning scope described below |

The probes lived under ignored `.build/` directories and did not alter tests
or implementation. The reproducible inputs and outcomes are recorded below;
local logs are supporting session evidence, not a dependency of this report.
The full-suite command is documented in [Development.md](../Development.md).

Not performed: native Windows/Linux reruns, real Swift/C# Native AOT/gtk-rs
consumers, GUI/accessibility tests, fresh retained-RSS measurements,
coverage-guided fuzzing, sanitizer/Miri campaigns or a current dependency
advisory-database scan. Existing published memory measurements are attributed
to their report rather than presented as new measurements. A passing license
gate does not establish freedom from known dependency vulnerabilities.

## Findings

Priorities are proposed scheduling advice. **P1** means address before exposing
the affected untrusted workload broadly; **P2** means a bounded follow-up before
the relevant consumer acceptance; **P3** means documentation or preventive
quality work. “Existing” means already identified or accepted, not newly found.

### C1 — P1, existing: source limits are not practical memory limits

**Evidence.** [Parser-Budgets.md](../Parser-Budgets.md#residual-risks) records
about 1,152 MiB peak heap for an 8 MiB dense delimiter storm before `NodeLimit`
returns. The measured 5 MiB prose snapshot retains about 102 MiB, before the
FFI flat representation, native copies and presentation allocations. D-024
explicitly accepts the node/event default change and retains this risk.

Workspace search admits that same 8 MiB per-file maximum by default
([content.rs:17](../../core/crates/markdown-core/src/workspace/content.rs#L17),
[content.rs:44](../../core/crates/markdown-core/src/workspace/content.rs#L44))
and parses each admitted file
([content.rs:303](../../core/crates/markdown-core/src/workspace/content.rs#L303)).
File-count, byte-count and result limits do not bound the peak cost of one
file. Cancellation polls and `catch_unwind` are also not memory quotas.

**Impact.** An automatically searched folder can contain a file that causes a
large transient allocation. Multiple queued or concurrently running document
jobs could compound this in a future host. The core starts no workers, so the
host controls that concurrency; this is not evidence of a current scheduler
bug. Native allocation failure or termination would not be prevented by a
successful semantic limit check at a later stage.

**Disposition.** Agree with Claude F2 and existing CORE-005C. Keep it required
before platform 002C. Evaluate a smaller search default and/or a measured
admission guard with a typed skip and Rust/FFI parity. Any density heuristic
needs legitimate-document false-positive tests. Do not treat a smaller limit
as an automatically sufficient memory budget.

Keep the newly accepted native 001B flat-view/RSS measurements. Measure
single and repeated documents and cap host concurrency before claiming an
application memory bound. No need to repeat the feasibility program or block
independent 001A shell work solely for this known risk.

### C2 — P2, existing with new scope: import-policy parity must cover v2 metadata

**Evidence.** v1 intent validation uses `relative`/`external` checks that are
looser than the parser's policy
([validate.rs:75](../../core/crates/markdown-core/src/validate.rs#L75)).
A public-API probe parsed `[go](safe.md)`, changed its destination to
`../../../etc/passwd` while keeping `RequiresCapability`, and successfully
constructed and JSON-decoded the edited document with the original source.
This confirms Claude F3; it does not demonstrate a file read.

The additional scope is shared metadata validation. It checks front-matter
geometry and nonempty keys, but does not apply the producer's control/Cf
filter to imported display titles or string properties
([validate.rs:333](../../core/crates/markdown-core/src/validate.rs#L333)).
V2 calls that same structural validator
([v2.rs:154](../../core/crates/markdown-core/src/v2.rs#L154)). Its stronger
intent proof does not validate metadata display policy.

Reproduction, for both model versions:

1. Parse `---\ntitle: Safe\n---\n[go](safe.md)\n`.
2. Consume the result with `into_input()`.
3. Set `metadata.title` and its `title` string property to
   `abc\u{202e}txt` (U+202E RIGHT-TO-LEFT OVERRIDE).
4. Call the respective `Document::new`, serialize, then `from_json` against
   the original source. Both versions accept it.
5. Put that same value directly in source front matter and parse it instead:
   the parser omits the title/properties and emits `front_matter_unsupported`.

**Impact.** A future cache or Rust consumer can mistake “validated” for
“producer-policy checked,” including under v2. The C ABI currently has no JSON
import, and native D-025 sessions parse source, so this is not an existing
native activation exploit. General semantic/source fidelity remains explicitly
outside the model digest's guarantees.

**Disposition.** Extend the already accepted CORE-002B acceptance evidence to
both versions' metadata, including title, scalar and list strings. Reuse
shared policy functions and preserve the deliberate allowance for legitimate
variation selectors/CGJ in display text. Do not require a new model version
merely to enforce agreed validation policy; check compatibility and goldens.

**Correction to an inference from Claude F3:** the v1 parser itself accepts
`[a](https://user:pw@example.com/)` as `External`; v2 denies it. Simply reusing
v1 `classify_link` will therefore **not** reject credentials. The v1 contract
explicitly promises a prefix check, not full URL sanitization. Distinguish
producer/decoder parity from a stronger policy change, and retain native v2
selection and adapter activation checks.

### C3 — P2, new requirements gap: bare GFM autolinks have no agreed disposition

**Evidence.** [Requirements R1.2](../Requirements.md#r12-basic-markdown) asks
for CommonMark plus GFM autolinks. The implemented surface supports angle
autolinks, while [Parsing.md](../Parsing.md#extension-semantics-core-003b3)
explicitly disables bare GFM URL autolinks. `PARSER_OPTIONS`
([parse.rs:179](../../core/crates/markdown-core/src/parse.rs#L179)) uses
`ENABLE_GFM` for alerts; the flag's name does not establish full GFM support.

Public-API probe on both v1 and v2:

| Source | Observed intents |
| --- | --- |
| `Visit https://example.com and www.example.com or mail@example.com.` | Zero links |
| `<https://example.com>` | One link |

The parser documentation is honest about this subset. I found no explicit
owner decision or delivery task reconciling the bare-autolink omission with
R1.2; the frozen corpus contains no requirement-complete bare-autolink gate.

**Impact.** Real documentation containing pasted URLs will show plain text,
and three native hosts should not independently invent different detection
rules. Passing CORE-003B corpus checks does not prove this R1 behavior.

**Disposition.** Before native link-conformance acceptance, decide whether
bare autolinks are required for the MVP or explicitly deferred. If required,
use a bounded shared implementation with exact spans, URI policy, punctuation
and code/HTML exclusions, and v2 provenance parity. If deferred, record the
user impact and milestone. This is an owner scope decision, not permission to
expand this review into an implementation pass.

### C4 — P2, new defense-scope gap: filename warnings cover only some invisible names

**Evidence.** `WorkspacePath::has_invisible()` checks the shared control/Cf
table, with intentional ZWJ/ZWNJ exceptions
([path.rs:96](../../core/crates/markdown-core/src/workspace/path.rs#L96)).
The implementation conforms to its explicit **Cf-only contract**; it is not
a general confusable-name detector.

Independent public-API probes using `report<character>.md`:

| Character | Accepted path | `has_invisible()` |
| --- | --- | --- |
| U+115F, U+1160, U+3164, U+FFA0 Hangul fillers | Yes | False |
| U+2800 BRAILLE PATTERN BLANK | Yes | False |
| U+200B ZERO WIDTH SPACE, comparison case | Yes | True |

The v2 URI classifier already treats the first two groups as unsafe invisible
target characters. Filename labels have a different compatibility requirement,
but a UI relying only on this flag will not mark these potentially confusing
names. Rendering varies by font; the probe verifies the flag, not every font's
visual appearance.

**Impact.** Potentially misleading workspace listings or search selection.
Results still carry the adapter's opaque entry ID, so this is not a traversal
or capability-escape finding.

**Disposition.** Before workspace browsing/search acceptance, consider a
filename-specific warning policy that includes blank non-Cf characters,
without rejecting valid names or broadening metadata rejection. Preserve the
exact name and capability ID; make the display distinction accessible. Add
Rust/FFI flag parity tests. Treat this as a policy extension requiring review.

### C5 — P2/P3, new coverage gap: reconciled corpus assertions primarily exercise v1

**Evidence.** The generated conformance suite runs all check tokens on v1.
Its v2 branch only compares outline and link count
([conformance.rs:274](../../core/crates/markdown-core/tests/conformance.rs#L274)).
The dedicated v2 suite does cover policy, provenance, references, migration,
schema and hostile inputs; this is not an absence of v2 testing.

**Impact.** After D-025, the shipped model is v2, but changes to v2 target
classification, resources, diagnostics or capabilities need not fail the
full fixture reconciliation as long as outline and link count match. Shared
builder reuse reduces this risk but is not an assertion of equivalent output.

**Disposition.** Adapt the supported corpus checks to both versions, with
explicit version-specific intent expectations rather than requiring identical
wire shapes. Include resources, metadata, diagnostics, capability flags and
denied schemes. Preserve coverage for references that v1 intentionally leaves
inert. This is a small testing improvement, not a new conformance program.

### C6 — P3, existing with new detail: documentation overstates or obscures contracts

Agree with Claude F5 and the accepted DOC-001 cleanup. Old phase statuses in
MEMORY/HANDOFF and “under review” contract headers coexist with merged-state
facts; D-020 still awaits a merge that occurred. These create unnecessary
ambiguity at the next platform handoff.

An additional concrete inconsistency is the snippet contract:
[workspace-search lines 181–195](../../specs/workspace-search/README.md#snippets)
says matchable controls display as U+FFFD and long matches can be clipped, but
also promises `normalize(match) == normalize(query)` without exceptions.
The implementation and regression tests deliberately sanitize the display
([snippet.rs:106](../../core/crates/markdown-core/src/workspace/snippet.rs#L106),
[workspace.rs:1137](../../core/crates/markdown-core/tests/workspace.rs#L1137)).

**Disposition.** Clarify the invariant for sanitized and clipped snippets.
Keep source ranges and anchors authoritative for navigation; do not reconstruct
the query or source match from display text. Fold this into the proposed docs
work, leaving the tested display behavior intact unless separately changed.

### C7 — P3, existing: validation and maintenance gates need a release-level step

The current CI is useful and appropriately isolated: pinned action commits,
read-only permissions, locked Rust dependencies, formatting/Clippy, multiple
feature combinations, schema/golden checks, generated hostile cases and heap
budgets. Independent ABI review confirmed typed non-reused handles, retained
snapshot backing storage, validated record sizes and output/progress writes
after fallible work. No additional ABI defect was supported.

Remaining gaps should stay explicit:

- Windows skips the native C compiler smoke until WIN-001B. Keep the accepted
  MSVC smoke mandatory there; Rust tests on Windows do not prove the C header
  and native calling convention.
- Production Swift/C# Native AOT/gtk-rs consumers, relocated packages, symbols,
  flat-view/RSS cost and panic-unwind configuration need real 001B evidence.
  Feasibility libraries do not establish production package correctness.
- FFI find/read-statistics/workspace calls have no cancellation handle, and
  two-call sizing can repeat expensive work. This is documented D-022 behavior,
  not an ABI violation. Complete CORE-002B's off-thread/preallocation guidance;
  measure before adding cancellable functions under the additive ABI rule.
- Generated/mutation tests are not coverage-guided fuzzing. Before public
  release, consider parser/model-decoder and safe FFI-input fuzz targets,
  sanitizer runs where supported, and current dependency advisory checks.
  FFI fuzzing must supply valid backing buffers rather than invoke undefined
  behavior with arbitrary addresses. No current advisory scan was run here.
- `parse.rs` combines building, spans, URI v1 policy, reference scanning and
  provenance logic in 2,025 lines. V1/v2 duplicate some ID/policy logic, while
  internal ordinal IDs are parsed back to indexes. These are maintenance
  risks, not reproduced defects. Split by responsibility when a concrete
  change benefits; preserve source/golden behavior and avoid a broad rewrite.

## Task and decision adherence

| Accepted direction or task | Assessment |
| --- | --- |
| D-008/D-015: Rust, pulldown-cmark, bounded shared extension passes | Followed. One pinned parser option set; restricted first-position metadata; raw HTML/math/diagrams remain inert. Bare autolinks need the C3 disposition. |
| CORE-002A and D-020: versioned models, v1 compatibility, opt-in v2 | Followed structurally, including strict versions/schemas and explicit upgrade. C2 qualifies policy validation; source fingerprinting is not a semantic signature. |
| CORE-003B4 and D-024: conformance, hostile input and budgets | Implemented and locally passing. Defensive ceilings and counted budgets must not be presented as native memory/performance acceptance; C1 and C5 remain relevant. |
| CORE-004A and D-021: immutable generations, cancellation, anchors | Followed. Publication is guarded by generation/token checks; old snapshots remain owned; IDs stay snapshot-local by accepted decision. Anchor relocation is intentionally heuristic. |
| CORE-005A/B: search and workspace semantics | Implemented with source-bound results, budgets, cancellation and opaque entry IDs. Approximate Unicode boundaries and no NFC/NFD normalization are documented tradeoffs. C1/C4/C6 identify the follow-ups. |
| D-009/D-022/D-023 and CORE-006A: hand-written value-oriented C ABI | Followed in inspected code. No shared UI types; no new handle kinds for search; caller-owned additive results; frozen flat representation tested. Native package evidence remains open. |
| D-025: native hosts build and request v2 | Accepted on this branch. It is a future native package requirement, not yet a tested native default. Do not switch the compatibility Rust default as an incidental fix. |
| D-014/D-017/D-018: post-MVP engines, conditional Linux accessibility, implementation gate | Preserved. No reason from this review to reopen feasibility or bypass LNX-001D1, native consumer gates or owner merge approval. |
| CORE-002B, CORE-005C, DOC-001 | Existing owner-approved follow-ups are suitable. C2 adds a scope clarification; other proposed items are not yet scheduled or accepted. |

The open CORE aggregates and platform tasks are not missing work that should
be silently included in this review. Shared-core tests do not close native
selection, reading order, permission containment, release packaging or launch
performance criteria.

## Comparison with Claude and proposed order

| Claude finding | Codex assessment |
| --- | --- |
| F1: default model | D-025 already resolves the choice. Enforce it in native package tests; no further default-model decision is needed now. |
| F2: memory | Confirmed as a significant known risk, C1. Retain CORE-005C and native 001B measurements. |
| F3: v1 import policy | Confirmed, C2. Extend metadata checks/tests to v2; v1 producer policy itself still permits credential URLs. |
| F4: FFI cancellation/sizing | Agree; documented constraint, host guidance first, new API only after measured need. |
| F5: docs drift | Agree, with the additional snippet-contract qualification in C6. |
| F6: Windows C ABI smoke | Agree; accepted WIN-001B requirement remains necessary. |
| F7: maintenance/fuzzing | Agree with targeted preventive work; no blanket refactor is justified. |
| Additional findings | C3 bare-autolink requirements disposition, C4 filename-warning scope, C5 v2 corpus assertions. |

Proposed order for owner agreement, without changing the live queue:

1. **CORE-002B:** import-policy parity plus both versions' metadata tests and
   off-thread FFI guidance. Keep compatibility policy changes explicit.
2. **CORE-005C:** define a measured workspace search admission/memory bound,
   retaining its platform 002C gate. Native 001B still measures its own cost.
3. **DOC-001:** remove contradictory status text and qualify snippet semantics.
   This can remain a small independent docs phase.
4. **Before native document/link acceptance:** decide C3 and strengthen the v2
   corpus assertions in C5. Do not invent platform-specific autolink passes.
5. **Before workspace UI acceptance:** agree on the filename warning scope in
   C4 and test Rust/FFI/UI behavior consistently.
6. **At native integration and before release:** perform C7's named-host
   package/ABI checks and proportionate fuzz/advisory hardening.

The sequence is proposed, not a merge or implementation authorization. This
report is intended for reconciliation with Grok's next review and the owner.

## Owner disposition (2026-10-28)

The owner reconciled this review with the other two shared-core reviews; see
the [reviews index](README.md). Accepted outcomes are in
[Decisions.md](../../agents/Decisions.md) D-026 (the CORE-FIX phase) and D-027
(linear task tracks), with task scope in [TODO.md](../../TODO.md). C2 (including the credential-URL correction) is in CORE-002B, C3 is CORE-003D (implement), C4 is CORE-005D (widen), C5 is CORE-003C, C6 is in DOC-001, C1 is CORE-005C, and C7's release items are HARD-001.
