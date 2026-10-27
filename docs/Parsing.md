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

## Opt-in v2

CORE-003B2 adds `parse_markdown_v2` behind feature `model-v2`. The owner selected
an additive version with [explicit reference provenance](../specs/semantic-model/v2/README.md)
and [typed URI targets](URI-Policy.md), preserving the default v1 API/schema.
V2 resolves references and supports legitimate Unicode/encoded destinations;
v1's reference fallbacks and conservative activation subset below remain its
contract. Shared parsing also corrects multiline quoted-container destination
spans without changing v1 wire meaning. No default format or facade is switched.

## Implemented surface

Pinned pulldown-cmark 0.13.5 runs with default features disabled and only tables,
task lists and strikethrough options enabled. Its HTML output module and CLI
are excluded. The adapter consumes offset events into an owned flat tree:

- Headings, paragraphs, quotes, ordered/unordered lists and list items.
- Emphasis, strong, strike, code spans, text, soft/hard breaks and rules.
- Indented/fenced literal code, including unknown or missing language info.
- GFM tables/alignment and task checked states.
- Inline links, angle autolinks/email links and inline images with plain alt text.
- Reading-order outline, source fingerprint and exact UTF-8 spans.

Tight items receive explicit paragraph nodes; loose lists retain their setting.
Entities and Markdown escapes become semantic text, while spans slice original
lexemes, including CRLF/NFC/NFD. There is no per-character display/source map
or syntax highlighting yet. Destination spans exclude optional angle brackets;
email autolinks retain their source address span while their semantic destination
includes `mailto:`. Autolinks nested in another link become inert inline source
fallbacks, with no inner activation intent. The outer link retains its destination.

Upstream tab-expanded container offsets can violate v1 child containment or
sibling ordering. For those two validation errors only, the parser discards the
partial graph and all its intents/outline and returns a fresh validated inert
source block covering the exact body, with `source_mapping_fallback` diagnostic.
Document/revision IDs, source fingerprint and first-position front matter are
preserved. This deliberately sacrifices rich semantics for the entire body while
keeping the file readable; it never widens, clips or invents source offsets.
Other source-mapping/model errors remain errors rather than publishing an
unchecked model.

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
Advanced `mermaid`, `d2`, `math` and `latex` fences are source fallbacks. Other
code fences are always literal code; language info is untrusted text.

Link classification uses the parser's entity/escape-decoded destination.
Non-ASCII destinations (including Unicode format/bidi characters), controls,
whitespace, backslashes and percent encoding are denied. Links permit
nonempty `#fragment`, http/https/mailto prefixes with nonempty remainder, or a
relative destination with no colon, absolute prefix or dot/dot-dot segment.
Every other link is a denied intent. Images permit only that relative subset;
remote/absolute/unsafe images remain denied intents with readable alt text.

This deliberately blocks some legitimate names/URLs (Unicode names/IDNs and
Unicode fragments, spaces, percent encoding and parent-relative navigation) until CORE-003B's URI/conformance work. It does
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

Bare GFM URL autolinks, math, alerts, footnotes, voice syntax and other extensions do not have their
options enabled; they retain basic Markdown/source behavior until CORE-003B.
The frozen FEAS corpus is unchanged. The tests assert the implemented foundation
subset (13 GFM headings, table/tasks, links, code and security invariants), not
full success against its deferred extension expectations.

## Limits and failure behavior

| Limit | Default | Enforcement |
| --- | --- | --- |
| Source | 8 MiB | Before hashing, initial model validation or parser construction |
| Events | 1,000,000 | Before adapting each emitted event |
| Nodes | 250,000 | Before adding a model node |
| Depth | 128 | Parser events and model/synthetic-paragraph ancestry |
| Copied text | 32 MiB | Cumulative copied source-derived strings, including repeated destinations/titles and metadata |

Limits are caller-adjustable defensive ceilings, not release performance
budgets or allocator quotas. Parser construction can scan/allocate for the
entire bounded source before it emits an event. The node/event/depth ceilings
do not interrupt that initial scan. Derived strings and string appends can
have temporary/spare capacity; static diagnostics and IDs have additional
bounded overhead. Model validation independently verifies the result; wire
limits apply to decoding, not to serialized output size.

Errors never echo source, destinations or caller identifiers. No deadline or
incremental update is promised. CORE-004A adds `parse_markdown_cancellable` and
`parse_markdown_v2_cancellable`, which poll a cooperative token every 256 events
and between phases with otherwise identical output; the upstream block scan and
final validation are not interruptible ([session contract](../specs/session/README.md)).
Host applications must schedule parsing away from the UI thread. CORE-003B owns hostile-input fuzz/property
coverage and measured parser time/memory budgets; native launch/scroll/memory
acceptance remains in platform phases.

## Reproduction and evidence

Run `python tools/check.py all` with the toolchain in
[Development.md](Development.md). Parser regressions run in default and schema
builds, debug and release, through the existing three-host CI matrix. They cover
basic and malformed syntax, exact spans, unsafe/obfuscated destinations, HTML,
limits, duplicates, Grok's email/nested-link/tab/Unicode reproductions, a bounded
20,000-case deterministic generated-markup parse/revalidation check, all eight
frozen corpus inputs and an original serialized
[golden](../specs/parsing/basic/expected.json) from
[its source](../specs/parsing/basic/input.source.txt), authored here under Apache-2.0.
The frozen model goldens/schema must remain unchanged. An independent read-only
safety/source-mapping review found and verified the heading HTML-label fix;
its initial final pass found no remaining actionable issue. Grok's subsequent
review identified email classification, Unicode URI obfuscation and nested-link/
tab-container robustness gaps, now fixed with the regressions above. The bounded
generated test does not establish exhaustive fuzz coverage. This does not replace
hosted checks, phase review or CORE-003B fuzz/performance acceptance.

The bounded stdin example can reproduce the new golden:

```sh
cargo run --locked --example parse_markdown < specs/parsing/basic/input.source.txt > build/parser.json
```

Use generated output only for deliberately supplied input; user snapshots must
not be committed or included in logs. The upstream
[parser API](https://docs.rs/pulldown-cmark/0.13.5/pulldown_cmark/struct.Parser.html)
and options are checked against the pinned crate source in the local Cargo
registry. No spike code is imported or added as a dependency.
