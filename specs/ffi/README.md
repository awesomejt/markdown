# C ABI v1 contract

Status: frozen at owner-authorized CORE-006A PR #7 merge (`a751f48`). The
flat-record snapshot view is a frozen ABI v1 representation (D-023), and ABI
v1 grows only under the additive rule below (D-022). The hand-written header [`core/include/markdown_core.h`](../../core/include/markdown_core.h)
is normative (D-009). Crate [`markdown-ffi`](../../core/crates/markdown-ffi/src/lib.rs)
implements it over the [session contract](../session/README.md) and the
[v1](../semantic-model/v1/README.md)/[v2](../semantic-model/v2/README.md)
semantic models. It adds no new dependency beyond the core graph, no generated
bindings, no I/O, no threads and no UI types. It enables the native B phases;
it does not by itself prove a Swift, C# or gtk-rs consumer (CORE-006 remains
open until those pass on their hosts).

## Artifacts

`markdown-ffi` builds `rlib` (Rust tests), `cdylib` (`libmarkdown_ffi.dylib`,
`.so` or `markdown_ffi.dll`) and `staticlib`. Exactly the 24 functions in the
header are exported with the `md_` prefix: the 18 from PR #7, the two
CORE-005A2 additions and the four CORE-005B2 workspace functions below. Feature `model-v2` forwards to the
core feature and adds model v2 to negotiation; the default build offers v1 only.
Packaging (install names, import libraries, XCFrameworks, signing) belongs to
each platform's B and I phases.

## Version negotiation

1. Call `md_abi_negotiate(MD_ABI_VERSION, &info)` with `info.struct_size =
   sizeof info`. A different requested version returns
   `MD_ERR_VERSION_MISMATCH` but still fills `info` with the library's values.
2. Compare every `*_size` field and `pointer_size` with the host's `sizeof`;
   stop on any mismatch. `model_versions` has bit `1 << n` for each model
   version `n` the build can produce; `anchor_version` is the anchor wire version.
3. Choose a model per session in `md_session_options.model_version`. A model
   the build lacks returns `MD_ERR_UNSUPPORTED_MODEL`. There is no implicit
   default upgrade; `md_snapshot_view.model_version` reports what was produced.

Every struct exchanged by pointer except `md_str` and the view's records
begins with `struct_size`. ABI v1 requires an exact match (`MD_ERR_STRUCT_SIZE`
otherwise) and then writes the whole struct. A future ABI version may accept
known smaller prefixes; v1 does not, so unknown sizes fail closed. Constants
are stable integers; later versions may add values, so hosts must treat
unknown status/kind codes as errors or unsupported content, not crash.

## ABI v1 growth rule (D-022)

