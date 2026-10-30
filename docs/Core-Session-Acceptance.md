# CORE-004 document-session acceptance

This records CORE-004 aggregate acceptance under D-027 for the session
implementation delivered in CORE-004A (PR #6), including its review fixes and
the later test-lifetime correction in CORE-003B4 (PR #13). The
[session contract](../specs/session/README.md) and
[implementation](../core/crates/markdown-core/src/session/mod.rs) remain
unchanged. This acceptance changes no runtime behavior, dependency, model
artifact, ABI or limit. Review disposition and completion live in the PR
and CORE-004 pass record in `project.yaml`.

## Acceptance coverage

The evidence is in
[session.rs](../core/crates/markdown-core/tests/session.rs), the
[generation-exhaustion unit test](../core/crates/markdown-core/src/session/mod.rs),
and the [anchor implementation](../core/crates/markdown-core/src/session/anchor.rs).

| CORE-004 criterion | Delivered behavior and tests |
| --- | --- |
| Generations and stale results | Submissions receive monotonically increasing generations. `stale_jobs_never_publish` rejects older jobs, while `concurrent_submissions_are_latest_wins` runs eight workers and a monotonic publication observer over 40 iterations. Only the newest generation can publish at the final locked gate. |
| Immutable revisions and lifetime | `generations_increase_and_snapshots_are_immutable` keeps old source/model values after a newer publication. Snapshot/job/session drop tests verify held snapshots remain usable, dropped jobs clear pending state and late jobs return `Closed`. Generation exhaustion returns a typed error without changing state. |
| Cancellation | Pre-cancelled parsing returns typed cancellation; explicit session cancellation preserves the old snapshot. Racing cancellation yields a complete validated result or typed cancellation. Uncancelled parsing equals ordinary parsing for v1 and v2; pre-cancel is also checked for v2. Cancellation never publishes a partial document. |
| Exact-byte deduplication | `unchanged_content_is_deduplicated` checks token handling and exact-byte equality. Unchanged bytes reuse the published `Arc` and confirm the newer generation without changing the snapshot's original generation/revision. A token alone is not a source-content proof. |
| External changes | Deletion, typed unreadability and reappearance tests retain the old snapshot as stale. Pending-token notices do not supersede their job, repeated idle availability notices consume no generation, and A-to-B-to-A reverts while B is pending require a read. |
| Failure and recovery | Invalid UTF-8 and parse/source-limit failures are typed, retain the old snapshot and make it non-current. Successful publication clears failure. Identical notices for an accepted failing token do not automatically retry it. |
| Identity and anchors | Insert/delete/edit/rename, duplicate-block context, containers, empty documents, relaunch persistence and forged-anchor tests cover exact/relocated/nearest/lost results and UTF-8 boundaries. D-021 defers persistent node IDs in favor of heuristic block anchors. |
| Explicit model selection | `v2_sessions_publish_v2_snapshots` checks v2 publication and anchor resolution after insertion. Session options fix the model for the lineage; the Rust default stays v1, while native packages explicitly select v2 under [D-025](../agents/Decisions.md#d-025--native-hosts-build-and-request-semantic-model-v2). |

`Job::run` checks generation/cancellation before work and again at publication.
Session state has no platform path, watcher or executor; adapters submit
already-read bytes and schedule work away from the UI thread. `current()` may
return stale content; `status().is_current()` and its captured snapshot expose
the session's currentness at that call. Native adapters must also gate their
asynchronous presentation against newer generations.

## Identity and race-evidence boundaries

[D-021](../agents/Decisions.md#d-021--snapshot-local-node-ids-with-block-anchors-for-the-mvp)
keeps parser node IDs deterministic and snapshot-local. Anchors preserve a
block fingerprint, neighbouring context and intra-block byte position; they
are a heuristic, not persistent whole-tree identities or a minimal edit script.
Fingerprints are non-cryptographic and can collide. Stored anchors are private
document-derived state and must not be logged or uploaded.

The latest-wins stress test uses default-v1 sessions: submissions are serialized
by its record lock to associate each payload with its generation, while job
execution and the observer race across threads. The cancellation race is also
v1. V2 has separate publication/anchor, pre-cancel and output-equivalence
coverage; enabling v2 does not make every default-session test use v2.
Both models share the session publication gate, but this is not exhaustive
interleaving proof, Loom/model checking, or independent v2 race coverage.

## Validation evidence

Baseline: `b958f6e58f8d072c24056eea3793fe31dfa7df0f` (`develop`, PR #35).
Host: Apple M5 Max, macOS 27.0.1, Rust 1.99.1, Python 3.14.7, Node 24.14.0.
With the isolated toolchain environment in [Development.md](Development.md):

```sh
cargo test --locked -p markdown-core --test session
cargo test --locked -p markdown-core --features model-v2 --test session
cargo test --locked -p markdown-core --all-features --test session
cargo test --locked -p markdown-core --all-features --release --test session
cargo test --locked -p markdown-core --all-features --lib generation_exhaustion_leaves_state_unchanged
```

All five commands passed on 2026-10-29 (America/Denver): 28 default session
tests, 29 in each v2-only/all-feature debug and all-feature release run, and
the generation-exhaustion unit test. The full local
`RUSTUP_TOOLCHAIN=stable CHECK_BASE=origin/develop .build/feas004-venv/bin/python tools/check.py all`
also passed, covering common checks, formatting/Clippy, all debug configurations,
all-feature release, C ABI smoke, conformance/hostile/budgets and dependency
policy. `tools/check.py common` passed with the acceptance documentation.

[Baseline CI](https://github.com/awesomejt/markdown/actions/runs/40995153389)
passed; its prose/state-only changes select no host work.
[The full three-OS core and extended run](https://github.com/awesomejt/markdown/actions/runs/40992357553)
at `0270e6b` passed on unchanged core, specs, fixtures and check commands
relative to this baseline. This acceptance PR also changes prose/state only,
so normal CI may skip host jobs. These are portable Rust checks, not native
watcher, permission or presentation evidence.

## Remaining platform work

Adapters own actual file reads, permissions, watcher/read ordering, revision
tokens, scheduling, restoration and presentation. Missing or changed tokens
can require reads; a token is opaque adapter evidence, not a cryptographic
content identity. The session neither reads nor repairs a file or grants access.

Cancellation is cooperative: the upstream block scan and final validation
are not interruptible, and the polling interval is not a deadline. Multiple
caller-held jobs, source buffers and snapshots can retain memory even though
the session keeps one published snapshot. The known dense 8 MiB parse can
peak near 1,152 MiB; session acceptance does not impose an application memory
quota. Each native 001B must measure and set a session source bound.

Under D-021, native accessibility/selection checks still decide whether
anchor-based restoration needs further work. Native packaging, the retained
macOS RSS risk and platform lifecycle/watch behavior remain their host tasks.
The facade is assessed in CORE-006; CORE-005 search/read metadata is next.
No native chain starts before the shared track closes through CORE-006.
