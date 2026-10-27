# Typed front matter

CORE-003B1 parses a bounded, explicitly restricted metadata grammar into the
existing semantic v1 value types. It is not a general YAML implementation.
No dependency, schema, URI policy, reference-link rule, engine or native UI is
changed. [Parsing.md](Parsing.md) defines the surrounding Markdown/safety policy.

## Detection and source preservation

Detection still requires a first-byte `---` line and a later exact `---` or `...`
closing line, with LF or CRLF. The full delimiter-inclusive UTF-8 source span
is retained, without newline/Unicode normalization. Unterminated, BOM-prefixed
or mid-document delimiters follow ordinary Markdown. No pulldown-cmark YAML
metadata option is enabled (D-015).

Delimiter precedence is independent of property validity: a Markdown-looking
line between initial delimiters still belongs to front matter. Empty/comment-only
metadata is an empty mapping. Body node offsets still address the original
source. The upstream tab-offset body fallback preserves typed metadata and its
diagnostics, discarding only the unrepresentable body graph/intents/outline.

## Supported grammar

A document contains flat top-level entries `key: value`, with no indentation.
A key starts with an ASCII letter or underscore and then contains only ASCII
letters, digits, underscore, dot or hyphen. No quoted keys or key trimming is
performed. A nonempty value must be separated from the colon by a space.
Blank lines and comment lines are ignored. Literal control characters, including
tabs, are unsupported. Escaped controls in double-quoted strings remain data.

| Value | Semantic type | Examples |
| --- | --- | --- |
| Empty or explicit null | Null | `key:`, `key: null`, `key: ~` |
| Boolean | Boolean | `draft: false`, `ready: TRUE` |
| Signed decimal within i64 | Integer | `count: 0`, `offset: -2` |
| Plain or quoted scalar | String | `author: Native reader`, `label: "42"` |
| Flat flow list of strings | Strings | `tags: [gfm, 'demo']` |
| Block list of strings | Strings | A key with no value followed by exactly two spaces, a hyphen, a space and a string per line |

Null/true/false accept lowercase, title-case or uppercase forms, not arbitrary
mixed case. Integers allow an optional minus. A plus, leading zeros (except zero itself),
separators, fractions, exponents, timestamps and alternate bases are rejected.
Numeric-looking unsupported values must be quoted to become strings. A scalar
`title` of type String also sets the display title; another type remains a typed
property without a display title. No title is inferred from a body heading.

Single-quoted strings escape an apostrophe by doubling it. Double-quoted strings
use exactly JSON string escaping, including Unicode escapes/surrogate pairs;
other YAML escape forms are unsupported. A hash at plain-scalar start or after
an ASCII space starts a comment. Quoted hashes stay literal. After a closing
quote/list, only spaces and an optional comment are accepted. A quoted scalar
may be followed immediately by a comment. Plain strings cannot contain flow
brackets/braces or a colon followed by whitespace/end, or start with YAML
anchor/alias/tag/block-scalar/directive/reserved indicators. This intentionally
accepts less than YAML; it never evaluates tags, aliases, scripts or templates.

Lists contain only strings: implicit numbers, booleans or nulls are rejected
rather than coerced. Quote them deliberately when intended as strings. Empty
flow lists are allowed; nested lists/maps, trailing/empty comma entries and
multiline flow/quoted scalars are unsupported. Unquoted hashes inside flow
lists are unsupported; quote those items. Block items use exactly two
spaces of indentation. An omitted scalar with no block items is null, not an
empty string or list. Quote an empty string or use `[]` explicitly.

```yaml
title: 'Café reader'
draft: false
count: 3
tags: [gfm, 'offline']
authors:
  - Ada
  - 'Grace'
```

## Atomic failure and diagnostics

Duplicate keys, unsupported syntax/types, or metadata ceilings discard the
entire property map and display title. The original source span/capability and
Markdown body remain available. Presenters must keep that source readable;
empty properties are not evidence the original metadata was empty.

Warnings are stable structural codes: `front_matter_duplicate`,
`front_matter_unsupported`, and `front_matter_limit`. Messages never echo keys,
values or source. JSON parser errors are not forwarded. Unsupported syntax is
not a claim that the source is invalid in full YAML. No partial metadata,
implicit repair or arbitrary YAML object construction is published.

## Bounds

| Ceiling | Value |
| --- | --- |
| Full front-matter span, including delimiters | 64 KiB |
| Top-level properties | 256 |
| Key bytes | 128 |
| Raw scalar token bytes, including quoting/comments when present | 16 KiB |
| Total string list items across all properties | 1,024 |

These fixed metadata ceilings yield the limit warning/source fallback. They
complement the caller-adjustable parser source/event/node/depth limits. Parser
`max_text_bytes` also charges keys, string values/list items and the promoted
title; attempted copies remain charged when metadata is later rejected. Exceeding
that shared budget returns `ParseError::TextLimit` for the whole parse, and is
never turned into a successful metadata warning. Empty mappings need no string
copy allowance.

The metadata parser walks bounded lines/tokens without recursive structures.
Decoding quoted scalars can allocate a temporary before the copy-budget check,
limited by the scalar ceiling. Model publication may clone the bounded metadata
for source-offset fallback; capacity, static diagnostics and internal clones are
not exact allocator accounting. These ceilings are not time/memory performance
budgets, a full YAML parser guarantee or native presentation evidence.

## Validation and remaining work

`cargo test --locked --test metadata` covers supported typed values, quoted
comments/escapes/Unicode, atomic duplicate/hostile rejection, each ceiling at its
boundary and one beyond, copy budgets, CRLF/exact offsets, tab-offset fallback,
the frozen GFM properties, and 4,096 deterministic generated metadata inputs.
The existing 20,000 generated-Markdown check and all parser/model goldens remain.
This bounded regression coverage is not exhaustive fuzzing.

CORE-003B remains an aggregate: B1 metadata; B2 references/URI compatibility;
B3 remaining extension semantics; B4 conformance hardening and measured budgets.
B2 retains the temporary ASCII-only URI activation limitation; B3 does not add
advanced engines or post-MVP product behavior. The parent is incomplete until
all children and its original acceptance criteria pass. Native packaging and
accessibility gates are unchanged.
