# C ABI v1 contract

Status: CORE-006A contract under phase-PR review; freeze at owner-authorized
merge. The hand-written header [`core/include/markdown_core.h`](../../core/include/markdown_core.h)
is normative (D-009). Crate [`markdown-ffi`](../../core/crates/markdown-ffi/src/lib.rs)
implements it over the [session contract](../session/README.md) and the
[v1](../semantic-model/v1/README.md)/[v2](../semantic-model/v2/README.md)
semantic models. It adds no new dependency beyond the core graph, no generated
bindings, no I/O, no threads and no UI types. It enables the native B phases;
it does not by itself prove a Swift, C# or gtk-rs consumer (CORE-006 remains
open until those pass on their hosts).

## Artifacts

`markdown-ffi` builds `rlib` (Rust tests), `cdylib` (`libmarkdown_ffi.dylib`,
`.so` or `markdown_ffi.dll`) and `staticlib`. Exactly the 18 functions in the
header are exported with the `md_` prefix. Feature `model-v2` forwards to the
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

## Ownership and lifetimes

| Value | Owner | Release |
| --- | --- | --- |
| `md_session` | core registry | `md_session_release` |
| `md_job` | core registry | `md_job_release` (also after `md_job_run`) |
| `md_snapshot` | core registry | `md_snapshot_release` |
| Arrays/strings reachable from `md_snapshot_view` | the snapshot handle | freed with that handle |
| Input bytes (`document_id`, source, tokens, anchors) | caller | copied during the call; never retained |
| Output structs, anchor buffers | caller | caller |
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
  view; it is freed when the last of them is released, and a later handle
  re-encodes on its first view.
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
required length, and `md_abi_negotiate` fills `md_abi_info` on a version
mismatch. Rejected versioned structs (wrong `struct_size`) are left
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
| `MD_ERR_LIMIT` | Handle counter, or a table/span exceeding `u32` |
| `MD_ERR_PANIC` / `MD_ERR_INTERNAL` | Contained core bug |

Job outcomes (`MD_OUTCOME_*`) are results, not errors: the call returns
`MD_OK` with the outcome, the job's generation, the newest generation when
superseded and an `MD_FAILURE_*` code (invalid UTF-8, each `ParseError`
ceiling, source mapping or model validation) when failed. Session status
reports the newest failure the same way.

Every entry point runs under `catch_unwind` and maps a panic to
`MD_ERR_PANIC`; the registry recovers a poisoned lock. Build with the default
`panic = "unwind"`; with `panic = "abort"` a core bug aborts the process
instead (it still never unwinds into the host). A panic message reaches the
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
  overlapping outputs and multi-threaded sessions.
  Every test asserts that all handles return to zero.
- `core/crates/markdown-ffi/tests/c_abi.rs` checks every header constant
  against Rust. It then builds the shared library for its own profile and
  feature set into `target/markdown-ffi-smoke/<profile>-models<mask>` (the
  unhashed `deps/` cdylib can belong to another feature set), copies it to a
  per-run directory (with an `@rpath` install name on macOS), compiles
  `tests/c/smoke.c` with the host C compiler (`-std=c11 -Wall -Wextra -Werror
  -pedantic`), compares every struct size and field offset with Rust and runs
  the C smoke, which asserts the library's model mask matches the build: negotiation, v1 and (when built) v2 sessions, bulk copy, anchor
  round trip, external change, cancellation, snapshot-outlives-session and
  200 lifecycle iterations ending with zero live handles.
- `tools/check.py core` runs both in debug and release across default,
  `model-v2` and all-feature builds; it requires the C smoke on macOS/Linux.
  Windows runs the Rust tests and constant check but skips the C smoke until
  the WIN-001B consumer wires MSVC and the import library.

Not covered here: Swift/AppKit, C# Native AOT and gtk-rs consumers, install
names/rpaths for relocated packages, symbol visibility audits on Windows/Linux,
performance budgets for encode/copy cost and the macOS repeated-session RSS
risk. Those are platform B-phase and CORE-003B4 work.
