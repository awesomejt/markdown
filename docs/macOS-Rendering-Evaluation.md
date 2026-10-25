# macOS Native Rendering Evaluation

## Result and recommendation

FEAS-004 measured a real Mac and supports **AppKit with TextKit 2 as the lead
macOS document-surface candidate**, with SwiftUI available for suitable shell
surfaces. Keep large/pathological blocks bounded and move parsing and model
conversion off the UI thread before production. The exact production layout,
minimum OS, FFI and signing/distribution choices remain GATE-001 decisions.

TextKit 2 retained continuous native selection, exported text correctly, and
reached the end of every input. TextKit 1 reached the end too, but its end
request stalled the UI thread for about 2.55 seconds on 5 MiB prose.
Reusable table rows reduced view count and memory but lost cross-row text
selection and document heading navigation; this composition is not ready to
replace the continuous text surface.

## Environment and prototype

- Mac17,6, Apple M5 Max, arm64, 48 GiB RAM.
- macOS 27.0.1 (26A434); Xcode 27.1 (27A9275); Swift 6.4; macOS SDK 27.0.
- One 5120×2880 display, 2560×1440 logical points, backing scale 2, 60 Hz.
- SwiftPM release build, Swift Markdown 0.9.0 and swift-cmark 0.9.0 pinned by
  Package.resolved; this is a disposable presentation adapter, not core/FFI work.
- Native dark appearance was visually inspected. Other OS versions,
  architectures, display scales and mixed-display transitions were not tested.

The [prototype and reproduction instructions](../spikes/feas004/README.md)
describe four variants: TextKit 1, TextKit 2, reusable bounded text rows in an
`NSTableView`, and the same TextKit 2/AppKit surface hosted through SwiftUI.
The SwiftUI variant measures hosting only, not an all-SwiftUI document UI.

The simple renderer styles headings, emphasis and code. Tables use tab/newline
text; lists and task markers are simplified; front matter is not extracted;
links are inert; images are placeholders; HTML and advanced syntax remain
literal source. No WebView or resource-fetch path is used. This is not full
GFM, link-policy, heading-role or image-accessibility conformance.

## Method and results

All eight semantic fixtures ran once per variant, and all four performance
profiles ran three times per variant: **80 successful fresh-process trials**,
zero timeouts or nonzero exits. Runs were serial, in fixed variant order, on
an active desktop with warm caches. Each process had a 20-second timeout.

“First draw” starts at the executable's Swift top-level timestamp and ends
after its first native drawing callback. It includes read/parse/adapter/setup,
but excludes earlier runtime/dyld startup and does not measure the moment a
frame reaches the display. External process duration includes deliberate
observation/scroll timers, so it is not used as startup latency.

Median first-draw milliseconds:

| Input | TextKit 1 | TextKit 2 | Reusable rows | TextKit 2 / SwiftUI host |
| --- | ---: | ---: | ---: | ---: |
| Prose 1 MiB | 520.60 | 490.56 | 760.12 | 492.28 |
| Prose 5 MiB | 2054.71 | 1992.21 | 3370.03 | 1977.58 |
| Delimiter storm 1 MiB | 434.13 | 397.43 | 649.25 | 403.97 |
| Long lines 1 MiB | 162.07 | 162.59 | 219.28 | 176.29 |

The 1 MiB TextKit 2 first-draw median narrowly meets the initial 500 ms
hypothesis under this metric; it does **not** establish a cold-launch or
presented-content budget pass. SwiftUI-host timing is similar at this sample
size; do not infer a speed advantage from the small median differences.

The adapter dominates initial display: on 5 MiB prose the TextKit 2 path spent
about 291 ms parsing and 1529 ms building attributed content, versus 20 ms
setting up the text surface. The row candidate additionally spent about
1352 ms in setup, including eager height calculation. The Rust parser and
future semantic-model cost are not represented by these numbers.

Median end-request milliseconds; every delayed end visibility check passed:

| Input | TextKit 1 | TextKit 2 | Reusable rows | TextKit 2 / SwiftUI host |
| --- | ---: | ---: | ---: | ---: |
| Prose 1 MiB | 506.46 | 0.89 | 1.88 | 1.00 |
| Prose 5 MiB | 2547.29 | 0.81 | 1.08 | 0.91 |
| Delimiter storm 1 MiB | 314.72 | 0.84 | 2.28 | 0.90 |
| Long lines 1 MiB | 120.93 | 17.59 | 1.55 | 18.19 |

Text surfaces were checked by hit testing near the bottom of the visible
viewport after 300 ms; row surfaces by final-row visibility. The row test
does not prove that every pixel of the final row's text is visible. First
paint, an end request and full document layout are distinct operations.

## Memory and scrolling

Median whole-process peak RSS in MiB, from Darwin `/usr/bin/time -l` byte units:

| Input | TextKit 1 | TextKit 2 | Reusable rows | TextKit 2 / SwiftUI host |
| --- | ---: | ---: | ---: | ---: |
| Prose 1 MiB | 187.52 | 168.25 | 158.89 | 172.95 |
| Prose 5 MiB | 520.86 | 387.70 | 362.22 | 391.66 |
| Delimiter storm 1 MiB | 191.06 | 179.02 | 172.22 | 183.84 |
| Long lines 1 MiB | 134.27 | 199.91 | 118.50 | 204.42 |

