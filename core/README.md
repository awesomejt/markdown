# Portable semantic core

CORE-002A implements the [v1 semantic model](../specs/semantic-model/v1/README.md)
as owned Rust values with strict JSON decoding, source association, structural
validation and schema/golden checks. It does not parse Markdown, reconcile IDs
across revisions, implement sessions/search, expose C ABI functions or render UI.
Those remain separate phases. Feasibility crates are not workspace dependencies.

`DocumentInput` is unchecked; only the validated `Document` boundary should be
published to adapters. The crate forbids unsafe code. A future facade crate
owns required unsafe FFI code separately. Platform filesystem capabilities and
UI remain native responsibilities; a resource intent grants no access.

Run [development checks](../docs/Development.md). The default library uses
serde/serde_json and SHA-256; optional feature `schema` enables schemars tooling.
[Dependency/source policy and inventory](../docs/Dependencies.md) cover the
resolved graph. Crates remain unpublished until explicitly authorized.
