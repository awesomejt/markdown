# Portable semantic core

CORE-002A implements the [v1 semantic model](../specs/semantic-model/v1/README.md)
as owned Rust values with strict JSON decoding, source association, structural
validation and schema/golden checks. CORE-003A adds bounded basic parsing and
conservative safety classification; see [Parsing.md](../docs/Parsing.md).
CORE-003B1 adds [bounded typed front matter](../docs/Front-Matter.md) without a
new dependency or wire schema. Invalid/unsupported metadata retains source.
It does not reconcile IDs across revisions, implement sessions/search, expose
C ABI functions or render UI.
Those remain separate phases. Feasibility crates are not workspace dependencies.

Feature `model-v2` adds [reference provenance and URI targets](../specs/semantic-model/v2/README.md)
through `parse_markdown_v2`/`v2::Document`; it preserves the default v1 APIs.
The optional URL/IDNA dependency graph is documented in the inventory. New
adapters must explicitly choose v2; no FFI/default-version switch is implied.

`DocumentInput` is unchecked; only the validated `Document` boundary should be
published to adapters. The crate forbids unsafe code. A future facade crate
owns required unsafe FFI code separately. Platform filesystem capabilities and
UI remain native responsibilities; a resource intent grants no access.

Run [development checks](../docs/Development.md). The default library uses
pulldown-cmark (without HTML/CLI), serde/serde_json and SHA-256; optional feature `schema` enables schemars tooling.
[Dependency/source policy and inventory](../docs/Dependencies.md) cover the
resolved graph. Crates remain unpublished until explicitly authorized.
