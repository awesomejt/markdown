# Parser and Core Evaluation

## Status and recommendation

FEAS-002 is complete. Use **Rust with pulldown-cmark as the lead candidate**
for the portable semantic-core and integration spikes. Keep Comrak as a
conformance oracle and fallback until CORE-003 proves the semantic model.
Do not select cmark-gfm or Swift Markdown as the cross-platform core parser.

This is a feasibility recommendation, not the GATE-001 production decision.
The shared-core packaging spike still has to prove Windows, macOS, and Linux
integration, and the first core implementation must measure the cost of
turning pulldown-cmark's event stream into the product's immutable document
model.

The [macOS supplement](Parser-Evaluation-macOS.md) adds native Apple Silicon
measurements, including Swift Markdown 0.9.0. It supports the latency
recommendation but identifies repeated-parser peak RSS as a production risk
to investigate; the original Linux measurements below remain unchanged.

## Candidates

| Candidate | Shape | Strongest fit | Principal cost |
| --- | --- | --- | --- |
| pulldown-cmark 0.13.5 | Safe Rust pull/event parser with byte ranges | Lowest measured time/RSS; exact event offsets; natural input to a purpose-built semantic model | No mutable AST; alerts and product syntax need semantic passes |
| Comrak 0.57.0 | Safe Rust arena AST derived from cmark-gfm | Richest built-in extension set, including front matter, math, alerts, and footnotes | More allocation; inline source positions carry documented reliability caveats |
| cmark-gfm 0.29.0.gfm.13 | Portable C99 mutable AST and C API | Reference GFM behavior, mature C ABI, compact static probe | Manual memory boundary, fewer product extensions, higher measured RSS, old CMake policy friction |
| Swift Markdown | Swift value-tree API backed by cmark-gfm | Idiomatic Swift document analysis | Adds Swift toolchain requirements to a shared core and another conversion layer |

The Rust candidates were pinned by `Cargo.lock`. cmark-gfm was pinned to tag
`0.29.0.gfm.13`, commit `587a12b`. The original Linux evaluation assessed Swift
Markdown from its public API only. The linked macOS supplement measures it.
The previous description of Swift Markdown as Apple-only was incorrect;
platform support alone does not disqualify it as a portable candidate.

## Measured results

The probe parses UTF-8 input and walks every resulting node or event. It does
not render, produce the product model, cross an FFI boundary, or touch the UI.
Each median is from five fresh-process trials after one in-process warmup.

| Corpus | Comrak | pulldown-cmark | cmark-gfm |
| --- | ---: | ---: | ---: |
| prose, 1 MiB | 11.07 ms | **4.46 ms** | 13.16 ms |
| prose, 5 MiB | 90.29 ms | **22.79 ms** | 90.23 ms |
| delimiter storm, 1 MiB | 24.09 ms | **9.11 ms** | 28.36 ms |
| long lines, 1 MiB | 0.53 ms | **0.46 ms** | 0.91 ms |

On the 5 MiB prose case, fresh-process peak RSS was approximately 105 MiB for
Comrak, 40 MiB for pulldown-cmark, and 117 MiB for cmark-gfm. On the hostile
delimiter case it was approximately 39 MiB, 17 MiB, and 44 MiB respectively.
These are whole-probe measurements and include input, runtime, and allocator
retention. The Rust probe binary is 1.36 MiB but contains both Rust engines;
the statically linked C probe is 360 KiB. Neither number predicts a packaged
application size.

Full environment, exact values, method, and limitations are recorded in
[`spikes/feas002/results-2026-10-24.yaml`](../spikes/feas002/results-2026-10-24.yaml).
The source and reproduction commands are in
[`spikes/feas002/README.md`](../spikes/feas002/README.md).

All three probes parsed all eight frozen semantic inputs without failure.
Comrak attached a nonzero line/column position to every walked node. cmark-gfm
did so for every node on five cases and omitted positions on some generated or
extension nodes in the other three. pulldown-cmark returned an in-bounds byte
range for every event. These checks prove availability, not range correctness;
CORE-002 must add exact span goldens before relying on spans for selection or
incremental updates.

## Requirement fit

### Semantics and extensions

- All three primary candidates implement CommonMark and the required GFM
  building blocks: tables, task items, strikethrough, and autolinks.
