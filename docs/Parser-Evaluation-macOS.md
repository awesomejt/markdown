# macOS parser evaluation supplement

## Result

The 2026-10-24 FEAS-002 supplement supports keeping Rust/pulldown-cmark as
the lead parser candidate. Swift Markdown builds and runs with Command Line
Tools alone, but its public parse-and-walk path is materially slower on these
inputs. Comrak remains the fallback. This does not accept a production stack
or satisfy FEAS-004 rendering or FEAS-006 integration.

The owner explicitly authorized this supplemental work without the missing
project-state backend. No pass was created and `project.yaml` was left
unchanged. This is a one-time exception, not a workflow change.

## Environment and reproduction

- Mac17,6, Apple M5 Max, arm64, 48 GiB RAM.
- macOS 27.0.1, build 26A434; macOS SDK 27.0.
- Swift 6.4, Apple Clang 21.0.0, Rust 1.99.1, CMake 4.4.3.
- Command Line Tools selected; full Xcode absent.
- Release builds. Existing Cargo.lock preserved; Swift Package.resolved added.
- Swift Markdown 0.9.0 (`25cb61d3482054b09ae76ca4f281b1bfe7fe5a43`),
  swift-cmark 0.9.0 (`08ddb528923cc1a6527e02b7a1aee9e516ca749a`).
- Comrak 0.57.0, pulldown-cmark 0.13.5, cmark-gfm 0.29.0.gfm.13;
  existing pins unchanged.

From the root, with Rust/Cargo, CMake and the pinned Python fixture
dependencies available:

```sh
python3 tools/validate_fixtures.py
python3 tools/generate_performance_fixtures.py
bash spikes/feas002/build.sh
swift build --package-path spikes/feas002/swift -c release
python3 spikes/feas002/benchmark_macos.py
```

The session used `.build/feas004-venv/bin/python` for Python and isolated
Rust under ignored `build/feas002-macos/toolchains/` without modifying shell
configuration. The C build script now omits GNU archive-group flags on Darwin;
the Apple linker built the unchanged pinned C probe successfully. SwiftPM
reported two missing Command Line Tools developer-library search paths at
link time; linking and all executable runs succeeded. No signing identity or
production package was used.

The runner emits raw CSV and a summary under `build/feas002-macos/`.
The checked-in [measurement record](../spikes/feas002/results-macos-2026-10-24.yaml)
also preserves environment, pins, binary sizes, range counts, and the extra
single-iteration memory observations. Binaries and downloaded source stay
ignored; this is a disposable spike.

## Parse and walk

Median milliseconds across five fresh processes, each with one untimed
in-process warmup and ten measured iterations for 1 MiB or three for 5 MiB.
Engines ran serially with order rotated between trials. Reading the file is
outside the measured interval; parsing, walking, range access, and per-iteration
tree destruction are included. Inputs were the four frozen performance profiles.

| Input | Comrak | pulldown-cmark | cmark-gfm | Swift Markdown |
| --- | ---: | ---: | ---: | ---: |
| Prose 1 MiB | 6.19 | 2.39 | 8.59 | 74.86 |
| Prose 5 MiB | 34.05 | 11.94 | 42.85 | 377.02 |
| Delimiter storm 1 MiB | 11.29 | 4.97 | 14.66 | 62.96 |
| Long lines 1 MiB | 0.31 | 0.23 | 0.63 | 0.70 |

Swift Markdown's 5 MiB result is about 32 times the pulldown-cmark result.
This compares public API paths, not identical internal parser work: Swift
builds its value tree over a different cmark fork/version and the probe uses
an explicit traversal stack. Comrak walks twice; pulldown emits events and
does not construct the product model. Do not attribute all the difference
to language or assume equivalent output from equal node counts.

## Memory

Darwin `/usr/bin/time -l` reports maximum resident set size in bytes. The
following MiB values use separate fresh processes with one warmup and one
measured iteration, matching the original Linux memory method. Each is one
observation, not a five-trial median.

| Input | Comrak | pulldown-cmark | cmark-gfm | Swift Markdown |
| --- | ---: | ---: | ---: | ---: |
| Prose 5 MiB | 108.53 | 63.63 | 115.77 | 229.30 |
| Delimiter storm 1 MiB | 40.84 | 25.23 | 45.66 | 69.80 |

Repeated parsing matters: the timing trials' median peak RSS for pulldown-cmark
was 119.22 MiB on 5 MiB prose and 116.92 MiB on the delimiter storm, compared
with Comrak's 108.58 and 41.05 MiB. This is consistent with retention/allocation
effects but does not establish the cause or prove a leak. The recommendation
therefore rests on latency and range fit, with a required repeated-session
memory investigation before production acceptance. Peak RSS includes input,
runtime, warmup and allocator retention; it is not live semantic-model memory.

## Semantics, source ranges and limitations

All four engines completed all eight semantic fixtures: 32 smoke runs,
including Unicode and NFD filename lookups. All four completed the 80
performance trials; eight additional memory runs also succeeded. These are
parse/walk smoke checks, not comparisons with semantic goldens or safety tests.

Swift Markdown supplied ranges for 215 of 226 nodes in the GFM tour, 23 of 27
in math/alerts, 74 of 76 in voice parity, and 12 of 19 in long lines. All
ordinary prose nodes had ranges. Comrak positioned every walked node and
pulldown returned an in-bounds byte range for every event in these runs.
Availability does not prove exact range correctness; CORE-002 still owns
span goldens and UTF-8 to platform-index conversion tests.

Swift's smart punctuation was disabled and source positions retained.
The pinned converter attaches table, strikethrough and task-list extensions;
it does not attach the autolink extension or enable footnotes here. Raw HTML
and dangerous links remain untrusted inputs; this probe neither renders nor
executes them. Dedicated policy/conformance work is still required.

Swift Markdown is not Apple-only: the earlier report's platform exclusion
was incorrect. Its pinned package explicitly contains Windows build handling.
The concern is the extra toolchain and semantic implementation if used beside
Rust, not an asserted inability to run elsewhere. This session tested only macOS.

Primary source: [Swift Markdown 0.9.0](https://github.com/swiftlang/swift-markdown/tree/25cb61d3482054b09ae76ca4f281b1bfe7fe5a43),
especially its package manifest, ParseOptions and CommonMarkConverter.

## Checks and next steps

- Passed: release C/Rust/Swift builds, 120 native probe runs, fixture/schema/hash
  validation, deterministic generation, Python compilation, shell syntax,
  whitespace checks, and repository Markdown lint (21 files, zero errors).
- Validation commands: `python3 tools/validate_fixtures.py`,
  `python3 tools/generate_performance_fixtures.py --check`,
  `python3 -m py_compile spikes/feas002/benchmark_macos.py`,
  `bash -n spikes/feas002/build.sh`, `git diff --check`, and
  `npx --offline --yes markdownlint-cli2@0.18.1`. The final build script also
  built successfully under macOS `/bin/bash` 3.2. Nested SwiftPM `.build`
  directories are now excluded from Markdown lint like other build output.
- No rendering, launch, selection, VoiceOver, display scaling, FFI, cold-cache,
  cancellation, live heap, signed package, or cross-platform behavior was tested.
- Blocker: the state backend remains absent; restore it before normal passes.
- Next task: FEAS-004 on this Mac. FEAS-003 still requires Windows. FEAS-006
  remains dependent on both. Investigate repeated-parser RSS when measuring
  semantic-model/session lifetimes; GATE-001 must assess it before acceptance.
