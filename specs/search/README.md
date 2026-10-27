# Document find and reading metadata

Status: CORE-005A contract under phase-PR review; freeze at owner-authorized
merge. Rust module `markdown_core::search`
([source](../../core/crates/markdown-core/src/search/mod.rs)) implements it
over one validated [v1](../semantic-model/v1/README.md) or
[v2](../semantic-model/v2/README.md) document or a published
[session](../session/README.md) snapshot. It is pure: no I/O, threads, clocks,
locale data, wire-model change or new dependency. V1/v2 APIs, schemas and
goldens are unchanged.

Out of scope here: C ABI exposure (CORE-005A2, owner-approved split, below),
workspace/multi-file search (CORE-005B) and native find UI, highlighting and
navigation commands (each platform's 001F phase).

## API

| Item | Role |
| --- | --- |
| `SearchDocument` | Borrowed document plus its exact source: `from_snapshot`, `from_v1(doc, source)`, `from_v2(doc, source)` |
| `find` / `find_cancellable` | Literal query to `FindResults` (`matches`, `truncated`) |
| `FindOptions` | `case_sensitive` (default false), `whole_word` (false), `max_matches` (10,000; 1..=1,000,000) |
| `FindMatch` | `unit` block node, covering `source` span, `exact`, per-node `segments` |
| `MatchSegment` | `node`, `text_start..text_end` in that node's display text, `source` span, `exact` |
| `FindResults::next_from` / `previous_before` | Index of the first match at/after, or last before, a source offset (no wrap) |
| `read_stats` / `read_stats_cancellable` | `ReadStats`: total words and seconds, rate, sections |
| `ReadOptions` | `words_per_minute` (160; 60..=600), `include_headings` (false), `include_code` (false) |
| `SectionStats` | `heading` node, `outline_index`, `level`, own and cumulative words/seconds |
| `estimated_seconds`, `word_count`, `fold_case` | The documented rules as standalone functions |
| `SearchError` | `SourceMismatch`, `QueryTooLong`, `InvalidOptions`, `Cancelled` |

`from_v1`/`from_v2` check the source length and SHA-256 (an O(n) hash) and
fail with `SourceMismatch`. Snapshots already own their source. Node indices
in results index the model's node array (`document.input().nodes`, also the
C ABI view's node order). They are snapshot-local (D-021). To keep a position
across generations, convert `match.source.start` with `Snapshot::anchor_at`.
Errors carry no document contents or query text.

## Visible text

Results are computed over the document's visible semantic text, not raw
Markdown source. Each match is then mapped back to exact source bytes. Text is
grouped into leaf **units** in tree reading order: paragraph, heading, table
cell, code block and unsupported block. Container blocks (block quote, list,
item, table, row, footnote definition, alert) are descended. Matches and words
never cross units. Inside a unit, inline formatting and link boundaries are
transparent, so `foo **bar**` contains `foo bar`.

| Content | Find | Word count |
| --- | --- | --- |
| Text, emphasis/strong/strikethrough/link text | Yes | Yes |
| Headings | Yes | Only `include_headings` |
| Table cells, list items, block quotes, alerts, footnote definitions | Yes | Yes |
| Code spans, code blocks | Yes | Only `include_code` |
| Image alt text | Yes (alt is the accessible and fallback text) | No |
| Unsupported block/inline (raw HTML, math/diagram fences) | Yes, as literal source | No |
| Soft/hard breaks, whitespace | One space | Separator |
| Footnote reference labels | No (separator) | No (separator) |
| Front matter, link destinations, titles, intents | No | No |

Advanced fences (`mermaid`, `d2`, `math`, `latex`) are unsupported blocks in
the model, so `include_code` never counts them. This matches markdown-viewer.
Excluded inline content, such as a code span when code is not counted, acts as
a word separator.

## Source mapping

Every piece of display text maps to source in one of two ways:

- **Exact:** the display text is a byte-for-byte copy of a source range. This
  applies to text nodes whose text equals their span slice (escapes are
  outside text spans), code-span content between its backtick runs (with one
  stripped space), plain image alt directly after `![`, unsupported source, and
  each code-block line that is a suffix of its source line (after the fence,
  indentation or `>`/list prefixes), plus its LF line ending. Whether a block
  is fenced comes from the model (`info` is present only for fenced blocks),
  never from its text, so an indented block whose first line is ```` ``` ````
  still maps line by line.
- **Atomic:** the display text is derived. Examples are entity and character
  references (`&amp;` displays as `&`), code spans whose line endings became
  spaces, alt text that contained markup, and code lines with expanded tabs.
  Any matched part maps to the whole node span, or to the code line. CRLF
  line endings normalized to LF in code text map to the source line ending.
  This applies to whitespace too: a space derived from a code span's line
  ending is atomic.

`FindMatch.source` is the smallest range covering every matched character. It
includes any markup interleaved within the match, for example `foo **bar`.
`exact` is true when every matched character, whitespace included, maps to
its own source bytes; a soft/hard break maps to the break itself. Any
character from an atomic piece clears it. Whitespace is compared in
normalized form, so `foo\r\nbar` satisfies `foo bar` and stays exact.
Segments split a match by node in reading order. An exact segment's source
slice equals its display-text range byte for byte, so one node yields several
segments when source bytes such as a `>` prefix lie between its lines;
`FindMatch.exact` equals "every segment is exact". Each segment has a
range in that node's display text: `text` for text, code spans and code
blocks; `alt` for images; the source slice for unsupported nodes. Breaks have
an empty range at 0. Native presenters highlight from segments (display
coordinates). They navigate by **match index** and use `source` only as a
coarse anchor: several matches inside one atomic node (for example two `a`
in a code span whose line ending became a space) share the same `source`, so `next_from`/`previous_before` and `Snapshot::anchor_at`
cannot tell them apart; segment text offsets can. All spans and text ranges
lie on UTF-8 boundaries, inside their unit and node spans.

## Matching rules

- **Literal:** no regex or wildcard syntax. Matches are non-overlapping and
  leftmost-first (`aa` in `aaaa` matches twice). They are returned in reading
  order with stable indices for a given snapshot, query and options.
- **Whitespace:** in both query and text, every run of Unicode `White_Space`
  (spaces, tabs, NBSP, CR/LF) and every soft/hard break collapses to one
  space. Invisible format characters (`is_format`: soft hyphen U+00AD, word
  joiner U+2060, BOM U+FEFF, bidi marks and controls and similar Cf
  characters) are skipped in both, so `foobar` matches `foo\u{AD}bar` and the
  match span still covers the skipped bytes. ZWSP, ZWNJ and ZWJ are not
  skipped. An empty or whitespace-only query returns no matches without error.
  Queries over 4,096 UTF-8 bytes fail with `QueryTooLong`.
- **Case:** `case_sensitive` compares scalars exactly. Otherwise each scalar
  passes through `fold_case`. This is Unicode simple case folding (status C+S):
  single-scalar lowercase plus an explicit table of exceptions (final sigma,
  Greek symbol variants, long s, micro sign, iota subscript and rounded
  Cyrillic letters). It uses the Rust std Unicode tables (Rust 1.99.1). A
  development check against Python's Unicode 16.0 `casefold` found no
  single-scalar class difference. The fold has no locale tailoring and no full
  folding: `ẞ` matches `ß` but `ß` does not match `ss`. Turkish `ı`/`İ` match
  only themselves, and `I` matches `i`. Matching never changes byte lengths in
  results, because spans come from the source.
- **No Unicode normalization:** NFC `é` (U+00E9) does not match NFD `e` +
  U+0301, and the reverse is also true. The model preserves NFC/NFD and adds no
  normalization tables. Hosts that need normalization-insensitive find must
  wait for the proposal below. They must not normalize the source.
- **Cluster boundaries (approximate):** a match may not start or end inside a
  grapheme cluster. It is rejected when the next character is a combining
  mark, ZWJ/ZWNJ, variation selector, emoji modifier or tag, or a
  Brahmic/Southeast-Asian sign. It is also rejected when it follows or ends
  with a ZWJ, or when it would split a regional-indicator (flag) pair.
  `cafe` does not match inside NFD `café`, and `👨` does not match inside a
  ZWJ family. Boundary checks use the original scalars, not case-folded keys:
  U+0345 folds to `ι` but stays a combining mark, so `ι` does not match it
  alone and `α` does not match inside `α\u{345}`. Hangul jamo sequences and
  full UAX #29 rules are not modelled.
- **Whole word:** when a query edge character is a word character, the
  adjacent text character must not join with it. Word characters are letters,
  digits, katakana, connector punctuation and attached marks. Ideographs and
  Hiragana are words of their own, so `中文` is a whole word inside `我中文好`.
  `cat.` matches `Cat.`; `cat` does not match `cats`, `cat_s` or `concat`.
  Edges use the word counter's mid-letter/mid-number context, so `cat` does
  not match in `cat's`, nor `3` in `3.14`, nor `000` in `1,000`.
- **RTL and bidi:** matching is in logical (source) order. Visual reordering
  and highlight geometry belong to the native text system.

## Word segmentation

Word counting is deterministic and locale-independent. It is a simplified
UAX #29 word-boundary rule over each unit's countable text:

- Letters (`Alphabetic`) and decimal-like digits form words. Letters and
  digits join each other (`abc123`). Connector punctuation such as `_` joins
  runs (`foo_bar`). Marks and joiners extend the previous character.
- `'`, `.`, U+2019 and similar join letter-letter or digit-digit (`don't`,
  `e.g`, `3.14`). `:` and `·` join letters, and `,`/`;` join digits
  (`1,000.5`). Hyphens, dashes, slashes, quotes and other punctuation separate
  words.
- Each Han ideograph or Hiragana scalar is one word. A Katakana run is one
  word. Hangul words are space-separated letter runs.
- Emoji, symbols, superscripts, fractions and enclosed numbers are not words.
- Format characters (`is_format`, such as a soft hyphen) are ignored like
  marks, so `foo\u{AD}bar` is one word; ZWSP separates words.
- Inline formatting boundaries do not split words (`foo**bar**` is one word).

Parity with markdown-viewer (`web/src/markdown/readTime.ts`, D-32, at
`b881034`), which counts `Intl.Segmenter` `isWordLike` segments, is as
follows. Latin, Cyrillic, Greek, Hangul, Arabic, Hebrew and Devanagari prose,
contractions, abbreviations, numbers and emoji agree on the regression cases.
The deliberate deviations are:

| Case | markdown-viewer | Core |
| --- | --- | --- |
| Chinese/Japanese | ICU dictionary words (`中文字符` is 2) | One per ideograph/Hiragana scalar (4), the common per-character CJK convention |
| Thai, Lao, Khmer, Myanmar | ICU dictionary words | One per space-separated run (undercounts) |
| Markup inside a word | Counts each mdast text node separately | Counts the unit's joined text |
| Alerts and math | Alert titles and math are skipped | The current basic parser has no alert/math nodes (CORE-003B3), so `[!NOTE]` and `$...$` count as text until B3 |
| Footnotes | GFM footnote references are not counted (`[^1]` plus definition `x y` is 2 words) | The basic parser does not enable footnotes yet, so `[^1]` and the definition label are literal text (4 words) until CORE-003B3; footnote nodes already follow the rules above |

markdown-viewer's options, defaults and exclusions match: 160 wpm, headings
and code excluded by default, front matter, HTML, images and math never
counted, and advanced fences never counted.

## Reading time and sections

- `words_per_minute` must be an integer in 60..=600 (markdown-viewer's
  bounds); otherwise `InvalidOptions`. The default is 160, the owner's measured
  script-reading speed (markdown-viewer D-32).
- Seconds are `round(words × 60 / wpm)`, with exact halves rounded up,
  computed in integers (`estimated_seconds`). markdown-viewer computes
  `Math.round(words / wpm * 60)` in floating point, where an exact half can
  land just below .5: across 60..=600 wpm and 0..=20,000 words, 1,317 pairs
  differ by one second (34 at 160 wpm; for example 164 words is 62 s here and
  61 s there). The core keeps the exact integer rule. Totals use total words,
  not a sum of rounded sections. Formatting such as "about 8 min" is native UI.
- Sections are the intro (text before the first heading, always present,
  level 0) followed by one section per heading of any level, in reading order.
  This includes headings nested in block quotes or list items. `words` covers
  a section's own content up to the next heading of any level, which is
  markdown-viewer's per-section count. `cumulative_words` adds the deeper
  subsections up to the next heading of the same or higher level. Use it for
  outline rows that represent a collapsed subtree. The intro's cumulative
  value equals its own. Heading text counts in the section it starts, and only
  with `include_headings`.
- `outline_index` is the matching `outline` entry. The validator guarantees
  one entry per heading in reading order.

## Bounds, cancellation and cost

Work is linear in the snapshot's visible text plus query length. The search
uses Knuth-Morris-Pratt over normalized scalars, so long queries cannot cause
quadratic scans. Results are capped by `max_matches`: when one more match
exists, `truncated` is set and the search stops. Each unit's text is
flattened into a transient buffer of about 24 bytes per scalar, which is about
24 MiB for a 1 MiB single-line paragraph, and dropped after the unit. Word
counting streams without that buffer. The cancellation token is polled before
work, per unit and every 16,384 scalars. Cancellation returns only
`Err(Cancelled)`, never partial results. These are defensive bounds, not
accepted search budgets; R4 throughput measurements belong to the native and
CORE-003B4 performance phases.

## Evidence

`core/crates/markdown-core/tests/search.rs` covers the following, run by
`tools/check.py core` in debug and release with default, `model-v2` and
all-feature builds:

- Literal and case rules: German ß/ẞ, Greek sigma, Kelvin and micro signs,
  Cherokee and Turkish i.
- Cross-inline matches, whitespace, breaks and CRLF.
- Entity atomic mapping, code spans, CRLF code blocks and quoted code blocks,
  indented blocks that look fenced, and atomic whitespace clearing `exact`.
- Original-scalar cluster checks (U+0345), mid-letter/number whole-word
  edges, format characters and shared coarse sources inside atomic nodes.
- 4,096 deterministic generated markup-heavy inputs (300,000 run locally)
  against the strict exactness and range invariants.
- Images, raw HTML and unsupported fences. Front matter is excluded.
- The repository's NFC/NFD Unicode fixture, emoji ZWJ, skin-tone and flag
  sequences, Hebrew/Arabic and CJK.
- Whole word, empty and over-long queries, option bounds and source mismatch.
- Truncation, navigation helpers and pre-cancelled and concurrent
  cancellation.
- 1 MiB single-line and 20,000-paragraph documents.
- Invariants for every word of the 14 corpus and spec sources and all five v1
  goldens, which cover all 24 node kinds. The invariants are UTF-8
  boundaries, containment, reading order, exact-segment source equality and
  segment round trip to the normalized query.
- Snapshot/document/v1/v2 parity and anchor round trips from match offsets.
- Word segmentation cases against `Intl.Segmenter` values (Node 24.14), with
  the deviations listed above.
- Inclusion rules, own and cumulative sections, rounding and rate bounds.
- Word and section goldens for the corpus.

## Decided follow-up and open proposals

1. **C ABI exposure** is decided: the owner split it into CORE-005A2 under
   [D-022](../../agents/Decisions.md). ABI v1 grows only by new functions that
   fill caller-owned arrays (for example `md_snapshot_find` and
   `md_snapshot_read_stats` over a snapshot handle). Each new record struct
   has a header size constant checked through `struct_size`. There are no new
   handle types and no changes to `md_abi_info` or `md_live_handles`. Without
   a cancel handle, FFI find is bounded by `max_matches` and the snapshot
   limits. Each platform's 001F depends on it.
2. **Normalization-insensitive find:** canonical-equivalence matching needs
   NFC/NFD data. `icu_normalizer` is already in the optional `model-v2` graph,
   so an opt-in mode could reuse it rather than add a package.
3. **Dictionary segmentation for CJK and Southeast Asian scripts** would need
   ICU segmenter data (a large dependency) or platform segmenters. That would
   make counts differ between platforms. The per-character rule is kept until
   the owner decides.
