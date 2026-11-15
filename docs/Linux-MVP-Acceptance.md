# Linux MVP acceptance

LNX-002F completes the Linux basic MVP delivery chain: LNX-002A through LNX-002F,
delivered into `develop` through PRs #74 to #79 on top of the
[Linux walking skeleton](Linux-Skeleton-Acceptance.md) (LNX-001, PRs #49–#52
and #63–#69) and the [DESK-001 cross-desktop skeleton gate](Desktop-Skeleton-Gate.md).
This page checks the [Phase 4 desktop basic MVP](Implementation-Plan.md#phase-4--desktop-basic-mvps)
and [R1 requirements](Requirements.md#r1--desktop-foundation) on this host, and
records what remains open for `DESK-002` and `HARD-001`. Per-task details,
commands, and measurements are in the [Linux app README](../apps/linux/README.md);
review and completion evidence is in each task's PR and `project.yaml` pass.

Host: AMD Ryzen 9 5950X (x86-64), CachyOS, KDE Plasma 6.8 Wayland, GTK 4.24.1,
Rust 1.99.1, Python 3.14. Product target: x86-64 Linux with GTK 4.14+ on KDE
Plasma Wayland (D-018); other desktops and architectures are not claimed.

## Slice criteria

| Criterion | Delivered by | Evidence on this host |
| --- | --- | --- |
| Folder capability grants, recents, and access recovery | LNX-002A (PR #74) | `FolderCapability` / `FolderError` with canonical root containment and revocation propagation; persistent JSON recents store (`recents.json`, capped at 10 files + 10 workspaces, lossless hex for non-UTF-8 paths, off-main availability checks); `File > Open Folder…` (`Ctrl+Shift+O`), `Recent Items…` dialog, and inline `RecoveryBar` |
| Native workspace browser and keyboard navigation | LNX-002B (PR #75) | Off-main `WorkspaceTree::enumerate` bounded at 100,000 entries / 256 levels (symlinks skipped); `md_workspace_classify_path` C ABI classification (`⚠` invisible-character flag); virtualized `gtk::ListView` + `CountModel` with `TreeItem` / `Level` / `Expanded` accessibility, keyboard navigation, `All` / `.*` / `↻` / `×` controls, and `Ctrl+B` sidebar toggle |
| Native workspace search UI | LNX-002C (PR #76) | `md_workspace_find_names`, `md_workspace_content_admit`, and `md_workspace_content_search` C ABI bindings in `apps/linux/core`; bounded cancellable background `workspace-search` worker with canonical containment; `WorkspaceSearchPanel` (`🔍` toggle, `Ctrl+Shift+F`, `Match Case`, `Whole Words`, `Stop`, Pango snippet highlights, and source-offset/anchor navigation) |
| Persistent preferences and native settings | LNX-002D (PR #77) | Versioned `preferences.json` schema v2 (`show_outline_by_default`, `restore_session_on_launch`, automatic v1→v2 migration, future-version preservation); native GTK 4 Preferences dialog (`Edit > Preferences…`, `Ctrl+,`) with live theme, text scale, outline, restoration, and `Reset to Defaults` controls; two-phase `--relaunch-check` migration verification |
| Workspace watching, restoration, and tuning | LNX-002E (PR #78) | `WorkspaceWatcher` (`gio::FileMonitor` over root, parent, and up to 256 subdirectories; 150 ms debounce, 1,000 ms max coalesce, in-flight refresh gating, automatic access-loss recovery); extended `WindowRestorationState` (`workspace_path`, sidebar visibility, expanded paths, filters, v1 back-compat); precomputed lowercase sort keys in `WorkspaceTree::enumerate` |
| Installable development packaging, R1 conformance, and budgets | LNX-002F (PR #79) | Workspace-contained relative Markdown links (`Target::Relative`) and `#fragment` jumps; collapsible YAML/TOML `Properties (N)` front-matter expander; semantic `Alert` labels (`Note:`–`Caution:`); bounded lazy syntax highlighting (`16 KiB` / `2,048` tokens cap); `--version` and About diagnostics (`model-v2`, ABI v1, anchor v1); `.tar.gz` with `install.sh` / `uninstall.sh`; named-host `window-launch` and `jump-frame` benchmarks |

Boundaries held throughout: the core receives bytes, opaque revision tokens,
and relative paths for classification/search, never host filesystem paths; file
and folder access stays in the Linux document layer under canonical containment;
the UI uses owned model values, never core pointers or lifetimes; raw HTML stays
inert text; no WebView renders documents; `apps/linux` remains its own Cargo
workspace under the dependency/license policy.

## Performance budgets

Release benchmarks on the named Linux host (AMD Ryzen 9 5950X, CachyOS KDE
Plasma Wayland, GTK 4.24.1, 5 runs each; details in
[apps/linux/README.md](../apps/linux/README.md#development-packaging-and-complete-r1-conformance-lnx-002f)):

- **End-to-end `window-launch` (process spawn to GTK `after-paint`):**
  - `prose-1mib`: median `357.83 ms`, max `368.30 ms` (`<= 400 ms` D-016 target met)
  - `long-lines-1mib`: median `409.68 ms`, max `412.50 ms` (single 1 MiB line Pango wrapping)
  - `prose-5mib` (stress): median `914.19 ms`
- **End-to-end `jump-frame` (`frame_clock` `after-paint`):**
  - `prose-1mib`: median `30.26 ms`, max `33.45 ms` (`<= 150 ms` D-016 target met)
  - `prose-5mib`: median `30.74 ms`, max `30.83 ms` (`<= 150 ms` D-016 target met)
  - `long-lines-1mib`: median `181.63 ms`, max `183.16 ms` (1 MiB single-line Pango wrapping on first jump)
- **In-process `launch` / `refresh` pipeline:** `prose-1mib` `82.17 ms` / `78.76 ms` (`5/5` `kept_place: true`), `prose-5mib` `534.90 ms` / `644.60 ms` (`5/5` `kept_place: true`), `long-lines-1mib` `8.90 ms` / `6.24 ms`.

## What remains open

None of these blocks `LNX-002`; each is recorded in
[TODO.md](../TODO.md#open-native-budgets) or the capability matrix for `DESK-002`
or later work, and none is waived.

- **Open budgets:** `prose-5mib` stress warm launch (`914.19 ms` > `400 ms`),
  `long-lines-1mib` `window-launch` (`409.68 ms`) and first `jump-frame`
  (`181.63 ms`) due to 1 MiB single-line Pango wrapping, unmeasured cold launch
  (`<= 700 ms` target), and mixed-DPI display scaling/pacing (`CAP-14`).
- **Manual checks not performed by the agent**
  ([README](../apps/linux/README.md#manual-shell-check)): physical keyboard
  navigation, window controls, About dialog appearance, live desktop portal
  file/folder selection, and real Orca screen-reader sessions.
- **Capability deviations** ([matrix](Platform-Capability-Evaluation.md)):
  off-screen Orca structural heading navigation and continuous reading remain
  deferred to `ADV-001` under D-035; network-share watching and remote URIs
  remain deferred/rejected (`CAP-06`); store/distribution packaging (Flatpak,
  Arch) and signing require separate owner authorization (`CAP-28`, `CAP-29`).

## LNX-002 aggregate

The aggregate adds no new implementation; it records already-merged evidence
against the `LNX-002` validation expectations (R1.1–R1.5 Linux acceptance and
named-host performance budgets across `LNX-002A`–`LNX-002F`, plus `HARD-001`
remaining separate before `DESK-002`).

**Fresh native evidence.** At the `develop` head after PR #79 (`ef97cd9`, source
unchanged; only docs and task state are edited), `python3 apps/linux/tools/check.py`
and `python3 tools/check.py common` passed on the host above:

- `python3 tools/check.py common` passed (fixtures, capability matrix, semantic
  model goldens, path/CI/check-tool unit tests, 8 conformance cases, 612 local
  links, and 82 Markdown lint files with 0 errors);
- staged debug and release model-v2 `libmarkdown_ffi.so` core packages with
  24 `md_` symbol audits and zero path leaks;
- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
  and `tools/check_dependencies.py linux` (65-package production graph);
- in Debug and Release: 26 `markdown_linux` unit tests, 20 `render` integration
  tests, and 20 `markdown-linux-core` consumer tests, plus the GTK 4 shell smoke,
  isolated `gio launch` desktop-entry association smoke, and two-phase relaunch
  restoration check;
- in Release: `core-probe single` on `prose-1mib.md` (92,235 nodes, parse
  67.72 ms, encode 11.63 ms, copy 1.18 ms, 0 native handles), `markdown-linux --version`
  reporting `model-v2 (ABI v1, anchor v1)`, license inventory for 74 linked Rust
  packages, `BuildInfo.json` record, and unsigned x86-64 `.tar.gz` development
  artifact verified both relocated outside the checkout and via full `install.sh`
  / `uninstall.sh` prefix installation.

**Hosted CI.** The `linux-native` job in the `develop` push run for PR #79's
merge ([43674118292](https://github.com/awesomejt/markdown/actions/runs/43674118292)
at `ef97cd9`) passed on `ubuntu-24.04` (GTK 4.14) under `dbus-run-session` and
`xvfb-run`.

## Validation

```sh
python3 apps/linux/tools/check.py
python3 tools/check.py common
```

`check.py` runs debug and release builds, `cargo fmt`, Clippy, dependency
policy, unit/render/consumer tests, GTK smokes, isolated D-Bus association,
relaunch restoration, the release core probe, and relocated plus prefix-installed
release artifact packaging and verification. Aggregate evidence is finalized in
the `LNX-002` pass record upon closeout per the delivery workflow.
