# FEAS-010 core extension probe

This disposable spike proves the product's syntax extensions as semantic
passes over pulldown-cmark, with Comrak as the comparison. It is evidence for
the GATE-001 core-extension decision, not production core scaffolding.

Each parser adapter (`src/pd.rs`, `src/cm.rs`) walks its own output into one
small intermediate form with byte spans. The shared passes in `src/common.rs`
then handle front matter, heading slugs, link classification, alert
validation, diagram and math fences, voice-over script syntax and span checks.
So the two adapters differ only in how they walk their parser.

## Reproduce

From the repository root, with Rust and the fixture-tool Python dependencies:

```sh
python3 tools/generate_performance_fixtures.py
cargo build --release --manifest-path spikes/feas010/Cargo.toml
cargo test --release --manifest-path spikes/feas010/Cargo.toml
python3 spikes/feas010/check.py --json build/feas010/check.json
python3 spikes/feas010/bench.py > build/feas010/bench.csv
```

`check.py` compares both engines with every frozen semantic expectation and
lists what it can't evaluate without an engine or renderer. `bench.py` runs
five fresh-process trials per input, engine and mode, with one untimed warmup:

- `parse` walks the parser output only, as FEAS-002 did;
- `full` adds every shared extension pass;
- `script` also forces the voice-over pass on non-script documents.

To print each parser's raw behavior on edge cases before the shared passes:

```sh
cargo test --release --manifest-path spikes/feas010/Cargo.toml -- --ignored --nocapture
```

The checked-in [results](results-linux-2026-10-25.yaml) record the environment,
medians and findings. Binaries and raw CSV stay under ignored `target/` and
`build/feas010/`.
