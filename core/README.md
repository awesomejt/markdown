# Portable semantic core

CORE-002A implements the [v1 semantic model](../specs/semantic-model/v1/README.md)
as owned Rust values with strict JSON decoding, source association, structural
validation and schema/golden checks. CORE-003A adds bounded basic parsing and
conservative safety classification; see [Parsing.md](../docs/Parsing.md).
CORE-003B1 adds [bounded typed front matter](../docs/Front-Matter.md) without a
new dependency or wire schema. Invalid/unsupported metadata retains source.
CORE-004A adds [document sessions](../specs/session/README.md): immutable
`Arc` snapshots, generations, latest-wins publication, cooperative cancellation,
typed external-change state and anchors, with no I/O or threads. Node IDs stay
snapshot-local. It does not implement search, expose C ABI functions or render UI.
Those remain separate phases. Feasibility crates are not workspace dependencies.

CORE-006A adds crate `markdown-ffi`, the hand-written versioned C ABI declared
in [`core/include/markdown_core.h`](include/markdown_core.h) and specified in
[the C ABI contract](../specs/ffi/README.md): ABI/struct-size and model
negotiation, registry handles with one release each, typed status codes,
contained panics, session/job/snapshot functions, anchor value copies and a
bulk flat snapshot view. It owns the only unsafe code; `markdown-core` still
forbids unsafe. A C smoke compiled against the header runs in the core checks.

Feature `model-v2` adds [reference provenance and URI targets](../specs/semantic-model/v2/README.md)
through `parse_markdown_v2`/`v2::Document`; it preserves the default v1 APIs.
The optional URL/IDNA dependency graph is documented in the inventory. New
adapters must explicitly choose v2; no FFI/default-version switch is implied.

`DocumentInput` is unchecked; only the validated `Document` boundary should be
published to adapters. The crate forbids unsafe code; `markdown-ffi` owns the
required unsafe FFI code separately. Platform filesystem capabilities and
UI remain native responsibilities; a resource intent grants no access.

Run [development checks](../docs/Development.md). The default library uses
pulldown-cmark (without HTML/CLI), serde/serde_json and SHA-256; optional feature `schema` enables schemars tooling.
[Dependency/source policy and inventory](../docs/Dependencies.md) cover the
resolved graph. Crates remain unpublished until explicitly authorized.
