# Diagram and Math Engine Evaluation — FEAS-009

## Status and scope

2026-10-26: **FEAS-009 closed by owner-approved deferral (D-014)** after the
best-effort Mac checkpoint. The retained measurements are a partial compiler
screen, not complete original technical acceptance or a production selection.
The owner moved Windows/Linux checks and deeper validation to ADV-005 after
DESK-002, before ADV-002. Unrun checks remain unrun. The spike prerequisite no
longer blocks the core MVP gate; other GATE-001 decisions remain open.

Keep Mermaid, math and D2 outside the core MVP as D-013 specifies. ADV-005 will
use the working MVP's real documents, layout, resource policy and performance
context. No Go/web engine, custom port or exception to the native document
guardrail is accepted by this deferral.

Source/reproduction: [spike README](../spikes/feas009/README.md).
Samples, versions and source hashes: [Mac results](../spikes/feas009/results-macos-2026-10-26.json).

## Measured on macOS

Apple M5 Max, 48 GiB RAM, arm64, macOS 27.0.1, Xcode 27.1/Swift 6.4,
Node 24.14.0 and Go 1.27.1. Engines: Mermaid 12.2.0, KaTeX 0.19.1,
D2 0.9.0/dagre, pinned with npm and Go locks. No global install was needed.

Five fresh processes per engine, one discarded operation then five timed
operations per synthetic case: 25 retained samples per case, 90 case outcomes
across 15 processes. The checker passed every expected outcome, including the
expected unsafe references in raw D2 output. These are not 90 security passes.

| Engine/path | Valid-case compile median (min–max), ms | Result and limitation |
| --- | --- | --- |
| KaTeX / JavaScriptCore without a view | Inline 0.114 (0.105–0.143); display 0.191 (0.184–0.217); integral 0.115 (0.110–0.129) | HTML/MathML generated; native presentation unproven |
| Mermaid / offscreen WKWebView | Flowchart 10 (7–1771); sequence 4 (3–6) | Valid SVG, no script/foreignObject in tested final configuration; substantial flowchart outlier retained |
| D2 / direct Go library | Flowchart 0.638 (0.474–1.266); sequence 0.969 (0.711–1.637) | SVG generated; Swift/C ABI and native presenter unproven |

Mermaid's unsafe-link and HTML-label cases took about 2,000 ms each, despite
small inputs. No root cause was established. The webview is offscreen and may
be throttled; these are not production WebKit latency estimates. Native
presentation, rasterization, font/layout equality and GPU delivery were not
measured. A valid SVG/HTML string proves neither visual fidelity nor accessible
native presentation.

| Component observation | Bytes |
| --- | ---: |
| Mermaid minified bundle | 5,494,801 |
| KaTeX minified bundle | 273,081 |
| Swift probe executable | 85,256 |
| Stripped Go/D2 probe executable | 20,220,290 |

KaTeX CSS/fonts, licenses, ABI glue and native presenters are additional.
System WebKit/JavaScriptCore are not bundled here. These sizes are neither
installed application deltas nor iOS archive sizes. Median host-only peak RSS
was 23.6 MiB KaTeX, 99.2 MiB Mermaid and 36.3 MiB Go/D2. WebKit child processes
are excluded, so these numbers cannot compare total engine memory.

There are **no D-012 cold/warm launch-to-presented-content measurements** in
this checkpoint. Unpurged fresh-process timing includes the complete multi-case
suite. Presentation timing needs a real native consumer and remains deferred.

## Safety findings

