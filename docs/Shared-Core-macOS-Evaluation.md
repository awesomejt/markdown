# Shared-core macOS integration — FEAS-006 checkpoint

Date: 2026-10-25. **Windows and macOS portions evaluated; FEAS-006 remains
partial pending native Linux/GTK evidence.** No production ABI, model, minimum
OS, package format or stack is accepted here.

## Outcome and scope

The existing Rust/pulldown-cmark core works through its C ABI in native Swift
and AppKit callers on Apple Silicon. Swift imports the header, negotiates
layouts, validates and bulk-copies values, and releases native snapshots through
the owning library before UI construction. Reading/parsing/copying runs off the
main thread; attributed conversion and layout still run on the main thread.

TextKit 2 reached content in a median **216.6 ms for 1 MiB** and **498.2 ms for
5 MiB** prose under the authored-entry/draw metric. The flat Rust projection
differs from FEAS-004's richer Swift Markdown projection: this is not an
equivalent-renderer or controlled language race. TextKit 1 still stalls on
large input. Bounded rows retain their global selection/navigation limitations.
Repeated large parses retain substantial RSS in empty malloc regions even after
native handles return to zero.

Source/reproduction: [spike README](../spikes/feas006/README.md).
Contract: [unchanged header](../spikes/feas006/include/md_core.h).
Durable evidence: [Mac measurements](../spikes/feas006/results-macos-2026-10-25.yaml).

Environment: Mac17,6/Apple M5 Max, arm64, 48 GiB; macOS 27.0.1 (26A434);
Xcode 27.1 (27A9275), Swift 6.4, Rust/Cargo 1.99.1; 5120×2880 display,
backing scale 2, 60 Hz. The Windows checkpoint's Rust source, C header and
Cargo.lock were unchanged. Only this architecture/OS was tested.

## Boundary, ownership and concurrency

Native layouts negotiated as pointer 8, options 40, block 48, snapshot 56 bytes.
The versioned payload/table uses snapshot-local IDs, source byte ranges and text
offsets. Swift bulk-copies both buffers, checks envelopes/IDs/kinds/ranges and
decodes strings into a pointer-free value model. `defer` releases the snapshot
on success or host conversion failure. No UI objects, filesystem paths, parser
arenas or borrowed input lifetimes cross the boundary.

The synchronized cancellation owner stays alive through parsing, closes through
the native allocator and rejects late requests. The core retains its own flag
reference while parsing. Cancellation is cooperative at event boundaries; it
cannot interrupt parser internals before the next event. Invalid non-null
pointer ranges, allocation aborts and access violations are outside the contract.

The final Swift test passed 1,000 sequential load/free cycles, including ten
5 MiB documents; 400 concurrent loads; ten cancellations after observing native
entry; input mutation without changing the native snapshot; copied values after
native release; injected Swift conversion-error cleanup; typed ABI/null/UTF-8/
limit/stale/wrong-class/double-release errors; and fixed-message panic containment
followed by a successful call. Snapshot/token counters returned to zero. Exact
cancellation request-to-return samples are retained in the measurement file.

Swift uses language mode 5 with complete concurrency diagnostics and warnings
treated as errors for adapter/console/AppKit builds. The UI delegate is explicitly
`@MainActor`; main-run-loop timers assert that isolation. Optional diagnostic
symbol lookup opens/closes a temporary process handle instead of sharing a raw
global pointer; the strongly linked Rust dependency remains loaded. This does
not establish a complete Swift 6 migration. The original FEAS-004 path remains
buildable and passed its focused native regression.

## Console and AppKit measurements

Console medians use iterations 2–5 of five after first use. Native-call time
includes validation/publication and bridge overhead; subtraction does not isolate
pure FFI cost. Copy/decode includes buffer copies, validation and Swift model
construction. End-to-end includes release and ABI negotiation.

| Input | Core/model ms | Native call ms | Copy/decode ms | End-to-end ms |
| --- | ---: | ---: | ---: | ---: |
| 1 MiB prose | 2.63 | 2.66 | 0.87 | 3.52 |
| 5 MiB prose | 13.25 | 13.39 | 4.06 | 17.51 |
| 1 MiB delimiters | 5.41 | 5.43 | 0.81 | 6.24 |
| 1 MiB long lines | 0.24 | 0.26 | 0.04 | 0.30 |

The final GUI matrix has **54 trials**: 24 semantic smokes, 24 main performance
trials, two TextKit 1 comparisons, two cancellations and two SwiftUI-host checks.
There were **51 completed presentations, two intended cancellations and one
retained 30-second TextKit 1/5 MiB timeout**. All reached end checks passed; all
31 reached continuous selections exported matching text. All 54 checkpoints
show zero native snapshots/tokens before UI or error completion. All completed
TextKit 2 surfaces retained TextKit 2.

TextKit 2 medians across three fresh processes per performance input:

