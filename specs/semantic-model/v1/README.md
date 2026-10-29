# Semantic document model v1

Status: frozen by owner-authorized CORE-002A PR #2 merge (`e5f1a17`).
CORE-003A implements basic parsing without changing this wire contract. Parsing, generation/cancellation and
identity reconciliation, search, C ABI negotiation and native UI remain their
own delivery phases. This is not the flat FEAS-006/008 model.

The Rust implementation is [markdown-core](../../../core/crates/markdown-core/src/lib.rs).
The [JSON Schema](document.schema.json) has ID `urn:markdown:semantic-document:1`.
It is generated from the wire types; the schema snapshot test detects drift.
The default core build has no schema dependency; feature `schema` enables
export and schema consistency tests. Serialization/validation use serde,
serde_json and SHA-256; v1 validation does not parse source or access files. It
does reuse the parser's pure URI classification for intents (CORE-002B).

## Values, ownership and versions

`DocumentInput` and its component values are unchecked construction/decoding
DTOs. Only `Document::new(input, source, limits)` or
`Document::from_json(bytes, source, limits)` yields an immutable `Document`.
There is no unchecked Deserialize implementation for `Document`. Reading it
returns shared references to owned values; consuming `into_input` requires
revalidation before that DTO can again become a `Document`. The crate forbids
unsafe code and exposes no UI, OS handles, parser pointers or borrowed arenas.

`model_version` must be the integer 1. Unknown fields/kinds/versions fail closed;
duplicate fields, metadata keys and capabilities are rejected by wire decoding.
New node kinds or incompatible meaning/required-field changes require a new
model version and fixture/schema directory. V1 is not an extension bag. An
unsupported feature can carry its name and reason through existing fallback
nodes without changing the version. Future facade negotiation is CORE-006A.
The schema checks shape, not all semantic or decoder constraints: consumers
must still use the Rust validation boundary. Canonical output uses integer
JSON tokens, explicit nullable optional values and deterministic field/collection
ordering; general JSON canonicalization/signatures are not specified.

## Source and identity

Source is the exact UTF-8 byte sequence supplied by the platform snapshot.
Offsets are half-open byte ranges `[start,end)`, not characters, UTF-16 units,
line/column coordinates or normalized offsets. They must be ordered, bounded
and UTF-8 character boundaries. Empty ranges are permitted. CRLF, NFC and NFD
are preserved; no newline/Unicode normalization occurs. Native adapters must
convert coordinates against the same source. SourceSpan::slice returns None
for invalid ranges without panicking.

Wire output stores source byte length and lowercase SHA-256, not another copy
of source. Validation binds the snapshot to the supplied bytes. Consumers retain
the matching source for slicing/fallback; this digest is an association check,
not a signature or proof that semantic text was correctly parsed.

Document/revision/node/link/resource IDs are distinct string types. They are
opaque ASCII tokens of 1–128 characters from `A-Z a-z 0-9 _ . : -`; they are
not filesystem paths. Node IDs are unique within a snapshot and scoped to a
document lineage, not globally unique. Surviving semantic identities should keep
IDs across revisions; offsets are not IDs. CORE-004A owns allocation/matching,
deletions and revision publication. CORE-002A tests that identity can survive
changed spans; it does not implement or prove a matching algorithm.

## Tree, outline and semantics

Nodes are a flat owned array referenced by IDs. Roots contain block nodes;
each node has exactly one parent/root membership and is reachable exactly once.
Cycles, orphans, duplicate IDs, reversed/overlapping sibling spans, child spans
outside parents and invalid parent/child categories are rejected. Tree walking
is iterative and depth-limited; no recursive Rust ownership/drop chain exists.

| Family | Node kinds |
| --- | --- |
| Basic blocks | paragraph, heading, block_quote, list, list_item, code_block, thematic_break |
| Table | table, table_row, table_cell |
| Extensions | footnote_definition, alert, unsupported_block |
| Inline | text, emphasis, strong, strikethrough, code_span, soft_break, hard_break, link, image, footnote_reference, unsupported_inline |

Lists have ordered/start/tight values and item checked state. Ordered start 0
is valid; unordered lists have no start. Tables have nonempty alignments/rows,
a header first row and matching column counts. Footnote labels are unique and
references must resolve. Formatting containers hold inlines; block containers
hold blocks. Links cannot nest. Text/code/alt/title values are plain untrusted
text, never instructions to construct HTML. Correct Markdown interpretation
and source mappings are CORE-003 acceptance, not inferred from structural validity.

