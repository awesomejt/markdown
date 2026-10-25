# Agent Session and Platform Handoff

Updated: 2026-10-25, Windows fixture and state portability repairs. Source
baseline: `c17acb2`, plus the uncommitted fixture/tooling/documentation/state
changes described below. Preserve those changes when syncing.

This file carries concise, commit-safe context between sessions and machines.
It does not replace [project.yaml](project.yaml) for operational state,
[TODO.md](TODO.md) for acceptance criteria, [MEMORY.md](MEMORY.md) for durable
context, or [agents/Decisions.md](agents/Decisions.md) for accepted decisions.
Recheck the current host and state; observations below are a dated snapshot.

## Resume here

Windows update: Developer Mode, Visual Studio 2026/MSVC, Windows SDKs, .NET 10,
Rust MSVC, WPA and the pinned fixture Python dependencies are installed.
Accessibility Insights was not detected in the last readiness check.
State coordination is restored by an owner-requested portability repair.
The installed Windows backend is patched and its original is backed up beside
it. On each other host, apply `tools/patch_project_state_backend.py` before
using the wrapper; it validates the original/patched hashes and will not replace
unknown local edits. The Mac's installed backend was not changed by the isolated
Mac regression run. Use `--platform-info` to confirm Python runtime detection.
The fixture-only exception is finished; subsequent work uses the normal lifecycle.

The fixture fix stores content under ASCII names and generates Unicode paths
at test time. Run these in Windows Git Bash:

```sh
.build/feasibility-venv/Scripts/python.exe tools/validate_fixtures.py
.build/feasibility-venv/Scripts/python.exe tools/test_path_fixtures.py
.build/feasibility-venv/Scripts/python.exe tools/generate_path_fixtures.py --check
.build/feasibility-venv/Scripts/python.exe tools/project_state.py --platform-info
.build/feasibility-venv/Scripts/python.exe tools/test_project_state.py
```

Windows checks passed: eight semantic cases, four performance profiles, three
generated paths and four regression tests. Re-run on Linux/macOS; no new native
evidence was collected there. See [fixture instructions](fixtures/README.md).
The fixture fix did not mutate revision 89. TOOL-001 subsequently used supported
state operations after the initial backend bootstrap and completed at revision
92: project active, no active/stale passes, no unleased working tasks, no warnings;
only FEAS-003 eligible. Re-inspect rather than assuming this snapshot is current.
The native Windows and isolated Mac state
suites each passed eight checks and skipped the other platform's specific check.
Linux state testing remains outstanding. See `AGENT_WORKFLOW.md` for setup and
write-durability limitations. The historical Mac snapshot below predates TOOL-001.

1. Read [AGENTS.md](AGENTS.md) and its required references. Inspect Git status;
   preserve changes made by the owner or other sessions.
2. Run the state inspection command below after confirming backend support on
   the current host. Recover any stale work before starting a pass.
3. Continue **FEAS-003 on a real Windows host**. The macOS FEAS-004 evaluation
   is complete; do not rerun it merely because the session changed. Run only
   one active pass at a time and re-inspect state on the destination host.
4. Keep FEAS-006, FEAS-007 and GATE-001 behind both native evaluations.
   Do not scaffold production applications or accept a production stack yet.

## State and workflow setup

The `project-yaml-state-management` skill and backend are now restored in the
standard Mac user-level Codex location. The repository wrapper finds:

```text
~/.codex/skills/project-yaml-state-management/scripts/project_state.py
```

The skill's `SKILL.md`, Python backend and `agents/openai.yaml` were copied
from the owner's Linux home over SSH, with matching SHA-256 hashes. Its
referenced contract was also restored to `~/.codex/workflows/project-state-v1.md`.
No Linux project files were changed; no credentials or cache files were copied.
On another machine, restore the same skill or point `MARKDOWN_STATE_BACKEND`
at its backend. Do not replace it with an improvised implementation or
manually edit task/pass/lease state.
See [AGENT_WORKFLOW.md](AGENT_WORKFLOW.md) and
[tools/project_state.py](tools/project_state.py).

The local Python environment already has the pinned fixture dependencies:

```sh
.build/feas004-venv/bin/python tools/project_state.py --file project.yaml inspect
```

On another machine, recreate an environment with
`python3 -m venv .build/feasibility-venv`, then install
`tools/requirements-fixtures.txt` with that environment's Python. Virtual
environments, downloaded dependencies and toolchains do not transfer in Git.

Use the supported backend inspection for the latest revision and lease state.
FEAS-004 closeout inspection succeeded at revision **89**: project active,
no active/stale passes, no working tasks without leases, and no warnings.
Only FEAS-003 is eligible.

