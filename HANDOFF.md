# Agent Session and Platform Handoff

Updated: 2026-10-24, macOS session. Source baseline: `fcb5bc5`
(`Added Handoff doc`). The worktree was clean before the backend-restoration update.

This file carries concise, commit-safe context between sessions and machines.
It does not replace [project.yaml](project.yaml) for operational state,
[TODO.md](TODO.md) for acceptance criteria, [MEMORY.md](MEMORY.md) for durable
context, or [agents/Decisions.md](agents/Decisions.md) for accepted decisions.
Recheck the current host and state; observations below are a dated snapshot.

## Resume here

1. Read [AGENTS.md](AGENTS.md) and its required references. Inspect Git status;
   preserve changes made by the owner or other sessions.
2. Run the state inspection command below. The backend is now restored on this
   Mac. Recover any stale work before starting a pass.
3. On macOS, select **FEAS-004**. On Windows, select **FEAS-003**. Run only one
   active pass at a time. Neither evaluation has started in the recorded state.
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

The restored backend's supported inspection succeeded at revision **87**:

| Task | Recorded status | Evidence or next action |
| --- | --- | --- |
| FEAS-001 | Completed | Frozen corpus and validator |
| FEAS-002 | Completed | Linux comparison plus committed macOS supplement |
| FEAS-003 | Pending | Requires a real Windows host |
| FEAS-004 | Pending | Next on this Mac; no rendering prototype exists yet |
| FEAS-005 | Completed | Linux native rendering evaluation |
| FEAS-006 | Pending | Wait for FEAS-003 and FEAS-004 |
| FEAS-007 | Pending | Wait for FEAS-003 and FEAS-004 |

No active or stale passes, working tasks without leases, blockers or warnings
were reported. FEAS-003 and FEAS-004 are eligible. Re-inspect before starting
work; no task status or revision was changed by restoring the skill.

## Owner instructions and authorization boundaries

- The owner requested continuing feasibility on the actual target hosts,
  then specifically requested the Mac supplement to FEAS-002 and FEAS-004.
- The owner explicitly approved **one exception for the FEAS-002 Mac
  benchmarks** without the backend, leaving `project.yaml` unchanged. That
  work is finished; this is not ongoing permission to bypass coordination.
- An exception for FEAS-004 was requested, but **not approved**. The owner
  instead asked what software to install. The backend is now available, so no
  exception is needed: use the normal pass lifecycle for the authorized task.
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

Validation passed on this Mac. That verifies corpus integrity, not complete
macOS filesystem behavior or semantic rendering. Unicode/NFD lookup fixtures
exist; do not casually rename or normalize them. The previously untracked
`fixtures/markdown/paths/café.md` is now tracked in the current checkout.

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
  current host. Successful FEAS-004 profiling has not yet been demonstrated.
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

## FEAS-004 work still to do

No AppKit/SwiftUI rendering prototype, UI benchmark, VoiceOver evaluation,
development package or FEAS-004 result report has been produced.

After restoring coordination, start a bounded FEAS-004 pass and:

1. Record Mac model, OS/build, Xcode/Swift versions, architecture, display
   resolution, refresh rate and backing scale. Reuse the frozen corpus.
2. Compare AppKit native text/layout and bounded or virtualized block
   approaches, with SwiftUI used selectively for suitable shell surfaces.
   TextKit 1 versus TextKit 2 is a candidate comparison, not an accepted design.
3. Measure startup/first useful content, parse/model/presentation costs,
   memory, far navigation, scroll pacing and pathological long-line behavior.
   Distinguish draw callbacks from actual presented frames and enforce timeouts.
4. Exercise selection/copy, heading navigation, keyboard commands, native
   accessibility plus VoiceOver, Retina/scaling, lifecycle and debugging.
   Record unrun checks as unproven. Do not equate an AX tree with screen-reader
   usability. Be aware that accessing `NSTextView.layoutManager` can switch a
   TextKit 2 view into compatibility mode; verify the active text system.
5. Produce a local development bundle/package and document its limitations.
   Ask before signing or publishing, as required by repository instructions.
6. Record results, deviations and remaining risks; update durable context and
   finish the pass through the state helper. FEAS-003 remains the next task
   on Windows; shared-core integration still waits for both evaluations.

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
