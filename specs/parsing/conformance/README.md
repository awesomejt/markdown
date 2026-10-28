# Parser Conformance Contract

`corpus.json` reconciles the frozen fixture corpus with the parser (CORE-003B4).
It is generated, not hand-edited:

```sh
python tools/generate_conformance.py          # rewrite corpus.json
python tools/generate_conformance.py --check  # common suite: fail if stale
```

The generator reads `fixtures/manifest.yaml`, every `fixtures/expected/*.yaml`
case and `fixtures/performance/profiles.yaml`. It classifies each expectation,
and fails on any key or invariant it does not recognize:

- **Checked.** The value is copied into the case, and its token is added to
  `checks`. `core/crates/markdown-core/tests/conformance.rs` runs exactly
  those tokens and fails on an unknown one.
- **Deferred.** The YAML path and a reason are added to `deferred`. Nothing is
  asserted.

Every case also checks `outline`, `front_matter`, `counts`, `html_inert` and
`blocked_links`.

## Check tokens

| Token | Assertion against the v1 model |
| --- | --- |
| `outline` | Outline `(level, title, anchor)` equals the fixture `(level, text, slug)` in order |
| `front_matter` | Front-matter presence and typed properties (string, boolean, integer, string list, null) |
| `counts` | Feature counts: headings, tables, task items, alerts, block quotes, fences, footnote definitions, inline/display/fenced math, diagram fences, raw HTML constructs, link intents |
| `code_fences` | Fenced blocks in order. Language is the first word of the info string (null when empty). Semantic is `code`, `math` (math/latex fallback) or `diagram` (mermaid/d2 fallback) |
| `links` | Link intents in order as destination plus `fragment`, `relative` (requires capability), `external` or `blocked` (denied) |
| `alerts` | Alert categories in order |
| `non_alert_marker` | The look-alike text stays inside an ordinary block quote |
| `math` | Inline, display and fenced math fallback counts |
| `voice_mode` | `mdv: script` front matter yields `voice_script_unavailable` |
| `no_links` | No link or resource intents |
| `fallback_diagnostics` | Every unsupported node has a diagnostic naming it and a non-empty exact span |
| `html_inert` | Every raw HTML node is an inert source fallback with a diagnostic, starting at `<` |
| `blocked_links` | Any link or image whose destination uses a listed scheme is denied |

With `model-v2`, each case also parses as v2, with the same outline and link
count. The `profiles` array pins each performance profile's generator, size
and SHA-256. The Rust replica in `tests/support/profiles.rs` must reproduce
those bytes exactly.

## Deferred categories

Each deferral in `corpus.json` carries its own reason. They fall into five
groups:

| Group | Fixture expectations | Owner |
| --- | --- | --- |
| Voice-over interpretation | voice notes and pronunciation counts; speakers, directions, notes, A/V tables, line numbers | Voice work after the MVP. CORE-003B3 only reports the opt-in |
| Diagram engines | diagram engines, fence `valid`, offline compilation, SVG safety | Advanced diagram work (ADV) |
| Syntax highlighting | highlighting failure invariant | Native presentation phases |
| Filesystem paths | NFC/NFD lookup, display-name round trip, workspace escape | Platform adapters and path fixtures |
| HTML policy | `safe-subset` (gfm-tour) and `sanitize` (unsafe-html) | None. The core is stricter: all raw HTML stays inert text |

See [Parser-Budgets.md](../../../docs/Parser-Budgets.md) for the hostile-input
and budget evidence that accompanies this contract.
