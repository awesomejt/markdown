# Dependency and license inventory

## Project source

The owner selected **Apache-2.0** for the new implementation on 2026-10-26.
The root [LICENSE](../LICENSE) provides the standard text, also present in the
frozen source license. Imported fixtures retain their original notices,
hashes and provenance under [fixtures/manifest.yaml](../fixtures/manifest.yaml).
Choosing a project license does not change third-party licenses.

## Production Rust graph

| Package | Version | License declaration | Source |
| --- | --- | --- | --- |
| bitflags | 2.13.2 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/bitflags/2.13.2) |
| block-buffer | 0.10.4 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/block-buffer/0.10.4) |
| cfg-if | 1.0.5 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/cfg-if/1.0.5) |
| cpufeatures | 0.2.17 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/cpufeatures/0.2.17) |
| crypto-common | 0.1.7 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/crypto-common/0.1.7) |
| digest | 0.10.7 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/digest/0.10.7) |
| dyn-clone | 1.0.20 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/dyn-clone/1.0.20) |
| generic-array | 0.14.7 | MIT | [crates.io](https://crates.io/crates/generic-array/0.14.7) |
| itoa | 1.0.18 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/itoa/1.0.18) |
| libc | 0.2.191 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/libc/0.2.191) |
| markdown-core | 0.1.0 | Apache-2.0 | This repository, unpublished |
| memchr | 2.8.3 | Unlicense OR MIT | [crates.io](https://crates.io/crates/memchr/2.8.3) |
| proc-macro2 | 1.0.107 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/proc-macro2/1.0.107) |
| pulldown-cmark | 0.13.5 | MIT | [crates.io](https://crates.io/crates/pulldown-cmark/0.13.5) |
| quote | 1.0.47 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/quote/1.0.47) |
| ref-cast | 1.0.27 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/ref-cast/1.0.27) |
| ref-cast-impl | 1.0.27 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/ref-cast-impl/1.0.27) |
| ryu | 1.0.23 | Apache-2.0 OR BSL-1.0 | [crates.io](https://crates.io/crates/ryu/1.0.23) |
| schemars | 1.0.4 | MIT | [crates.io](https://crates.io/crates/schemars/1.0.4) |
| schemars_derive | 1.0.4 | MIT | [crates.io](https://crates.io/crates/schemars_derive/1.0.4) |
| serde | 1.0.228 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/serde/1.0.228) |
| serde_core | 1.0.228 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/serde_core/1.0.228) |
| serde_derive | 1.0.228 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/serde_derive/1.0.228) |
| serde_derive_internals | 0.29.1 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/serde_derive_internals/0.29.1) |
| serde_json | 1.0.145 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/serde_json/1.0.145) |
| sha2 | 0.10.9 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/sha2/0.10.9) |
| syn | 2.0.119 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/syn/2.0.119) |
| syn | 3.0.9 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/syn/3.0.9) |
| typenum | 1.20.1 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/typenum/1.20.1) |
| unicase | 2.10.0 | MIT OR Apache-2.0 | [crates.io](https://crates.io/crates/unicase/2.10.0) |
| unicode-ident | 1.0.27 | (MIT OR Apache-2.0) AND Unicode-3.0 | [crates.io](https://crates.io/crates/unicode-ident/1.0.27) |
| version_check | 0.9.5 | MIT/Apache-2.0 | [crates.io](https://crates.io/crates/version_check/0.9.5) |

CORE-003A adds pinned pulldown-cmark with `default-features = false`: only
bitflags and unicase join the existing graph (memchr is already present). The
parser HTML output/CLI and optional SIMD/serde features are excluded. All three
new packages use existing MIT/Apache policy; no license exception is added.
The reviewed local package manifests and LICENSE files agree with the table.

CORE-002A adds serde/serde_json for the wire contract and sha2 for exact source
fingerprints. The optional `schema` feature adds schemars for schema export;
it is disabled in the default library build but included in CI/metadata policy
checks. The table is the full resolved all-feature graph, not a claim that every
optional/target-specific package ships in each app.

The policy checks all resolved/transitive package licenses and origins, rejects
publishable workspace members, members outside core/, and spike dependencies.
Workspace packages must be local; non-workspace packages must use canonical
crates.io. Other sources require deliberate policy review. Cargo.lock pins the
graph; origin approval is a separate constraint.

CORE-002A reviewed these exact additional declarations: `Unlicense OR MIT`
(memchr), `Apache-2.0 OR BSL-1.0` (ryu), `MIT/Apache-2.0` (version_check,
which supplies both license texts), and `(MIT OR Apache-2.0) AND Unicode-3.0`
(unicode-ident). Exceptions are scoped to each reviewed package and version in the
inventory above, and regression tests reject other packages, versions and
expressions. This is not a general SPDX-expression evaluator. The OR alternatives provide an already permitted
MIT/Apache option; unicode-ident additionally carries Unicode notices. Preserve
its included LICENSE-UNICODE in eventual distribution inventories. See
[Cargo license declarations](https://doc.rust-lang.org/cargo/reference/manifest.html#the-license-and-license-file-fields)
and [Unicode License V3](https://www.unicode.org/license.txt).

Further unlisted declarations fail and need dependency/policy review. The parser is added under CORE-003A with the bounds in [Parsing.md](Parsing.md). A dedicated tool such as cargo-deny can replace the
small policy once the graph warrants it; source/license tests remain required.

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
