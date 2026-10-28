# Workspace search primitives

Status: CORE-005B contract under phase-PR review; freeze at owner-authorized
merge. Rust module `markdown_core::workspace`
([source](../../core/crates/markdown-core/src/workspace/mod.rs)) implements it
on top of [document find](../search/README.md) and
[session anchors](../session/README.md). It is pure: no I/O, threads, clocks,
locale data, wire-model change or new dependency. V1/v2 APIs, schemas,
goldens and the C ABI are unchanged.

Out of scope here: the C ABI encoding (CORE-005B2, owner-approved split),
folder enumeration, watching, indexing and persistence (platform adapters),
and the workspace search UI with result navigation (each platform's 002C).

## Capability model

The platform adapter owns folder access, permissions (security-scoped
bookmarks, portals, broker handles), enumeration, file reads, change watching
and scheduling ([Architecture](../../docs/Architecture.md)). The core receives
capabilities, not paths:

- A `WorkspaceEntry` is an adapter-chosen opaque `id: u64` plus a
  `WorkspacePath`. The ID is how the adapter finds its own file capability
  again. The core only copies it into results.
- A `WorkspacePath` is a `/`-separated label relative to the workspace root.
  It is used for display, filtering and file-name matching. It grants no
  access, and the core never resolves, opens or normalizes it.
- For content search, the adapter reads a file under its own permissions and
  passes the bytes. The core never asks for a file. It only answers whether a
  file is worth reading.

Adapters should build display paths with their platform's string APIs. They
may pass names in NFC to make name search insensitive to how the file system
stores the name (see [limits](#unicode-limits)).

### Path validation

`WorkspacePath::new` validates without changing the string. Errors carry no
path text.

| Rule | Error |
| --- | --- |
| Non-empty, at most 4,096 UTF-8 bytes | `Empty`, `TooLong` |
| At most 256 components | `TooDeep` |
| Relative: no leading `/` | `Absolute` |
| No empty component (`//`, trailing `/`) | `EmptyComponent` |
| No `.` or `..` component | `DotComponent` |
| No control character (Cc, including DEL and C1) and no bidi formatting character: the ALM, LRM and RLM marks (U+061C, U+200E, U+200F), embeddings and overrides (U+202A–202E) and isolates (U+2066–2069) | `Control` |

Backslash, colon and other characters that are legal in POSIX names are
ordinary characters. Windows adapters convert `\` separators to `/` before
building a path. An adapter skips entries it cannot represent. A rejected name
is never shown.

Other invisible format characters are accepted, because rejecting them would
hide real files, but `WorkspacePath::has_invisible()` reports them. It uses the
Unicode 16.0 General_Category=Cf table shared with
[front matter](../../docs/Front-Matter.md), except ZWNJ and ZWJ, which emoji,
Persian and Indic names need. Examples are U+200B ZWSP, U+FEFF, U+2060 and the
soft hyphen. Such a name can display exactly like another name, so the UI
should mark it.

### Shared entry filter

Every host uses the same rules, matching markdown-viewer `fsroot.ClassifyName`
and FR-1.4 at `b881034`:

- **Markdown:** the final component's extension is `md`, `markdown` or `mdx`
  (ASCII case-insensitive). A leading dot does not start an extension, so
  `.md` is not Markdown, and neither is `a.md.txt`.
- **Hidden:** any component starts with `.`. Hidden entries are excluded
  unless `include_hidden` is set (default off).

## File-name search

`find_names(entries, query, options)` and `find_names_cancellable` do a
literal search of each entry's display path.

| Option | Default | Range |
| --- | --- | --- |
| `case_sensitive` | false (simple case folding, as find) | |
| `include_hidden` | false | |
| `markdown_only` | true | |
| `max_results` | 200 | 1..=10,000 |
| `max_entries` | 100,000 | 1..=1,000,000 |

- The query is trimmed and folded. Otherwise it is literal: inner whitespace
  and format characters must match, because names can legitimately contain
  them. A query may contain `/` to match across folders. An empty query is
  `EmptyQuery`. A query over `MAX_QUERY_BYTES` (4,096) is `QueryTooLong`.
- **Rank:** `NamePrefix` when the file name starts with the query, `Name`
  when it occurs elsewhere in the file name, and `Path` when it occurs only
  in the folder part or across a `/`. Hits are ordered by rank, then path
  bytes, then entry index. The order is deterministic and independent of
  input order, apart from exact duplicate paths.
- Each hit reports the entry index, its ID, the rank and the byte range of
  the first occurrence in the path. For name ranks this is within the file
  name. Ranges are on UTF-8 boundaries because folding maps one scalar to one
  scalar.
- `total_hits` counts every matching entry examined. `truncated` is set when
  more exist than `max_results`. Only the first `max_entries` entries are
  examined, and `entries_limited` reports a longer slice. Skips are counted
  per filter.
- **Cost:** O(total path bytes + query) per call, plus O(hits × log
  max_results). Memory is O(max_results + longest path). KMP matching keeps
  long queries linear. Cancellation is polled every 1,024 entries and returns
  only `Err(Cancelled)`.

## Content search

A `ContentQuery` holds a validated query and `ContentOptions`. It is
immutable, `Send` and `Sync`. A caller-held `ContentProgress` value carries the
whole search's accounting from file to file:

```text
query = ContentQuery::new(text, options)?            // validate once
progress = ContentProgress::default()
for entry in adapter enumeration:
    match query.admit(&mut progress, &entry, size):  // before reading
        Read    -> bytes = adapter.read(entry)
                   outcome = query.search(&mut progress, &entry, &bytes, &token)?
        Skip(_) -> continue
        Stop(_) -> break
```

`search` re-checks admission with the real byte length, so `admit` is an
optimization that avoids reading files that would be skipped or would exceed
a budget. It is not a safety requirement, and a wrong size estimate is safe.
The progress value is plain data: it holds no handle, lock or pointer. This is
the shape CORE-005B2 needs under [D-022](../../agents/Decisions.md).

### Per-file processing

1. Filters: `Skipped(Hidden)` and `Skipped(NotMarkdown)`. Content search is
   always Markdown-only.
2. `Skipped(TooLarge)` when the file has more than `max_file_bytes`.
3. Budgets (below) may return `Stopped(reason)`.
4. `Skipped(InvalidUtf8)` for invalid UTF-8. A BOM is handled by the parser.
5. Bounded v1 parse with the default `ParseLimits`, with `max_source_bytes`
   set to `max_file_bytes`. A `ParseError` (nesting, node, event or text
   limit) is `Skipped(ParseFailed)`, because a document the reader cannot open
   is not searched. Cancellation is honoured.
6. [Document find](../search/README.md) with the query, `case_sensitive`,
   `whole_word` and `max_matches = min(max_matches_per_file, remaining total)`.
   Matching, visible text, Unicode rules and source mapping are exactly those
   of find.
7. Each match becomes a `ContentHit`, and the file returns
   `Searched(DocumentHits)`. Its `hits` may be empty.

Pre-read skips (`Hidden`, `NotMarkdown`, `TooLarge`) do not consume the file
or byte budgets. Every file that passes admission in `search` is charged to
`files_read` and `bytes_read` before decoding. That includes files later
skipped as `InvalidUtf8` or `ParseFailed`, so a workspace of unreadable or
unparseable files still reaches `Stop(Files)` or `Stop(Bytes)`. Each skip is
counted once in `progress.skipped` per reason.

### Results

| Field | Meaning |
| --- | --- |
| `DocumentHits.id` | The entry's adapter ID |
| `DocumentHits.source` | `SourceInfo` (length and SHA-256) of the searched bytes |
| `DocumentHits.truncated` | More matches exist in this file than `hits` holds |
| `ContentHit.unit`, `source`, `exact` | As find's `FindMatch`; `unit` is snapshot-local (D-021) |
| `ContentHit.line` | 1-based line of `source.start`, counting LF (a CRLF counts once) |
| `ContentHit.snippet` | Bounded visible-text excerpt (below) |
| `ContentHit.anchor` | Session anchor taken at `source.start` |

**Navigation:** when the user opens a hit, the adapter loads the file into a
document session. If the snapshot's `source_info()` equals
`DocumentHits.source`, the file is unchanged and `source` and `unit` apply
directly. Otherwise, it resolves `anchor` with `Snapshot::resolve` (exact,
relocated or nearest), as for reading position. Anchors come from a
standalone generation-1 parse, and their `generation` field is informational.
Every block that can hold a match is anchorable, so `anchor` is always `Some`.
The `Option` makes that invariant explicit.

### Snippets

A snippet is one line of visible text from the hit's block: the same text
find searches, not raw Markdown. Inline markup is gone, entities are decoded,
image alt text is shown and destinations are not. Whitespace runs, line
breaks, code-block line endings and footnote-reference separators collapse to
one space. Other control characters, which find matches literally, are shown
as U+FFFD. Format characters are dropped, as find skips them.

- `text[match_start..match_end]` (byte offsets) is the matched text, so
  `normalize(match) == normalize(query)` under find's rules.
- Up to `snippet_context` characters (default 40, at most 200) on each side,
  never crossing the block. Edge spaces are trimmed. `leading_more` and
  `trailing_more` report omitted text in the same block, so the UI can draw
  an ellipsis.
- A match longer than `MAX_SNIPPET_MATCH_CHARS` (256) is clipped. The snippet
  then ends at the clip, and `trailing_more` is set.
- A snippet holds at most 2 × 200 + 256 characters. Snippets are built in one
  streaming pass per matched block, so their cost is linear in that block's
  visible text plus matches × (context + match), regardless of block size.

### Budgets

| `ContentLimits` field | Default | Range | When exhausted |
| --- | --- | --- | --- |
| `max_files` | 10,000 | 1..=1,000,000 | `Stop(Files)` at the next admission |
| `max_total_bytes` | 256 MiB | 1..=16 GiB | `Stop(Bytes)` when a file would exceed it |
| `max_file_bytes` | 8 MiB | 1..=8 MiB (reader parse ceiling) | `Skip(TooLarge)`; the search continues |
| `max_matches_per_file` | 100 | 1..=10,000 | File `truncated`; counted in `files_truncated` |
| `max_total_matches` | 2,000 | 1..=100,000 | `Stop(Matches)` once reached |
| `snippet_context` | 40 chars | 0..=200 | |

`ContentQuery::new` rejects out-of-range values (`InvalidOptions`), empty
queries (`EmptyQuery`, after find's normalization) and long queries
(`QueryTooLong`). A stop is permanent: `progress.stopped` is set, and every
later `admit` or `search` returns the same `Stop`. A stop means the results may be incomplete. The UI should say so
rather than present the list as exhaustive.

`ContentProgress` reports `files_read`, `files_matched`, `bytes_read`,
`matches`, `files_truncated`, per-reason `skipped` counts and `stopped`.
`files_read` includes the `invalid_utf8` and `parse_failed` skips. These are enough for a progress line and a final summary.

**Retained memory** is bounded by the results the caller keeps: at most
`max_total_matches` hits, each up to a snippet (≤ 656 characters) plus an
anchor (about 200 bytes). With defaults, that is about 2,000 × 3 KiB. Per-call
working memory is the file's bytes (copied once into the snapshot source),
its bounded parse model and find's O(query) scanner.

### Cancellation

Each call takes a `CancellationToken`. It is polled before work and by the
bounded parser, find (per block and every 16,384 characters) and the snippet
pass. Cancellation returns `Err(Cancelled)`, no partial result, and leaves
`progress` unchanged. Hosts that stop between files simply stop calling. That
is also the only cancellation available to CORE-005B2's handle-free ABI
functions, where each call is bounded by `max_file_bytes`.

### Concurrency and determinism

`ContentQuery` can be shared across threads, but accounting is sequential:
one `ContentProgress` is updated by one call at a time. A host that reads
files in parallel serializes the `search` calls, or partitions the budgets
into one progress value per worker. Sequential calls in the same entry order
produce identical outcomes and progress. Name search is deterministic for any
input order.

## Unicode limits

These are inherited from find and stated here because names make them
visible:

- No normalization. `Café.md` written precomposed does not match `Cafe` plus
  U+0301 combining acute. macOS file systems may report either form.
  Adapters can pass NFC display paths. Normalization-insensitive matching is
  find's open proposal 2.
- Simple case folding only: no locale tailoring (Turkish i) and no
  multi-character folds (`ß` does not match `ss`).

## Evidence

`core/crates/markdown-core/tests/workspace.rs` runs in `tools/check.py core`,
in debug and release with default, `model-v2` and all-feature builds. It
covers:

- Path validation for every error kind, including the bidi marks; the size
  and depth bounds; POSIX characters (backslash, colon); and errors that carry
  no path text. Invisible format characters are flagged, while ZWJ and ZWNJ
  names are not.
- Classification, including case, a leading dot and double extensions, and
  hidden detection.
- Name ranks, order, highlight ranges, filters, case sensitivity, matches
  across folders, truncation, `max_entries`, option and query errors,
  cancellation, Unicode folding (ẞ/ß, Greek), and NFC/NFD non-equivalence.
- Name search over 100,000 entries, with determinism.
- Content hits equal to find for up to 40 words in each of the 14 corpus and
  spec sources, with snippet invariants, line numbers and anchor offsets.
- Snippet text for markup, entities, breaks, code spans and blocks, alt text,
  format characters and matched control characters. Dense matches with
  overlapping context, zero context, clipped long matches, and bounded
  snippets inside an 8 MB paragraph.
- Anchors resolving exactly in an unchanged session snapshot (equal
  `SourceInfo`) and after an edit (different `SourceInfo`).
- Admission filters and skips, a wrong size estimate, invalid UTF-8, parse
  failure, sticky file, byte and match stops, per-file truncation and
  limit-range validation.
- Invalid-UTF-8 and parse-failing files exhausting the byte and file budgets:
  the PR #10 review probe.
- Pre-cancelled and concurrent cancellation, with progress left unchanged.
- A deterministic repeated run over the corpus as a workspace.
- 4,096 deterministic generated markup-heavy inputs (300,000 run locally on
  2026-10-28) checking find parity and snippet invariants. Set
  `WORKSPACE_CASES` for more.

An informal measurement (not a budget) on an Apple M5 Max with Rust 1.99.1,
release, best of five: 1,000 files of about 10 KiB markup-dense prose with no
match took 330 ms, about 29 MB/s. Parsing was 262 ms of that and find 65 ms.
Name search over 100,000 paths of about 28 bytes took 13 ms. A 5 MiB file of the
same dense prose exceeds the reader's 250,000-node parse ceiling and is
skipped as `ParseFailed`. Throughput budgets belong to the native 002C and
CORE-003B4 performance phases.

## Decided follow-up and open proposals

1. **C ABI exposure** (CORE-005B2, owner-approved 2026-10-28) follows D-022:
   new functions only, a caller-owned `ContentProgress`-shaped struct and
   record arrays, header size constants and no handles. Each platform's 002C
   depends on it.
2. **Raw-source prefilter:** most no-match cost is parsing. A safe prefilter
   would need to account for entities, escapes and folding, which can make
   visible text differ from the source bytes. Measure on real workspaces
   before adding one.
3. **Persistent indexing** is not provided. If 002C measurements show that
   re-reading is too slow, an index belongs in a new task with its own
   storage and invalidation contract.
