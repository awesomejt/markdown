# Agent Session and Platform Handoff

Updated: 2026-10-25, FEAS-006 macOS checkpoint. Source baseline: `c2ddf26`
(`FEAS-006 on Windows`), plus the uncommitted Mac adapter/builds, FEAS-004
conditional reuse, evidence, documentation and state handoff.
Preserve source/locks/header/measurements together; do not transfer environments
or build products to another platform.

This file carries concise, commit-safe context between sessions and machines.
It does not replace [project.yaml](project.yaml) for operational state,
[TODO.md](TODO.md) for acceptance criteria, [MEMORY.md](MEMORY.md) for durable
context, or [agents/Decisions.md](agents/Decisions.md) for accepted decisions.
Recheck the current host and state; observations below are a dated snapshot.

## Future implementation workflow (owner decision, 2026-10-25)

After GATE-001, use the small platform-focused phase catalog in
[Implementation-Plan.md](docs/Implementation-Plan.md) and the branch/PR lifecycle
in [Delivery-Workflow.md](docs/Delivery-Workflow.md). Each phase gets a feature
branch and PR; post a separate comment exactly `/grok review` once ready, address
feedback and wait for owner merge approval. Batch ready work on the current OS.
Record branch/PR/reviewed head/checks and resume instructions before switching.
GATE-001 closeout must materialize the initial child-task graph through the
helper, including early native tasks in the portable-core bucket. This policy
edit does not change the current revision-98 FEAS-006 Linux handoff below.

## Resume here

FEAS-006 Windows and macOS are evaluated; the all-desktop task remains **partial**.
Resume its explicit handoff on native Linux/GTK. The partial outcome
releases the pass lease while retaining logical ownership of FEAS-006; this is
intentional recovery state, not permission to bypass it for another task.
Inspect before resuming and recover only genuinely expired work.

Mac closeout inspection: revision **98**, project active, no active/stale
passes and no warnings. The backend intentionally reports FEAS-006 under
`working_tasks_without_active_lease` and recommends `recover_stale_task` because
`partial` preserves its working status. This pass has an explicit handoff;
resume/recover that same task before selecting another one.

1. Read the repository's required references and inspect Git status.
2. Inspect `project.yaml` with the current host's environment. Recover any
   stale state before starting one pass; the concurrency limit is one.
3. Resume FEAS-006 on native Linux/GTK, then FEAS-007
   capability reconciliation. Windows FEAS-003, macOS FEAS-004 and Linux FEAS-005 are
   evaluated; do not rerun them merely because the host/session changed.
4. Keep production scaffolding/stack acceptance behind GATE-001 and owner
   approval. FEAS-003 found significant renderer/adapter limits, not a finished
   Windows implementation.

The Windows shared-core checkpoint is in
[the report](docs/Shared-Core-Windows-Evaluation.md),
[spike instructions](spikes/feas006/README.md) and
[measurements](spikes/feas006/results-windows-2026-10-25.yaml). The C ABI receives
UTF-8 bytes/limits/cancellation capabilities only. C# copies values and frees
Rust snapshots before UI construction; C++ uses the same header/ownership rules.
At 5 MiB, virtualized authored-entry-to-render was about 459 ms, including about
42.6 ms core parse/model and 19.9 ms managed copy/decode. Continuous WinUI
controls retain layout stalls; cross-row selection/accessibility remains open.

Six Rust debug/release tests, Clippy, C#/C++ ABI clients, 1,000 sequential plus
400 concurrent loads, ten in-flight cancellation tests, fixed-message panic
containment, eight corrected outline contracts and Rust source/PDB debugging
passed. A fresh local package uses the normal DLL (probe exports disabled),
contains no Markdig runtime, and launches all three WinUI variants and the native
client. No signing, installation, registration or publishing was performed.

The macOS checkpoint is in [the report](docs/Shared-Core-macOS-Evaluation.md)
and [measurements](spikes/feas006/results-macos-2026-10-25.yaml). Swift imports
the existing header, validates and copies values, and frees native snapshots
before UI construction. The final 54 GUI trials have 51 completed presentations,
two intended cancellations and a retained TextKit 1/5 MiB timeout. TextKit 2
entry-to-draw medians were about 217/498 ms for 1/5 MiB prose. Strict Swift
concurrency diagnostics, 1,000 sequential/400 concurrent loads, ten observed
in-flight cancellations, eight outlines, live mixed source debugging and fresh
normal-library package relocation passed. No production ABI/model was accepted.