- pulldown-cmark directly emits footnote, math, and YAML metadata events. A
  semantic pass can recognize GitHub alerts from blockquote content while
  retaining the original ranges. Product-specific voice, diagram, and fenced
  math meanings already require such passes regardless of parser.
- Comrak has direct nodes/options for front matter, math, alerts, and
  footnotes. It therefore remains a useful oracle for extension fixtures.
- cmark-gfm supports GFM and footnotes but not first-class front matter, math,
  alerts, or the product dialect. Its extension API is capable but moves more
  parsing and ownership work into C.
- Swift Markdown inherits cmark-gfm parsing. Its immutable copy-on-write Swift
  tree is attractive for Swift applications. Using it only in the Apple adapter
  alongside a Rust parser would create a second semantic implementation to
  maintain against the shared desktop contract.

### Source mapping

pulldown-cmark's `into_offset_iter()` returns source byte ranges with events,
which matches the proposed core contract most closely. Comrak and cmark-gfm
offer line/column source positions; Comrak's own history notes that some inline
positions are best-effort. The production model should store UTF-8 byte spans
and derive platform string indices at the adapter boundary.

No candidate alone supplies stable identities across reparses. CORE-002 must
define those identities above the parser.

### Safety

Comrak and cmark-gfm render raw HTML and dangerous links safely by default;
pulldown-cmark is implemented in safe Rust when its optional SIMD feature is
disabled. These properties reduce parser risk but do not implement the product
security policy: the native renderer must still classify raw HTML, block
dangerous URL schemes, deny implicit remote resources, bound input/work, and
sanitize any later SVG output.

A Rust core contains panics before its stable facade and avoids exposing
parser arenas or Rust types. Choosing Rust does not predetermine UniFFI versus
a handwritten C ABI; FEAS-006 owns that measurement.

### Maintenance and packaging

- pulldown-cmark and Comrak are current Rust crates and build without their
  optional CLI/rendering features in this probe.
- cmark-gfm has no external runtime dependency and a stable C surface, but its
  current release needed an explicit compatibility policy under CMake 4.4.
- Swift Markdown is appropriate only in an Apple adapter if a later spike
  proves a platform-specific benefit that outweighs semantic duplication.
- C++ adds no parser advantage in this comparison. A C++ host can consume the
  C facade, but the shared contract must remain language-neutral.

## Decision rationale

pulldown-cmark wins the next-step recommendation because its measured headroom
and byte-offset event stream align with the actual product boundary: a custom,
immutable semantic model rather than a parser-owned AST. The model-construction
cost remains unknown, but the 5 MiB case leaves roughly 67 ms of measured
headroom versus the tree parsers on this machine. That is enough to justify
testing the architecture rather than selecting the richest parser API early.

Comrak remains the fallback because it meets the initial latency hypothesis,
has the closest extension coverage, and stays within Rust. If CORE-003 shows
that constructing correct semantics over events is complex, fragile, or loses
the measured advantage, switching to Comrak does not change the external core
contract.

cmark-gfm remains valuable as a GFM conformance oracle. Its C ABI alone is not
a reason to make it the implementation: the product still needs a coarse,
versioned facade above whichever parser is chosen.

## Follow-up acceptance checks

1. CORE-002 adds exact byte-span and semantic-model goldens for the frozen
   corpus, including Unicode and extension nodes.
2. CORE-003 measures parse plus semantic-model construction and compares its
   output with Comrak/cmark-gfm on the foundation cases.
3. FEAS-006 packages a minimal Rust core for all desktop hosts and measures
   copies, cancellation, lifetime handling, diagnostics, and package size.
4. GATE-001 accepts or rejects pulldown-cmark using those results; no parser
   implementation type crosses the public facade.

## Primary references

- [pulldown-cmark repository and source-map design](https://github.com/pulldown-cmark/pulldown-cmark)
- [pulldown-cmark 0.13.5 documentation](https://docs.rs/crate/pulldown-cmark/0.13.5)
- [Comrak repository, extensions, and safety model](https://github.com/kivikakk/comrak)
- [Comrak 0.57.0 documentation](https://docs.rs/crate/comrak/0.57.0)
- [cmark-gfm repository, portability, and security](https://github.com/github/cmark-gfm)
- [cmark-gfm 0.29.0.gfm.13 release](https://github.com/github/cmark-gfm/releases/tag/0.29.0.gfm.13)
- [Swift Markdown repository and value-tree design](https://github.com/swiftlang/swift-markdown)
