# Parser Conformance, Hostile Input and Budgets

CORE-003B4 closes the parser hardening work under CORE-003B. It reconciles the
fixture corpus with parser output, adds hostile-input and property coverage,
measures parse time and memory on a named host and sets regression ceilings.
It makes no native UI, launch, layout or scroll claim; those remain platform
phase work under [Validation.md](Validation.md).

## Corpus reconciliation

[`specs/parsing/conformance/corpus.json`](../specs/parsing/conformance/README.md)
is generated from `fixtures/expected/*.yaml` by
`tools/generate_conformance.py`. Every expectation maps either to a named check
in `core/crates/markdown-core/tests/conformance.rs` or to a deferral with a
reason, and an unclassified key or invariant fails generation. The common
suite runs `--check`, so fixture edits cannot drift from the contract silently.

All eight cases pass their supported checks: outline text and slugs, front
matter and typed properties, feature counts, fence languages and semantics,
link targets and dispositions, alerts and the alert look-alike, math counts,
fallback diagnostics, inert raw HTML, blocked schemes and the voice opt-in
diagnostic. The contract also lists the deferred expectations: voice-over
interpretation, diagram engines and validity, syntax highlighting, filesystem
lookup and display names, and the `safe-subset`/`sanitize` HTML policies
(where the core is stricter and keeps all HTML as inert text).

## Hostile input and properties

`tests/hostile.rs` adds three deterministic layers to the token generators in
`tests/parser.rs`:

- **Corpus-seeded mutation.** Each case applies one to eight structural edits
  (inserting hostile tokens, deleting, duplicating, rewriting line endings or
  prefixing containers) at character boundaries to a corpus file. Every result
  must parse to a model that round-trips through JSON revalidation, reparses
  identically, matches the cancellable entry point, supports find and reading
  statistics, and parses under v2 with the same tree size when the document
  has no reference syntax. A localized source-mapping fallback must be a root,
  and the whole-body fallback must never fire. CI runs 4,000 cases per build
  configuration. `MD_FUZZ_CASES` raises the count; a 300,000-case release run
  passed on the named host below.
- **Pathological catalog.** Thirty-three constructs, including 50,000-deep
  quotes and emphasis, bracket and link-opener storms, unclosed comments,
  entity and control-character storms, bidi/BOM/combining runs, CR-only line
  endings, 4,000-column and 25,000-row tables, footnote and reference storms,
  dollar/math storms, heading and setext storms, and repeated tab containers.
  Each must produce a validated model or a typed limit, on v1 and v2. Release
  builds fail any construct over five seconds. On the named host, the ten
  slowest were also run at 4 and 16 times their catalog size. All ten scaled
  close to linearly up to the node or source limit. The slowest was 1.6 MB of
  setext headings, at about 400 ms.
- **Profiles and ceilings.** All four generated profiles parse under default
  limits without fallbacks, v1 and v2 agree on tree size, and a pre-cancelled
  token stops before work. Source over 8 MiB is a `SourceLimit`.
  Representative prose at the ceiling parses. The delimiter storm at the
  ceiling stops with `NodeLimit`.

This is deterministic generated and mutated input, not coverage-guided
fuzzing; see the residual risks.

## Default limit change

The 5 MiB prose profile, the accepted upper-bound document, failed with
`NodeLimit` under the previous defaults. Prose produces about 92,000 nodes and
116,000 events per MiB. At the 8 MiB source ceiling it needs about 738,000
nodes and 932,000 events. The defaults are now 1,000,000 nodes, equal to the
model and C ABI node ceiling, and 2,000,000 events. Depth (128), source (8 MiB)
and copied text (32 MiB) are unchanged. Zero in `md_session_options` still
selects these defaults; the ABI does not change. See D-024.

## Source-offset fallback

pulldown-cmark 0.13.5 reports some tab-expanded container offsets outside
their parent or before a sibling (for example `1. >\t- a`, where the inner list
starts before its quote). Previously one such container discarded the whole
document's semantics. Now `Builder::add` flags the conflict in constant time,
and `localize_source_mapping` replaces only each affected top-level block with
an inert `unrepresentable_source_mapping` block over that block's exact
upstream span, with a `source_mapping_fallback` warning. The replaced block's
links, images, headings and diagnostics are dropped. A footnote reference
elsewhere whose definition was inside the block becomes an unresolved
reference fallback. Capabilities are recomputed. Offsets are never clipped,
widened or invented. The whole-body fallback remains only as a safety net
if validation still finds a containment or order error. The mutation tests
assert that it never fires.

## Named-host measurements

Host: Apple M5 Max, 48 GiB, macOS 27.0.1, Rust 1.99.1, release profile,
measured on 2026-10-28 with
`cargo run --release --locked --features model-v2 --example parse_budget 11`.
Times are wall-clock milliseconds over 11 runs. Peak heap is the counted
live-heap peak above the baseline, excluding the source string.
`links-synthetic` is a non-fixture input of about 1.1 MB, with 40,960 link
intents and 8,192 images, that exercises v2 URI and IDNA classification.

