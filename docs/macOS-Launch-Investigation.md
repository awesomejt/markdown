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
individual frameworks or prove a regression's cause.

A direct-executable Instruments Time Profiler attempt reported a debug bundle
path in its trace metadata despite the Release executable argument. Its
attribution was therefore not used. A follow-up must verify the traced
executable and account for profiler overhead before drawing CPU conclusions.
The trace and local measurement output are ignored build artifacts, not
portable evidence. Cold launch and the other release checks remain open; no
budget is waived.
