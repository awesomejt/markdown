# Semantic and Performance Corpus

This directory is the implementation-independent behavior and performance
input for the native Markdown applications. It is intentionally smaller than
the `markdown-viewer` test tree: each committed case represents a distinct
contract needed by parser, core, or native presentation feasibility work.

## Contents

- `manifest.yaml`: provenance, hashes, requirement traceability, and case paths.
- `markdown/foundation/`: inputs required by the basic desktop product.
- `markdown/advanced/`: deferred inputs that constrain the semantic model.
- `expected/`: parser-independent semantic expectations, not HTML/DOM goldens.
- `performance/profiles.yaml`: deterministic large and hostile input recipes.
- `schema/`: JSON Schema 2020-12 documents expressed as readable YAML.
- `licenses/`: license text covering copied source fixtures.

The source snapshot is `markdown-viewer` commit
`0ab59c0f7bf91730bf9683b47a5eb2658f331b00`, licensed Apache-2.0. Copied inputs
are byte-identical to the paths in the manifest; their hashes are validated
without reading the sibling repository.

## Expectations

Semantic expectation files freeze observable requirements without selecting a
parser AST or rendering implementation. They cover front matter, outline,
feature identification, code/diagram fence classification, link intent,
security invariants, and advanced extension behavior. FEAS-002 may add
candidate-specific measurements, but it must not rewrite expectations merely
to match a candidate parser.

Some advanced details remain intentionally open, such as the exact safe HTML
subset and inline-math heading slug. Those cases carry mandatory invariants and
explicit notes; GATE-001 resolves the remaining choices.

## Performance fixtures

Large files are generated instead of committed:

```sh
python3 tools/generate_performance_fixtures.py --check
python3 tools/generate_performance_fixtures.py
```

The first command verifies deterministic byte counts and SHA-256 values. The
second writes ignored files under `build/fixtures/performance/`. Profiles cover
representative 1 MiB and 5 MiB prose plus hostile delimiters and long lines.

These inputs define comparable work; they do not define performance budgets.
FEAS-002 and the platform rendering spikes record timing and memory on named
hardware using release builds.

## Validation

Requires Python 3 plus the pinned fixture-tool dependencies:

```sh
python3 -m pip install -r tools/requirements-fixtures.txt
python3 tools/validate_fixtures.py
```

Validation checks schemas, unique IDs, paths, hashes, license metadata,
requirement references, corpus completeness, expected case IDs, and generated
performance hashes. It never accesses `../markdown-viewer`.
