# Dependency and license inventory

## Project source

The owner selected **Apache-2.0** for the new implementation on 2026-10-26.
The root [LICENSE](../LICENSE) provides the standard text, also present in the
frozen source license. Imported fixtures retain their original notices,
hashes and provenance under [fixtures/manifest.yaml](../fixtures/manifest.yaml).
Choosing a project license does not change third-party licenses.

## Production Rust graph

| Package | Version | License | Source |
| --- | --- | --- | --- |
| markdown-core | 0.1.0 | Apache-2.0 | This repository, unpublished |

There are **no third-party production dependencies** in CORE-001A. Cargo.lock
pins the resolved graph; `python tools/check_dependencies.py` checks every
resolved package, including transitive packages. It rejects missing/unapproved
license expressions, publishable workspace members, portable workspace members
outside `core/`, and dependencies on `spikes/`. Workspace packages must be
local; non-workspace packages must come from the canonical crates.io registry.
Git, alternate-registry and non-workspace path dependencies require a deliberate
policy change/review, even when they carry an allowed SPDX expression.
Cargo.lock can pin Git revisions; origin approval is a separate constraint.
Metadata is resolved with all features so optional dependencies are checked too.

The initial SPDX allowlist is Apache-2.0, MIT, and their explicit OR combinations.
An unlisted expression fails and requires deliberate dependency/policy review;
it is not a judgment that the dependency cannot be used. Future phases must
update this inventory with purpose, version, source and provenance when adding
production dependencies. Parser selection does not add a dependency until its
implementation phase. Consider a dedicated policy tool such as cargo-deny if
the graph grows; adopting another tool is unnecessary for this dependency-free
scaffold, and the present checks have targeted regressions.

## Development tooling and feasibility

| Tool/graph | Pin | Policy boundary |
| --- | --- | --- |
| Rust, rustfmt, Clippy | rust-toolchain.toml: 1.99.1 | Build toolchain, not an app dependency |
| Python fixture tooling | tools/requirements-fixtures.txt | PyYAML 6.0.3, jsonschema 4.26.0; development-only, transitives resolved by pip |
| Markdown lint | package-lock.json | markdownlint-cli2 0.18.1 and locked npm transitives; development-only |
| GitHub Actions | source SHAs in workflow | checkout/setup-python/setup-node and rust-toolchain; automation only |
| Feasibility engines | spike-specific locks/reports | Experimental, outside production workspace; never linked by sibling paths |

The Rust license gate is not an npm/Python license audit, vulnerability scanner,
font audit or distribution approval. Preserve dependency notices and add a
distribution inventory for the actual runtime/package when that phase exists.
Signing, release publishing and production deployment still need owner approval.
