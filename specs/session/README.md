# Document session contract

Status: CORE-004A contract under phase-PR review; freeze at owner-authorized
merge. Rust module `markdown_core::session` implements it with no new
dependency, no unsafe code and no wire-model change. V1/v2 APIs, schemas and
goldens are unchanged. It adds no C ABI, file I/O, threads, clocks, search or
UI; CORE-006A wraps these values in the versioned facade.

## Responsibilities

The core session owns generation assignment, latest-wins publication,
cancellation, typed availability state and anchor resolution. The platform
adapter owns file access, permissions, watching, scheduling and presentation
([Architecture](../../docs/Architecture.md)). An adapter reads bytes under its
own permission model and submits them; the session never receives a path.

| Type | Role |
| --- | --- |
| `DocumentSession` | One document lineage: `new(document_id, options)`, `submit`, `external_change`, `cancel`, `current`, `status` |
| `Job` | One generation's owned bytes; `Send`; `run()` validates, deduplicates, parses and publishes |
| `Snapshot` | Immutable `Arc`-shared revision: generation, exact source, validated v1 or v2 model, anchors |
| `Outcome` | `Published`, `Unchanged`, `Superseded`, `Cancelled`, `Failed`, `Closed` |
| `SessionStatus` | Copy of latest/confirmed/pending generations, availability, failure and published snapshot |
| `CancellationToken` | Cloneable atomic flag; root `parse_markdown_cancellable` / `parse_markdown_v2_cancellable` poll it |
| `Anchor` | Serializable position resolved as `Exact`, `Relocated`, `Nearest` or lost |

`SessionOptions` fixes the model version (`V1` or, with feature `model-v2`,
`V2`) and `ParseLimits` for the lineage. The document ID uses model ID rules;
revision IDs are `g<generation>`.

## Generations and publication

- Each `submit` and each `Deleted`/`Unreadable` change takes the next
  generation under the session lock. Generations start at 1, increase strictly
  and fail with `GenerationExhausted` rather than wrap. They are per-session
  sequence numbers, not dense publication counters or persisted identities.
- A new generation cancels the previous in-flight job's token. At most one job
  (the newest) can publish; an older job returns `Superseded`, even if it
  completed parsing.
- Publication happens only at one point under the lock, after a complete
  validated snapshot exists. Cancelled, superseded, failed and closed work
  publishes nothing; there are no partial snapshots.
- Published generations never decrease. Readers obtain `Arc<Snapshot>` values
  that stay valid after newer publications and after the session is dropped.
- `Unchanged` means the bytes equal the published source exactly (byte
  comparison, no hash collision risk). Nothing is reparsed; the existing `Arc`
  is confirmed for the newer generation.
- `Failed` (invalid UTF-8, `ParseError` including source limit) records a typed
  failure for the newest generation and leaves the previous snapshot published
  but not current. A later success clears it.
- `status().is_current()` is true only when a snapshot exists, the newest
  generation is confirmed and the document is available.

Jobs run wherever the adapter schedules them. A session holds only a weak
reference from each job, the published snapshot and the newest job's token, so
retained memory is bounded by the published snapshot plus caller-held values.
Dropping the session cancels the in-flight token; its job returns `Closed`.
Dropping an unrun job clears its pending state. Lock poisoning is recovered,
never propagated as a panic.

## Cancellation

Cancellation is cooperative and idempotent. The parser polls before work,
before the upstream parse, every `CANCEL_CHECK_EVENTS` (256) events, before the
v2 shadowed-definition scan and before final validation. With an uncancelled
token, output equals `parse_markdown`/`parse_markdown_v2`; the default entry
points use an uninhabited interrupt type and cannot observe cancellation.
Results are `CancellableParseError::{Cancelled, Parse}`.

Limits: pulldown-cmark's initial block scan and final model validation are not
interruptible; both are linear and bounded by `max_source_bytes`. The poll
interval is a granularity, not a deadline. Worst-case latency is unmeasured
here and belongs to the CORE-003B4 performance and native A-phase timing work.

