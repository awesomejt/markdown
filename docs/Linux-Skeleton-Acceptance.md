# Linux skeleton acceptance

LNX-001I completes the Linux walking skeleton: LNX-001A to LNX-001I (including
LNX-001D1), delivered into `develop` through PRs #49 to #52 and #63 to #67 plus
the LNX-001I PR. This page checks the
[Phase 3 slice](Implementation-Plan.md#phase-3--desktop-walking-skeletons)
and the [LNX-001 milestone](../TODO.md) on this host, and lists what stays
open. Per-task details, commands and measurements are in the
[Linux app README](../apps/linux/README.md); review and completion evidence is in
each task's PR and `project.yaml` pass.

Host: AMD Ryzen 9 5950X (x86-64), CachyOS, KDE Plasma 6.8 Wayland, GTK 4.24.1,
Rust 1.99.1, Python 3.14. Product target: x86-64 Linux with GTK 4.14+ on KDE
Plasma Wayland (D-018); other desktops and architectures are not claimed.

## Slice criteria

| Criterion | Delivered by | Evidence on this host |
| --- | --- | --- |
| Native launch, windows and commands | LNX-001A (PR #49) | GTK 4 shell (`gtk4` 0.11.5 without libadwaita); single-instance D-Bus app (`io.github.awesomejt.markdown`), window lifecycle (New Window, Close Window, Quit), menu accelerators, debug and release smokes on Wayland and X11 (`linux-native` CI) |
| Core package and adapter | LNX-001B (PR #50) | Model-v2 `libmarkdown_ffi.so` package (24 `md_` exports), ABI/model negotiation, owned snapshot copies, bounded worker and admission (2 parses), cancellation, `$ORIGIN` RUNPATH, 15+ consumer tests, and release core probe |
| File open, association and errors | LNX-001C (PR #51) | `GtkFileChooserNative`, Reload (`Ctrl+R`), isolated `gio launch` desktop-entry delivery including non-UTF-8 Linux names, typed path-free errors, bounded reads, generation-safe background loading |
| Headings, prose, lists, links, code; outline; selection | LNX-001D (PR #52) | Virtualized `GtkListView` over off-thread `RenderPlan`; 512-byte fragments; all 8 corpus outlines match; epoch-bound safe links; within-fragment selection; zero leaked handles |
| Document accessibility gate | LNX-001D1 (PR #63) | Automated AT-SPI probe (`probe_accessibility.py`) confirms heading roles with level 1 in model order, paragraph roles, and outline navigation focus; off-screen Orca structural navigation and continuous reading deferred to ADV-001 under D-035 |
| Tables, task lists, images and resource policy | LNX-001E (PR #64) | Aligned table rows with TSV clipboard copy; task list markers (`☑`/`☐`); folder-contained, `/proc/self/fd` TOCTOU-hardened, size- and pixel-bounded raster image reads; SVG/remote/corrupt targets rejected; `gtk::Picture` caching |
| Find, theme and text controls | LNX-001F (PR #65) | Core find over snapshot (`md_snapshot_find`) with Pango highlights, Match Case / Whole Words, and `FindBar`; 9 text scaling steps (`Ctrl++`/`Ctrl--`/`Ctrl+0`); System/Light/Dark themes; atomic `$XDG_CONFIG_HOME` JSON preferences |
| Large documents, global selection, accessibility | LNX-001G (PR #66) | Multi-row cross-fragment selection with drag auto-scrolling, keyboard extension, and Select All (`Ctrl+A`); dual-format GDK clipboard (`text/plain` and `text/markdown`); AT-SPI verification; measured launch/copy/repeated-load budgets with `malloc_trim(0)` |
| External refresh and reading restoration | LNX-001H (PR #67) | `gio::FileMonitor` (inotify) with 100 ms debounce; background external change detection; snapshot anchors; reading position retention across edits; stale-view deletion recovery; `$XDG_CONFIG_HOME` restoration store; two-phase relaunch checks |
| Local development artifact, license inventory, relocated launch | LNX-001I (PR #68) | Unsigned x86-64 `.tar.gz` development artifact with license inventory and notices for all linked Rust packages, zero build-machine paths, `$ORIGIN/../lib` RUNPATH; extracted outside the checkout, verified relocated with ELF/symbol/ldd checks, smokes, association, and relaunch |

Boundaries held throughout: the core receives bytes and opaque revision
tokens, never paths (file access stays in the Linux document layer); the UI
uses owned model values, never core pointers or lifetimes; raw HTML stays inert
text; no WebView renders documents; `apps/linux` remains its own Cargo workspace
under the dependency/license policy.

## What remains open

None of these blocks the skeleton; each is recorded where it will be
resolved, and none is waived.

- **Budgets** ([TODO.md](../TODO.md#open-native-budgets)): prose 5 MiB warm
  launch/refresh (~578 ms / ~582 ms against 400 ms, stress fixture), unmeasured
  cold launches, end-to-end GTK frame-clock jump timing, and mixed-DPI scaling.
- **Manual checks not performed by the agent**
  ([README](../apps/linux/README.md#manual-shell-check)): physical keyboard
  navigation, window controls, About dialog appearance, live desktop portal
  file selection, and real Orca screen-reader sessions. The automated smokes,
  AT-SPI probe, association check, and relaunch check cover these paths with
  synthetic and harness-driven input.
- **Capability deviations** ([matrix](Platform-Capability-Evaluation.md)):
  sibling images require canonical directory containment without escaping
  (CAP-04, LNX-002A); network-share watching and remote URIs are rejected or
  unverified (CAP-06); off-screen Orca structural heading navigation and
  continuous reading are deferred to ADV-001 (D-035); Flatpak/distribution
  packaging and signing are deferred to LNX-002 (CAP-28). The artifact is
  verified only on the Linux host that built it and in `ubuntu-24.04` CI.
- **Not in the skeleton:** Flatpak or distro packaging, signing (requires owner
  approval), and math/diagrams (ADV-005).

## LNX-001 aggregate

The aggregate adds no new implementation; it records already-merged evidence
against the LNX-001 validation expectations (basic corpus, native GTK 4
interaction, accessibility, refresh, restoration, licenses, and package artifact,
plus the native CI job from LNX-001A and the release-mode core consumer from
LNX-001B).

**GTK 4 and rust/gtk-rs.** The implementation uses GTK 4 (`gtk4` 0.11.5 without
libadwaita) in its own `apps/linux` Cargo workspace, completely avoiding
general-purpose WebViews for document rendering, maintaining UI-free isolation
in `apps/linux/core`, and consuming the model-v2 `libmarkdown_ffi.so` package
over the hand-written C ABI.

**Fresh native evidence.** At the `develop` head after PR #68 (`072cf41`, source
unchanged; only docs and task state are edited), `apps/linux/tools/check.py`
passed on the host above:

- generated performance fixtures (`tools/generate_performance_fixtures.py`);
- staged debug and release model-v2 `libmarkdown_ffi.so` core packages with
  24 `md_` symbol audits and zero path leaks;
- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
  and `tools/check_dependencies.py linux` (65-package production graph);
- in Debug and Release: 14 `markdown_linux` unit tests (document access and
  restoration), 19 `render` integration tests, 18 `markdown-linux-core` consumer
  tests, the GTK 4 shell smoke, isolated `gio launch` desktop-entry association
  smoke, and two-phase relaunch restoration check;
- in Release: `core-probe single` on `prose-1mib.md` (92,235 nodes, parse
  56.53 ms, encode 11.37 ms, copy 1.83 ms, 0 native handles), license inventory
  for 74 linked Rust packages, `BuildInfo.json` record, and unsigned x86-64
  `.tar.gz` development artifact verified relocated outside the checkout (smoke,
  association, and relaunch passed).

**Hosted CI.** The `linux-native` GitHub Actions workflow runs
`apps/linux/tools/check.py` on `ubuntu-24.04` (GTK 4.14) under `dbus-run-session`
and `xvfb-run`. The `develop` push run for PR #68's merge
([43542760925](https://github.com/awesomejt/markdown/actions/runs/43542760925)
at `072cf41`) passed every job.

**Milestone.** The skeleton is the D-030 rule 3 boundary for Linux (completing
all three desktop walking skeletons before `DESK-001`, and past the 8-PR cap
since `m-core-fix`). The milestone review follows this aggregate: freeze green
`develop` with no task PR waiting as `review/<milestone>`, then Claude prepares
the packet and only the owner merges into `main`. The open items above go into
that packet.

## Validation

```sh
python3 apps/linux/tools/check.py
python3 tools/check.py common
```

`check.py` runs debug and release builds, `cargo fmt`, Clippy, dependency
policy, unit/consumer tests, GTK smokes, isolated D-Bus association, relaunch
restoration, the release core probe, and relocated release artifact
packaging and verification. Results and hosted runs are in the LNX-001I pass
record and PR #68; aggregate evidence is finalized in the LNX-001 pass record
upon closeout per the delivery workflow.
