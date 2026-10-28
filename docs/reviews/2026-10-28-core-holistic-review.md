# Holistic CORE review

- **Date:** 2026-10-28
- **Reviewer:** Claude (Opus 5.5), for the owner's end-of-phase review
- **Baseline:** `main` at `39d1a56`, after PR #13 and the CORE-003B closeout
  (project state revision 370, no active passes)
- **Scope:** the shared CORE delivery phases merged so far: CORE-001A,
  CORE-002A, CORE-003A, CORE-003B1–B4 (with B2F), CORE-004A, CORE-005A/A2,
  CORE-005B/B2 and CORE-006A. Platform phases, the CORE-001 to CORE-006
  aggregates and ADV work are out of scope.

## Verdict

The shared core can carry the platform phases. The boundaries in `AGENTS.md`
hold in the code, not just in the docs. The core has no I/O and no unsafe
code. Raw HTML, math and diagrams stay inert. The C ABI is narrow,
value-oriented and versioned. Tests are deep and all pass. Known limits are
documented honestly.

No correctness or memory-safety defect was found. The findings below are
about defaults, defence in depth and upkeep. One of them, F1, should be
decided before any platform 001B adapter phase starts. The 001A shell phases
depend only on CORE-001A and can start now.

## Evidence

- `tools/check.py all` on Apple M5 Max, macOS 27.0.1, Rust 1.99.1 (isolated
  toolchain), Python from `.build/feas004-venv`. Exit 0. 837 Rust tests and
  the Python suites passed: fmt, Clippy with warnings denied, default,
  `model-v2`, all-feature and release test runs, the C smoke with layout
  checks, conformance drift, dependency/license policy and Markdown lint.
- Source read: `markdown-core` (`parse.rs`, `validate.rs`, `model.rs`,
  `v2.rs`, `cancel.rs`, `session/`, and the outlines of `search/` and
  `workspace/`) and `markdown-ffi` (`lib.rs` registry and boundary helpers,
  `search.rs` record arrays, `flat.rs` view). The contracts read were
  [Parser-Budgets.md](../Parser-Budgets.md), the [C ABI spec](../../specs/ffi/README.md),
  [Architecture.md](../Architecture.md) and D-018 to D-024 in
  [Decisions.md](../../agents/Decisions.md).
- Two probes against the core in a scratch crate outside the repository.
  Their results are quoted under F1 and F3.
- Not run: native hosts, Windows or Linux timing, and coverage-guided fuzzing.
  No platform phase exists yet to run them.

## What is working well

- **Boundaries hold.** `markdown-core` is `#![forbid(unsafe_code)]` and does
  no I/O. Sessions take bytes from the adapter. Workspace search takes opaque
  entry IDs and validated relative display paths, never filesystem paths.
  Link and resource intents grant no access.
- **The FFI boundary is careful.** Handles are registry keys, so a stale or
  double release returns `MD_ERR_INVALID_HANDLE` and frees nothing. Every
  entry point runs inside `catch_unwind`. Outputs are zeroed on entry and
  written once after all fallible work. Versioned structs need an exact
  `struct_size`. No Rust reference to caller memory is ever made. Record
  arrays check null, alignment and size overflow. The 24 exports match the
  header, and the C smoke checks the layouts.
- **Validation is independent and strict.** It checks spans on character
  boundaries, containment and sibling order, parent counts, cycles, outline
  coverage, recomputed capabilities and intent use counts. In v2, every link
  and image target is proved against a second bounded parse of the source.
- **Failure is closed and leaks nothing.** Limits return typed errors. Errors
  carry structural paths, never document text. Upstream offset conflicts now
  replace only the affected top-level block. `Debug` output for snapshots,
  jobs and revision tokens prints sizes only.
- **Tests are deep.** There are the generated conformance contract, 4,000
  mutation cases per configuration, 33 pathological constructs on v1 and v2,
  counted heap budgets, session race and lifetime tests, and FFI tests for
  panics, stale handles and concurrency.
- **Residual risks are written down.** Parser-Budgets.md states the worst
  memory case, the limits on cancellation and the lack of coverage-guided
  fuzzing. Nothing is hidden.

## Findings

Severity is about the impact on the platform phases, not about code defects.

### F1 — High: the default model denies most real relative links