## External changes

`external_change` performs no I/O:

| Change | Effect |
| --- | --- |
| `Modified { token }` | `Unchanged` when available and `token` equals the newest accepted submission's token; otherwise `ReadRequired`. No generation is consumed |
| `Deleted` / `Unreadable(reason)` | New generation, in-flight job superseded, availability recorded, token forgotten; returns `Recorded(generation)` |

`RevisionToken` is 1–256 opaque adapter bytes (for example size/mtime/file ID
or provider version). The core compares equality only; tokens must not hold
paths or contents, and `Debug` prints only their length. A token is accepted
when its submission resolves as published, unchanged or failed, so a failing
file is not retried for an identical notice. `UnreadableReason` is
`PermissionDenied`, `AccessRevoked`, `Busy` or `Io`. The last snapshot remains
readable as stale content. Reappearance is an ordinary submission; identical
bytes produce `Unchanged` and restore availability.

## Node identity and anchors

Node IDs remain the model's snapshot-local deterministic ordinals; the session
does not rewrite IDs. Adapters must not treat a `NodeId` as persistent across
generations. Cross-generation correspondence is block-level through anchors.

Anchorable blocks are leaf blocks in reading order: paragraph, heading, code
block, thematic break, table and unsupported block. Quotes, lists, items,
alerts and footnotes are descended. Each block has a kind, a deterministic
FNV-1a 64-bit fingerprint of its exact source slice, an occurrence ordinal among
equal fingerprints, neighbouring fingerprints and the preceding heading.

`Snapshot::anchor_at(source_offset)` and `anchor_for(node, offset)` record the
block, intra-block byte offset, absolute offset/source length and that context.
Offsets between blocks snap forward; past the end they snap to the last block.
`resolve(anchor)` against any snapshot returns:

1. `Exact`: same kind and fingerprint, ties broken by neighbours, occurrence
   and proportional position. The intra-block offset is preserved.
2. `Relocated`: same kind with a matching previous or next neighbour that is
   not itself a former neighbour (for example a renamed heading or edited
   paragraph). The offset is clamped to the new block.
3. `Nearest`: block after the old predecessor, else the old successor, else
   the preceding heading, else the proportional source position; offset zero.
4. `Ok(None)` (lost): no anchorable block, such as an empty document.

All resolved offsets are UTF-8 boundaries inside the returned block. Anchor
fields are untrusted: forged values never panic, unknown fields are rejected
and a different `version` returns `UnsupportedVersion`. Fingerprints are not
cryptographic and can collide; short blocks are guessable. Treat stored
anchors as private document-derived state: never log or upload them.
Resolution is linear in block count and the per-snapshot index is built lazily
once. This is a heuristic, not a diff or minimal edit script.

## CORE-006A facade expectations

The C ABI should expose opaque session/job/snapshot handles with explicit
release, generation integers, typed status/outcome codes, token cancellation
and value copies of anchors (wire version `ANCHOR_VERSION`) and resolutions.
No Rust lifetime, `Arc` or parser pointer crosses the boundary. CORE-005A find,
word count and section timing operate on published snapshots. Native A phases
own threads, file reads, watchers and restoration UI.

## Evidence

`core/crates/markdown-core/tests/session.rs` covers Send/Sync assertions,
cancellable/default equivalence across corpus inputs for v1 and v2, pre-set and
racing cancellation, generation order, stale-job rejection, eight-thread
latest-wins races with a concurrent monotonic observer over 40 iterations,
explicit cancel, drop of the session or an unrun job, snapshot liveness,
deduplication, deletion/unreadable/reappearance, typed failures and anchors
across insertion, deletion, heading rename, edit, duplicates, containers,
empty documents, persistence and forged values. Run the
[development checks](../../docs/Development.md). Loom/model checking is not
used; races are exercised with repeated `std::thread` iterations only.
