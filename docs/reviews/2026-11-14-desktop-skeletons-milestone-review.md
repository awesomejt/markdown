# Desktop Skeletons Milestone Review (D-030)

Baseline: `develop` frozen at `2c40fc2` (after PR #70).
Target branch: `main` (intended tag `m-desktop-skeletons` upon owner merge).

## Scope: tasks and PRs merged since `m-core-fix`

- **Workflow & housekeeping (`patch/`):** PR #29 (`m-core-fix` sync), PR #30 (`patch/grok-github-reviews`), PR #31 (`patch/require-checks-on-main`).
- **Portable-core closeout (`CORE-003F`, `CORE-001`–`CORE-006`):**
  - `CORE-003F` (PR #32, D-034: front-matter ZWNJ/ZWJ joiners)
  - `CORE-001` (PR #33), `CORE-002` (PR #34), `CORE-003` (PR #35), `CORE-004` (PR #36), `CORE-005` (PR #37), `CORE-006` (PR #38)
- **macOS walking skeleton (`MAC-001`):**
  - `MAC-001A`–`MAC-001I` (PRs #39–#47) and `MAC-001` aggregate (PR #48); see [macOS-Skeleton-Acceptance.md](../macOS-Skeleton-Acceptance.md).
- **Windows walking skeleton (`WIN-001`):**
  - `WIN-001A`–`WIN-001I` (PRs #53–#61) and `WIN-001` aggregate (PR #62); see [Windows-Skeleton-Acceptance.md](../Windows-Skeleton-Acceptance.md).
- **Linux walking skeleton (`LNX-001`):**
  - `LNX-001A`–`LNX-001D` (PRs #49–#52), `LNX-001D1` (PR #63, D-035), `LNX-001E`–`LNX-001I` (PRs #64–#68), and `LNX-001` aggregate (PR #69); see [Linux-Skeleton-Acceptance.md](../Linux-Skeleton-Acceptance.md).
- **Cross-desktop skeleton gate (`DESK-001`):**
  - `DESK-001` (PR #70); see [Desktop-Skeleton-Gate.md](../Desktop-Skeleton-Gate.md). Materialized `MAC-002A`–`F`, `WIN-002A`–`F`, and `LNX-002A`–`F` in [project.yaml](../../project.yaml).

## Decisions taken

- **D-029:** Task PRs merged into `develop` after Grok review with no Blocker/high/critical finding (or after fixing all such findings on the branch).
- **D-030:** Milestone review boundary into `main` (only the owner merges `review/desktop-skeletons` into `main`).
- **D-033:** Unreviewed `patch/` housekeeping PRs for workflow/docs sync.
- **D-034:** Front-matter titles and values allow ZWNJ/ZWJ between visible characters and subdivision-flag tag sequences; keys and bidi controls remain rejected.
- **D-035:** `LNX-001D1` verified AT-SPI heading levels, paragraph roles, and outline focus; off-screen Orca structural heading navigation and continuous reading deferred to `ADV-001`.
- **Architecture boundaries held:** Shared Rust core (`model-v2`, 24-symbol `markdown_ffi` C ABI) with zero general-purpose WebViews, zero core-owned filesystem paths, and zero core pointers exposed to UI code.

## Deferred findings and open native budgets

- **Deferred review findings:** Tracked in [TODO.md](../../TODO.md#deferred-review-findings):
  - `WIN-001A` / PR #53 Windows host clock synchronization (`w32tm`).
  - `WIN-001G` / PR #59 sub-escape/entity character slicing fallback in `SourceBetween`.
  - `WIN-001C` / PR #55 serialized file reads on `markdown-files`.
  - `LNX-001C` / PR #51 single file dispatcher / bounded disposal follow-up.
  - `MAC-001C` / PR #41 duplicate windows for case-variant paths or hard links (deferred to `MAC-002A`).
  - Hosted `macos-native` `cargo-fmt` rustup conflict and runner variance.
- **Open native budgets:** Reconciled across all three desktop hosts in [TODO.md](../../TODO.md#open-native-budgets):
  - 5 MiB prose warm launch (macOS) and 5 MiB pipeline/refresh (> 400 ms on all three hosts).
  - Unmeasured Windows/Linux warm launch to presented frame (D-012/D-016).
  - Unmeasured cold launches (≤ 700 ms target) and mixed-scale display pacing (CAP-14) across all three hosts.
  - macOS first VoiceOver line query on 5 MiB (3.6 s, ~1 GiB) and 5 MiB refresh footprint (887–1,043 MiB).
  - Linux end-to-end GTK frame-clock jump timing.

## CI and local check evidence

- **Frozen `develop` head (`2c40fc2`):** Hosted `Checks` workflow passed (`changes`, `common`, `core` on `ubuntu-latest`/`windows-latest`/`macos-latest`, `complete`).
- **Per-host native acceptance:**
  - macOS (`MAC-001`, PR #48): `apps/macos/tools/check.py` passed in Debug and Release (30 render, 11 document, 13 core tests, bundle smokes, two-launch relaunch check, relocated arm64 `.zip` artifact with 43 packages).
  - Windows (`WIN-001`, PR #62): `apps/windows/tools/check.py` passed in Debug and Release (20 core, 14 document, 20 render tests, 13-stage Native AOT smoke, association check, two-phase relaunch check, relocated x64 `.zip` artifact with 42 Rust + 19 NuGet packages).
  - Linux (`LNX-001`, PR #69): `apps/linux/tools/check.py` passed in Debug and Release (14 unit, 19 render, 18 core consumer tests, GTK 4 smokes, isolated `gio launch` association, two-phase relaunch check, relocated x86-64 `.tar.gz` artifact with 74 Rust packages), plus `ubuntu-24.04` `linux-native` CI.
- **Common checks on `review/desktop-skeletons`:** `python3 tools/check.py common` passed.

## Manual checks for the owner

- **macOS (Apple Silicon, macOS 27):** Interactive Open panel / Finder Open With, physical keyboard find/settings, real VoiceOver reading/rotors and 5 MiB pause, styled copy after quit, real editor external saves, Finder deletion, and `measure.py launch --cold` ([apps/macos/README.md](../../apps/macos/README.md#manual-shell-check)).
- **Windows (Windows 11 x64):** Physical keyboard navigation, real Narrator continuous reading, Explorer file association, and physical removable drive detachment ([apps/windows/README.md](../../apps/windows/README.md#native-interaction-checks)).
- **Linux (KDE Plasma Wayland):** Physical keyboard navigation, window controls, About dialog appearance, live desktop portal file selection, and real Orca screen-reader session ([apps/linux/README.md](../../apps/linux/README.md#manual-shell-check)).

## Owner decisions requested

1. Merge this milestone review PR into `main` (with a merge commit under D-030); then tag `m-desktop-skeletons`, delete `review/desktop-skeletons`, and sync `main` back into `develop` via a `patch/` PR.
2. Confirm whether any open native budget (e.g., 5 MiB stress fixture > 400 ms or macOS VoiceOver 5 MiB line query) should be targeted in `002` MVP tuning or revised as a target.
