# Validation Strategy

## Evidence levels

### Documentation changes

- Markdown lint.
- Link/path existence checks for local references.
- `project.yaml` inspection and dependency consistency.
- Review against `README.md`, `AGENTS.md`, and accepted decisions.

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

## Initial feasibility budgets

These are starting hypotheses to measure and revise at GATE-001, not release
promises:

- A 1 MiB ordinary Markdown file begins showing useful content within 500 ms on
  each selected baseline desktop and 1 s on the selected baseline iPad.
- Scrolling a fully presented representative document should sustain the
  platform's normal refresh rate without repeated long main-thread stalls.
- Opening a 5 MiB file must remain cancellable and must not require an
  unbounded number of platform views at once.
- Search and parse results from an obsolete document generation are never
  presented.
- Offline tests fail on unexpected network access.

FEAS-003 records named Windows measurements in
[the Windows evaluation](Windows-Rendering-Evaluation.md). The spike does not
meet the 1 MiB/500 ms hypothesis: synchronous adapter parsing and layout remain
too expensive. These observations do not approve slower release budgets;
GATE-001 must set credible budgets after the core/incremental rendering work.

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
