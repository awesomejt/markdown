# FEAS-002 parser probe

This disposable spike compares parse-and-walk cost for Comrak, pulldown-cmark,
and cmark-gfm. It is evidence for the parser decision, not production core
scaffolding.

## Reproduce

From the repository root:

```sh
python3 tools/generate_performance_fixtures.py
spikes/feas002/build.sh
python3 spikes/feas002/benchmark.py > build/feas002/trials.csv
```

The build script pins cmark-gfm to `0.29.0.gfm.13` and Cargo.lock pins the two
Rust crates. cmark-gfm needs `CMAKE_POLICY_VERSION_MINIMUM=3.5` when configured
with CMake 4.4. The probe enables comparable published extensions where they
exist, parses each UTF-8 file, walks all produced nodes/events, and discards the
result. Rendering, semantic-model conversion, and FFI are deliberately absent.

The checked-in result summary records the test environment, medians, peak RSS,
and limitations. Generated dependencies, binaries, fixtures, and raw results
remain under ignored `build/` or Cargo `target/` directories.

## Interpretation guardrails

- Compare orders of magnitude and scaling, not sub-millisecond differences.
- pulldown-cmark emits events; Comrak and cmark-gfm allocate trees. Its speed
  and memory advantage does not include building the product's semantic tree.
- Comrak and cmark-gfm produce the same node count on the synthetic profiles,
  making their measurements particularly comparable.
- A parser renderer's HTML safety defaults do not replace the native product's
  URL, raw HTML, image, and resource policies.
