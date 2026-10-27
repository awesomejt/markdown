# Semantic document model v2

Status: opt-in CORE-003B2 contract under phase-PR review; freeze at owner-authorized
merge. The owner selected additive v2 with explicit reference provenance while
retaining v1. Enable Rust feature `model-v2`; use `parse_markdown_v2` and
`markdown_core::v2::{Document, DocumentInput}`. Default `parse_markdown`, root v1
APIs, v1 schema and v1 goldens remain intact. No default-format switch is made.

The [schema](document.schema.json) has ID `urn:markdown:semantic-document:2`.
Shared IDs, nodes, metadata, outline, diagnostics, capabilities, UTF-8 spans,
source fingerprint and immutable validated-boundary rules follow
[v1](../v1/README.md). V2 changes the model version and link/resource intent
shape; it does not reinterpret v1 fields. It adds no UI/FFI type or parser arena.

## Explicit source provenance

Each link/image occurrence still owns exactly one unique intent. Occurrence
spans live on the referring Node. Each intent has original parser-decoded
`destination`, a typed `target`, and one source variant:

| Kind | Fields and constraints |
| --- | --- |
| `inline` | `span` is the destination lexeme inside the occurrence |
| `reference` | `definition` is the full selected definition span; `span` is the destination lexeme within it, separate from the occurrence |

Definitions may precede/follow occurrences and be reused. Each occurrence has a
distinct intent ID; shared source spans do not imply shared ownership/activation.
Full, collapsed and shortcut reference matching, Unicode case folding, escapes,
multiline definitions and first-duplicate precedence use pinned pulldown-cmark.
Unresolved labels stay ordinary source text. Nested links remain inert inline
fallbacks; linked images retain their own resource intent. Raw HTML and advanced
engine fences keep their inert source fallback policy.

All spans are exact half-open original UTF-8 ranges. Delimiters/normalization
are not invented; reference spans never get forced into a link node. Quote/list
continuation markers are excluded from destination lexemes, including multiline
link/image destinations. CRLF/NFC/NFD and original source hashes are preserved.

## Validation and proof scope

Only `Document::new`/`from_json` publishes a v2 Document. Strict decoding rejects
unknown fields/kinds/versions, duplicate capabilities/keys and malformed enums.
Validation shares v1's version-independent tree/source/metadata checks using an
internal unchecked structural DTO; it never publishes a synthetic v1 Document.
V2 independently checks intent IDs/uses, span geometry and target policy.

Intent validation reparses bounded source into an expected-intent map, keyed by
occurrence span and link/image kind. Destination strings, selected definitions
and exact lexemes must match the parser's facts. Escaped/entity lexemes are
checked with the pinned parser's decoding rules. A forged destination with a
self-consistent target, or another unrelated definition containing the same
string, is rejected. Source association alone is not accepted as activation
provenance. A caller can always choose a stricter denied target.

This proves intent provenance under the pinned parser/options, not the fidelity
of every display-text/metadata value, a signature, trust in user documents, OS
permission, renderer safety or full CommonMark conformance. Metadata/text remain
untrusted plain values. Native adapters must keep their own safe presentation
and activation/capability boundaries.

The proof pass is nonrecursive over the finite event iterator and bounded source;
it caps source depth and fact count with ModelLimits and copied destination facts
at 64 MiB. Node count is not used as an event budget: captured image alt can have
many events but one published node. Parser `max_events` still applies to the
primary parse. V2 primary parsing charges retained original/normalized target
strings to its copied-text budget. Temporary URL/IDNA/decoding and proof-map
allocations are additional bounded work, not an allocator quota or deadline.
`from_json` checks wire size before decoding. No-intent documents need no proof
map. Structural/provenance errors remain private codes, not source transcripts.

The known upstream tab-offset child/sibling failures keep a fresh validated inert
whole-body fallback, dropping body intents/outline while preserving metadata,
source and diagnostics. Other structural/provenance errors fail closed.

## Migration and compatibility

V1 and v2 decoders reject each other's wire versions/shapes. No auto-upgrade,
field reinterpretation or lossy downgrade exists. Explicit
`v2::Document::from_v1(validated_v1, source, limits)` revalidates under v2,
preserving node/document/revision IDs, source, metadata and outline. Existing
inline provenance is retained; a legacy denial remains `legacy_policy` denied,
even when the new URI policy would permit it. New constraints can reject a
legacy snapshot whose source/provenance is inconsistent; callers retain the
v1 API for such data rather than silently repairing it.

Reference fallback nodes in an old v1 snapshot do not magically become active
links on upgrade. Reparse the original source through `parse_markdown_v2` to
obtain reference intents. Stable cross-revision identity remains CORE-004A;
facade negotiation/package consumers remain CORE-006A/platform work. Future
adapters must explicitly select/advertise support before adopting v2.

## URI targets

[URI and resource policy](../../../docs/URI-Policy.md) defines pure classification:
fragment, decoded-once capability path plus optional decoded fragment, canonical
external URI, or denied reason. These are value types, never I/O grants.
Original decoded destinations and exact source lexemes remain available alongside
normalized targets; UI must not mistake normalized values for original source.

## Evidence and regeneration

The original Apache-2.0 [reference fixture](fixtures/references.source.txt) and
[golden](fixtures/references.json) exercise reference forms/images, Unicode,
spaces, IDN/external/mail/fragment targets, and denied script/remote images.
Their bytes are pinned via `.gitattributes`. Typed Rust roundtrips/schema snapshots
and independent Python schema/hash/version checks validate the artifacts.

```sh
cargo test --locked --features model-v2
cargo test --locked --all-features
cargo run --locked --features schema,model-v2 --example export_schema_v2 > build/model-v2.schema.json
cargo run --locked --features model-v2 --example parse_markdown_v2 < specs/semantic-model/v2/fixtures/references.source.txt > build/model-v2.json
python tools/validate_model.py
```

Only deliberately supplied authored input belongs in artifacts. Never commit or
log private user snapshots. Required hosted Linux/macOS/Windows checks establish
portable Rust behavior, not native file permissions, accessibility, packaging
or performance acceptance. CORE-003B4 retains full conformance/fuzz/performance
and residual source-offset work.