| Task | Recorded status | Evidence or next action |
| --- | --- | --- |
| FEAS-001 | Completed | Frozen corpus and validator |
| FEAS-002 | Completed | Linux comparison plus committed macOS supplement |
| FEAS-003 | Pending | Requires a real Windows host |
| FEAS-004 | Completed | AppKit/TextKit comparison and native checks; report below |
| FEAS-005 | Completed | Linux native rendering evaluation |
| FEAS-006 | Pending | Wait for Windows FEAS-003 |
| FEAS-007 | Pending | Wait for Windows FEAS-003 |

FEAS-003 is the next dependency-ready task. The skill restoration itself did
not mutate task state; FEAS-004 used the normal start/finish lifecycle.

## Owner instructions and authorization boundaries

- The owner requested continuing feasibility on the actual target hosts,
  then specifically requested the Mac supplement to FEAS-002 and FEAS-004.
- The owner explicitly approved **one exception for the FEAS-002 Mac
  benchmarks** without the backend, leaving `project.yaml` unchanged. That
  work is finished; this is not ongoing permission to bypass coordination.
- An exception for FEAS-004 was requested, but **not approved**. The owner
  instead asked what software to install. The backend is now available, so no
  exception is needed: use the normal pass lifecycle for the authorized task.
- The owner approved a temporary VoiceOver test and confirmed audible document
  reading. Its original off state was restored and verified. This approval
  does not authorize leaving accessibility settings changed.
- This handoff file was explicitly requested for committing and transferring
  notes. No commit, push, signing or publishing was requested of the agent.
- If the backend is still absent, follow the workflow's read-only restriction
  for feasibility work unless the owner explicitly authorizes an exception.
- Keep `markdown-viewer` read-only. No build/runtime sibling dependency or
  shared UI toolkit. Production baseline decisions remain gated.

## Completed evidence to carry forward

### FEAS-001: corpus

Eight semantic cases and four deterministic performance profiles are frozen
with schemas, hashes and Apache-2.0 provenance. Origin:
`markdown-viewer` commit `0ab59c0f7bf91730bf9683b47a5eb2658f331b00`.
See [fixtures/README.md](fixtures/README.md).

Validation passed on the Mac before the Windows portability fix. That verifies
the earlier corpus, not complete macOS filesystem behavior or semantic
rendering. The Windows fix replaces tracked Unicode/NFD filenames and the
redundant NFC alias with ASCII-named content plus runtime path recipes. Original
content hashes and provenance remain unchanged; rerun the new generator on Mac.

### FEAS-002: parser comparison and Mac supplement

Keep Rust/pulldown-cmark as the lead feasibility candidate, Comrak as the
fallback/oracle, and cmark-gfm as the GFM oracle. No production parser or FFI
choice has been accepted. Swift Markdown was measured on Mac and is **not
Apple-only**; the earlier report's platform exclusion was corrected.

On Apple M5 Max, 48 GiB RAM, macOS 27.0.1 (26A434), the supplement completed
120 native probe runs: 80 performance trials, 32 semantic smoke checks and
eight additional memory checks. Median 5 MiB prose parse-and-walk times:

| Engine | Time |
| --- | ---: |
| pulldown-cmark 0.13.5 | 11.94 ms |
| Comrak 0.57.0 | 34.05 ms |
| cmark-gfm 0.29.0.gfm.13 | 42.85 ms |
| Swift Markdown 0.9.0 | 377.02 ms |

**Open risk:** pulldown-cmark peak RSS was 63.63 MiB in a fresh process with
one warmup and one measured 5 MiB parse, versus about 119 MiB in repeated
timing trials. The cause is unproven; investigate allocation/retention and
session/model lifetimes before production acceptance. Do not call it a
confirmed leak or reuse Linux RSS conclusions without the Mac caveat.

These tests exclude rendering, UI launch, product-model construction, FFI,
cancellation, signing and packaging. Semantic smoke checks and available
source ranges do not establish exact semantic/span correctness or safety.

Committed references:

- [Parser evaluation](docs/Parser-Evaluation.md)
- [Mac report, methods and limitations](docs/Parser-Evaluation-macOS.md)
- [Mac measurements](spikes/feas002/results-macos-2026-10-24.yaml)
- [Probe build/reproduction instructions](spikes/feas002/README.md)
- [Mac benchmark runner](spikes/feas002/benchmark_macos.py)
- [Swift dependency lock](spikes/feas002/swift/Package.resolved)

The C build script now handles Darwin linking and was checked under system
Bash 3.2. Nested SwiftPM `.build` directories are excluded from Markdown lint.

### FEAS-005: Linux renderer