| Input | Core/model ms | Copy/decode ms | AppKit adapter ms | Entry-to-content draw ms | Peak RSS MiB |
| --- | ---: | ---: | ---: | ---: | ---: |
| 1 MiB prose | 3.03 | 1.00 | 64.49 | 216.62 | 137.31 |
| 5 MiB prose | 14.61 | 5.50 | 318.58 | 498.20 | 247.16 |
| 1 MiB delimiters | 5.89 | 1.01 | 63.45 | 222.71 | 137.72 |
| 1 MiB long lines | 0.35 | 0.17 | 0.55 | 202.35 | 194.86 |

Rows reached 5 MiB content in about 1.74 seconds at 232 MiB RSS, dominated by
eager height calculation. TextKit 1's 1 MiB comparison took about 11.5 seconds
to first content; 5 MiB timed out during UI construction after native release.
The flat model changes attributed-run structure, so the TextKit 1 regression
is not attributed to Rust parsing. No failure is relabeled as successful UI.

The loading window starts work after its first main-loop tick. This corrected
an initial cancellation observer test that missed parsing during AppKit bootstrap.
Both final cancellation cases return `Cancelled`; the in-flight case records
observed native entry. Close/quit during loading requests cancellation and
discards results if closure was requested. Multi-document generations/restoration
are not implemented.

Runs are serial in fixed order on an active desktop with warm caches. Authored
entry excludes earlier dyld/runtime startup; the callback excludes loading-label
drawing and is not first GPU presentation or a cold-launch budget pass. Scroll
timer gaps measure main-thread delay, not display frame pacing or trackpad input.
Long-line TextKit 2 p95 timer delay was about 32 ms. SwiftUI hosting is only a
smoke comparison. Attributed conversion/layout remains synchronous and cannot
be cancelled through the native token.

## Repeated-session memory investigation

Twenty 5 MiB loads in separate native-only versus Swift-copy processes grew RSS
from about 48.5 to **583.4 MiB** and 78.1 to **607.6 MiB**, respectively. Counters
stayed zero and final live malloc usage stayed near **5.1 MiB**, including the
retained 5 MiB input. A post-sampling diagnostic pressure-relief request released
zero bytes. That API is not used by the normal UI/benchmark path or adopted as
a production remedy.

Apple `leaks --noContent --atExit` found **zero leaks/zero leaked bytes** in a
native-only process, with about 183 KiB malloced at exit. A separate `vmmap`
snapshot classified about **565.7 MiB as resident `Malloc Large (empty)`**.
This supports freed-region retention for the observed growth rather than live
snapshot/Swift-model growth. It is not a universal leak proof or an acceptable
long-running RSS budget. The FEAS-002 repeated-parser risk remains a production
concern; no allocator flags or production workaround were adopted.

## Semantics, debugging and development package

All eight frozen level/text outline contracts pass through Swift, including
inline math, without changing expectations. Rich inline types, metadata
properties, complete mappings, resource policy, diagnostics and edit-stable IDs
remain unimplemented. HTML/math remain inert source; links/images are labels.
No document resource fetching or execution path was added.

LLDB stopped in `md_parse_v1` called from Swift, displayed Rust source, showed
mixed Swift/Rust frames and continued to exit 0. ASLR stayed enabled. Earlier
launch/attach attempts stalled; their owned processes were terminated. The owner
approved developer debugging and performed the administrator step, after which
a functional retry succeeded. The agent's separate status query still reported
disabled: no final global-setting state is assumed and no further authorization
changes were made. Apple LLDB reports no Rust language plugin, and optimized
variable inspection is limited; full Rust IDE/expression support is unproven.

A fresh ZIP includes the **normal, probe-free dylib**, Swift console, three
AppKit bundles, C header, fixtures/provenance and locked Rust dependency licenses.
It has no Swift Markdown runtime dependency. ZIP integrity and executable modes
passed. After relocation with macOS `ditto`, the normal console passed 100 loads,
typed errors, cancellation and host-error recovery; all three app surfaces,
5 MiB TextKit 2 and pre-cancellation launched from outside the repo directory
with `DYLD_*` removed. Small fixtures came from the package; the large input was
host-supplied. Python `ZipFile.extractall` had dropped executable bits in the
first extraction harness; the archive records them and `ditto` restores them.
Only default linker ad-hoc signatures are present. No identity signing, resource
seal, notarization, installer, publication or clean-machine/minimum-OS proof.

## Validation and handoff

Passed: six Rust tests in debug/release, formatting and Clippy with warnings
denied; strict Swift concurrency checks; lifecycle/error/cancellation tests;
outline contracts; final GUI matrix with failures retained; live source debugging;
relocated normal-library package checks; original FEAS-004 regression; fixture/path,
Python compile, local links, Markdown and whitespace checks. Earlier observer and
concurrency-development matrices remain ignored; final conclusions use the
fingerprinted actor-checked matrix.

**Resume FEAS-006 on native Linux/GTK next.** Consume the same C facade and verify
ownership, errors, cancellation, copy cost, source debugging and package behavior.
Do not infer facade acceptance from a direct Rust call or transfer Mac/Windows
binaries. Keep FEAS-006 partial until Linux evidence exists. GATE-001 still owns
the production model/bindings, memory budgets and architecture decision.