ABI v1 grows only by adding functions. New functions fill caller-owned arrays
or structs and introduce no new handle types. Existing functions, structs,
constants, `md_abi_info` and `md_live_handles` never change. Each new struct
has a header size constant and is checked through `struct_size` on every call.
Hosts compare those constants with `sizeof` after negotiation. An older library
lacks the new symbols, so it fails at link or load time; there is no runtime
probe. Anything incompatible needs a new ABI version. CORE-005A2 is the first
phase to use this rule ([find and reading statistics](#find-and-reading-statistics-core-005a2)),
and CORE-005B2 the second ([workspace search](#workspace-search-core-005b2)).

## Ownership and lifetimes

| Value | Owner | Release |
| --- | --- | --- |
| `md_session` | core registry | `md_session_release` |
| `md_job` | core registry | `md_job_release` (also after `md_job_run`) |
| `md_snapshot` | core registry | `md_snapshot_release` |
| Arrays/strings reachable from `md_snapshot_view` | the snapshot handle | freed with that handle |
| Input bytes (`document_id`, source, tokens, anchors) | caller | copied during the call; never retained |
| Output structs, anchor buffers, find/read/workspace record, text and anchor arrays | caller | caller |
| `md_content_progress` (in/out) | caller | caller; written back only on success |
| `md_status_name` strings | static | never freed |

Handles are nonzero 64-bit registry keys, not pointers, and are never reused
within a process. Releasing 0 is a no-op; releasing or using an unknown,
already released or wrong-type handle returns `MD_ERR_INVALID_HANDLE` without
effect, so a double release is detected rather than freeing memory twice.
`md_live_handles` reports process-wide live session/job/snapshot counts for
leak checks; it never exposes contents.

- Each `md_session_current` and each published/unchanged `md_job_run` returns
  a new snapshot handle that must be released independently.
- A snapshot outlives its session and newer generations. Releasing a session
  cancels its in-flight job; that job later returns `MD_OUTCOME_CLOSED`.
- `md_job_run` consumes the job's work; a second run is
  `MD_ERR_INVALID_STATE`. Release an unrun job to clear its pending state.
- View pointers stay valid and unchanged until the snapshot handle is released.
  Handles to the same published snapshot (a job result and later
  `md_session_current` calls) share one flat encoding, built on the first
  view and found in O(1) through an address-keyed side index; it is freed
  (and its index entry pruned) when the last of them is released, and a
  later handle re-encodes on its first view.
  Hosts must not free or write them, keep them past release, or race
  `md_snapshot_release` against reads (the same rule as FEAS-006). Copy what
  the UI needs, then release (see the C smoke).
- Anchors are value copies: serialized bytes (at most `MD_ANCHOR_MAX_BYTES`)
  with no handle. They are private document-derived state; never log them.

## Threading

All functions are thread-safe. Sessions serialize internally; snapshots are
immutable and may be read from several threads at once. A job runs
synchronously on the calling thread, which should be a host worker, never the
UI thread; `md_job_cancel` and `md_session_cancel` may be called from any
thread before, during or after the run. The core starts no threads and never
calls back into the host. Registry access is a short global lock; jobs never
hold it while parsing.

## Errors and panics

Functions return `md_status`. Outputs are zeroed (keeping `struct_size`)
before other work and receive their final values in one write after all
fallible work, so every failure (including a contained panic) leaves them
zeroed. The exceptions are deliberate: `MD_ERR_BUFFER_TOO_SMALL` reports the
required length, `md_abi_negotiate` fills `md_abi_info` on a version
mismatch, and the in/out `md_content_progress` is left unchanged on every
failure. Rejected versioned structs (wrong `struct_size`) are left
unwritten. `md_status_name` returns static ASCII names; it is the one
function without `catch_unwind`, because a match over constants returning
static strings cannot panic.

Output pointers must not overlap each other, any input or any buffer. The
core never forms Rust references to caller output memory and writes it only
through raw stores, so overlap yields unspecified values, not undefined
behaviour (tested for repeated `md_live_handles` and submit outputs).
No status, name or field carries document contents, paths or tokens.

| Status | Meaning |
| --- | --- |
| `MD_ERR_INVALID_ARGUMENT` | Null/misaligned output, null input with nonzero length, out-of-range enum, limit above the ABI ceiling |
| `MD_ERR_VERSION_MISMATCH` / `MD_ERR_STRUCT_SIZE` | Negotiation or struct size mismatch |
| `MD_ERR_INVALID_HANDLE` | Unknown, released or wrong-type handle |
| `MD_ERR_UNSUPPORTED_MODEL` | Requested model not built |
| `MD_ERR_BUFFER_TOO_SMALL` | `*out_length` holds the required size |
| `MD_ERR_INVALID_STATE` | Job already run |
| `MD_ERR_INVALID_DOCUMENT_ID` / `MD_ERR_INVALID_REVISION_TOKEN` / `MD_ERR_GENERATION_EXHAUSTED` | Session contract failures |
| `MD_ERR_ANCHOR_INVALID` / `MD_ERR_ANCHOR_VERSION` / `MD_ERR_NO_BLOCKS` / `MD_ERR_UNKNOWN_NODE` | Anchor failures |
| `MD_ERR_LIMIT` | Handle counter, a table/span exceeding `u32`, or name-search paths over `MD_NAMES_MAX_PATH_BYTES` |
| `MD_ERR_PANIC` / `MD_ERR_INTERNAL` | Contained core bug |

Job outcomes (`MD_OUTCOME_*`) are results, not errors: the call returns
`MD_OK` with the outcome, the job's generation, the newest generation when
superseded and an `MD_FAILURE_*` code (invalid UTF-8, each `ParseError`
ceiling, source mapping or model validation) when failed. Session status
reports the newest failure the same way.

Every entry point runs under `catch_unwind` and maps a panic to
`MD_ERR_PANIC`; the registry recovers a poisoned lock. Build with the default
`panic = "unwind"`; with `panic = "abort"` a core bug aborts the process
instead (it still never unwinds into the host). ABI v1 has no field that
reports the panic strategy, so hosts cannot query it; packaging phases must
ship the default `unwind` build. A panic message reaches the
process panic hook (normally stderr); core panic messages must not contain
document contents.

## Sessions and anchors

The functions map one-to-one to the [session contract](../session/README.md):
`md_session_submit` copies bytes and an optional 1–256 byte revision token
(length 0 means none). Input longer than the session's source limit still
takes a generation and fails as `MD_FAILURE_SOURCE_LIMIT`, but only the limit
plus one byte is copied; `md_session_external_change` takes `MD_CHANGE_*` plus a
reason (`MD_REASON_*`, only for `MD_CHANGE_UNREADABLE`) and a token (only for
`MD_CHANGE_MODIFIED`), and returns an `MD_DISPOSITION_*` with the pending or
recorded generation. Generations start at 1; 0 means "none" in every field.
Document IDs use the model ID rules. Session limits are
`md_session_options` fields; 0 selects the core default and values above the
ABI ceilings (64 MiB source, 1,000,000 nodes, depth 256, 16,000,000 events,
256 MiB copied text) are rejected so every span and index fits `uint32_t`.

`md_snapshot_anchor_at` (absolute offset) and `md_snapshot_anchor_for_node`
(view node index plus intra-node offset) write the anchor's versioned JSON
form into a caller buffer; query the size with a zero capacity, or use a
`MD_ANCHOR_MAX_BYTES` buffer. `md_snapshot_resolve_anchor` treats the bytes
as untrusted (size-capped, strict decode) and returns quality
(`EXACT`/`RELOCATED`/`NEAREST`/`LOST`), the node index in this snapshot's
view, the intra-node offset and the absolute source offset.

## Bulk snapshot view

`md_snapshot_get_view` fills an `md_snapshot_view` with counts and read-only
pointers to fixed-size records. The flat encoding is built once per handle on
first request and reused; it holds every value of the v1 or v2 document, and
tests rebuild the exact Rust model from it. Arrays with count 0 are `NULL`.

- `source`: the exact UTF-8 bytes; all spans are `[start, end)` byte offsets
  into it, on character boundaries.
- `strings`: one UTF-8 table; every `md_str` is `(offset, length)` into it,
  not NUL-terminated. Optional strings use a flag bit; absent is not empty.
- `nodes`: one `md_node` per model node in model order. Node indices replace
  string node IDs in `roots`, `children`, `parent`, outline and diagnostics;
  `md_node.id` keeps the snapshot-local ID (not persistent, see D-021).
- Per-kind fields: `value` holds the heading level, `MD_ALERT_*` or
  `MD_UNSUPPORTED_*`; `text` holds text, code, the heading anchor, footnote
  label, unsupported feature name or image alt; `extra` holds the code info
  string or link/image title; `aux` indexes `links` (link) or `resources`
  (image), or holds the first `alignments` entry and `aux_count` the column
  count (table); `number` holds an ordered list start; `flags` carries list,
  task, header and optional-string bits.
- `links`/`resources` share `md_intent`. V1 intents have
  `MD_SOURCE_UNSPECIFIED`, map dispositions/access to `MD_TARGET_*` and put a
  denial reason in `target`. V2 intents report inline/reference provenance
  (with the definition span), typed targets (anchor, decoded capability path
  plus optional fragment, canonical URI) and `MD_DENIAL_*`. Targets grant no
  I/O; hosts apply the [URI policy](../../docs/URI-Policy.md).
- `outline`, `diagnostics`, `properties` (metadata values; string lists index
  `string_list`), `capabilities` bits, front matter span and title complete it.

All indices, spans and string offsets are `uint32_t`; a snapshot that could
not fit returns `MD_ERR_LIMIT`, which the session ceilings prevent in practice.

## Find and reading statistics (CORE-005A2)

Two functions expose [`markdown_core::search`](../search/README.md) over a
snapshot handle under D-022. The semantics, Unicode rules, source mapping and
bounds are those of the search contract; the ABI only validates arguments and
flattens results. Both are thread-safe, start no threads and should run on a
host worker.

- `md_snapshot_find(snapshot, query, query_length, options, matches,
  match_capacity, segments, segment_capacity, out_result)`: the query is UTF-8
  (invalid UTF-8 or more than `MD_FIND_MAX_QUERY_BYTES` bytes is
  `MD_ERR_INVALID_ARGUMENT`); an empty or whitespace-only query succeeds with no
  matches. `md_find_options` carries `MD_FIND_CASE_SENSITIVE`/`MD_FIND_WHOLE_WORD`
  flags and `max_matches` (0 selects 10,000; at most 1,000,000).
  `md_find_result` reports `MD_FIND_TRUNCATED` and the match and segment
  counts. Each `md_find_match` (leaf `unit` node, covering source span,
  `MD_MATCH_EXACT`) owns the contiguous range `segments[first_segment ..
  first_segment + segment_count)`. Each `md_match_segment` has its node, the
  byte range in that node's display text (`md_node.text` for text, code and
  image alt; the source slice for unsupported nodes; empty at 0 for breaks),
  its source span and `MD_MATCH_EXACT`.
- `md_snapshot_read_stats(snapshot, options, sections, section_capacity,
  out_stats)`: `md_read_options` carries `MD_READ_INCLUDE_HEADINGS`/
  `MD_READ_INCLUDE_CODE` and `words_per_minute` (0 selects 160; otherwise
  60..600). `md_read_stats` reports the rate used, total words and seconds,
  and the section count, which always equals the view's `outline_count + 1`.
  `md_read_section` is the intro (`heading`/`outline_index` `MD_NONE`, level 0)
  or one heading, with own and cumulative words and seconds.

Node indices are view node indices and spans are source byte ranges, so hosts
highlight from segments and keep positions across generations with
`md_snapshot_anchor_at(match.span_start)`. Several matches inside one atomic
node can share a span, so navigate by match index (see the search contract).

**Sizes.** Every new struct has a header constant (`MD_FIND_OPTIONS_SIZE`,
`MD_FIND_RESULT_SIZE`, `MD_FIND_MATCH_SIZE`, `MD_MATCH_SEGMENT_SIZE`,
`MD_READ_OPTIONS_SIZE`, `MD_READ_STATS_SIZE`, `MD_READ_SECTION_SIZE`). All
fields are fixed-width, so the sizes are the same on every target. The options
and result structs begin with `struct_size`. The record arrays have no
per-element header, so the options carry `match_size`, `segment_size` and
`section_size`, which the caller sets to `sizeof` the record type. Any
mismatch is `MD_ERR_STRUCT_SIZE` on every call. Unknown flag bits and a
nonzero `md_find_options.reserved` are `MD_ERR_INVALID_ARGUMENT`.

**Buffers.** Record arrays are caller-owned. Each may be `NULL` only with a
zero capacity, must be aligned for its record type, and its byte size must
not exceed `PTRDIFF_MAX`. If any array is too small, the call returns
`MD_ERR_BUFFER_TOO_SMALL`, writes the result struct in full (counts, flags,
totals) and writes no records; the host grows its arrays and calls again,
which recomputes the work. Hosts can size `matches` from `max_matches` and
`sections` from the view's `outline_count + 1` to avoid the sizing call. On
`MD_OK` exactly `match_count`/`segment_count`/`section_count` leading records
are written and the rest of each array is untouched. On every other failure,
including a contained panic, the result struct is zeroed (keeping
`struct_size`) and no record is written.

**Cost and cancellation.** There is no cancel handle, because D-022 adds no
handle types. Work is linear in the snapshot's visible text and query length,
and results are capped by `max_matches`. The call first converts every record
to validate it, then writes records straight into the caller arrays; it keeps
no second copy, and a sizing call allocates no records. Transient heap memory
is therefore the core `FindResults`: about 250 bytes per single-segment match,
mostly its per-match segment vector. That is about 2.5 MiB at the default
10,000 matches and about 240 MiB at `MD_FIND_MAX_MATCHES`. This was measured
with a counting allocator in a release build for 1,000,000 matches; before the
PR #9 review fix, an intermediate flat copy made it 312 bytes per match. A
flat or visitor result form in the core could remove most of it (a candidate
follow-up). The caller's arrays add 24 bytes per match and per segment, and
`read_stats` adds 48 per section. The snapshot ceilings bound a call. Hosts that type-ahead should
debounce on a worker and drop stale results. Cancellable find is a candidate
for a later ABI version.

## Workspace search (CORE-005B2)

Four functions expose [`markdown_core::workspace`](../workspace-search/README.md)
under D-022. None takes a handle. The adapter keeps folder access,
enumeration and file reads. It passes display paths, which are relative,
`/`-separated UTF-8 labels that grant no access, and the file bytes it read.
The core opens nothing. Validation rules, ranking, matching, snippets,
budgets and Unicode limits are those of the workspace contract; the ABI
validates arguments, maps zero limits to defaults and flattens results.

- `md_workspace_classify_path(path, path_length, out_info)` returns `MD_OK`
  for any bytes and puts the verdict in `md_workspace_path_info.error`
  (`MD_PATH_VALID` or one `MD_PATH_*` code per `PathError`, plus
  `MD_PATH_INVALID_UTF8`). Input over `MD_WORKSPACE_MAX_PATH_BYTES` is
  `MD_PATH_TOO_LONG` without being decoded. Valid paths report
  `MD_PATH_MARKDOWN`, `MD_PATH_HIDDEN` and `MD_PATH_INVISIBLE` (the
  `has_invisible` rule) and the file name's byte offset. Adapters use it to
  skip names the core rejects and to mark invisible ones.
- `md_workspace_find_names(paths, paths_length, entries, entry_count, query,
  query_length, options, hits, hit_capacity, out_result)`: each
  `md_workspace_entry` holds the adapter's opaque `id` and a byte range in the
  single `paths` buffer (`reserved` must be 0). Only the first `max_entries`
  entries are examined. Among them, a range outside `paths` or a nonzero
  `reserved` is `MD_ERR_INVALID_ARGUMENT`. A path the core rejects is skipped
  and counted in `skipped_invalid` (included in `entries_examined`), so one
  bad name cannot fail a search, and it is never shown. Ranges may alias, so
  the examined entries' `path_length` total is capped at
  `MD_NAMES_MAX_PATH_BYTES` (64 MiB, like the session source ceiling); more
  is `MD_ERR_LIMIT` before any work (PR #11 review). `md_name_options`
  carries `MD_NAMES_CASE_SENSITIVE`, `MD_NAMES_INCLUDE_HIDDEN` and
  `MD_NAMES_ALL_FILES` (Markdown-only is the default) and `max_results`/
  `max_entries` (0 selects 200/100,000; at most 10,000/1,000,000). An empty or
  whitespace-only query succeeds with a zeroed result, as `md_snapshot_find`
  does. Each `md_name_hit` reports the entry `id`, its index in `entries`,
  `MD_NAME_RANK_PREFIX`/`NAME`/`PATH` and the byte range of the first match
  in the path. Hits are in the core order, and `md_name_result` reports
  `MD_NAMES_TRUNCATED`, `MD_NAMES_ENTRIES_LIMITED` and every core count.
- `md_workspace_content_admit(query, query_length, options, path,
  path_length, byte_length, progress, out_outcome, out_reason)` decides before
  a read: `MD_CONTENT_READ`, or `MD_CONTENT_SKIPPED` with an `MD_SKIP_*`
  reason, or `MD_CONTENT_STOPPED` with an `MD_STOP_*` reason. It records
  skips and stops in `progress`.
- `md_workspace_content_search(query, query_length, options, path,
  path_length, content, content_length, progress, hits, hit_capacity, text,
  text_capacity, anchors, anchor_capacity, out_result)` re-checks admission
  with the real length, so calling admit first is optional. `md_content_result`
  reports `MD_CONTENT_SEARCHED`, `SKIPPED` or `STOPPED` with the reason,
  `MD_CONTENT_TRUNCATED`, the hit, text and anchor byte counts, and the
  searched bytes' length and SHA-256 (64 lowercase hex bytes, not
  NUL-terminated, comparable with the view's `source_sha256`). Each
  `md_content_hit` has the covering source span, the 1-based line,
  `MD_HIT_EXACT`, `MD_HIT_LEADING_MORE`, `MD_HIT_TRAILING_MORE` and
  `MD_HIT_ANCHOR`. Its snippet is `text[text_offset .. + text_length)`, with
  the match at `[match_start, match_end)` inside it. Its serialized anchor is
  `anchors[anchor_offset .. + anchor_length)`. Snippets and anchors are packed
  in hit order.

**Options.** `md_content_options` carries `MD_CONTENT_CASE_SENSITIVE`,
`MD_CONTENT_WHOLE_WORD` and `MD_CONTENT_INCLUDE_HIDDEN` and the six
`ContentLimits`. Zero selects the documented default for `max_files`,
`max_total_bytes`, `max_file_bytes`, `max_matches_per_file` and
`max_total_matches`. `snippet_context` is used as given, because zero context
is meaningful; `MD_CONTENT_DEFAULT_SNIPPET_CONTEXT` (40) is the default.
Out-of-range limits, an empty or whitespace-only query (an error here, unlike
name search, because a workspace pass would otherwise charge budgets for
files it cannot match) and an invalid path are `MD_ERR_INVALID_ARGUMENT`.
Queries are UTF-8 of at most `MD_WORKSPACE_MAX_QUERY_BYTES`.

**Progress.** `md_content_progress` is the caller-owned form of
`ContentProgress`: `stopped` (0 or `MD_STOP_*`), files read, matched and
truncated, bytes read, matches, and one count per skip reason. Start from a
zeroed struct with `struct_size` set and pass the same struct to every admit
and search call of one workspace search. The call reads it (exact
`struct_size`; an unknown `stopped` code is `MD_ERR_INVALID_ARGUMENT`) and
writes it back only on `MD_OK`. Every failure, including
`MD_ERR_BUFFER_TOO_SMALL` and a contained panic, leaves it unchanged, so a
retry is identical. A nonzero `stopped` is permanent, and the UI should say
the results may be incomplete. The progress struct is plain data that the
host can edit. The core never trusts it for memory safety, and counts that no
budget bounds (skips, matched and truncated files) saturate instead of
overflowing. Calls on one progress struct must be sequential. Parallel
readers serialize their calls or keep one progress struct per worker with
partitioned budgets.

**Sizes and buffers.** Every new struct has a header constant
(`MD_WORKSPACE_PATH_INFO_SIZE`, `MD_WORKSPACE_ENTRY_SIZE`,
`MD_NAME_OPTIONS_SIZE`, `MD_NAME_RESULT_SIZE`, `MD_NAME_HIT_SIZE`,
`MD_CONTENT_OPTIONS_SIZE`, `MD_CONTENT_PROGRESS_SIZE`,
`MD_CONTENT_RESULT_SIZE`, `MD_CONTENT_HIT_SIZE`). All fields are fixed-width.
`md_name_options` carries `entry_size` and `hit_size`, and
`md_content_options` carries `hit_size`, checked on every call. Arrays follow
the find rules: null only with zero capacity, aligned, and at most
`PTRDIFF_MAX` bytes. If any array is too small, the result struct is written
in full and nothing else is. Hosts avoid the retry, which re-parses the file,
by sizing from the limits: `max_matches_per_file` hits,
`max_matches_per_file × MD_CONTENT_MAX_SNIPPET_BYTES` (2,624, from
2 × 200 + 256 characters of at most 4 bytes) of text and
`max_matches_per_file × MD_ANCHOR_MAX_BYTES` of anchors.

**Navigation.** When the user opens a hit, the host reads the file into a
session. If the snapshot view's `source_length` and `source_sha256` equal the
result's, the span applies directly, and for a v1 view so does `unit`. V2
adds nodes, so v2 hosts navigate by span or anchor. Otherwise the host
passes the anchor bytes to `md_snapshot_resolve_anchor` (exact, relocated or
nearest). Anchors and snippets are document-derived private state; never
log them.

**Cost and cancellation.** Name search is O(examined path bytes + query)
plus O(hits × log max_results) and copies each examined valid path once, so
time and memory are bounded by the 64 MiB path total. A content call holds the file's parse, its hits (at most
`max_matches_per_file` records) and their serialized anchors (up to 1 KiB
each) before writing, about 10 MiB at the 10,000-match ceiling. There is no
cancel handle. Each content call is bounded by `max_file_bytes`, and hosts
cancel by not making the next call.

## Host guidance for search calls (CORE-002B)

`md_snapshot_find`, `md_snapshot_read_stats` and the workspace content
functions cannot be interrupted, and an `MD_ERR_BUFFER_TOO_SMALL` retry runs
the whole search again. At 5 MiB one call takes about 31 ms for find and 61 ms
for reading statistics, and parse validation adds up to about 50 ms (holistic
review F4). Hosts therefore:

- Call these functions on a host worker, never on the UI thread, and drop
  results that a newer query or generation has superseded.
- Size the arrays up front so that one call is enough: `matches` from
  `max_matches`, `sections` from the view's `outline_count + 1`, and the
  workspace content arrays from the limits listed under
  [Workspace search](#workspace-search-core-005b2). A find match has one
  segment per node it crosses, so the segment count has no fixed bound per
  match. Start `segments` at `max_matches` and keep grown arrays for later
  calls, so a retry happens at most once per growth.

Cancellable variants would be new functions under the D-022 growth rule. They
are added only if the 001F host measurements show that this latency matters
in practice.

## Evidence

- `core/crates/markdown-ffi/src/tests.rs` calls the exported functions as a
  host would: negotiation and misaligned/null/size errors, status names, all
  argument checks, typed/stale/double release, v1 and v2 bulk-view round trips
  over the corpus and all v1 goldens (all 24 node kinds), empty documents,
  snapshot-outlives-session, outcomes/status/failures/limits, cancellation
  before and during runs from another thread, external-change dispositions,
  anchor round trips and forged anchors, panic injection at every entry point
  (including while the registry lock is held, and after a job ran, proving
  zeroed results), shared encodings, bounded oversized submissions,
  overlapping outputs and multi-threaded sessions. For find and reading
  statistics it checks parity with the Rust API (every match, segment and
  section field) over the corpus in v1 and v2, for fixed Unicode queries
  (NFC/NFD, ß/ẞ, emoji, CJK), each source's first 60 distinct words and first
  10 lines, all flag combinations and five rates. Every exact segment is checked
  against the bulk view's display text. Further tests cover cross-inline,
  atomic entity, cluster-boundary and case-folding cases, two-call sizing with
  untouched arrays, truncation, empty queries, every argument and size check
  with zeroed results, released handles, injected panics and concurrent calls
  on one snapshot.
  For workspace search (`src/tests/workspace.rs`), path classification
  matches `WorkspacePath` for every error kind, invalid UTF-8 and over-long
  input. Name search matches `find_names` over a mixed workspace (Markdown,
  other, hidden, invalid and duplicate names; Unicode folding) for 16
  queries, five flag sets and five result/entry limits, with entry indices
  mapped through skipped invalid paths. It is also checked over 100,001
  entries, and with aliased 4 KiB ranges just under and over the path-byte
  cap. Content search matches `ContentQuery` file by file over the corpus
  for four flag sets and three snippet/per-file settings, comparing every
  hit field, snippet byte, anchor byte and the progress after each file.
  Admit-then-search and search-only runs match the Rust sequence through
  every skip reason and every stop. Hits open into v1 and v2 session
  snapshots of the same bytes, where the hashes are equal and anchors resolve
  exactly; units index the v1 view. Further tests cover sizing from the
  limits, each array being too small with progress unchanged, every argument
  and limit check with zeroed results and unchanged progress, saturation of
  forged counters, injected panics in all four functions, concurrent content
  searches and equality of the ABI and core constants.
  Every test asserts that all handles return to zero.
- `core/crates/markdown-ffi/tests/c_abi.rs` checks every header constant
  against Rust. It then builds the shared library for its own profile and
  feature set into `target/markdown-ffi-smoke/<profile>-models<mask>` (the
  unhashed `deps/` cdylib can belong to another feature set), copies it to a
  per-run directory (with an `@rpath` install name on macOS; DT_RPATH via
  `--disable-new-dtags` on Linux), compiles
  `tests/c/smoke.c` with the host C compiler (`-std=c11 -Wall -Wextra -Werror
  -pedantic`), compares every struct size and field offset with Rust and runs
  the C smoke with `LD_LIBRARY_PATH`, `DYLD_LIBRARY_PATH` and
  `DYLD_FALLBACK_LIBRARY_PATH` removed. The smoke reports the file that
  provides `md_abi_negotiate` via `dladdr`, which must canonicalize to the
  staged copy, and asserts the library's model mask matches the build: negotiation, v1 and (when built) v2 sessions, bulk copy, two-call
  find with segment highlighting checked against the view, reading statistics
  and their argument checks, workspace path classification, name search over
  an entry array with hidden, non-Markdown and invalid names, a content
  admit/search pass with arrays sized from the limits, unchanged progress on
  failure, a sticky stop, and navigation from a hit into a session snapshot
  (equal hash, exact anchor), anchor round trip, external change, cancellation,
  snapshot-outlives-session and 200 lifecycle iterations ending with zero live
  handles. The smoke also `_Static_assert`s every D-022 size constant.
- `tools/check.py core` runs both in debug and release across default,
  `model-v2` and all-feature builds; it requires the C smoke on macOS/Linux.
  Windows runs the Rust tests and constant check but skips the C smoke until
  the WIN-001B consumer wires MSVC and the import library.

Not covered here: Swift/AppKit, C# Native AOT and gtk-rs consumers, install
names/rpaths for relocated packages, symbol visibility audits on Windows/Linux,
performance budgets for encode/copy cost and the macOS repeated-session RSS
risk. Those are platform B-phase work. CORE-003B4 raised the core node and
event defaults selected by zero limits to 1,000,000 and 2,000,000 (D-024),
within the unchanged ABI ceilings.
