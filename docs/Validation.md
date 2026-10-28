# Validation Strategy

Common and core entry points are documented in [Development.md](Development.md).
CORE-001A established portable CI/policy checks; CORE-002A adds v1 model,
schema and golden validation. CORE-003A adds bounded basic parser and safety
regressions under [Parsing.md](Parsing.md); extension/fuzz/performance acceptance
remains in CORE-003B children, and native acceptance remains in platform phases.
CORE-003B1 adds [typed metadata](Front-Matter.md) boundary, atomic-rejection,
copy-budget and deterministic generated-input regressions.

CORE-003B2 adds opt-in v2 source-backed intents/URI targets, strict version
separation, explicit-upgrade tests, v2 schema/golden checks and generated input.
Default v1 behavior/schema compatibility is required throughout.

CORE-003B4 adds the generated [conformance contract](../specs/parsing/conformance/README.md).
The common suite fails when it is stale, and `tests/conformance.rs` checks every
supported expectation. It also adds corpus-seeded mutation and pathological-input
tests, counted heap budgets and release time guards for the performance profiles,
and named-host measurements ([Parser-Budgets.md](Parser-Budgets.md)).

TOOL-002 splits the core suite into tiers (D-028). Hosted PR runs use
`tools/check.py core`; main and manual runs add `core-extended`, which holds
the model-v2-only build and the release build with its time guards. The pass
runner's local `tools/check.py all` covers both tiers before review, so release
guards still gate every PR on the current host.

## Evidence levels

### Documentation changes

- Markdown lint.
- Link/path existence checks for local references.
- `project.yaml` inspection and dependency consistency.
- Review against `README.md`, `AGENTS.md`, and accepted decisions.
- For the capability matrix, `python3 tools/validate_capabilities.py` and
  `python3 tools/test_validate_capabilities.py`. They check structure and
  evidence rules, not technical truth.

### Core changes

- Formatting, lint with warnings denied, unit tests, and documentation tests.
- Semantic golden corpus on every supported host.
- Property/fuzz tests for untrusted parser/search inputs where practical.
- Benchmarks or budget tests for representative large documents.
- Dependency, license, and vulnerability review appropriate to release risk.

### FFI/package changes

- Build every promised architecture/slice in debug and release.
- Run Swift host tests against the packaged artifact, not only Rust source.
- Test typed errors, cancellation, repeated creation/destruction, large values,
  and Swift concurrency checking.
- Confirm symbols and package contents; record exact Xcode/Rust versions.

FEAS-006's [Windows checkpoint](Shared-Core-Windows-Evaluation.md) includes
C#/C++ ABI/layout checks, typed failures, handle cleanup, in-flight cancellation,
panic containment, native source/PDB debugging and normal-DLL package launches.
It does not substitute for Swift/AppKit and GTK package tests, mixed managed/Rust
IDE stepping, full semantic conformance or clean-machine deployment.

CORE-005A's [find and reading metadata](../specs/search/README.md) checks run in
`tools/check.py core`: Unicode/range invariants over the corpus and all v1
goldens, case-fold/cluster/whole-word rules, budgets, cancellation, v1/v2
parity and word/section goldens. Native highlighting is platform 001F work.
CORE-005B's [workspace search](../specs/workspace-search/README.md) checks also
run there: path/filter rules, name ranking, find parity, snippet invariants,
budget stops, cancellation and generated inputs. Native result navigation is
platform 002C work.

CORE-006A's [C ABI](../specs/ffi/README.md) checks run in `tools/check.py core`:
extern-function lifetime/error/panic tests and a current-host C package smoke
with header layout comparison. Swift, C# Native AOT and gtk-rs consumers remain
the platform 001B evidence.

### Native platform UI changes

- Platform-native format/lint/static-analysis rules selected at GATE-001.
- Unit tests for view models and document sessions.
- UI tests for open/navigation/restoration/error flows.
- Native accessibility tooling plus screen-reader/manual scenarios for major
  surfaces (Narrator, VoiceOver, and selected Linux accessibility stack).
- Named-machine checks for lifecycle, memory, pointer/touch/keyboard, sharing,
  printing, high DPI, and platform packaging where applicable.
- Snapshot tests may support review but cannot replace semantic/accessibility
  assertions.

## Initial desktop implementation targets

D-016 accepts these initial targets for the 1 MiB/5 MiB prose and 1 MiB
long-line profiles on named baseline hardware:

- Warm launch to presented content: at most 400 ms.
- Cold launch to presented content: at most 700 ms.
- Off-screen document jumps: at most 150 ms.

Apply D-012: process launch to the first presented frame of document content,
at least five cold and five warm runs with spread, named hardware/toolchains,
cache treatment and package/runtime conditions. Keep outliers and describe
presentation proxies. FEAS-008 supports these targets on its Windows/Linux
workstation; macOS still needs equivalent measurements. Earlier Mac first-draw
and core-entry metrics do not prove compliance.

Validate and explicitly revisit these targets using the real MVP. No current
product, universal hardware guarantee or final release readiness is implied.
Scrolling should sustain the display's normal refresh rate without repeated
main-thread stalls; 5 MiB loads must remain cancellable and view counts bounded.
Obsolete-generation results must never be presented, and offline checks fail
on unexpected network access. Numeric memory/search budgets still need task-
appropriate measurements; Mac repeated-session RSS growth is an open risk.

Historical initial feasibility hypotheses were 500 ms useful content on a
baseline desktop and 1 s on a baseline iPad. D-016 supersedes the desktop
startup hypothesis. No iPad launch target was accepted here. FEAS-003 failures
remain recorded in [the Windows evaluation](Windows-Rendering-Evaluation.md);
do not reinterpret them as passes under a changed metric.

## Required evidence format

```text
Checks run:
Passed:
Failed:
Evidence:
Blocking issues:
Residual risk:
Recommended next task:
```

An unrun check is not a pass. Simulator evidence must be labeled as simulator
evidence; platform and real-device claims require the named environment.