GTK 4/gtk-rs is viable for the native shell and small-document text surface.
A monolithic `GtkTextView` is rejected for unbounded production documents:
large-file far-scroll did not converge, a pathological line stalled, and
peak process RSS was roughly 254–289 MiB on the measured Linux host.
LNX-001 must prove virtualized blocks while preserving continuous selection
and accessible reading order. Do not assume macOS has the same outcome.
See [Linux evaluation](docs/Linux-Rendering-Evaluation.md).

## Current Mac setup

Rechecked while writing this handoff:

- **Xcode 27.1 (27A9275) is now installed.** `xcode-select -p` returns
  `/Applications/Xcode.app/Contents/Developer`.
- Instruments and Accessibility Inspector exist inside Xcode. Earlier
  statements that Xcode was absent describe the benchmark session, not the
  current host. FEAS-004 subsequently demonstrated LLDB and Instruments capture.
- The parser benchmarks used Command Line Tools, Swift 6.4, Apple Clang
  21.0.0, Rust 1.99.1 and CMake 4.4.3. Re-record tool versions after the switch
  to Xcode; do not label the old measurements with the new toolchain.
- Python environment: `.build/feas004-venv/`.
- Isolated Rust: `build/feas002-macos/toolchains/cargo/bin/` and
  `build/feas002-macos/toolchains/rustup/`. Shell configuration was not changed.
- Raw parser CSV, build logs and generated summaries are under ignored
  `build/feas002-macos/`; durable measurements are already committed.
- Performance Markdown inputs are generated under ignored
  `build/fixtures/performance/`.
- VoiceOver is included in macOS; no separate screen-reader install is needed.

To reuse the isolated Rust toolchain in the current shell:

```sh
export RUSTUP_HOME="$PWD/build/feas002-macos/toolchains/rustup"
export CARGO_HOME="$PWD/build/feas002-macos/toolchains/cargo"
export PATH="$CARGO_HOME/bin:$PATH"
```

These paths assume the repository root and this host's existing ignored
files. Install/recreate tools on a new machine instead of copying binaries.

## FEAS-004 completed evidence and remaining production work

See [macOS evaluation](docs/macOS-Rendering-Evaluation.md),
[probe instructions](spikes/feas004/README.md) and
[measurements](spikes/feas004/results-2026-10-24.yaml).

Compared TextKit 1, TextKit 2, reusable text rows and a SwiftUI-hosted TextKit 2
surface in 80 successful native trials with no timeouts. All end checks passed;
all 60 continuous-text cases exported matching selections. The measured
display was 5120×2880 pixels, scale 2, 60 Hz. The owner confirmed VoiceOver
document reading; LLDB source debugging, Instruments Time Profiler capture
and local development-bundle assembly/launch were exercised.

Recommend AppKit/TextKit 2 as the lead macOS surface for the next accepted
walking skeleton. On 5 MiB prose its median end request was 0.81 ms versus
2547 ms for TextKit 1. Its first draw was about 1992 ms and peak RSS about
388 MiB. Initial attributed conversion dominates; move work off the main
thread and prove cancellation/incremental presentation. First draw is not
cold launch or a presented frame. Long-line timer delay is not frame pacing.

Reusable rows use few views but lack cross-row selection and heading
navigation. The adapter deliberately simplifies Markdown and does not prove
full semantic or accessibility conformance. Large-document VoiceOver/rotor
behavior, dynamic/mixed scale, restoration, interactive frame pacing, memory
reduction and Rust/Swift FFI remain future acceptance checks. Development
bundles have only linker ad-hoc signatures: no identity signing, resource
seal, notarization or distribution approval. No production baseline was accepted.

Debug/release binaries, four local `.app` bundles, raw metrics/logs and a
Time Profiler trace are under ignored `build/feas004/` and
`spikes/feas004/.build/`. Rebuild on another machine; commit the source,
lockfile, durable measurements, reports, memory, handoff and state together.

## Validation carried over

The FEAS-002 Mac session passed native release builds, its 120 probe runs,
fixture/schema/hash checks, deterministic generation, Python compilation,
shell syntax, whitespace checks and Markdown lint (21 files, zero errors).
Those results apply to the parser supplement, not FEAS-004.

Useful commands after recreating dependencies:

```sh
python3 tools/validate_fixtures.py
python3 tools/generate_performance_fixtures.py --check
git diff --check
npx --yes markdownlint-cli2@0.18.1
```

## Updating this handoff

Before switching sessions or machines, refresh this file with the source
commit, host/toolchain, actual task/pass state, evidence links, unresolved
risks, authorization boundaries and exact next action. Keep detailed results
in their reports. Replace stale setup observations rather than accumulating
transcripts. Never include credentials, signing identities, private document
contents, runtime request envelopes or machine-specific secrets.

Review Git status and commit the intended source, reports and this file
together when the owner chooses. Ignored build output and `.agents/` runtime
handoffs are not a substitute for committed evidence. Committing remains an
owner action unless explicitly delegated.
