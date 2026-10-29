# Development checks

## Setup

Use Git, Rust/rustup, Python 3.14 and Node 24/npm. `rust-toolchain.toml` pins
Rust 1.99.1 with rustfmt and Clippy. Native app SDKs belong to the later
platform delivery phases; common checks do not require GTK, Xcode or WinUI.

Create a local Python environment and install tooling:

```sh
python3 -m venv .venv
.venv/bin/python -m pip install -r tools/requirements-fixtures.txt
npm ci --ignore-scripts --no-audit --no-fund
.venv/bin/python tools/check.py all
```

On Windows use `python` and `.venv/Scripts/python.exe` (Git Bash), or the
equivalent PowerShell paths. Keep environments and native build outputs local.
Existing project environments may be reused if their dependencies match.

The current Mac has an isolated Rust installation outside PATH. It can run the
same checks without installing another toolchain globally:

```sh
export CARGO_HOME="$PWD/build/feas002-macos/toolchains/cargo"
export RUSTUP_HOME="$PWD/build/feas002-macos/toolchains/rustup"
export PATH="$CARGO_HOME/bin:$PATH"
# The isolated stable toolchain is verified as Rust 1.99.1 on this host.
RUSTUP_TOOLCHAIN=stable .build/feas004-venv/bin/python tools/check.py all
```

Do not use that override on another machine without verifying its version.
Normally rustup uses the committed toolchain file automatically.

## Suites

| Command | Scope |
| --- | --- |
| `python tools/check.py common` | Fixture/provenance and capability validation, conformance-contract drift, path/capability/host-filter regressions, local link paths, Markdown lint and whitespace |
| `python tools/check.py core` | PR tier: workspace formatting, Clippy with warnings denied, default and all-features debug tests, resolved dependency/license policy and policy regressions |
| `python tools/check.py core-extended` | Model-v2-only debug tests and all-features release tests, which enable the release-only time guards |
| `python tools/check.py all` | All three; run it locally before every review request (D-028) |

CORE-002A tests the owned v1 model in debug/release, with default and optional
schema builds. Checked-in goldens cover all node kinds; Rust verifies typed
round trips and generated-schema consistency, while Python independently
validates the schema/fixtures and source hashes. CORE-003A adds parser tests and an original serialized snapshot for the
[bounded basic surface](Parsing.md), including explicit extension deferrals.
CORE-003B1 adds restricted typed metadata, ceiling/atomic-failure/privacy checks
and 4,096 generated metadata cases; see [Front-Matter.md](Front-Matter.md).
Feature `model-v2` adds v2 reference/provenance/URI, source-forgery and migration
regressions plus 8,192 generated inputs. Core checks run default, model-v2-only
and all-feature configurations; schemas/source goldens are checked independently.
CORE-004A adds session generation/latest-wins race, cancellation, lifetime,
external-change and anchor regressions ([session contract](../specs/session/README.md)).
CORE-005A adds `tests/search.rs` for [find and reading metadata](../specs/search/README.md):
source-span/UTF-8 invariants, Unicode folding/cluster cases, budgets,
cancellation, v1/v2 parity and word/section goldens.
CORE-005B adds `tests/workspace.rs` for [workspace search](../specs/workspace-search/README.md):
path validation and the shared Markdown/hidden filter, name ranking, content
hits equal to find, snippet invariants, anchors, budget stops, cancellation
and 4,096 generated inputs (`WORKSPACE_CASES` raises the count).
CORE-006A adds `markdown-ffi` extern-function tests (negotiation, handles,
ownership, panics, cancellation, bulk-view round trips) and `tests/c_abi.rs`:
a header constant drift check and a C smoke compiled with the host `cc`
against `core/include`, linked to the built library, comparing struct
layouts and exercising the [C ABI](../specs/ffi/README.md). `check.py` sets
`MD_REQUIRE_C_SMOKE=1` on macOS/Linux so a missing compiler fails; Windows
skips the C smoke until WIN-001B wires MSVC.
CORE-003B4 adds `tests/conformance.rs` against the generated
[conformance contract](../specs/parsing/conformance/README.md). Regenerate the
contract with `python tools/generate_conformance.py` after changing fixture
expectations. It also adds `tests/hostile.rs` (corpus-seeded mutation;
`MD_FUZZ_CASES` raises the default of 4,000) and `tests/budgets.rs` (counted
heap budgets, plus time guards in release). Re-measure named-host numbers with
`cargo run --release --locked --features model-v2 --example parse_budget 11`
(add `-- --cancel` for cancellation latency); see [Parser-Budgets.md](Parser-Budgets.md).
These suites do not prove native UI conformance. Regenerate the schema deliberately with:

```sh
cargo run --locked --example export_schema --features schema > build/document.schema.json
```

Compare it with `specs/semantic-model/v1/document.schema.json`; changes to the
frozen v1 wire contract require compatibility/version review. Exact source
fixture bytes are preserved across checkouts with `.gitattributes`.
Local links check common inline link/image file targets, excluding code fences,
inline code and URI schemes. Titles, angle destinations and percent-encoded
paths are supported; missing targets are aggregated and locally deleted docs
are skipped. Reference-style links, nested labels, heading anchors and network
availability are not checked by this bounded tooling parser.

Whitespace checks use `CHECK_BASE...HEAD` in CI with full history, then include
staged/unstaged tracked changes. A branch's first push has an all-zero base;
it flags a problem only on a line that is new against every other `origin`
branch, judged from HEAD's fork point with each (with rename detection on). Lines inherited through a
merge, conflict resolution or octopus merge pass; committed conflict markers
and new dirty lines fail. The pushed branch is named by `GITHUB_REF_NAME`; when
it is unset or not found, or no other branch exists, the whole tree is checked. Without CHECK_BASE, local/manual runs check the latest commit plus
staged/unstaged changes. Set `CHECK_BASE=origin/develop` for the full local branch
range. Untracked files enter the Git whitespace check once staged.
`tools/test_project_state.py` remains a separate host setup check requiring the
installed Codex state backend; CI does not depend on a private user-level skill.

## CI and native boundaries

`.github/workflows/ci.yml` runs common checks on Ubuntu for every PR and for
pushes to `main`, `develop` and `review/*` (D-030).
Unknown build/configuration paths select every host by default; only known
prose/operational paths skip host steps. Shared core/spec/fixture/tool/workflow
changes select Linux, macOS and Windows. Known platform-only app paths select
their matching host; iOS selects macOS, and unknown app roots select every host.
Renames count both source and destination. Manual, missing-base and initial-push
events select all hosts. A PR push skips the host steps when the previous head is
an ancestor, its `Checks / complete` run by GitHub Actions succeeded, and the
commits since then touch only those prose/operational paths, such as a
`chore(state)` closeout. Force pushes, missing commits and red or cancelled
previous runs fall back to the full rule. Host jobs cache Rust builds with
`Swatinem/rust-cache`. Task PRs run `core`; pushes, manual runs and milestone
PRs into `main` also run `core-extended` (D-028, D-030). Only PR runs share a
cancellable group; push/manual
runs have unique groups so newer pushes cannot cancel their pending checks. `Checks / complete` fails if a required job
fails or is cancelled. Host-selection/event logic, real Git rename/whitespace behavior and inline
link cases have regression tests.

Actions use pinned source commits, read-only contents permission and no retained
checkout credentials. No deploy, signing, release or automatic merge is present.
Native app build jobs are added in MAC/LNX/WIN-001A when those apps exist;
running a portable Rust suite on a hosted runner does not prove native UI,
minimum-OS packaging, accessibility or performance acceptance.

CORE-001A includes GATE-001's approved documentation/state closeout in its PR,
as permitted by the delivery workflow. After checks pass, set the PR ready,
request the Grok review through the `grok` CLI and assess the actual feedback. Keep the
task partial until the merge is approved; do not start dependent phases.