| Profile | Entry | Min ms | Median ms | Max ms | Peak heap MiB | Notes |
| --- | --- | ---: | ---: | ---: | ---: | --- |
| prose-1mib | parse_markdown | 22.3 | 24.9 | 33.5 | 34.7 | 92,235 nodes |
| prose-1mib | parse_markdown_v2 | 22.0 | 22.5 | 24.2 | 35.2 | |
| prose-1mib | to_json | 6.9 | 7.0 | 7.6 | 24.0 | 11.3 MB JSON |
| prose-1mib | find | 8.4 | 8.7 | 9.1 | 5.0 | 4,854 matches |
| prose-1mib | read_stats | 9.2 | 10.0 | 10.8 | 4.5 | 121,361 words |
| prose-5mib | parse_markdown | 139.2 | 143.3 | 152.8 | 144.9 | 461,177 nodes |
| prose-5mib | parse_markdown_v2 | 139.8 | 146.3 | 149.6 | 146.9 | |
| prose-5mib | to_json | 35.1 | 35.5 | 38.1 | 96.0 | 58.4 MB JSON |
| prose-5mib | find | 30.5 | 31.1 | 40.7 | 29.8 | 10,000 matches (cap) |
| prose-5mib | read_stats | 57.8 | 61.3 | 82.6 | 30.4 | 606,814 words |
| delimiter-storm-1mib | parse_markdown | 51.7 | 53.3 | 62.1 | 66.0 | 222,050 nodes |
| delimiter-storm-1mib | parse_markdown_v2 | 51.8 | 53.1 | 60.3 | 66.1 | |
| long-lines-1mib | parse_markdown | 5.2 | 5.2 | 5.3 | 2.5 | 18 nodes |
| long-lines-1mib | parse_markdown_v2 | 5.2 | 5.2 | 5.3 | 2.5 | |
| links-synthetic | parse_markdown | 42.3 | 43.3 | 44.3 | 80.0 | 139,264 nodes |
| links-synthetic | parse_markdown_v2 | 77.5 | 78.5 | 81.2 | 80.0 | 40,960 link intents |

A completed 5 MiB snapshot retains 102.4 MiB of its 144.9 MiB parse peak. The
rest (the upstream tree, builder maps and validation index) is released.
`parse_budget --cancel` cancels a 5 MiB parse after delays of 0–156 ms.
Latency from `cancel()` to return had a median of 5–9 ms and a maximum of
15–50 ms across three sweeps. The maximum is the uninterruptible final
validation, and the upstream block scan is the other uninterruptible phase.

Before this phase, the 1 MiB prose parse took about 45 ms, and the 5 MiB parse
failed on the node limit (about 265 ms with raised limits). Most of that time
was model validation, which used a `BTreeMap` of string IDs and a `BTreeSet`
visit set. A hash index and a visit bitmap halved parse time with no output
change. SHA-256 fingerprinting, run once in the builder and once by validation,
is now the largest single cost, at about 20% of a 5 MiB parse.

## Budgets and CI ceilings

`tests/budgets.rs` uses a counting global allocator. Counted sizes are the same
in debug and release and on every 64-bit host, so the heap budgets are enforced
everywhere. Release builds also guard wall time, using the best of three runs.

| Profile | Heap budget (v1 and v2) | Release time guard | Named-host median |
| --- | ---: | ---: | ---: |
| prose-1mib | 44 MiB | 250 ms | 24.9 ms |
| prose-5mib | 184 MiB | 1,500 ms | 143.3 ms |
| delimiter-storm-1mib | 84 MiB | 600 ms | 53.3 ms |
| long-lines-1mib | 4 MiB | 100 ms | 5.2 ms |

Heap budgets are about 1.25 times the measured peak. Time guards are about 10
times the named-host median, which leaves room for slower hosted runners while
still catching superlinear regressions. A completed 5 MiB snapshot must also
retain no more than 128 MiB. These figures are the parser's share of D-016's
400 ms warm-launch target on this host. They are not Windows or Linux
named-host measurements, and they are not native launch or presentation
evidence.

## Residual risks

- No coverage-guided fuzzing (cargo-fuzz needs a nightly toolchain, and the
  project pins stable). The mutation and catalog layers are deterministic and
  bounded.
- Time was measured only on the Mac above. Hosted CI enforces the coarse release
  guards on Linux, macOS and Windows; named Windows and Linux workstations
  still need their own measurements in the platform phases.
- Memory: a parse peaks at about 29 times the source size for 5 MiB prose, and
  a snapshot retains about 20 times. A 1,000,000-node hostile document can
  approach 300 MiB at peak. Reducing per-node cost (string IDs, per-node child
  vectors) would need a model change and is not attempted here.
- SHA-256 fingerprinting is duplicated between the builder and validation.
  Removing the second pass would weaken independent validation, so it is left
  as an optimization opportunity.
- The upstream block scan and final validation cannot be interrupted, so
  cancellation latency grows with document size (50 ms maximum at 5 MiB here).
- Source-offset conflicts still drop the whole affected top-level block. A
  narrower repair would have to invent offsets, which the frozen model
  forbids.
- C ABI flat-view encode/copy cost and the macOS repeated-session RSS growth
  remain platform B-phase measurements.
