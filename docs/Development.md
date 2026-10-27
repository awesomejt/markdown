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
| `python tools/check.py common` | Fixture/provenance and capability validation, path/capability/host-filter regressions, local link paths, Markdown lint and whitespace |
| `python tools/check.py core` | Workspace formatting, Clippy with warnings denied, debug/release test harnesses, resolved dependency/license policy and policy regressions |
| `python tools/check.py all` | Both suites |

CORE-002A tests the owned v1 model in debug/release, with default and optional
schema builds. Checked-in goldens cover all node kinds; Rust verifies typed
round trips and generated-schema consistency, while Python independently
validates the schema/fixtures and source hashes. CORE-003A adds parser tests and an original serialized snapshot for the
[bounded basic surface](Parsing.md), including explicit extension deferrals.
CORE-003B1 adds restricted typed metadata, ceiling/atomic-failure/privacy checks
and 4,096 generated metadata cases; see [Front-Matter.md](Front-Matter.md).
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
staged/unstaged tracked changes. An all-zero initial-push base checks the whole
initial tree. Without CHECK_BASE, local/manual runs check the latest commit plus
staged/unstaged changes. Set `CHECK_BASE=origin/main` for the full local branch
range. Untracked files enter the Git whitespace check once staged.
`tools/test_project_state.py` remains a separate host setup check requiring the
installed Codex state backend; CI does not depend on a private user-level skill.

## CI and native boundaries

`.github/workflows/ci.yml` runs common checks on Ubuntu for every PR/main push.
Unknown build/configuration paths select every host by default; only known
prose/operational paths skip host steps. Shared core/spec/fixture/tool/workflow
changes select Linux, macOS and Windows. Known platform-only app paths select
their matching host; iOS selects macOS, and unknown app roots select every host.
Renames count both source and destination. Manual, missing-base and initial-push
events select all hosts. Only PR runs share a cancellable group; main/manual
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
post a separate `/grok review` comment and assess the actual feedback. Keep the
task partial until an owner-authorized merge; do not start dependent phases.