- KaTeX uses `trust: false`, strict errors, `maxExpand: 100` and `maxSize: 20`.
  Invalid braces and a recursive macro fail; tested JavaScript links and remote
  images do not produce active target attributes. This samples policy behavior,
  not complete HTML/MathML sanitization. See [KaTeX security](https://katex.org/docs/security)
  and [options](https://katex.org/docs/options).
- Mermaid uses strict security, global and flowchart `htmlLabels: false`, and
  input/edge limits. Valid and invalid sources behave as expected. The final
  five-case screen finds no script, foreignObject or tested unsafe URI/event
  attribute. An earlier exploratory configuration with only flowchart-level
  `htmlLabels: false` still returned foreignObject markup and a remote image
  reference; it was corrected before the retained sweep. Do not treat engine
  configuration as a complete output sanitizer. See [Mermaid usage](https://mermaid.js.org/config/usage).
- D2 receives an empty `fs.FS`; the import probe fails without using the host
  filesystem. Raw output nevertheless retains a remote icon and a
  `javascript:` link. Reject or sanitize these before any native renderer or
  browser sees the output. The direct library is not a sandbox, and a Go
  context timeout is not a demonstrated hard limit for every render stage.
  See the pinned [library implementation](https://github.com/d2lang/d2/blob/v0.9.0/d2lib/d2.go).

The WK harness injects only trusted pinned code and uses a deny-network CSP.
It does not observe every attempted request or prove production sandboxing.
Attribute inspection is diagnostic, not a reusable sanitizer. Hostile CSS,
embedded resources, import graphs, expansion limits, cancellation and renderer
isolation need a separate acceptance corpus after the MVP.

## Existing Rust alternatives and port effort

These are **source-reviewed candidates, not built or benchmarked**. Upstream
compatibility statements are claims to validate, not project evidence.

| Feature | Existing candidate | Integration/port assessment |
| --- | --- | --- |
| Mermaid | [Merman 0.9.2](https://github.com/Latias94/merman) advertises headless Rust parsing/layout/SVG and upstream parity tests | Evaluate before considering a new port. Need pinned-version corpus/fidelity, fonts, safe SVG, package and iOS checks. Full independent Mermaid parity is a large parser/layout/compatibility commitment. |
| Math | [RaTeX](https://github.com/erweixin/RaTeX) advertises a Rust core, C ABI and native Apple rendering; [latex-rust](https://github.com/jscarr64/latex-rust) advertises native SVG/layout | Promising ways to avoid an HTML presenter. Need exact KaTeX-syntax coverage, inline metrics, font licensing, accessibility and device evidence. Do not mistake the crates.io `ratex` 0.0.1 placeholder for an evaluated distributable. A new TeX parser/layout/font engine is outside this spike. |
| D2 | [d2-render 0.1.0](https://docs.rs/d2-render/0.1.0/d2_render/) documents a pure-Rust subset and an optional full-language CLI backend | Subset support is a compatibility risk. Compare real D2 documents before selecting it. A full custom port also owns layout/measurement/import behavior and upstream license provenance; defer it. |

Mermaid and KaTeX declare MIT; D2's pinned [license](https://github.com/d2lang/d2/blob/v0.9.0/LICENSE.txt)
is MPL-2.0. Merman declares MIT OR Apache-2.0, and RaTeX's repository declares
MIT. These are upstream declarations, not a complete dependency/font or port
provenance audit. Preserve notices; translated/copied D2 files retain their
source-license obligations as required by the open GATE-001 decision. No
third-party implementation source is vendored in this spike; lock files name
downloaded dependencies. Audit the chosen release and its full dependency/font
tree before distribution.

## iOS fit and software readiness

An arm64 iOS 27 simulator library importing JavaScriptCore and WebKit builds
and links with warnings treated as errors. This proves SDK availability only;
no engine was run in the simulator, no device was tested, and no D2 Go iOS
archive was built. Rust iOS targets are not installed in the isolated toolchain.
Device/runtime work, signing and distribution remain separate.

**No additional Homebrew install is required for this checkpoint.** Xcode,
Node/npm, Go and the existing Python environment suffice. Local npm modules and
Go cache downloads were needed. Do not install Chromium, Graphviz, TeX Live or
additional global engines just for these deferred comparisons. If a later
candidate needs a renderer or device tooling, identify that dependency when its
feature phase is selected.

## Deferred work — ADV-005

The owner approved deferral on 2026-10-26 (D-014). ADV-005 depends on DESK-002
and blocks ADV-002 engine adoption. Before implementation, split it into bounded
platform/engine delivery tasks under the feature-branch/PR workflow.

1. Short native Windows/Linux checks of shortlisted WebView compilers and the
   direct Go library: valid/invalid inputs, denied resources, five compile
   samples, full process memory and package dependencies. Reuse sources/versions.
2. Real-document/reference fidelity, native SVG/image output, inline typography
   and accessibility, robust sanitizer/resource policy, repeated sessions,
   D-012 presentation timings, Rust-candidate comparisons and relevant iOS
   engine/device fit. Further deferrals require an explicit scoped disposition.
3. Recommend per-engine adoption or deferral; do not begin a custom port without
   a concrete compatibility gap and justified scope. GATE-001 no longer needs
   engine selection for the core MVP but still needs its remaining baseline
   choices and delivery-graph rehearsal/materialization.

FEAS-009 closure records the accepted scope change. It does not convert the
missing original acceptance evidence into passes or close GATE-001.
