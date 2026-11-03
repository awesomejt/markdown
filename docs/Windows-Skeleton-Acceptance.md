# Windows skeleton acceptance

WIN-001I completes the Windows walking skeleton: WIN-001A to WIN-001I, delivered
into `develop` through PRs #53 to #61. This page checks the
[Phase 3 slice](Implementation-Plan.md#phase-3--desktop-walking-skeletons)
and the [WIN-001 milestone](../TODO.md) on this host, and lists what stays
open. The WIN-001 aggregate closes on it, with the
[aggregate evidence](#win-001-aggregate) below. Per-task details, commands and
measurements are in the [Windows app README](../apps/windows/README.md); review
and completion evidence is in each task's PR and `project.yaml` pass.

Host: Windows 11 Pro build 26300.9711, Ryzen 9 5950X x64, Rust 1.99.1,
.NET SDK 10.0.402/runtime 10.0.13, MSVC 14.51.36231, and Windows App SDK 2.5.1
(WinUI package 2.3.9). Product target: Windows 11 x64 (D-018); other OS versions
and architectures are not claimed.

## Slice criteria

| Criterion | Delivered by | Evidence on this host |
| --- | --- | --- |
| Native launch, windows and commands | WIN-001A (PR #53) | WinUI 3 Native AOT shell; window lifecycle (New Window, Close Window, Exit), menu accelerators, PerMonitorV2 DPI awareness in debug and release, locally and in the required `windows-native` CI job |
| Core package and adapter | WIN-001B (PR #54) | Model-v2 `markdown_ffi.dll` package, ABI negotiation, owned snapshot copies, bounded worker and admission (MaxAdmitted=2), cancellation, path-leak checks, 20 consumer tests, and relocated core probe |
| File open, association and errors | WIN-001C (PR #55) | Native Win32 `GetOpenFileNameW` dialog, Reload (`Ctrl+R`), named pipe / mutex association delivery, typed path-free errors (missing, non-regular, non-local, permission/NTFS ACL revocation, detached volume, size cap, UTF-8), chunked 64 KiB reads, off-thread loading |
| Headings, prose, lists, links, code; outline; selection | WIN-001D (PR #56) | Virtualized WinUI `ListView` over off-thread presentation plan; 512-byte fragment chunks; all 8 corpus outlines match; safe links; UIA heading levels; within-fragment text selection; zero leaked core handles |
| Tables, task lists, images and resource policy | WIN-001E (PR #57) | Aligned table rows with TSV clipboard copy; task list markers (`☑`/`☐`); folder-contained, size- and pixel-bounded raster image reads; vector/remote/corrupt targets rejected; off-thread decode with `ImageSourceCache` |
| Find, theme and text controls | WIN-001F (PR #58) | Core find over snapshot (`md_snapshot_find`) with UTF-8 to UTF-16 run mapping and TextHighlighter highlights; nine text scaling steps (`Ctrl++`/`Ctrl--`/`Ctrl+0`); System/Light/Dark themes; LocalSettings JSON preferences |
| Large documents, global selection, accessibility | WIN-001G (PR #59) | Multi-row cross-fragment selection with pointer drag auto-scrolling; Shift+arrow extension; Select All (`Ctrl+A`); dual-format native clipboard (`CF_UNICODETEXT` and `text/markdown`); UIA single-reading fix eliminating Narrator duplicate reading; measured performance budgets |
| External refresh and reading restoration | WIN-001H (PR #60) | FileSystemWatcher with debouncing, parent-dir fallback, burst coalescing; background external change detection; snapshot anchors; reading position retention across edits; deletion recovery with warning banner; window state restoration store; automated relaunch checks |
| Local development artifact, license inventory, relocated launch | WIN-001I (PR #61) | Unsigned x64 zip development artifact with license inventory and notices for the 42 linked Rust packages and 19 NuGet packages, zero build-machine paths; extracted outside the checkout, verified relocated with tests, smoke, association, relaunch, and probe |

Boundaries held throughout: the core receives bytes and opaque revision
tokens, never paths (file access stays in `DocumentAccess`/`DocumentLoader`);
the UI uses owned model values, never core pointers or lifetimes; raw HTML stays
inert text; no WebView renders documents; C# Native AOT with Windows App SDK 2.5.1
is the sole Windows production host (D-017).

## What remains open

None of these blocks the skeleton; each is recorded where it will be
resolved, and none is waived.

- **Budgets** ([TODO.md](../TODO.md#open-native-budgets)): prose 5 MiB warm
  launch/refresh (median ~996 ms against 400 ms, reflecting the large-document
  stress fixture), unmeasured cold launches, physical display scaling and
  mixed-DPI transitions.
- **Manual checks not performed by the agent**
  ([README](../apps/windows/README.md#native-interaction-checks)): physical
  keyboard navigation, Narrator continuous reading across synthetic and real
  assistive technology sessions, real Windows Explorer file association
  registration, and physical removable drive detachment during an active read.
  The automated smoke, association, and relaunch checks cover these paths with
  synthetic and harness-driven input.
- **Capability deviations** ([matrix](Platform-Capability-Evaluation.md)):
  sibling images require canonical directory containment without escaping
  (CAP-04, WIN-002A); network-share file access and volume detachment
  handling are tested via Win32 errors, while UNC network shares are rejected
  by design (CAP-06); full MSIX package identity and registration are
  deferred to WIN-002. The artifact is verified only on the Windows host that
  built it (CAP-28).
- **Deferred review findings and CI** ([TODO.md](../TODO.md#deferred-review-findings)):
  hosted `windows-native` CI runner performance variance.
- **Not in the skeleton:** MSIX store packaging, authentic Authenticode code
  signing certificate (requires owner approval), and math/diagrams (ADV-005).

## WIN-001 aggregate

The aggregate adds no new implementation; it records already-merged evidence
against the WIN-001 validation expectations (basic corpus, native WinUI 3
interaction, accessibility, refresh, restoration, licenses, and package artifact,
plus the native CI job from WIN-001A and the release-mode core consumer from
WIN-001B).

**C# Native AOT and WinUI 3.** The implementation uses C# Native AOT with
WinUI 3 and the Windows App SDK 2.5.1 throughout, completely avoiding general-purpose
WebViews for document rendering, maintaining strict UI-free isolation in the
Core and Document layers, and validating all native behavior without JIT or CoreCLR.

**Fresh native evidence.** At the `develop` head after PR #61
(`5aeed91`, source unchanged; only docs and task state were edited),
`apps/windows/tools/check.py` passed on the host above:

- generated performance fixtures (`tools/generate_performance_fixtures.py`);
- staged debug and release model-v2 core packages with PE export audits and zero path leaks;
- locked NuGet restore (`dotnet restore ... --locked-mode`);
- `dotnet format ... --no-restore --verify-no-changes --severity warn` clean;
- in Debug and Release: 20 core consumer tests, 14 document access/watcher tests,
  20 render/navigation/selection tests, the 13-stage Native AOT smoke,
  multi-process association verification, and real 2-phase relaunch restoration;
- in Release: license inventory generation (42 statically linked Rust packages and
  19 locked NuGet platform packages), `BuildInfo.json` record, local development
  zip artifact packaging (`Markdown-0.1.0-<commit>-windows-x64.zip`), and relocated
  artifact extraction and verification outside the checkout (confirming PE32+ x64
  architecture, 24 exported `md_` symbols, unsigned development status, zero leaked
  build-machine paths, license notices integrity, relocated document/render/smoke tests,
  association check, relaunch check, and relocated single-session core probe with zero
  leaked handles).

**Hosted CI.** The `windows-native` GitHub Actions workflow runs
`apps/windows/tools/check.py` on `windows-latest`. All task PRs (PRs #53 to #61)
passed their native CI runs before merge under D-029.

**Milestone.** The skeleton is the D-030 rule 3 boundary for Windows (and the
8-PR cap has passed since `m-core-fix`). The milestone review follows this
aggregate: freeze green `develop` with no task PR waiting as
`review/<milestone>`, then Claude prepares the packet and only the owner
merges into `main`. The open items above go into that packet.

## Validation

```powershell
python apps/windows/tools/check.py
python tools/check.py common
```

`check.py` ran on this host in debug and release at WIN-001I's head and
again for the aggregate at `5aeed91`. Results and hosted runs are in the
WIN-001I pass record and PR #61; aggregate evidence is recorded in the
WIN-001 pass record upon closeout per the delivery workflow.
