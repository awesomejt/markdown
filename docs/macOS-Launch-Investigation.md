# MAC-003A macOS launch investigation

On Apple M5 Max, macOS 27.0.1, Xcode 27.1 and Rust 1.99.1,
source `0f982e3` was built in Release with
`PATH="$HOME/.cargo/bin:$PATH" TMPDIR=/private/tmp .build/feas004-venv/bin/python apps/macos/tools/build.py --configuration release`.
Generated fixtures were refreshed with `tools/generate_performance_fixtures.py`.
`apps/macos/tools/measure.py launch --configuration release --runs 5` used
one unrecorded warmup per profile and five Launch Services launches. Times
are milliseconds from process start; the presented mark follows a laid-out
viewport commit and the next display frame. The tool requires every run to
meet the 400 ms D-016 target.

| Profile | Presented median (min–max) | Median launch callback | Median loaded | Median installed | Median committed |
| --- | --- | --- | --- | --- | --- |
| prose 1 MiB | 410.33 (378.25–419.53) | 287.19 | 333.92 | 339.74 | 386.94 |
| prose 5 MiB | 606.65 (580.53–613.88) | 279.97 | 536.72 | 543.19 | 575.01 |
| long lines 1 MiB | 390.51 (385.53–400.38) | 280.14 | 298.20 | 304.12 | 359.78 |

All three profiles had at least one run over 400 ms. These are milestone
timestamps, not additive component timings: loading can overlap launch.
The five-run median of each individual loaded-to-presented difference was
72.29, 65.70 and 98.24 ms respectively. The 5 MiB profile reaches loaded
substantially later than the others, so document loading is a useful next
investigation; this evidence does not isolate file reads, parsing, decoding,
thread scheduling or AppKit startup as the cause. MAC-003B must instrument
those stages and determine a bounded fix. Launch callback time also consumes
a large portion of the target, but this measurement cannot attribute it to
individual frameworks or prove a regression's cause. MAC-003C owns startup
and loaded-to-presented attribution for the 1 MiB profiles.

A direct-executable Instruments Time Profiler attempt reported a debug bundle
path in its trace metadata despite the Release executable argument. Its
attribution was therefore not used. MAC-003B must verify the traced
executable and account for profiler overhead before drawing CPU conclusions.
The trace and local measurement output are ignored build artifacts, not
portable evidence. Cold launch and the other release checks remain open; no
budget is waived.

## MAC-003B load-stage attribution and profiling

On Apple M5 Max, macOS 27.2.1 (26B131), Xcode 27.2 (27B52) and Rust 1.99.1,
`CoreTimings`, `DocumentReadTimings`, `DocumentLoadTimings`, `Measurement.swift`
and `apps/macos/tools/measure.py` were instrumented to report per-stage
durations (`load_stages_ms`) alongside launch milestones (`load_started`).
`DocumentTests.loadStageTimingsAreOrderedAndBounded` verifies finite,
non-negative, parent-bounded stage timings.

### Before and after `Task.detached` scheduling fix

Five warm Launch Services runs per profile (one unrecorded warmup each) with
`apps/macos/tools/measure.py launch --configuration release --runs 5`:

| Profile | Stage / Metric | Before fix median (min–max) ms | After fix median (min–max) ms |
| --- | --- | --- | --- |
| prose 1 MiB | `task_start_ms` | 10.91 (9.73–13.93) | 0.02 (0.02–0.02) |
| prose 1 MiB | `read_total_ms` | 1.35 (1.32–1.58) | 1.48 (1.37–1.51) |
| prose 1 MiB | `core_total_ms` | 34.56 (32.95–34.96) | 35.79 (33.98–36.22) |
| prose 1 MiB | `load total_ms` | 47.34 (44.42–50.18) | 37.59 (35.64–38.07) |
| prose 1 MiB | `presented_ms` | 373.13 (368.86–393.77) | 359.82 (356.79–392.40) |
| prose 5 MiB | `task_start_ms` | 11.40 (6.66–12.96) | 0.02 (0.02–0.03) |
| prose 5 MiB | `read_total_ms` (`coord` / `io`) | 1.94 (0.98 / 0.67) | 1.91 (0.96 / 0.65) |
| prose 5 MiB | `core_parse_ms` | 176.56 (170.96–185.47) | 186.61 (180.93–192.63) |
| prose 5 MiB | `core_encode_ms` | 41.67 (36.79–48.11) | 46.53 (45.37–52.43) |
| prose 5 MiB | `core_copy_ms` | 2.30 (2.13–2.34) | 2.17 (2.07–2.31) |
| prose 5 MiB | `core_total_ms` | 220.62 (210.29–236.28) | 235.57 (231.66–247.80) |
| prose 5 MiB | `publish_hop_ms` | 0.01 (0.01–2.56) | 0.01 (0.01–0.01) |
| prose 5 MiB | `load total_ms` | 237.95 (223.86–245.19) | 237.73 (233.82–250.17) |
| prose 5 MiB | `presented_ms` | 579.87 (558.15–598.16) | 574.55 (569.81–586.28) |
| long lines 1 MiB | `task_start_ms` | 10.97 (10.43–12.34) | 0.02 (0.02–0.02) |
| long lines 1 MiB | `load total_ms` | 19.66 (19.09–21.36) | 11.63 (9.90–15.07) |
| long lines 1 MiB | `presented_ms` | 377.44 (371.29–386.59) | 362.98 (350.36–368.44) |

