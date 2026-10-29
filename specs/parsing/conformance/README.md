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
  those tokens, on v1 and also on v2 with `model-v2`, and fails on an
  unknown one.
- **Deferred.** The YAML path and a reason are added to `deferred`. Nothing is
  asserted.

Every case also checks `outline`, `front_matter`, `counts`, `html_inert` and
`blocked_links`.

## Check tokens

| Token | Assertion against each model version |
| --- | --- |
| `outline` | Outline `(level, title, anchor)` equals the fixture `(level, text, slug)` in order |
| `front_matter` | Front-matter presence and typed properties (string, boolean, integer, string list, null) |
| `counts` | Feature counts: headings, tables, task items, alerts, block quotes, fences, footnote definitions, inline/display/fenced math, diagram fences, raw HTML constructs, link intents |
| `code_fences` | Fenced blocks in order. Language is the first word of the info string (null when empty). Semantic is `code`, `math` (math/latex fallback) or `diagram` (mermaid/d2 fallback) |
| `links` | Link intents in order as destination plus `fragment`, `relative` (requires capability), `external` or `blocked` (denied). The kind is the v1 disposition or the v2 target kind |
| `alerts` | Alert categories in order |
| `non_alert_marker` | The look-alike text stays inside an ordinary block quote |
| `math` | Inline, display and fenced math fallback counts |
| `voice_mode` | `mdv: script` front matter yields `voice_script_unavailable` |
| `no_links` | No link or resource intents |
| `fallback_diagnostics` | Every unsupported node has a diagnostic naming it and a non-empty exact span |
| `html_inert` | Every raw HTML node is an inert source fallback with a diagnostic, starting at `<` |
| `blocked_links` | Any link or image whose destination uses a listed scheme is denied |

The `profiles` array pins each performance profile's generator, size
and SHA-256. The Rust replica in `tests/support/profiles.rs` must reproduce
those bytes exactly.

## Model v2

D-025 makes v2 the model native hosts ship. Features are opt-in, so the
default-feature run stays v1-only. Runs with `model-v2` (`--features model-v2`
in `core-extended` and `--all-features` in the PR tier) also parse every case
as v2, and the tokens run through a version-neutral view of nodes, outline,
metadata, diagnostics and intents. The fixtures name only intent kinds, so the
version-specific expectations are written in the test rather than the
generated contract:

- **Shared fields.** V2 equals v1 in document and revision IDs, source,
  capability flags, roots, nodes, outline, metadata and diagnostics.
- **Intent pairing.** Links and resources pair with v1 in order, with the same
  ID and destination, and an `inline` source whose span is the v1 span. The
  targets must take one of these forms:

  | v1 | v2 target | Constraint |
  | --- | --- | --- |
  | `fragment` | `fragment` | Same anchor |
  | `requires_capability` | `requires_capability` | `path` plus `#fragment` gives back the destination |
  | `external` | `external` | `uri` is the destination, or the destination with a root `/` added by URL normalization |
  | `denied` (`unsafe_or_unsupported_destination`) | `denied` | Reason `unsafe` or `unsupported_scheme` |
  | Resource `requires_capability` or `denied` | The same kind | None |

  Any other pairing fails.
- **Round trip.** The v2 snapshot re-decodes through `from_json` and its
  provenance proof to an equal document.
- **Denied schemes.** Every spelling of `javascript`, `vbscript`, `data` and
  `file` is denied as a link and as an image under both versions.

V1 deliberately leaves reference links and images inert, and the frozen
corpus has none. A separate test uses the
[v2 reference fixture](../../semantic-model/v2/fixtures/references.source.txt).
Under v1, each reference occurrence is a `reference_link` or `reference_image`
fallback with a `reference_link_deferred` diagnostic and no intent. Under v2,
exactly those occurrence spans become reference-sourced intents, and the
inline intents pair with v1 by destination and span (v2 also numbers the
reference intents, so snapshot-local IDs differ). Their targets are
pinned exactly, including the deliberate split: v1 denies the non-ASCII and
percent-encoded destinations that v2 decodes into typed targets. The fixture
also fixes the `unsupported_scheme`, `remote_resource` and decoded local image
targets, and one intent per reuse of a definition.

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
