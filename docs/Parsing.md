# Basic parsing and safety contract

CORE-003A adds `parse_markdown(source, document_id, revision_id, limits)` to
`markdown-core`. It returns a validated immutable semantic v1 `Document` or a
structural `ParseError`. Source is exact UTF-8 supplied by the caller. The core
performs no filesystem, network, URI activation, rendering or process calls.
The [model contract](../specs/semantic-model/v1/README.md) remains frozen and
unchanged. Snapshot IDs are deterministic ordinals; CORE-004A must reconcile
identities across revisions before an adapter relies on persistent node IDs.

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
includes `mailto:`. An unrepresentable source mapping fails the whole parse
rather than publishing an unchecked model.

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
Controls, whitespace, backslashes and percent encoding are denied. Links permit
nonempty `#fragment`, http/https/mailto prefixes with nonempty remainder, or a
relative destination with no colon, absolute prefix or dot/dot-dot segment.
Every other link is a denied intent. Images permit only that relative subset;
remote/absolute/unsafe images remain denied intents with readable alt text.

This deliberately blocks some legitimate names/URLs (spaces, percent encoding
and parent-relative navigation) until CORE-003B's URI/conformance work. It does
not validate full URL syntax, normalize paths, grant permissions or prove the
safety of a platform URL launcher. Native adapters must revalidate external
activation and decode/resolve local resources under actual OS capabilities,
including symlinks, containment, device paths and permission revocation. A
`requires_capability` intent alone never authorizes I/O. Blocked targets are
retained as untrusted values, never copied into shell commands or HTML.

## Explicit phase boundaries

Only a first-byte `---` line, terminated by a later exact `---` or `...` line,
is recognized as front matter (LF/CRLF). Its full source span is retained with
a diagnostic and front-matter capability; properties remain empty. Presenters
must keep that source span readable while property parsing is deferred. D-015
delimiter precedence also applies when the body is not a YAML mapping; for
example a heading between initial delimiters belongs to deferred front matter.
Unterminated
or mid-document delimiters follow CommonMark. No YAML metadata parser option
is enabled, preserving D-015's first-position rule. CORE-003B owns typed property
parsing, syntax errors and the remaining extension rules.

Resolved reference links/images use explicit inline source fallbacks and a
`reference_link_deferred` diagnostic. V1 intent spans must be inside the
occurrence, while reference destinations live in a separate definition; this
phase does not invent a destination span or change the frozen model. CORE-003B
must settle reference source representation/compatibility and activation.
Unresolved references remain ordinary CommonMark text. Reference definitions
are parser syntax and need not appear as body nodes, as with other delimiters.

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
| Copied text | 32 MiB | Cumulative copied source-derived strings, including repeated destinations/titles |

Limits are caller-adjustable defensive ceilings, not release performance
budgets or allocator quotas. Parser construction can scan/allocate for the
entire bounded source before it emits an event. The node/event/depth ceilings
do not interrupt that initial scan. Derived strings and string appends can
have temporary/spare capacity; static diagnostics and IDs have additional
bounded overhead. Model validation independently verifies the result; wire
limits apply to decoding, not to serialized output size.

Errors never echo source, destinations or caller identifiers. No deadline,
mid-parse cancellation or incremental update is promised in this phase;
CORE-004A owns session/generation cancellation. Host applications must schedule
parsing away from the UI thread. CORE-003B owns hostile-input fuzz/property
coverage and measured parser time/memory budgets; native launch/scroll/memory
acceptance remains in platform phases.

## Reproduction and evidence

Run `python tools/check.py all` with the toolchain in
[Development.md](Development.md). Parser regressions run in default and schema
builds, debug and release, through the existing three-host CI matrix. They cover
basic and malformed syntax, exact spans, unsafe/obfuscated destinations, HTML,
limits, duplicates, all eight frozen corpus inputs and an original serialized
[golden](../specs/parsing/basic/expected.json) from
[its source](../specs/parsing/basic/input.source.txt), authored here under Apache-2.0.
The frozen model goldens/schema must remain unchanged. An independent read-only
safety/source-mapping review found and verified the heading HTML-label fix;
its final pass found no remaining actionable issue. This does not replace
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
