# CORE-005 search and reading-metadata acceptance

CORE-005 closes the shared search/read-metadata aggregate under D-027.
The implementation is already merged: CORE-005A/A2 (PRs #8/#9), B/B2
(PRs #10/#11), C (PR #25, content-memory guard) and D (PR #26, invisible-name
warnings). This acceptance adds a reference-syntax regression and corrects an
overbroad cross-model parity statement; runtime behavior, default limits,
dependencies, ABI, schemas and golden bytes are unchanged. Review disposition
and completion live in the PR and CORE-005 pass record in `project.yaml`.

The [find/read contract](../specs/search/README.md) and
[workspace contract](../specs/workspace-search/README.md) define the delivered
surface. These APIs consume validated snapshots or adapter-supplied entries
and bytes; they do not enumerate directories, read files or grant access.

## Criteria and evidence

| CORE-005 criterion | Delivered checks |
| --- | --- |
| Source-accurate find | Literal, cross-inline, whitespace/CRLF, Unicode, whole-word and derived-text tests verify UTF-8 boundaries, containment, reading order and per-segment exactness. Derived entities/markup can map atomically to a coarse source span; native highlighting uses segments and navigation uses match indices. |
| Golden semantics | `corpus_reading_goldens` fixes word and per-section counts for 14 corpus/spec sources. Other tests cover all five v1 model goldens, source binding, snapshot parity, model-version boundaries and anchor navigation. |
| Word count and section timing | Tests cover inclusion/exclusion rules, simplified Unicode segmentation, own/cumulative sections, rate bounds and exact integer rounding. Default speed is 160 words/minute; headings and code are excluded by default. |
| Name search | Workspace tests cover path validation, Markdown/hidden filters, Unicode folding, deterministic rank/order/highlight ranges, limits and cancellation, including 100,000 entries. Invisible names are flagged without rewriting or hiding valid entries (CORE-005D). |
| Content search and snippets | Hits are compared with document find over the corpus. Tests cover visible-text snippets, markup/entities, overlapping/clipped matches, control replacement, exact source/anchor navigation and 4,096 generated cases. Display snippets are not authoritative query/source reconstructions. |
| Budgets and accounting | Admission uses actual byte lengths; tests cover invalid UTF-8 and parse failures consuming file/byte budgets, per-file truncation, permanent file/byte/match stops and cancellation leaving progress unchanged. |
| Memory/performance guards | `search_memory` enforces less than 1 MiB extra live heap for find/read calls over a few-node 8 MB paragraph. `content_budgets` enforces the 420 MiB hostile-call ceiling at the 2 MiB default, exercises legitimate dense inputs and documents the narrow-table false-positive class. |
| Rust/C ABI parity | Facade tests compare find/read stats and workspace classification, names, hits, snippets, progress, defaults and skip/stop outcomes against Rust. Tests cover caller arrays, invalid arguments and panic containment. Native UI responsiveness remains a platform check. |

The core evidence is in
[search.rs](../core/crates/markdown-core/tests/search.rs),
[workspace.rs](../core/crates/markdown-core/tests/workspace.rs),
[search_memory.rs](../core/crates/markdown-core/tests/search_memory.rs), and
[content_budgets.rs](../core/crates/markdown-core/tests/content_budgets.rs).
Facade evidence is in
[tests.rs](../core/crates/markdown-ffi/src/tests.rs) and
[workspace tests](../core/crates/markdown-ffi/src/tests/workspace.rs).

## Cross-model reference boundary

The earlier workspace contract claimed every match and source range was the
same under v1 and v2. The existing `v1_and_v2_results_are_identical` test
explicitly excludes reference syntax. The new
`reference_fallbacks_keep_find_and_word_counts_model_specific` regression uses
`[label][ref]` with a definition: v1 find matches the literal `[label]` fallback
and reading stats count zero words; v2 find matches `label`, not `[label]`,
and counts one word. Both locate `label` at its original source bytes.

Workspace content search deliberately keeps its v1 text index, per the accepted
CORE-005C disposition. It is independent of the host's chosen session model,
not universally equivalent to v2 document find. A hit remains a v1-index result
when opened in v2; source ranges and best-effort anchors provide navigation,
and a v2 find rerun need not reproduce it. No parser/index migration or policy
change is made here. `unit` is a v1 node index and must not index a v2 view.

## Validation

Source baseline: `75487e9fc8b12045c5479776e0b40fc3e88c0b64` (`develop`, PR #36),
plus the reference regression and contract clarification in this task.
Host: Apple M5 Max, macOS 27.0.1, Rust 1.99.1, Python 3.14.7, Node 24.14.0.
Use the isolated toolchain environment in [Development.md](Development.md):

```sh
RUSTUP_TOOLCHAIN=stable cargo test --locked -p markdown-core --features model-v2 --test search reference_fallbacks_keep_find_and_word_counts_model_specific
RUSTUP_TOOLCHAIN=stable CHECK_BASE=origin/develop .build/feas004-venv/bin/python tools/check.py all
```

The focused reference regression and final full local suite passed on
2026-10-30 (America/Denver), including all debug configurations, all-feature
release, C ABI smoke, semantic goldens, heap/time guards and dependency checks.
Common checks passed with the documentation. The pass and PR record hosted
PR CI. Unlike earlier prose-only aggregate acceptances,
this test change selects the three hosted core jobs under normal CI rules.
Full local checks include release-only guards that task PR CI does not run.

## Limits and remaining gates

Matching uses simple case folding, no NFC/NFD equivalence, approximate cluster
boundaries and no locale tailoring. Word counts intentionally differ from ICU
dictionary segmentation for CJK/Southeast Asian text; reading-time rounding
uses exact integer halves rather than floating-point rounding. These are
documented semantics, not complete Unicode segmentation or web parity.

Atomic source spans can cover multiple distinct matches. Sanitized controls
and clipped snippets need not normalize to the query; navigate with source
and anchors, and highlight with find segments. `has_invisible` is a display
warning, not a permission or filename-validity proof.

Memory guards bound a measured call, not an application's RSS or concurrent
search workload. Large/complex files can be skipped; narrow tables around
the default node allowance are a known false-positive class. Adapters must
report skips and incomplete/truncated results. Callers choosing up to 8 MiB
per file accept the reader's higher parse cost, and must cap concurrency.

Rust cancellation is cooperative; the handle-free workspace C ABI permits
stopping between bounded file calls, not interrupting a running call. Native
001F/002C owns UI scheduling, find highlighting/navigation, filesystem access,
workspace enumeration and throughput acceptance. There is no persistent index
or raw-source prefilter. CORE-006 still records the earlier 8-to-2 MiB FFI
default change; that documentation follow-up is not waived by this acceptance.
CORE-006 is next before any native chain starts.
