# Feasibility Review — REV-001

## Purpose and result

Date: 2026-10-25. The owner asked for a quality review of FEAS-001 through
FEAS-007 before GATE-001. This review checked each report against its durable
raw data and reproduced everything the Linux workstation allows. It doesn't
accept any baseline; GATE-001 still owns that decision.

**Overall, the feasibility evidence is sound.** Every number that could be
recomputed from raw data or rerun on this host matched. Each study records its
failures and timeouts instead of dropping them. The gaps are in what was not
tested, and one of them is material for GATE-001: no virtualized document view
on Windows or Linux has yet shown document-wide selection and screen-reader
reading order.

## Method

- Read all seven reports and their durable measurement files.
- Recomputed report tables from raw per-trial data where it is recorded.
- Reran on the FEAS-005 Linux workstation (CachyOS, Ryzen 9 5950X, Rust
  1.99.1, glibc 2.44):
  - fixture validation;
  - the FEAS-002 benchmark;
  - single and repeated parse RSS;
  - the FEAS-006 Rust tests and Clippy;
  - the FEAS-006 Linux self-test.
- Compared every copied fixture byte-for-byte with `markdown-viewer` commit
  `0ab59c0` through read-only `git show`; that repository wasn't changed.
- Windows and macOS results couldn't be rerun from this host. They were checked
  against their checked-in data only.

Rerun outputs stay under ignored `build/review/`.

## Findings by study

| Study | Verdict | Evidence from this review |
| --- | --- | --- |
| FEAS-001 corpus | Sound; one provenance error, now fixed | Seven of eight inputs are byte-identical to the pinned commit. `nfd-filename` isn't tracked upstream: `testdata/mkfixtures.sh` generates it as an ignored file. Its bytes equal that script's fixed output, so content was right but the manifest claimed a tracked origin. The manifest now names the generator, and the schema and validator accept it. |
| FEAS-002 parsers | Sound; reproduced | Linux medians reproduced within about 12% with the same ranking: pulldown-cmark 5 MiB prose 22.9 ms against 22.8 ms recorded; Comrak 87.2 against 90.3 ms; cmark-gfm 89.4 against 90.2 ms. |
| FEAS-003 Windows rendering | Sound; recomputed exactly | All twelve performance medians and all twelve timeouts recompute exactly from the 60 raw trials. |
| FEAS-004 macOS rendering | Sound; weaker record | The report agrees with its data file, but the file keeps only medians and ranges. Per-trial values stayed in ignored build output, so the tables can't be independently recomputed. |
| FEAS-005 Linux rendering | Sound but narrow | KDE Wayland only, scale 1 only. Orca was never run. |
| FEAS-006 shared core | Strong | Windows virtualized medians recompute exactly: 381, 383, 459 and 560 ms. The Linux self-test reproduced 1,000 sequential and 400 concurrent loads, ten observed-entry cancellations, typed errors and panic containment with zero live owners. Six Rust tests and Clippy pass. |
| FEAS-007 capability matrix | Adequate | Mechanically validated, but self-reviewed only. Most cells rest on vendor documentation rather than measurement. |

## New evidence

### The macOS repeated-parse RSS growth is allocator-specific

FEAS-002's macOS supplement saw pulldown-cmark peak RSS roughly double across
repeated parses, and FEAS-006 macOS grew to about 583 MiB over twenty 5 MiB
loads. On Linux the same work is flat:

| Linux peak RSS | 1 parse | 3 parses | 20 parses |
| --- | ---: | ---: | ---: |
| pulldown-cmark, 5 MiB prose | 40 MiB | 40 MiB | 40 MiB |
| Comrak, 5 MiB prose | 105 MiB | 105 MiB | 105 MiB |
| cmark-gfm, 5 MiB prose | 117 MiB | 116 MiB | 117 MiB |

The Linux FEAS-006 self-test also held resident memory at about 92 MiB across
all ten batches. Together with the macOS `vmmap` result (about 566 MiB in empty
large malloc regions), this places the risk in macOS allocator behavior rather
than in the parser or the facade.

### The macOS growth shows no plateau

The macOS FEAS-006 data has one sample per load. After the first, every one of
the 19 steps adds 27.8–27.9 MiB, in both native-only and Swift-copy runs, while
live malloc usage stays at about 5.1 MiB. The report calls
this retention, but no plateau appears in the recorded samples, so a long
session could keep growing. MAC-001 needs a remedy or an accepted repeated-session
budget with a test that runs long enough to show whether growth stops.

## Cross-cutting gaps

1. **The renderer proof is incomplete on two platforms.** Windows and Linux
   showed that continuous controls fail at scale and that virtualized rows
   render fast. Neither prototyped document-wide selection or screen-reader
   reading order over virtualized blocks. Only macOS (TextKit 2) has a proven
   large-document surface.
2. **Timings aren't comparable across platforms.** Windows measures authored
   `Main` to a composition callback; macOS, Swift entry to first draw; Linux,
   process start to a frame-clock or main-loop tick. Hardware also differs. None
   measures cold launch or presented content, so the 500 ms hypothesis has never
   been tested on one definition.
3. **Samples are small.** Most performance cases have three warm runs with
   little spread reported. That is enough for feasibility, not for budgets.
4. **Scroll smoothness is unmeasured.** Studies recorded main-thread timer
   delays, not display frame pacing under real wheel, trackpad or touch input.
5. **Accessibility evidence is thin.** One owner-confirmed small-document read
   exists for Narrator and for VoiceOver; Linux has none.
6. **Semantic checks are shallow.** Cross-host checks compare heading level and
   text only. Source-span correctness is still untested; CORE-002 owns it.
7. **Packaging is development-only.** No clean-machine, signing, minimum-OS or
   installer test ran on any platform.

## Recommendations for GATE-001

1. Address the renderer gap before or at the gate. Either run a short spike of
   virtualized blocks with document-wide selection and screen-reader order on
   Windows or Linux, or accept the gate with that recorded as an unproven risk
   owned by WIN-001 and LNX-001.
2. Define one “time to useful content” metric, from process launch to presented
   content, and require it for every platform budget.
3. Record the macOS memory growth as a macOS allocator risk with an explicit
   repeated-session budget and a long-running test, not as a parser risk.
4. Require at least five cold and warm runs, with spread, for any measurement
   used to set a budget.

## Checks run

```text
Checks run: fixture validation and generator tests; byte comparison of eight
  fixtures with the pinned commit; FEAS-002 Linux benchmark rerun and repeated
  RSS probe; FEAS-006 Rust tests, Clippy and Linux self-test; recomputation of
  FEAS-003 and FEAS-006 Windows medians; spot checks of FEAS-004, FEAS-006 macOS
  and Linux headline values.
Passed: all of the above; values match their reports.
Failed: nfd-filename provenance described a tracked origin; corrected.
Evidence: this document; fixtures/manifest.yaml; ignored build/review/.
Blocking issues: none for recording the review.
Residual risk: Windows and macOS results were checked against data, not rerun;
  FEAS-004 tables lack durable per-trial values.
Recommended next task: GATE-001 with the owner.
```
