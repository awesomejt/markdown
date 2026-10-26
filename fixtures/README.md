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
without reading the sibling repository. One case, `nfd-filename`, is not tracked
upstream: its origin `generator` writes it as an ignored file, and the committed
bytes equal that generator's fixed output at the pinned commit.

All committed Markdown input filenames are ASCII. The Unicode cases retain
their original content hashes and origin paths; `runtime_path` in the manifest
describes the filenames to generate for filesystem tests. The duplicate NFC
copy of the NFD input is no longer stored in Git. `.gitattributes` disables
line-ending conversion for the frozen corpus so Windows checkouts retain the
same hashed bytes as Linux and macOS.

## Unicode filesystem fixtures

Generate Unicode paths only when a filesystem or native-adapter test needs them:

```sh
python tools/generate_path_fixtures.py
python tools/generate_path_fixtures.py --check
```

Use the project environment's Python (`.build/feasibility-venv/Scripts/python.exe`
in Windows Git Bash, or `.build/feasibility-venv/bin/python` on Linux/macOS).
The first command creates a unique run directory under ignored
`build/fixtures/paths/` and writes `paths.json` with paths for native test
consumers. Each run has fresh files and never overwrites an earlier run.
The second command creates, verifies and removes a temporary run on that
filesystem. Use `--output` to probe another filesystem; it names the parent
directory, not an existing file to overwrite. Reports from `--check` describe
temporary files that have already been removed.

The cases cover spaces/non-ASCII characters and both NFC and NFD spellings of
`café.md`. Each spelling lives in its own directory, so the test works even
when the filesystem treats canonical equivalents as one name. Reports use
escaped JSON and record requested spelling, enumerated spelling/code points,
content hash and whether the alternate spelling resolves to the same file.
Canonical-equivalent lookup may succeed or fail depending on the filesystem;
neither outcome alone is an adapter conformance result. Native adapters still
need to prove the fallback and security invariants in the semantic expectations.

Parser/renderer corpus consumers continue using manifest `input` paths for
content tests. Use generated report paths for Unicode filename integration
tests; reading an ASCII fixture does not establish Unicode lookup behavior.

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
performance hashes. It also materializes and checks all three Unicode paths in
a temporary directory on the checkout filesystem, then removes them. It never
accesses `../markdown-viewer`. Generator regression checks run with:

```sh
python tools/test_path_fixtures.py
```
