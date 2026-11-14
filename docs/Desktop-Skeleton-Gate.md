# DESK-001 Cross-Desktop Skeleton Gate

## Gate status and scope

`DESK-001` joins the three completed desktop walking skeletons:

- **macOS (`MAC-001`):** [macOS-Skeleton-Acceptance.md](macOS-Skeleton-Acceptance.md) (PRs #39–#48, Apple M5 Max, macOS 27.0.1, AppKit + TextKit 2)
- **Windows (`WIN-001`):** [Windows-Skeleton-Acceptance.md](Windows-Skeleton-Acceptance.md) (PRs #53–#62, Ryzen 9 5950X, Windows 11 x64, WinUI 3 C# Native AOT)
- **Linux (`LNX-001`):** [Linux-Skeleton-Acceptance.md](Linux-Skeleton-Acceptance.md) (PRs #49–#52, #63–#69, Ryzen 9 5950X, CachyOS KDE Plasma Wayland, GTK 4 / gtk-rs)

All three hosts satisfy the [Phase 3 walking-skeleton slice](Implementation-Plan.md#phase-3--desktop-walking-skeletons) against one shared Rust core (`model-v2` behind the 24-symbol `markdown_ffi` C ABI) with zero general-purpose WebViews, zero core-owned filesystem paths, and zero core pointers exposed to UI code.

## Semantic conformance comparison

| Capability / Criterion | macOS (`MAC-001`) | Windows (`WIN-001`) | Linux (`LNX-001`) | Status |
| --- | --- | --- | --- | --- |
| Core ABI & `model-v2` bounds | Static `markdown_ffi`, 5 MiB / 500k / 1M / 128 MiB | `markdown_ffi.dll`, 5 MiB / 500k / 1M / 128 MiB | `libmarkdown_ffi.so`, 5 MiB / 500k / 1M / 128 MiB | Aligned |
| Corpus outlines (8 cases) | 8 / 8 match | 8 / 8 match | 8 / 8 match | Aligned |
| File open, association & typed errors | Open panel, Launch Services, path-free errors | `GetOpenFileNameW`, mutex/pipe association, NTFS ACL & volume errors | `GtkFileChooserNative`, `gio launch` desktop entry, non-UTF-8 paths | Aligned |
| Text / block rendering & links | TextKit 2 off-main attributed string, safe links | Virtualized `ListView` (512 B fragments), safe links | Virtualized `GtkListView` (512 B fragments), epoch-bound safe links | Aligned |
| Tables & TSV clipboard copy | Tab-aligned rows + `TableRowFragment` grid, TSV copy | Tab-separated monospace rows, TSV copy | Aligned table rows, TSV copy | Aligned |
| Task lists | Task markers rendered | `☑` / `☐` with accent + UIA labels | `☑` / `☐` markers | Aligned |
| Local images & resource policy | Folder-contained (`realpath`, `O_NOFOLLOW_ANY`), 16 MiB / 32 MP / 2048 px / 128 MiB, no SVG | Folder-contained, raster-only, 32 MP / 2048 px / 128 MiB, no SVG | Folder-contained (`/proc/self/fd`), raster-only, size/pixel-bounded, no SVG | Aligned |
| Document find | `md_snapshot_find`, eager TextKit 2 attributes | `md_snapshot_find`, UTF-8→UTF-16 mapping, `TextHighlighter` | `md_snapshot_find`, Pango highlights, Match Case / Whole Words | Aligned |
| Text scaling & themes | 9 steps (75–250%), System/Light/Dark, Settings | 9 steps (`Ctrl++`/`-`/`0`), System/Light/Dark, JSON prefs | 9 steps (`Ctrl++`/`-`/`0`), System/Light/Dark, `$XDG_CONFIG_HOME` JSON | Aligned |
| Global selection & clipboard | Continuous `NSTextView`, lazy RTF/plain copy | Cross-fragment drag/keyboard, `CF_UNICODETEXT` + `text/markdown` | Cross-fragment drag/keyboard, `text/plain` + `text/markdown` | Aligned |
| External refresh & restoration | `kqueue` + activation stat, core anchors, AppKit state | `FileSystemWatcher` (100 ms debounce), core anchors, JSON store | `gio::FileMonitor` (100 ms debounce), core anchors, `$XDG_CONFIG_HOME` store | Aligned |
| Accessibility | VoiceOver heading/level/table/image rotors | UIA heading levels & single-reading fix (no duplicate Narrator read) | AT-SPI heading levels & paragraph roles (`LNX-001D1`); off-screen Orca deferred (D-035) | Deviated (accepted) |

## Performance and memory comparison

Measurements below are taken from each host's Release acceptance evidence (`apps/macos/README.md`, `apps/windows/README.md`, `apps/linux/README.md`). Note that macOS `measure.py launch` measures end-to-end Launch Services startup to presented frame, whereas Windows and Linux `measure.py` measure the in-process read/parse/plan pipeline and refresh/copy operations.

| Metric | macOS (`MAC-001G/H`) | Windows (`WIN-001G/H`) | Linux (`LNX-001G/H`) | Target / Notes |
| --- | --- | --- | --- | --- |
| `prose-1mib` warm launch / pipeline | 355 ms first text / 438 ms whole doc (Launch Services) | 170.19 ms pipeline | 89.86 ms pipeline | D-016 ≤ 400 ms (macOS 1/5 at 404 ms) |
| `long-lines-1mib` warm launch / pipeline | 354 ms first text / 277 ms whole doc | 22.56 ms pipeline | 9.20 ms pipeline | D-016 ≤ 400 ms: met |
| `prose-5mib` warm launch / pipeline (stress) | 547 ms first text / 1,155 ms whole doc | 993.20 ms pipeline | 578.36 ms pipeline | > 400 ms on all 3 hosts (open budget) |
| Outline jump | 16.6–46.3 ms (to presented frame) | 0.02–0.03 ms (target resolution) | 0.01 ms (target resolution) | D-016 ≤ 150 ms; Linux frame-clock unmeasured |
| Copy selection (`prose-1mib` / `prose-5mib`) | 5.0 ms / 26.6 ms | 8.84 ms / 75.76 ms | 13.34 ms / 35.14 ms | Within interactive budget |
| External refresh (`prose-1mib` / `5mib` / `long-lines`) | 310 / 1,050 / 167 ms (`10/10` kept place) | 168.91 / 996.00 / 24.70 ms (`kept_place: true`) | 82.14 / 582.07 / 6.28 ms (`kept_place: true`) | Anchors preserved across all 3 hosts |
| Refresh footprint (`prose-5mib`) | 531 → 887–1,043 MiB | 363.02 MiB peak | ~774.9 MiB | macOS retains freed blocks across revisions |
| Repeated-load RSS | 227–581 MiB (5 MiB, 20 loads, allocator wrapper) | ~19.1 MiB (1 MiB) / ~23.9 MiB (5 MiB, 5 loads) | ~51.1 MiB trimmed (1 MiB) / ~153.4 MiB trimmed (5 MiB) | Bounded on all 3 hosts |
| Local dev artifact | Unsigned arm64 `.zip` (~1.1 MB, 43 packages) | Unsigned x64 `.zip` (42 Rust + 19 NuGet packages) | Unsigned x86-64 `.tar.gz` (74 Rust packages) | Relocated verification passed on all 3 hosts |

## Contract bias and capability deviations

1. **No core contract bias found:** All three hosts consume the same `model-v2` C ABI (`24` `md_` exports), pass all 8 conformance cases, enforce identical host bounds (`5 MiB` source, `500k` nodes, `1M` events, `16 MiB` text, `128 MiB` copied payload), and keep raw HTML inert.
2. **Accepted and open deviations:**
   - **D-035 (Linux accessibility):** `LNX-001D1` verified AT-SPI heading levels, paragraph roles, and outline focus; off-screen Orca structural heading navigation and continuous reading across unrealized `GtkListView` rows are deferred to `ADV-001`.
   - **CAP-04 (Sibling images):** All three skeletons enforce canonical folder containment without escaping; sandboxed macOS folder grants move to `MAC-002A`.
   - **CAP-06 (Network shares):** Network-folder watching and remote URIs remain deferred/rejected across all three hosts.
   - **CAP-14 (Mixed-DPI / display pacing):** Multi-display mixed scaling and frame pacing remain unmeasured on all three hosts.
   - **CAP-28 / CAP-29 / CAP-30 (Packaging & signing):** Unsigned local development artifacts are verified only on their build hosts (plus `ubuntu-24.04` CI for Linux); store/distro packaging (`002F`) and signing/notarization require separate owner authorization.
   - **Open native budgets:** Reconciled in [TODO.md](../TODO.md#open-native-budgets) to track 5 MiB prose warm launch (macOS) and 5 MiB pipeline/refresh (> 400 ms on all three hosts), unmeasured Windows/Linux warm launch to presented frame, unmeasured cold launches (≤ 700 ms target) on all three hosts, macOS VoiceOver first 5 MiB line query (3.6 s, ~1 GiB), macOS 5 MiB refresh footprint (887–1,043 MiB), and Linux end-to-end GTK frame-clock jump timing.

## MVP deepening order and materialized tasks

Per [Implementation-Plan.md](Implementation-Plan.md#desktop-mvp-after-desk-001) and [Delivery-Workflow.md](Delivery-Workflow.md#work-in-batches-on-the-current-host), each platform's `002` MVP chain is independent after `DESK-001`, and `HARD-001` (shared release hardening: fuzzing, sanitizers, dependency advisory scan) runs in parallel before `DESK-002`.

- **Per-host phase sequence:**
  - `002A` (Folder capability/access recovery and recents) → `002B` (Native workspace browser and navigation) → `002C` (Workspace search UI).
  - `002D` (Persistent preferences/native settings) runs after `002A` in parallel with `002B`–`002C`; `002E` (Workspace watching/restoration and performance) waits for both `002C` and `002D`, then `002F` (Development packaging and complete R1 conformance) → `002` aggregate.
- **Deepening order:** Retain the D-018 baseline preference (`macOS`, `Linux`, `Windows` when choosing between hosts), while allowing the active host to batch its ready `002A`–`002F` phases without waiting on another OS's counterpart (`Delivery-Workflow.md`).
- **Materialized in `project.yaml`:** `MAC-002A`–`MAC-002F`, `WIN-002A`–`WIN-002F`, and `LNX-002A`–`LNX-002F` (18 tasks), wired into `MAC-002`, `WIN-002`, and `LNX-002` aggregates and `DESK-002`.

## Milestone boundary (D-030)

Closing `DESK-001` completes Phase 3 (Desktop Walking Skeletons) across all three desktop hosts and reaches the **D-030 milestone review boundary** (`review/<milestone>` → `main`, owner-merged).