Findings from stage attribution:

1. **Main-actor scheduling (`task_start_ms`, fixed):** `DocumentLoader.start`
   previously created a `@MainActor`-inherited `Task`, delaying background
   loading by ~11 ms across all profiles until launch/window setup yielded the
   main thread. Switching to `Task.detached(priority: .userInitiated)` reduced
   `task_start_ms` to 0.02 ms and dropped warm `presented_ms` medians by
   ~13–14 ms on both 1 MiB profiles (both now meet the 400 ms D-016 target on
   all five runs). On 5 MiB, `core_total_ms` shifted from 220.62 to 235.57 ms
   in the post-fix run (within run-to-run variance and possible CPU contention
   while core work overlaps main-thread window setup), leaving `load total_ms`
   flat (237.95 vs 237.73 ms) and `presented_ms` ~5 ms lower (579.87 vs
   574.55 ms). `CORE-007` should compare against the post-fix release baseline
   (`core_parse_ms` 186.61 ms, `core_encode_ms` 46.53 ms, `core_total_ms`
   235.57 ms, `presented_ms` 574.55 ms).
2. **File access (`read_total_ms`):** 1.91 ms median on 5 MiB (`NSFileCoordinator`
   0.96 ms, `readFile` I/O 0.65 ms, queue 0.02 ms) — less than 1% of load time.
3. **Swift FFI copy (`core_copy_ms`) and main-actor publish (`publish_hop_ms`):**
   2.17 ms and 0.01 ms respectively on 5 MiB — ~1% of load time.
4. **Dominant cost — shared Rust core (`core_total_ms`):** 235.57 ms median on
   5 MiB (99.1% of load time after the scheduling fix): 186.61 ms in
   `md_job_run` (`core_parse_ms`) and 46.53 ms in `md_snapshot_get_view`
   (`core_encode_ms`).

### Verified Time Profiler attribution and overhead accounting

Before the `Task.detached` edit, `xcrun xctrace record --template 'Time Profiler'`
was run on `build/macos/release/Markdown.app/Contents/MacOS/Markdown --measure repeated=3 build/fixtures/performance/prose-5mib.md`:

- **Executable verification:** `dwarfdump --uuid` reported arm64 UUID
  `A42A8624-4FCA-302B-B768-9B1CFDAC293E` for
  `build/macos/release/Markdown.app/Contents/MacOS/Markdown`. Both the trace
  TOC `<process>` path and every sampled `Markdown` `<binary>` frame matched
  that exact Release bundle path and UUID.
- **Profiler overhead (3 iterations of `repeated=3`):**
  - Unprofiled medians: `core_parse_ms` 195.81 ms, `core_encode_ms` 47.77 ms,
    `task_start_ms` 7.87 ms, `presented_ms` 348.32 ms.
  - Profiled medians (`--target-stdout -`): `core_parse_ms` 188.86 ms,
    `core_encode_ms` 46.99 ms, `task_start_ms` 20.17 ms, `presented_ms`
    361.36 ms.
  - Worker-thread sampling overhead on `md_job_run` and `md_snapshot_get_view`
    was within run-to-run noise; Instruments added ~12 ms on the main thread
    during window creation (`task_start_ms`).
- **CPU hotspots (3,853 total 1 ms samples; 601 in `md_job_run`, 161 in
  `md_snapshot_get_view`, 6 in `CoreDocument(copying:)`):**
  - Inside `md_job_run` (601 inclusive samples; 600 in
    `parse_markdown_v2_cancellable`):
    - Sibling 1 — `markdown_core::parse::finish_v2`: **342 inclusive samples
      (56.9%)**, of which `v2::Document::new` is **341 inclusive (56.7%)**.
    - Sibling 2 — `markdown_core::parse::build`: **255 inclusive samples
      (42.4%)**, including `Builder::event` (158 inclusive) and
      `sha2::sha256::compress256` (**81 self samples, 13.5% of `md_job_run`**).
    - Across both siblings (`v2::Document::new` and `parse::build`),
      `markdown_core::validate::validate_parts` appears in **356 inclusive
      samples (59.2%)** — dominated by `NodeKind::children` (73 self),
      `Sip13Rounds::write` + `RandomState::hash_one` (103 self combined)
      hashing 461,177 string node IDs, `validate_parts` / closures (70 self),
      `memcmp` (26 self) and `id_valid` (17 self).
  - Inside `md_snapshot_get_view` (161 samples):
    - `markdown_ffi::flat::Builder::finish` (54 self, 33.5%) and
      `HashMap<&str, u32, RandomState>` (`get` 37, `Sip13Rounds::write` 20,
      `hash_one` 13, `memcmp` 10, `insert` 7 — **54.0% self** mapping 461,177
      string node IDs back to `u32` indices).

Because these dominant costs live in the shared Rust core and affect all three
desktop hosts, they are tracked in follow-up task `CORE-007` (`MAC-003B ->
CORE-007 -> MAC-003`). D-016 remains unwaived.