RSS includes Swift/runtime input, parser allocations, joined attributed text,
all attributed blocks, native layout and allocator retention. Every variant
keeps both the joined text and blocks to simplify comparison; these are not
production app memory budgets or live-heap measurements. The 5 MiB footprint
needs reduction, and the FEAS-002 repeated-parser RSS risk remains open.

The row candidate created seven text cells while traversing the prose profiles
and two for the long-line profile. It targets 8192 UTF-16 units per cell while
preserving composed-character boundaries, but keeps all blocks and eagerly
computes row heights. It proves native view reuse, not a lazy semantic model.

During 60 programmed scroll steps, median per-trial p95 timer gaps were about
17 ms on ordinary prose. For long lines they rose to 27.32 ms in TextKit 2 and
33.46 ms with its SwiftUI host; the median maximum gap was about 35 ms.
Bounded rows remained near 17 ms. These are main-thread timer delays, **not
display frame pacing**. Actual presentation, wheel/trackpad inertia, interactive
selection during scrolling and thermal/cold-cache behavior remain unverified.

## Native interaction, accessibility and lifecycle

All **60 continuous-text runs** selected a composed-character-safe initial
range and exported exactly matching text through native selection writing to
a private pasteboard. The probe must use the view's advertised pasteboard
types; imposing only the modern string type caused the initial harness check
to fail. No user clipboard contents were logged or used by these checks.

All TextKit 2 runs retained a non-nil `textLayoutManager`; the probe never
queries `.layoutManager` on that path. Native heading navigation selected the
GFM heading during external UI inspection. The packaged document exposed an
`AXTextArea`, readable text and scroll controls. An AppKit screenshot showed
native text and selection at backing scale 2. No claim is made for semantic
heading/table/link roles, mixed DPI or a full accessibility audit.

With the owner's approval, VoiceOver was temporarily enabled. Document
interaction/read-all commands were invoked and the **owner confirmed audible
document announcement/reading**. The original off state was restored and
verified in System Settings; the already-enabled caption preference was not
changed. Large-document speech, heading rotor navigation, table semantics and
cross-row reading quality still need dedicated acceptance checks.

The matrix exercised 80 process/window creation and automatic termination
cycles. It does not implement state restoration, multiple-document lifecycle,
permission recovery, native file associations or print/export.

## Debugging, profiling and development bundles

A debug build launched in LLDB, stopped at `main` with a Swift source line,
produced a stack, then continued to exit 0 and a completed automatic check.
ASLR remained enabled. A separate Instruments Time Profiler launch of the
1 MiB TextKit 2 probe recorded successfully; its exported table of contents
includes the time-profile schema. Raw debugger logs and traces stay ignored
under `build/feas004/`; this report makes no sample-based hotspot claim.

Four local `.app` bundles were assembled under `build/feas004/` with the
fixture, license and provenance metadata; the TextKit 2 bundle launched through
the native application surface. The release executable is arm64 and links
Apple system frameworks and Swift libraries; dependencies are not read from
the sibling repository. `codesign -dv` reports only the linker's default ad-hoc
signature, no team identity, unbound Info.plist and no sealed resources.
No signing identity, resource sealing, notarization, Gatekeeper distribution
claim or publishing was used. The bundle's declared macOS 13 floor is a
prototype manifest value, not evidence that this build works on macOS 13.

## Follow-up and validation

Recommend AppKit/TextKit 2 for MAC-001's next walking-skeleton hypothesis.
Before production, prove off-main-thread immutable model construction,
cancellation, incremental updates, bounded pathological content, reduced
memory, semantic accessibility and actual scroll frame pacing. Reusable rows
need a document-level selection/navigation/accessibility design before adoption.
SwiftUI shell responsibilities remain open beyond the hosting smoke comparison.

Checks run: release/debug builds; 80 corpus trials; 60 matching selection
exports; native AX and Retina visual inspection; owner-confirmed VoiceOver
reading/restoration; LLDB source breakpoint; Instruments capture; development
bundle assembly/signature inspection; fixture/schema/hash validation; Python
compilation; Markdown lint; local link and whitespace checks.

Failed or limited hypotheses: TextKit 1 far-navigation latency; synchronous
large-file opening; reusable-row continuous selection/navigation; long-line
TextKit 2 timer delays; large-document memory. These are measured feasibility
findings, not production acceptance passes. No technical blocker remains for
recording this evaluation. Next dependency-ready task: **FEAS-003 on Windows**;
FEAS-006 and FEAS-007 still wait for its real-host evidence.

Exact measurements and caveats:
[results-2026-10-24.yaml](../spikes/feas004/results-2026-10-24.yaml).

Primary references:

- [NSTextView](https://developer.apple.com/documentation/appkit/nstextview)
- [NSTableView](https://developer.apple.com/documentation/appkit/nstableview)
- [Apple VoiceOver text reading](https://support.apple.com/guide/voiceover/read-text-vo2706/mac)
