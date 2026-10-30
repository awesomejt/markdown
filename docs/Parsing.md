# Basic parsing and safety contract

CORE-003A added `parse_markdown(source, document_id, revision_id, limits)` to
`markdown-core`. It returns a validated immutable semantic v1 `Document` or a
structural `ParseError`. Source is exact UTF-8 supplied by the caller. The core
performs no filesystem, network, URI activation, rendering or process calls.
The [model contract](../specs/semantic-model/v1/README.md) remains frozen and
unchanged. Snapshot IDs are deterministic ordinals and are not persistent
across revisions; CORE-004A [sessions](../specs/session/README.md) provide
block-level cross-generation correspondence through anchors instead. The owner
accepted snapshot-local IDs for the MVP in [D-021](../agents/Decisions.md).

[CORE-003 acceptance](Core-Parser-Acceptance.md) maps the aggregate corpus,
safety, generated-input and benchmark criteria to the delivered checks and
records the remaining limitations and platform gates.

## Opt-in v2

CORE-003B2 adds `parse_markdown_v2` behind feature `model-v2`. The owner selected
an additive version with [explicit reference provenance](../specs/semantic-model/v2/README.md)
and [typed URI targets](URI-Policy.md), preserving the default v1 API/schema.
V2 resolves references and supports legitimate Unicode/encoded destinations;
v1's reference fallbacks and conservative activation subset below remain its
contract. Shared parsing also corrects multiline quoted-container destination
spans without changing v1 wire meaning. No default format or facade is switched.

## Implemented surface

Pinned pulldown-cmark 0.13.5 runs with default features disabled. One shared
option set enables tables, task lists, strikethrough, GFM footnotes, math and
`ENABLE_GFM` (alert tags only). V2 provenance reparsing uses the same set. Its
HTML output module and CLI are excluded. The adapter consumes offset events
into an owned flat tree:

- Headings, paragraphs, quotes, ordered/unordered lists and list items.
- Emphasis, strong, strike, code spans, text, soft/hard breaks and rules.
- Indented/fenced literal code, including unknown or missing language info.
- GFM tables/alignment and task checked states.
- Inline links, angle autolinks/email links and inline images with plain alt text.
- Bare GFM autolinks: `http(s)://`, `www.` and email (CORE-003D, below).
- Reading-order outline, source fingerprint and exact UTF-8 spans.

Tight items receive explicit paragraph nodes; loose lists retain their setting.
Entities and Markdown escapes become semantic text, while spans slice original
lexemes, including CRLF/NFC/NFD. There is no per-character display/source map
or syntax highlighting yet. Destination spans exclude optional angle brackets;
email autolinks retain their source address span while their semantic destination
includes `mailto:`. Autolinks nested in another link become inert inline source
fallbacks, with no inner activation intent. The outer link retains its destination.

### Bare autolinks

One bounded shared pass (`src/autolink.rs`, D-031) follows cmark-gfm's
extended-autolink rules, which GitHub uses:

- `http://` or `https://` (any case) not preceded by a letter, then a domain
  (`localhost` allowed). `www.` at a line start or after whitespace, `*`, `_`,
  `~` or `(`, then a domain with a dot. An email has a local part of
  alphanumerics and `.+-_`, `@`, and a dotted domain ending in a letter.
- No underscore in a domain's last two labels. A link runs to whitespace or `<`.
- Trailing `?!.,:*_~'"` is dropped, as are an entity-like `&name;` or lone `;`
  at the end and each `)` that is unbalanced within the link.

Detection runs over a run of contiguous text events whose text equals their
exact source, so a link can cross pulldown-cmark's event splits at `_`, `*` or
`$`. Anything else ends the run: an escape, a decoded entity, emphasis, a code
span, inline math or HTML. A link is never found inside an existing link,
image alt, code span or block, HTML block or fenced engine block. Text between
inline HTML tags is ordinary text in this model, since the tags are inert, so
it can link.

The link node and its one text child cover the exact lexeme, which is also the
intent span. Destinations add `http://` to `www.` links and `mailto:` to email
links, then take the normal URI policy: v1 is ASCII-only, and v2 classifies and
normalizes the target (IDNA, percent-encoding). The v2 provenance pass calls
the same detector over the same runs, so it proves these intents like any
other inline link. Runs without a link keep one text node per event, so
documents without bare links parse exactly as before.