**Mac memory risk:** twenty native-only 5 MiB parses grew RSS to about 583 MiB,
with zero native owners and about 5.1 MiB live malloc usage. Leaks reported zero
leaked bytes; vmmap attributed most growth to `Malloc Large (empty)`. Swift-copy
runs retained similar RSS; diagnostic pressure relief reclaimed zero bytes.
This is not a leak proof or an acceptable repeated-session budget. Keep the risk
open alongside synchronous AppKit conversion/layout, row selection/navigation,
long-line timer delays and incomplete semantics.

Next host: build Rust natively on Linux, negotiate layouts rather than assuming
Mac/Windows sizes, and consume **the C facade** from a GTK host. A direct internal
Rust model call does not prove this boundary. Copy values and release native
owners before GTK work; verify errors, cooperative cancellation after observed
entry, repeated/concurrent sessions, source debugging and a local normal-library
package with dependency licenses. Keep diagnostic exports out of the normal
package. Use scoped relative library loading rather than global loader changes.
Carry both host reports and retain failures. The model has snapshot-local IDs
and partial semantics; GATE-001 still owns the production contract and budgets.

Windows evidence is in [the evaluation](docs/Windows-Rendering-Evaluation.md),
[probe README](spikes/feas003/README.md) and
[durable measurements](spikes/feas003/results-2026-10-25.yaml). The 60-trial
comparison completed 48 trials and retained 12 timeouts. Continuous rich edit
is a bounded-document candidate; virtualized rows need global selection/UIA
reading order. The provisional synchronous Markdig adapter is not a production
parser. C#/C++ hosts work, but the literal C++ smoke is not a language benchmark.

The owner confirmed audible Narrator reading and restored it off. Native copy
matched the GFM fixture; the corrected DPI hook traversed three displays at
150% and restored geometry. Mixed DPI, full external UIA, managed/FFI debugging,
real input frame pacing and clean-machine installation remain unverified.
Computer Use's native pipe was unavailable after recovery attempts. WPR capture
failed on profiling policy (`0xc5585011`); no policy changed and no recording
remains. Native source/PDB debugging and unsigned development-folder launches
passed. No signing, registration, publishing or production baseline was accepted.

Windows setup: Developer Mode; Visual Studio 2026/MSVC; Windows SDKs; .NET 10;
Rust MSVC; WPA; Accessibility Insights 1.1.2924.1; pinned fixture Python packages.
Raw metrics, diagnostics, packages and binaries are under ignored `build/feas003/`.

The state backend is patched on Windows. On another host, apply the guarded
repository patch to the installed skill and verify runtime detection before
coordination. The Mac's installed backend was not changed by its isolated test.
No previous exception authorizes bypassing the normal lifecycle now.

```sh
.build/feasibility-venv/Scripts/python.exe tools/project_state.py --platform-info
.build/feasibility-venv/Scripts/python.exe tools/project_state.py --file project.yaml inspect
.build/feasibility-venv/Scripts/python.exe tools/validate_fixtures.py
```

Use `bin/python` in a Linux/macOS environment. The corpus has ASCII tracked
filenames and generates Unicode path tests dynamically; original hashes and
provenance remain frozen. The workflow/fixture portability checks and Linux
rerun limitations remain documented in `AGENT_WORKFLOW.md` and `fixtures/README.md`.

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
At that historical snapshot only FEAS-003 was eligible; use current inspection now.

| Task | Recorded status | Evidence or next action |
| --- | --- | --- |
| FEAS-001 | Completed | Frozen corpus and validator |
| FEAS-002 | Completed | Linux comparison plus committed macOS supplement |
| FEAS-003 | Completed | Windows report and 60-trial comparison; limits retained |
| FEAS-004 | Completed | AppKit/TextKit comparison and native checks; report below |
| FEAS-005 | Completed | Linux native rendering evaluation |
| FEAS-006 | Partial | Windows/Mac evaluated; resume native Linux/GTK facade/package |
| FEAS-007 | Pending | Capability reconciliation after native evaluations |

Resume the FEAS-006 partial handoff before selecting other work. The skill
restoration itself did not mutate state; FEAS-004 and FEAS-003 used the normal
start/finish lifecycle. Historical revisions below are not current inspections.

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
- For FEAS-006 the owner approved enabling developer debugging and performed
  the administrator step. A live Swift-to-Rust breakpoint subsequently worked,
  although the agent's separate status query still reported disabled. Record
  that discrepancy; do not repeat authorization changes based only on that
  status. Optimized Rust variable inspection and full IDE support are unverified.
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