Model v1 is the default in Rust and the only model in the default
`markdown-ffi` build. Its URI subset (`safe_uri` and `local` in
[parse.rs:1660](../../core/crates/markdown-core/src/parse.rs#L1660)) denies
parent-relative paths, non-ASCII file names and any percent-encoding. That
covers a large share of the links in real documentation folders. Probe
results:

| Source | v1 | v2 |
| --- | --- | --- |
| `[a](../sib.md)` | Denied | RequiresCapability `../sib.md` |
| `[a](my%20file.md)` | Denied | RequiresCapability `my file.md` |
| `[a](café.md)` | Denied | RequiresCapability `café.md` |
| `[a](docs/R%C3%A9sum%C3%A9.md)` | Denied | RequiresCapability `docs/Résumé.md` |
| `![i](img/a%20b.png)` | Denied | RequiresCapability `img/a b.png` |

V2 handles all of these under the reviewed URI policy. D-020 and
Architecture.md say that "new adapters explicitly opt into v2". But no
decision says which model the native packages build and request. The ABI
spec says the default build offers v1 only.

**Recommendation (owner decision, proposed D-025):** native 001B packages
build `markdown-ffi` with `model-v2` and request `MD_MODEL_V2`. V1 stays for
compatibility, goldens and tests. The measured cost is a second bounded
parse when a document has intents: 78 ms against 43 ms on the synthetic
40,960-link input, and about the same as v1 for prose. Record the choice
before MAC-001B, WIN-001B or LNX-001B starts, so that the three hosts do not
diverge.

### F2 — Medium: memory per document is the main platform risk

Parser-Budgets.md measures a 5 MiB prose parse at a peak of about 29 times
the source and a retained snapshot at about 20 times. That is roughly
145 MiB peak and 100 MiB retained. A dense delimiter storm at the 8 MiB
ceiling peaks at about 1,152 MiB before `NodeLimit` fires. Three things add
to this:

- The FFI flat view is a second copy of the model. Its cost is not measured
  yet; the budgets doc leaves it to the B phases.
- macOS kept about 583 MiB RSS after twenty 5 MiB parses (FEAS-006). The
  allocator does not return the memory.
- Workspace content search parses every admitted file with v1
  ([content.rs:306](../../core/crates/markdown-core/src/workspace/content.rs#L306))
  and defaults `max_file_bytes` to 8 MiB. One hostile file in a scanned
  folder can therefore briefly cost more than 1 GiB.

**Recommendation:**

1. Each 001B phase measures the flat-view encode cost and the retained RSS
   for one document and for repeated documents, on its own host, against the
   D-016 targets.
2. Before any 002C phase (workspace search UI), lower the default
   `max_file_bytes` for content search (for example to 2 MiB), add a cheap
   delimiter-density pre-check, or do both. Track this as a CORE task so
   that the three hosts inherit one answer.
3. Keep a per-node cost reduction (string node IDs, a child vector per node)
   as a candidate. It would change the Rust `DocumentInput` API and the
   frozen v1 JSON, so it needs a model decision. The C ABI flat view already
   uses `u32` indices and would not change.

### F3 — Medium: v1 `from_json` accepts intents the parser never emits

The v1 validator checks link and resource policy with `relative` and
`external` in
[validate.rs:75](../../core/crates/markdown-core/src/validate.rs#L75). These
are much looser than the parser's own policy. A probe edited one parsed
document's JSON. `Document::from_json` accepted all of these:

- `RequiresCapability` for `../../../etc/passwd`, `%2e%2e/%2e%2e/x`,
  `a b.md` and a destination starting with U+202E (right-to-left override)
- `External` for `https://user:pw@evil.example/`

V2 does not have this gap, because its validator classifies every target
that is not denied again. Today nothing can reach this path: the C ABI has no
JSON import, and intents grant no I/O. But a future Rust consumer, cache or
test harness that round-trips v1 JSON gets a "validated" document whose
intents are not the producer's policy. Metadata strings from JSON also skip
the Cf/bidi guards that the front-matter parser applies.

**Recommendation:** in a small CORE follow-up, have v1 validation re-run
`classify_link` and `local` for intents that are not denied, as v2 does.
Wire format, schema and goldens stay unchanged, because goldens are
parser-produced. Some hand-built test documents may need updating. If the
owner prefers not to change v1, document `from_json` as structural
validation only and say that adapters must re-check policy.

### F4 — Low: FFI search calls cannot be cancelled and may run twice

`md_snapshot_find`, `md_snapshot_read_stats` and the workspace content
functions use the two-call `MD_ERR_BUFFER_TOO_SMALL` protocol and take no
cancel handle. When the first buffer is too small, the whole search runs
again. At 5 MiB that is about 31 ms for find and 61 ms for read stats, each
time. Parse validation and the upstream block scan also cannot be
interrupted: up to about 50 ms at 5 MiB.

**Recommendation:** add host guidance to the ABI spec: call these off the
UI thread and size the arrays from `max_matches` up front, so that one call
is enough. If 001F shows the latency in practice, add a cancellable variant
as a D-022 function addition.

### F5 — Low: required-reading docs have drifted

`AGENTS.md` makes MEMORY.md, HANDOFF.md and TODO.md required first reads, so
stale text costs every session and can mislead it:

- [MEMORY.md](../../MEMORY.md) is 619 lines against its "short durable
  context" charter. Its "Current posture" still opens with "CORE-001A,
  CORE-002A, CORE-003A and CORE-003B1 are merged" and still lists
  "Previously selected: CORE-003B2".
- [HANDOFF.md](../../HANDOFF.md) has two "Resume here" sections. The second
  is the old CORE-003B2 handoff. It also calls D-024 "proposed", although
  D-024 is accepted.
- D-020's status still says the implementation "awaits phase review and
  owner-authorized merge"; PR #5 merged it.
- TODO.md's "Immediate session handoff" repeats the PR-by-PR merge history
  that HANDOFF.md already holds.

**Recommendation:** one docs-only PR that cuts MEMORY.md's posture down to a
current summary of about 30 lines and moves the run logs and per-phase notes
to an archive file. It should also remove the stale HANDOFF.md section, fix
the D-020 and D-024 status text, and trim TODO.md's handoff to pointers.

### F6 — Low: Windows has no C ABI check yet

`tools/check.py` skips the C smoke on Windows until WIN-001B wires up MSVC.
So the header and layouts have never been compiled by MSVC, and Windows-only
layout or calling-convention issues would first appear in WIN-001B. This is
planned, but WIN-001B should make the MSVC smoke a required CI step, not an
optional one.

### F7 — Low: maintainability notes

These need no action before the platform phases:

- An unexpected upstream event gives a hard `ParseError::SourceMapping` for
  the whole document, not a local fallback (for example
  [parse.rs:1220](../../core/crates/markdown-core/src/parse.rs#L1220)). The
  hostile suites show this never happens with the exact pin
  `pulldown-cmark =0.13.5`. A future upgrade should be its own gated task
  that re-runs the conformance, mutation and pathological suites.
- `finish` and the link and image span code parse node and intent ordinals
  back out of their IDs (`id.0[1..].parse()`,
  [parse.rs:1441](../../core/crates/markdown-core/src/parse.rs#L1441)). This
  is correct, because the builder allocates them, but it is fragile. A side
  index would make the dependency explicit.
- A footnote definition label is charged to the copy budget twice: once for
  the case-fold key and once for the stored label
  ([parse.rs:998](../../core/crates/markdown-core/src/parse.rs#L998)).
  The effect is small.
- `parse.rs` has 2,025 lines. It holds the builder, URI policy, destination
  scanning, shadowed-definition detection and v2 provenance. Splitting it
  into modules would make later reviews of platform-driven parser changes
  easier.
- Fuzzing is generated and mutation-based, as documented. Before a public
  release, add coverage-guided targets for the parser and for the untrusted
  inputs to the C ABI: anchor bytes in `md_snapshot_resolve_anchor`, queries
  and workspace entries.

## Decisions for the owner

1. **Host model (F1).** Should native packages build with `model-v2` and
   request v2 by default? Recommended: yes, recorded as D-025 before any
   001B phase.
2. **Content-search memory (F2).** Should a CORE task lower the per-file
   default or add a density pre-check before 002C? Recommended: yes.
3. **v1 JSON validation (F3).** Tighten v1 validation to the producer
   policy, or document it as structural only? Recommended: tighten.
4. **Docs cleanup (F5).** Approve one docs-only PR. Recommended: yes.

### Owner disposition (2026-10-28)

The owner accepted all four recommendations. Each decision is recorded where
it belongs when this review closes or when its follow-up is created: D-025
for the host model, and project-state tasks for F2, F3 and F5. Grok and
Codex review passes on this PR come before closeout.

## Suggested order

1. A small CORE follow-up PR for F3 and the F4 host guidance, plus the F5
   docs cleanup, either as one PR or as two.
2. MAC-001A on this Mac; LNX-001A and WIN-001A on their hosts. None of them
   depends on F1 to F4.
3. Record the F1 decision before the first 001B phase. Add the F2
   measurements to each 001B's acceptance.
4. Settle the F2 content-search default before the first 002C phase.

The CORE-001 to CORE-006 aggregates stay open by design until the native
consumers pass in their platform phases. This review does not change that.