Headings have levels 1–6 and unique nonempty whitespace-free anchors. Outline
entries cover every heading in tree reading order with matching ID, level and
anchor; titles are plain display text. Native hierarchy derives from levels.

Metadata carries an optional first-position front-matter span, a display title
and a deterministic property map. Property values are null, boolean, i64 integer,
string or string list; nested objects and floats are not v1 metadata. Properties
require front matter, which cannot overlap body roots. Detection/parsing remains
CORE-003; no YAML parser or whole-document YAML mode is enabled here. Since
CORE-002B, validation applies the front-matter producer's display guard to the
title, property keys and string or list values: controls, General_Category=Cf
and bidi formatting characters are rejected (`metadata_text`/`metadata_key`).
Since CORE-003E (D-032) the blank default-ignorable values that are not Cf,
such as the Hangul fillers and U+2800, are rejected too. Variation selectors,
Mongolian FVS and CGJ remain allowed, as in
[Front-Matter.md](../../../docs/Front-Matter.md). V2 shares this check.

Capabilities are the exact set of semantic extensions present: tables, task
lists, footnotes, strikethrough, front matter and alerts. They are not permission
grants, advertised renderer support or evidence an advanced engine exists.
Unsupported math/diagram/raw-HTML syntax uses a fallback node and original span;
unknown feature names stay visible as source. No raw-HTML execution node exists.

Diagnostics have stable code, severity, plain message and optional span/node.
References and ranges are validated. Validation errors use codes and structural
indices, without echoing source/identifiers/private paths. Applications must not
log user snapshots or their serialized contents merely because they validate.

## Link/resource intents

Each link/image occurrence references exactly one unique intent; unused or
multiply-used intent records and destinations outside the referring node's span
are rejected. Intent spans cover the destination lexeme. Denied intents remain
visible with a reason. Fragment links carry a matching `#anchor`; unresolved
fragments can be diagnosed by later parsing/navigation without invalidating the
whole document. Non-denied destinations are nonempty ASCII without controls,
whitespace, backslashes or percent encoding. External link intents also need an
http, https or mailto prefix with a nonempty remainder. This is not a full URL
sanitizer.

Local links and image resources use `requires_capability`: relative URI text
without a scheme, absolute prefix or `.`/`..` segments. Since CORE-002B, validation
re-runs the parser's own classification for every intent that is not denied, so
a decoded document can narrow an intent to denied but cannot claim one the
parser would withhold. That policy still classifies `https://user:pw@…` as
external; v2 denies credentials (D-025). Adapters still resolve against an
actual OS-approved capability and enforce containment before I/O. No model value grants
access, fetches resources, opens a URI or launches a process. Remote images,
file/absolute destinations and other blocked requests must be denied by policy.
Platform/core safety phases still validate real URI and resource behavior.

## Bounds and evidence

Default ModelLimits: 1,000,000 nodes, depth 256, 64 MiB source bytes and 64 MiB
wire bytes. Callers may lower/adjust limits deliberately. The wire-byte ceiling
is checked before decoding; source length before fingerprinting; node count
before graph indexing. Typed DTO allocation is the caller's responsibility.
These are defensive ceilings, not accepted memory/performance budgets.

Five original fixtures cover all 24 kinds: Unicode/CRLF, GFM tables/tasks and
footnotes, metadata/links/images/denied requests, block containers/breaks/code,
and advanced inline/block fallbacks. They were authored in this repository
under Apache-2.0; no external fixture was copied. Their source text stays exact
via `.gitattributes`, whose fixture-scoped whitespace rule permits intentional
CRLF and trailing-space Markdown hard breaks. Ordinary code retains whitespace
checks. Each JSON embeds its source length/hash. The original
FEAS corpus and provenance manifest are unchanged.

Reproduce from the repository root:

```sh
cargo test --workspace --all-features --locked
cargo run --locked --example export_schema --features schema > build/document.schema.json
python tools/validate_model.py
```

Tests compare generated/checked-in schemas, typed JSON round trips and original
UTF-8 slices, and reject malformed versions, IDs, fields, graph shapes, table/
outline/capability mismatches, invalid spans, unsafe intent categories and
limit violations. Goldens are independently checked with JSON Schema in Python.
None of this establishes native selection, accessibility, parsing conformance,
C ABI layout or performance budgets. Those remain their named future phases.
