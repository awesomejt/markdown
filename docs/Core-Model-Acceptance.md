# CORE-002 semantic-model acceptance

This records aggregate acceptance evidence for CORE-002, following the shared
track in D-027. The implementation is already merged: CORE-002A in PR #2,
opt-in v2 in PR #5, CORE-002B in PR #18, and the metadata refinements through
CORE-003F in PR #32. This acceptance changes no Rust code, schema, fixture,
golden, dependency or ABI. Review disposition and completion live in the PR
and the CORE-002 pass record in `project.yaml`.

## Contract coverage

The [v1 contract](../specs/semantic-model/v1/README.md) and
[v2 contract](../specs/semantic-model/v2/README.md) define the portable values.
The [wire types](../core/crates/markdown-core/src/model.rs),
[validation boundary](../core/crates/markdown-core/src/validate.rs), and
[v2 implementation](../core/crates/markdown-core/src/v2.rs) supply them.

| CORE-002 criterion | Delivered evidence and checks |
| --- | --- |
| Versioned blocks and inlines | V1 has 24 node kinds across five authored goldens. `model_v1::all_goldens_roundtrip_owned_values_and_preserve_source_spans` checks typed round trips; `tools/validate_model.py` checks kind coverage. |
| Source spans and association | Half-open UTF-8 byte spans, exact source byte count and SHA-256. Tests preserve CRLF, NFC/NFD, reject character-boundary violations and reject mismatched source. |
| IDs and graph structure | Distinct document/revision/node/link/resource IDs, unique node membership, reachable bounded trees, ordered contained spans. Tests reject duplicate/dangling IDs, cycles, orphans and invalid child kinds. D-021 defines the MVP identity policy below. |
| Outline, metadata and capabilities | Outline covers headings in tree order; typed metadata and the exact semantic capability set are validated. Tests reject missing/mismatched outline entries, false capabilities, invalid table shapes and malformed metadata. |
| Link/resource intents | V1 imports re-run the producer policy (CORE-002B). V2 adds typed URI targets and source provenance. Tests reject forged intents and unsafe targets; explicit v1-to-v2 migration preserves existing denials. |
| Diagnostics and fallbacks | Stable diagnostic code/severity with optional bounded span/node; inert unsupported block/inline values preserve source. Tests reject dangling diagnostic nodes and keep private document contents out of validation errors. |
| Golden serialization and compatibility | Rust schema snapshots and JSON round trips, plus independent Python schema/source-hash checks. Unknown versions, fields and kinds fail closed; v1 and v2 reject each other's wire format. |
| Owned portable boundary | `DocumentInput` is unchecked; only `Document::new`/`from_json` publishes an immutable validated document. Values contain no UI types, OS handles or parser lifetimes. The core forbids unsafe code. |

The named model tests are in
[model_v1.rs](../core/crates/markdown-core/tests/model_v1.rs) and
[v2.rs](../core/crates/markdown-core/tests/v2.rs).
[validate_model.py](../tools/validate_model.py) independently checks all five
v1 goldens, their 24 kinds, the v2 reference golden, source hashes and strict
schema version/field rejection. Schema validation alone does not replace
the Rust semantic validation boundary.

## Identity and compatibility decisions

[D-021](../agents/Decisions.md#d-021--snapshot-local-node-ids-with-block-anchors-for-the-mvp)
explicitly resolves the frozen v1 README's cross-revision identity aspiration:
MVP parser node IDs are deterministic ordinals local to a snapshot. The frozen
README is preserved as the decision requires. The test named
`stable_ids_are_not_offsets_and_are_scoped_to_document_lineage` demonstrates
that the value model can represent unchanged IDs at changed offsets; it does
not establish a matching algorithm or promise persistent IDs. Adapters must
not persist node IDs. Reading position uses the block anchors in the
[session contract](../specs/session/README.md).

[D-020](../agents/Decisions.md) keeps v1 as the Rust default and v2 opt-in;
[D-025](../agents/Decisions.md) requires each native 001B package to build
`model-v2` and explicitly request v2. The two versions are not interchangeable.
V2 upgrades revalidate provenance and may reject inconsistent legacy data.
The frozen schemas and golden bytes remain unchanged by this acceptance.
CORE-002B and D-032/D-034 refine shared metadata validation; existing v1 and v2
tests cover both producer output and imported snapshots.

## Validation evidence

Source baseline: `0270e6bb887c4fd8c8d771c22b2a00e82aa760d3` (`develop`, PR #33).
On macOS 27.0.1, Python 3.14.7, Node 24.14.0 and Rust 1.99.1, run:

```sh
export CARGO_HOME="$PWD/build/feas002-macos/toolchains/cargo"
export RUSTUP_HOME="$PWD/build/feas002-macos/toolchains/rustup"
export PATH="$CARGO_HOME/bin:$PATH"
RUSTUP_TOOLCHAIN=stable CHECK_BASE=origin/develop .build/feas004-venv/bin/python tools/check.py all
```

Result: passed on 2026-10-29 (America/Denver) at the source baseline above.
The isolated stable toolchain matches the committed Rust pin. The full command
covers common checks, formatting, Clippy, default/all-feature/model-v2-only
debug tests, all-feature release tests, C ABI smoke, conformance and budgets,
and dependency/license checks. The pass and PR retain the result and review
disposition; no model artifact changed during validation.

[Hosted three-OS PR CI](https://github.com/awesomejt/markdown/actions/runs/40991275826)
passed at `29b87d423f5829278f158e5bf531dfe30295328c`. There is no diff in
`core/`, `specs/semantic-model/` or `tools/validate_model.py` between that
commit and the source baseline. This proves portable Rust/model checks on
the named hosted OSes; it does not prove native app behavior. This acceptance
PR changes prose/state only, so its normal CI may skip the host jobs.

## Limits and remaining gates

Model validation establishes structural/source consistency and, for v2
intents, bounded provenance under the pinned parser. It does not prove the
semantic accuracy of every display-text value, trust document contents or
grant file/network access. Capabilities describe present semantic extensions,
not renderer features or permissions. Local access still requires the
platform adapter's containment and OS capability checks.

ModelLimits are defensive ceilings, not an allocator quota or a release
performance budget. Parsing/conformance acceptance is CORE-003; sessions and
anchors are CORE-004; search/read metadata is CORE-005; facade acceptance is
CORE-006. Native selection, accessibility, file access, packaging and memory
limits remain in the host tasks. Closing CORE-002 does not open those tasks:
the shared track continues with CORE-003 through CORE-006 first.