The pass is linear in the run: a failed candidate resumes past the domain it
scanned, an email scan never passes the next `@`, and the trailing-delimiter
counts are taken once. Each link adds two nodes and copied text, so the node
and text limits bound it, and the parser's cancellation checks still apply.
Differences from cmark-gfm: no `ftp://` or `mailto:`/`xmpp:` forms, any
Unicode whitespace ends a link, a decoded entity ends a run (cmark-gfm links
across `&amp;`), and matched emphasis or strikethrough inside a URL ends it,
so `https://example.com/a*b*c` links only `https://example.com/a`. Each keeps
spans exact source; escape or angle-bracket such URLs to link them whole.

Upstream tab-expanded container offsets can violate v1 child containment or
sibling ordering. Since CORE-003B4, only each affected top-level block becomes
an inert `unrepresentable_source_mapping` block over its exact upstream span,
with a `source_mapping_fallback` warning. Its intents, outline entries and
diagnostics are dropped, and footnote references elsewhere to a definition
inside it become unresolved-reference fallbacks. Other blocks keep full
semantics ([details](Parser-Budgets.md#source-offset-fallback)). If validation
still reports one of those two errors, a safety net discards the partial graph
and returns a single inert block covering the exact body. Document/revision
IDs, source fingerprint and first-position front matter are preserved. Offsets
are never widened, clipped or invented. Other source-mapping/model errors remain
errors rather than publishing an unchecked model.

Heading titles flatten plain text, code, image alt and source fallbacks, omitting
raw HTML tag fallbacks from outline labels while preserving those source nodes.
Anchors lowercase Unicode letters/numbers, keep hyphens/underscores, replace
whitespace with hyphens and drop other characters; empty results use `section`.
Collisions receive increasing numeric suffixes, also checking preexisting
suffixed anchors. This is the initial deterministic rule, not a claim of exact
GitHub slug compatibility or identity reconciliation. Duplicate headings have
a regression with 10,000 occurrences; that is not a timing benchmark.

## Conservative safety policy

Raw HTML blocks/tags are unsupported source nodes with diagnostics. Native
presenters must display fallback source as plain text. No executable HTML model
node or sanitizer output exists, including for otherwise harmless details/br.
Advanced `mermaid`, `d2`, `math` and `latex` fences, and inline/display math,
are source fallbacks (see the extension semantics section). Other code fences
are always literal code; language info is untrusted text.

Link classification uses the parser's entity/escape-decoded destination.
Non-ASCII destinations (including Unicode format/bidi characters), controls,
whitespace, backslashes and percent encoding are denied. Links permit
nonempty `#fragment`, http/https/mailto prefixes with nonempty remainder, or a
relative destination with no colon, absolute prefix or dot/dot-dot segment.
Every other link is a denied intent. Images permit only that relative subset;
remote/absolute/unsafe images remain denied intents with readable alt text.

V1 deliberately blocks some legitimate names/URLs (Unicode names/IDNs and
Unicode fragments, spaces, percent encoding and parent-relative navigation).
Opt-in v2 supports these under [its typed URI policy](URI-Policy.md). V1 does
not validate full URL syntax, normalize paths, grant permissions or prove the
safety of a platform URL launcher. Native adapters must revalidate external
activation and decode/resolve local resources under actual OS capabilities,
including symlinks, containment, device paths and permission revocation. A
`requires_capability` intent alone never authorizes I/O. Blocked targets are
retained as untrusted values, never copied into shell commands or HTML.

## Explicit phase boundaries

Only an initial `---` line (optionally preceded by a UTF-8 BOM), terminated
by a later exact `---` or `...` line,
is recognized as front matter (LF/CRLF). CORE-003B1 adds the
[bounded typed metadata grammar](Front-Matter.md): flat null/bool/i64/string/
string-list properties and an optional string display title. Unsupported,
duplicate or oversized metadata atomically omits properties with a structural
warning, preserving the exact source span and front-matter capability. Presenters
must keep that source readable. Empty/comment-only mappings are valid.
An immediately blank line after the opening rule selects ordinary Markdown;
otherwise delimiter precedence follows D-015, even for Markdown-looking bodies.
Unterminated or mid-document delimiters follow CommonMark. No YAML
metadata parser option is enabled. Body offsets and source fingerprints are
unchanged; source-offset fallback also preserves metadata warnings/properties.

Resolved reference links/images use explicit inline source fallbacks and a
`reference_link_deferred` diagnostic. V1 intent spans must be inside the
occurrence, while reference destinations live in a separate definition; this
phase does not invent a destination span or change the frozen model. CORE-003B2 implements these under opt-in v2 rather than changing v1.
Unresolved references remain ordinary CommonMark text. Reference definitions
are parser syntax and need not appear as body nodes, as with other delimiters.
V2 adds an info `reference_definition_shadowed` diagnostic (no node) on each
ignored later duplicate definition's exact span; the first still wins and v1 is
unchanged. Detection reparses only event-free body gaps, flattened, capped at
twice the body and 1,024 notes; it fails closed, so exotic layouts and long
same-label runs without blank lines can be under-reported, never misreported.

## Extension semantics (CORE-003B3)

CORE-003B3 maps the MVP extensions onto existing v1/v2 node kinds. The frozen
model schemas and model goldens do not change. v1/v2 trees and reading stats do
change for documents that use these extensions; see `specs/search/README.md`.

- **Alerts.** `> [!NOTE]`, `[!TIP]`, `[!IMPORTANT]`, `[!WARNING]` and
  `[!CAUTION]` on their own first quote line become `alert` nodes. Markers are
  case-insensitive. The span covers the marker line, and children are the
  remaining blocks. An empty `> [!TIP]` is an alert with no children.
  Alerts nest and may sit in lists. Unknown kinds, and a marker followed by
  text on the same line, stay ordinary block quotes.
- **Footnotes.** GFM syntax is used, not the old syntax. Definitions are
  `footnote_definition` blocks at their source position, including inside
  quotes and list items. Labels match case-insensitively with the parser's own
  folding (`unicase`). Each reference names the first definition's exact label,
  so the model's exact-label rule holds. A later duplicate under the same
  folding is an `unsupported_block` (`footnote_definition`, `invalid_input`)
  covering its whole source, with an info `footnote_definition_duplicate`. Its
  contents, including links, are inert. Undefined references stay ordinary
  text, as in GFM, and the capability is absent without a definition. A
  reference inside image brackets prevents the image upstream, as a link does.
  Heading titles and anchors omit footnote markers.
- **Math.** `$…$` and `$$…$$` become `unsupported_inline` (`inline_math` or
  `display_math`, `unavailable`) over the exact delimited lexeme, each with an
  info `math_unavailable`. Display math is an inline inside its paragraph, as
  pulldown-cmark emits it. Math content is opaque, so a link inside math
  creates no intent. Heading titles keep the math source (`# Math $E=mc^2$`
  anchors as `math-emc2`), and image alt keeps it as text. Brace-unbalanced
  math (`$\frac{1}{$`), prices (`$5 and $10`), spaced or escaped dollars and
  code spans stay ordinary text, so invalid math remains readable without an
  error node (FEAS-010). Upstream splits text at every non-math `$`; the
  builder rejoins contiguous text pieces at a `$` source boundary, so dollar
  prose costs the same nodes as without math. `math`/`latex` fences stay
  `unsupported_block`.
- **Diagrams.** Fences whose first info word is exactly `mermaid` or `d2`
  (case-sensitive, as in markdown-viewer) are `unsupported_block` over the
  whole fence, with `advanced_fence_disabled`. Other spellings are literal
  code. No engine validates or compiles diagram source.
- **Voice-over scripts.** Script mode is selected by front-matter
  `mdv: script`. When typed properties are omitted, an exact `mdv: script`
  front-matter line still opts in (FEAS-010 E1). In script mode the
  front-matter span gets one info `voice_script_unavailable`, with no node.
  It survives the inert-body source-mapping fallback. Speakers, directions,
  notes, pronunciation guides and A/V tables are not interpreted. They read
  as ordinary Markdown, and `[direction]` never becomes a link without a
  definition. Body lines are never an opt-in.

Diagnostics are one per fallback node and are bounded by the node limit. No
engine, renderer, executable node or post-MVP syntax is added. Definition
lists, super/subscript and wikilinks stay disabled. Bare GFM autolinks are
supported (CORE-003D, D-031; see [Bare autolinks](#bare-autolinks)).

The frozen FEAS corpus is unchanged. The tests assert the implemented subset:
13 GFM headings, tables and tasks, links, code, security invariants, and the
B3 alert, footnote, math, diagram and voice expectations above. Diagram
validity and rendering stay engine work and are not asserted.

## Limits and failure behavior

| Limit | Default | Enforcement |
| --- | --- | --- |
| Source | 8 MiB | Before hashing, initial model validation or parser construction |
| Events | 2,000,000 | Before adapting each emitted event; text rejoined across a `$` split is refunded |
| Nodes | 1,000,000 | Before adding a model node |
| Depth | 128 | Parser events and model/synthetic-paragraph ancestry |
| Copied text | 32 MiB | Cumulative copied source-derived strings, including repeated destinations/titles and metadata |

Limits are caller-adjustable defensive ceilings, not release performance
budgets or allocator quotas. Parser construction can scan/allocate for the
entire bounded source before it emits an event. The node/event/depth ceilings
do not interrupt that initial scan, so dense delimiter input can peak at
over 1 GiB of heap at the source ceiling before the node limit stops it (see
[Parser-Budgets.md](Parser-Budgets.md)). Derived strings and string appends can
have temporary/spare capacity; static diagnostics and IDs have additional
bounded overhead. Model validation independently verifies the result; wire
limits apply to decoding, not to serialized output size.

Errors never echo source, destinations or caller identifiers. No deadline or
incremental update is promised. CORE-004A adds `parse_markdown_cancellable` and
`parse_markdown_v2_cancellable`, which poll a cooperative token every 256 events
and between phases with otherwise identical output; the upstream block scan and
final validation are not interruptible ([session contract](../specs/session/README.md)).
Host applications must schedule parsing away from the UI thread. CORE-003B4 raised
the node/event defaults from 250,000/1,000,000 so representative prose at the
source ceiling parses (D-024), and added hostile-input/property coverage,
measured time/memory and heap budgets in [Parser-Budgets.md](Parser-Budgets.md);
native launch/scroll/memory acceptance remains in platform phases.

## Reproduction and evidence

Run `python tools/check.py all` with the toolchain in
[Development.md](Development.md). Parser regressions run in default and schema
builds, debug and release, through the existing three-host CI matrix. They cover
basic and malformed syntax, exact spans, unsafe/obfuscated destinations, HTML,
limits, duplicates, Grok's email/nested-link/tab/Unicode reproductions, two bounded
20,000-case deterministic generated-markup parse/revalidation checks (the
second mixes B3 extension tokens and also runs v2), all eight
frozen corpus inputs and an original serialized
[golden](../specs/parsing/basic/expected.json) from
[its source](../specs/parsing/basic/input.source.txt), authored here under Apache-2.0.
The frozen model goldens/schema must remain unchanged. An independent read-only
safety/source-mapping review found and verified the heading HTML-label fix;
its initial final pass found no remaining actionable issue. Grok's subsequent
review identified email classification, Unicode URI obfuscation and nested-link/
tab-container robustness gaps, now fixed with the regressions above. CORE-003B4
adds the [conformance contract](../specs/parsing/conformance/README.md),
corpus-seeded mutation, a pathological-construct catalog and heap budgets
([Parser-Budgets.md](Parser-Budgets.md)). CORE-003D adds the bare-autolink
[fixture](../specs/parsing/autolinks/input.source.txt) with
[v1](../specs/parsing/autolinks/expected.json) and
[v2](../specs/parsing/autolinks/expected.v2.json) goldens, forged-intent
rejection, a 20,000-case generated autolink check in both versions, and
autolink storms in the hostile catalog. None of this is coverage-guided fuzzing.

The bounded stdin example can reproduce the new golden:

```sh
cargo run --locked --example parse_markdown < specs/parsing/basic/input.source.txt > build/parser.json
```

The autolink goldens were generated the same way, deliberately, and reviewed:

```sh
cargo run --locked --example parse_markdown < specs/parsing/autolinks/input.source.txt > specs/parsing/autolinks/expected.json
cargo run --locked --features model-v2 --example parse_markdown_v2 < specs/parsing/autolinks/input.source.txt > specs/parsing/autolinks/expected.v2.json
```

Use generated output only for deliberately supplied input; user snapshots must
not be committed or included in logs. The upstream
[parser API](https://docs.rs/pulldown-cmark/0.13.5/pulldown_cmark/struct.Parser.html)
and options are checked against the pinned crate source in the local Cargo
registry. No spike code is imported or added as a dependency.
