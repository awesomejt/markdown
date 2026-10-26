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

The initial crate has no behavior tests: Cargo's empty unit/doc test harnesses
pass but prove no product behavior. Model/parser tests belong to later phases.
Local links check file existence, not heading anchors or network availability.
`tools/test_project_state.py` remains a separate host setup check requiring the
installed Codex state backend; CI does not depend on a private user-level skill.

## CI and native boundaries

`.github/workflows/ci.yml` runs common checks on Ubuntu for every PR/main push.
Shared core/spec/fixture/tool/workflow changes select the Rust suite on Linux,
macOS and Windows. Platform-only app paths select their matching host; iOS
paths select macOS. Docs-only changes run common checks and skip host steps.
Manual dispatch selects all hosts. `Checks / complete` fails if a required job
fails or is cancelled. Host-selection logic has regression tests.

Actions use pinned source commits, read-only contents permission and no retained
checkout credentials. No deploy, signing, release or automatic merge is present.
Native app build jobs are added in MAC/LNX/WIN-001A when those apps exist;
running a portable Rust suite on a hosted runner does not prove native UI,
minimum-OS packaging, accessibility or performance acceptance.

CORE-001A includes GATE-001's approved documentation/state closeout in its PR,
as permitted by the delivery workflow. After checks pass, set the PR ready,
post a separate `/grok review` comment and assess the actual feedback. Keep the
task partial until an owner-authorized merge; do not start dependent phases.
