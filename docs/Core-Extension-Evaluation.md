# Core Extension Evaluation — FEAS-010

## Result and recommendation

Date: 2026-10-25. The owner asked at GATE-001 what it takes to support front
matter, math, alerts and the advanced and custom syntax on the accepted
pulldown-cmark core (D-008). The answer: **keep pulldown-cmark.** Every
product extension works as a semantic pass with exact byte spans. It matched
all eight frozen semantic cases, and with all extensions applied it stays
about 2.6× faster than Comrak.

Two pulldown-cmark behaviors need deliberate handling:

1. **Don't use its YAML metadata option.** It accepts a `---` block anywhere,
   not only on the first line. A thematic break followed by a setext heading
   then becomes hidden metadata, so the heading silently disappears. The shared
   layer detects front matter on line 1 instead and parses the rest.
2. **Brace-unbalanced inline math stays text.** `$\frac{1}{$` isn't math in
   pulldown-cmark, which matches the frozen expectation: the reader sees the
   source rather than an error box. Comrak treats it as math. CORE-002 should
   confirm this behavior in the contract.

This recommends an approach; it doesn't accept the production model.
GATE-001 records the decision with the owner.

Source and reproduction: [spike README](../spikes/feas010/README.md).
Durable evidence: [Linux measurements](../spikes/feas010/results-linux-2026-10-25.yaml).

## Method

The same named Linux workstation as FEAS-002 and FEAS-006: CachyOS,
kernel 7.3.0-4-cachyos, Ryzen 9 5950X, Rust 1.99.1, pulldown-cmark 0.13.5 and
Comrak 0.57.0, both locked.

Each parser adapter walks its own output into one small intermediate form:
blocks (paragraphs, headings, table cells) of inline items (text with byte
spans, strong runs, breaks, and opaque code, math, links or HTML). One shared
layer then runs every product pass, so the comparison isolates what each
parser contributes:

| Pass | Where it runs | Notes |
| --- | --- | --- |
| Tables, task lists, footnotes, strikethrough | Parser | Native in both |
| Inline and display math | Parser | Native in both; they differ on unbalanced braces |
| Alerts | Parser, then shared validation | Both accept an empty `> [!TIP]`; Comrak also accepts `> [!NOTE] text` on one line |
| Front matter | Shared | First-line `---` block; restricted parser rejects anchors, aliases, tags, nested maps and duplicate keys |
| Heading slugs and link policy | Shared | GitHub-style slugs; `javascript:`, `vbscript:`, `data:` and unknown schemes blocked |
| Diagram and math fences | Shared | Classified by info string; compiling them is FEAS-009 |
| Voice-over script syntax | Shared | Rules from `markdown-viewer` VO0 section 12 |

The voice-over pass implements the binding rules:

- **Opt-in:** `mdv: script` in front matter, with a line-scan fallback.
- **Speakers:** bold labels without lowercase letters, or names listed under `speakers:`. Speakers carry over between paragraphs and reset at headings.
- **Directions:** `[direction]` and a leading `(parenthetical)`, excluding digit-only brackets and `[sic]`.
- **Notes and pronunciation:** `[[note]]`, and `{word|reading}`, written `{word\|reading}` inside tables.
- **A/V tables:** a table with Visual and Audio columns, where Visual cells are never spoken.
- **Line numbers:** each spoken line gets its source line number.

The checker compares both engines with every frozen expectation in
[fixtures/expected](../fixtures/expected/), and verifies that every span slices
to the construct it names.

## Conformance

| Engine | Cases passed | Span checks | Span failures |
| --- | ---: | ---: | ---: |
| pulldown-cmark | 8 of 8 | 88 | 0 |
| Comrak | 7 of 8 | 89 | 0 |

Comrak's one failure is the unbalanced inline math in `math-alerts`. Three
checks can't be evaluated by a parser and are reported, not passed:

- diagram validity needs an engine (FEAS-009);
- HTML policy and security invariants belong to the renderer;
- filesystem path semantics belong to the platform adapters.

Thirteen misparse and hostile-input tests also pass on both engines. They
cover the misparse fixtures VO0 calls for:

- `**Note:**`, `[1]`, `[sic]` and task markers aren't voice syntax;
- a defined `[beat]: url` reference, links, code, math and headings aren't
  either;
- voice syntax is inert without the opt-in;
- front matter with aliases, nested maps or duplicate keys is rejected while
  the opt-in still works;
- unclosed or mid-document `---` blocks are never front matter;
- alert look-alikes are plain quotes;
- both engines produce identical voice spans.

## Cost

Median milliseconds over five fresh processes. `parse` walks the parser only,
as FEAS-002 did. `full` adds every shared pass. `script` also forces the
voice-over pass on these non-script inputs, as a worst case.

| Input | pulldown parse | pulldown full | pulldown script | Comrak parse | Comrak full | Comrak script |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Prose 1 MiB | 4.61 | 7.24 | 9.84 | 10.71 | 14.81 | 17.30 |
| Prose 5 MiB | 22.49 | 38.86 | 53.85 | 85.85 | 104.17 | 122.93 |
| Delimiter storm 1 MiB | 8.87 | 14.00 | 16.43 | 21.78 | 29.54 | 32.11 |
| Long lines 1 MiB | 0.25 | 0.26 | 2.69 | 0.53 | 0.60 | 3.66 |

Parse-only medians reproduce FEAS-002 within a few percent. The shared passes
cost roughly the same on either parser, about 16–18 ms at 5 MiB. That cost is
the same projection work CORE-002 must build anyway; it is not yet the
production semantic model.

The first voice scanner here was quadratic: it recounted newlines for every
line and searched unboundedly for closing markers. On 5 MiB it ran for well
over a minute. A line index and a 256-byte limit on marker length made it
linear. CORE-003's hostile-input checks should keep such scans bounded.

## Effort

Non-blank, non-comment Rust lines: 229 for the pulldown-cmark adapter, 184 for
Comrak and 545 for the shared passes, plus 162 lines of tests. pulldown-cmark
needs a little more adapter code for three reasons:

- tight list items have no paragraph events;
- strong runs and table headers arrive as event pairs;
- front matter must be stripped and offsets restored.

None of these is hard. The extension logic itself is shared and
parser-independent, so CORE-003 could still switch to Comrak behind the same
interface (D-008).

## Open questions for CORE-002

- **Front-matter value types.** The frozen expectations type `false` and lists;
  `markdown-viewer` keeps every value as text and joins lists with “, ”. This
  spike types booleans, null and integers. The contract must pick one.
- **Unbalanced inline math.** Record that `$\frac{1}{$` is text, matching the
  expectation, or choose Comrak's behavior with a diagnostic.
- **Voice details.** Word counts, A/V layout, colours and teleprompter timing
  are presentation work for ADV-004; this spike proves the semantics and spans.

## Checks run

```text
Checks run: cargo fmt, Clippy with warnings denied and 13 tests (release);
  check.py over 8 cases and 2 engines; bench.py, 5 trials x 4 inputs x
  2 engines x 3 modes; raw-parser diagnostics.
Passed: pulldown-cmark 8 of 8 cases; Comrak 7 of 8; 0 span failures; all tests.
Failed: Comrak math-alerts inline-math count (behavior difference, explained).
Evidence: spikes/feas010/results-linux-2026-10-25.yaml; ignored build/feas010/.
Blocking issues: none.
Residual risk: one Linux host; spike projection, not the CORE-002 model;
  diagram validity and renderer security untested here.
Recommended next task: FEAS-008 Linux part, or FEAS-009.
```
