# CORE-003 parsing and safety acceptance

CORE-003 closes the shared parsing aggregate under D-027. The implementation
is already merged: CORE-003A (PR #3), B1 (PR #4), B2/B2F (PR #5), B3 (PR #12),
B4 (PR #13), C (PR #21), D (PR #24), E (PR #27) and F (PR #32).
CORE-003B's aggregate closed in PR #14. This acceptance changes no parser
code, limits, dependencies, schemas, fixtures, goldens or safety policy.
The CORE-003 pass and [PR #35](https://github.com/awesomejt/markdown/pull/35)
record review disposition and completion.

The delivered behavior is defined by [Parsing.md](Parsing.md),
[Front-Matter.md](Front-Matter.md), [URI-Policy.md](URI-Policy.md), and the
[generated conformance contract](../specs/parsing/conformance/README.md).
The frozen corpus is the supported subset plus explicit, reasoned deferrals;
acceptance does not claim complete CommonMark/GFM or web-application parity.

## Acceptance evidence

| Criterion | Delivered checks |
| --- | --- |
| Corpus | `conformance::supported_corpus_matches_reconciled_fixture_expectations` checks all eight cases against each supported token, under v1 and opt-in v2. Version-specific intent pairing and the separate reference fixture preserve the deliberate v1/v2 differences. Common checks reject a stale generated contract. |
| Malformed input and source mapping | Parser tests cover empty/unclosed syntax, Unicode and CRLF, entity/escape lexemes, nested links, tab containers and exact destination spans. Unrepresentable upstream offsets become inert localized block fallbacks, rather than invented spans or unchecked models. |
| HTML and resource safety | Raw HTML and engine blocks remain inert source fallbacks with diagnostics. URI tests cover blocked/obfuscated schemes, controls/bidi, traversal/device forms, encoded values, credentials, remote images and forged v2 source provenance. Neither model grants I/O or activates a URI. |
| Metadata and extensions | Typed first-position metadata has atomic duplicate/unsupported rejection and copy/depth/size boundaries. Tests cover D-032/D-034 text guards, alert look-alikes, footnote resolution, exact math/diagram fallbacks and voice opt-in diagnostics. |
| Bare GFM autolinks | Authored v1/v2 goldens, exact lexemes across text-event splits, code/HTML/link exclusions, normal URI policy, forged-intent rejection, limit accounting and 20,000 generated cases in both models. D-031's documented syntax differences remain. |
| Generated and hostile inputs | Deterministic token generators and 4,000 corpus-seeded mutations per build configuration exercise validated JSON round trips, deterministic reparsing and cancellable-v1 parity. The hostile catalog accepts only a validated model or typed limit; release runs guard pathological runtime. This is not coverage-guided fuzzing. |
| Limits and benchmarks | Tests enforce source/event/node/depth/copied-text failures, source-ceiling behavior, parser heap ceilings and retained snapshot bounds. Release tests apply v1 profile time guards and pathological-catalog guards; the benchmark below also measures v2. |

Evidence is in
[parser.rs](../core/crates/markdown-core/tests/parser.rs),
[metadata.rs](../core/crates/markdown-core/tests/metadata.rs),
[v2.rs](../core/crates/markdown-core/tests/v2.rs),
[autolinks.rs](../core/crates/markdown-core/tests/autolinks.rs),
[conformance.rs](../core/crates/markdown-core/tests/conformance.rs),
[hostile.rs](../core/crates/markdown-core/tests/hostile.rs), and
[budgets.rs](../core/crates/markdown-core/tests/budgets.rs).
The producer and imported-model policy parity added by CORE-002B (PR #18)
is also tested in [model_v1.rs](../core/crates/markdown-core/tests/model_v1.rs).

## Validation and measurements

Source baseline: `cfe3e31924af2be4ea45e830d09b771ff534b591` (`develop`, PR #34).
Host: Apple M5 Max, 48 GiB RAM, macOS 27.0.1, Rust 1.99.1, Python 3.14.7,
Node 24.14.0. Use the documented isolated toolchain on this Mac:

```sh
export CARGO_HOME="$PWD/build/feas002-macos/toolchains/cargo"
export RUSTUP_HOME="$PWD/build/feas002-macos/toolchains/rustup"
export PATH="$CARGO_HOME/bin:$PATH"
RUSTUP_TOOLCHAIN=stable CHECK_BASE=origin/develop .build/feas004-venv/bin/python tools/check.py all
RUSTUP_TOOLCHAIN=stable cargo run --release --locked --features model-v2 --example parse_budget 11
```

Both commands passed on 2026-10-29 (America/Denver). The benchmark ran after
the full suite finished. These are the parser rows from its 11-run output;
times are wall-clock milliseconds, and peak heap is counted live allocation
above baseline, excluding the source string. This is not process RSS or
native launch/presentation latency. `links-synthetic` is the example's
approximately 1.1 MB generated link/IDNA workload, not a frozen corpus profile.

| Profile | Entry | Min ms | Median ms | Max ms | Peak heap MiB |
| --- | --- | ---: | ---: | ---: | ---: |
| prose-1mib | v1 | 24.3 | 24.6 | 30.4 | 34.7 |
| prose-1mib | v2 | 24.4 | 24.7 | 26.8 | 35.2 |
| prose-5mib | v1 | 148.3 | 153.5 | 166.4 | 144.9 |
| prose-5mib | v2 | 149.7 | 150.4 | 157.4 | 146.9 |
| delimiter-storm-1mib | v1 | 55.1 | 55.7 | 60.9 | 66.0 |
| delimiter-storm-1mib | v2 | 55.6 | 56.2 | 58.8 | 66.1 |
| long-lines-1mib | v1 | 5.9 | 6.0 | 6.1 | 2.5 |
| long-lines-1mib | v2 | 5.9 | 5.9 | 6.1 | 2.5 |
| links-synthetic | v1 | 44.3 | 45.1 | 47.5 | 80.0 |
| links-synthetic | v2 | 80.1 | 80.9 | 82.6 | 80.0 |

The roughly 1.8-times v2/v1 median ratio on `links-synthetic` matches the
earlier ratio in [Parser-Budgets.md](Parser-Budgets.md). This workload exercises
v2 URI/IDNA classification and source-provenance validation; its cost is not
representative of prose-only input.

The isolated stable toolchain matches the committed Rust pin. The full suite
includes common/schema/fixture checks, formatting and Clippy, default,
all-feature and model-v2-only debug tests, all-feature release tests, C ABI
smoke, conformance, hostile input, heap/time budgets and dependency/licenses.
The pass and PR record actual command outcomes and CI.

[Hosted baseline CI](https://github.com/awesomejt/markdown/actions/runs/40993878819)
passed on the source baseline. Its prose/state changes select no host work;
[the earlier full three-OS run](https://github.com/awesomejt/markdown/actions/runs/40992357553)
at `0270e6b` passed the core and extended tiers, including release tests.
The parser/core, fixtures, specs and check commands are identical between
that commit and this baseline. This acceptance PR also changes only prose/state;
host jobs may be skipped by the normal CI selector. Hosted Rust checks do
not establish native UI behavior.

## Safety and scope limits

The core never opens files or fetches resources. V1 retains its conservative
ASCII/inline-intent policy, which still permits credential-bearing HTTP(S)
URLs as link intents;
native packages explicitly request v2 under
[D-025](../agents/Decisions.md#d-025--native-hosts-build-and-request-semantic-model-v2),
which rejects credentials and proves bounded source provenance. A v2 relative
`..` path is a capability request, not permission to escape an approved root.
Adapters must decode local targets only once, enforce actual OS containment
and access, and revalidate external activation under native policy.

Voice-over interpretation, diagram engines, syntax highlighting and native
filesystem behavior remain explicit conformance deferrals. All raw HTML is
inert; no safe-HTML subset or sanitizer is delivered. Autolink fidelity
differences and localized source-offset fallbacks remain as documented in
Parsing.md. Metadata is a bounded supported grammar, not general YAML.

Budgets are regression ceilings, not application memory or startup guarantees.
The known dense 8 MiB parser input can peak around 1,152 MiB before a node
limit stops it; the initial upstream scan and final validation are not
interruptible. The separate workspace content-search guard does not bound
reader sessions. Each native 001B still measures memory and sets its session
source bound. Named Windows/Linux workstation timings, native RSS retention,
launch, rendering, accessibility and resource decoding remain platform work.
See [Parser-Budgets.md](Parser-Budgets.md) for dated measurements, limits,
cancellation evidence and the coverage-guided-fuzzing gap.

CORE-004 is next for session acceptance. The remaining shared aggregates run
through CORE-006 before any platform chain starts; this closeout does not
waive that gate.
