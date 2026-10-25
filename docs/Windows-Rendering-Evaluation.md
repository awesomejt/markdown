# Windows rendering evaluation — FEAS-003

Date: 2026-10-25. Native Windows measurements; no Wine, VM rendering substitute,
cross-build inference or production stack acceptance.

## Conclusion

WinUI 3 is viable for the native Windows shell. Neither continuous-text control
in this spike is an acceptable universal large-document renderer. Keep a
read-only `RichEditBox` as a bounded-document candidate and develop virtualized
semantic blocks with a real document-wide selection/accessibility strategy.
Do not adopt the measured Markdig adapter as a production parser or perform
its synchronous work on the UI thread.

C# and C++/WinRT both built and launched native WinUI windows. The C# spike
exercises all three layout surfaces; the C++ probe is a literal-text host smoke,
not a second equivalent Markdown implementation. Its lower startup/memory
numbers do not establish a language winner. Preserve both host options until
FEAS-006 measures the core boundary, packaging and lifecycle costs. GATE-001
still owns the production decision.

Source and reproduction: [probe README](../spikes/feas003/README.md).
Durable evidence: [measurements](../spikes/feas003/results-2026-10-25.yaml).

## Named environment and method

- Windows 11 Pro 26H2, build 26300.9711, x64.
- AMD Ryzen 9 5950X, 16 cores/32 logical processors, 68,640,882,688 bytes RAM.
- NVIDIA GeForce RTX 4080, driver 32.0.16.1088. WMI reports the primary mode as
  3840×2160 at 30 Hz; this is not an independent refresh-rate measurement.
- Three 3840-wide display work areas were exercised, all at scale 1.5. No
  different-scale display transition was available in that sweep.
- .NET SDK 10.0.402/runtime 10.0.13; Visual Studio 2026 18.11.1; MSVC
  14.51.36231; Windows SDK 10.0.26100; Windows App SDK 2.5.1; SDK BuildTools
  10.0.28000.2705; Markdig 1.5.0; C++/WinRT 3.0.260818.1.
- Source baseline `016710638744a8c1fef55cbef5d5219f54f6cb3f`, plus this spike.
  Dependencies are locked; the measured managed assembly's SHA-256 is recorded.

The primary comparison has 60 serial, fresh-process trials: eight semantic
inputs once and four performance inputs three times for each surface. A trial
times out after 45 seconds. Forty-eight trials completed and twelve timed out;
timeouts remain failures, not omitted samples. All 24 semantic trials completed.
The final instrumentation was also checked against all 24 semantic combinations.

Entry-to-render starts in authored managed `Main` and ends at a composition
render callback after adding the document. It excludes earlier CLR/bootstrap
work and does not measure GPU presentation or cold-cache launch. Read, parse,
adapter and surface-construction timers are separate. Caches were not flushed,
and early primary trials overlapped native compilation. These are exploratory
feasibility observations, not controlled-host release budgets.

The primary binary predates interactive clipboard/DPI hooks and additional
phase/checkpoint instrumentation. Renderer/model/navigation algorithms are
unchanged. Supplementary runs identify the newer instrumentation explicitly;
do not mix them into the primary medians.

## Renderer comparison

Median entry-to-render callback, milliseconds; three performance trials each:

| Input | RichTextBlock | Read-only RichEditBox | Virtualized ListView |
| --- | ---: | ---: | ---: |
| 1 MiB prose | 3,611 | 3,945 | 1,199 |
| 5 MiB prose | No callback before timeout | No callback before timeout | 16,146 |
| 1 MiB hostile delimiters | 6,832 | 12,241 | 4,383 |
| 1 MiB long lines | 2,694 | 7,399 | 548 |

These numbers include the adapter parser. At 5 MiB, block mode spends about
15,679 ms parsing, 150 ms building the flat model and 11 ms constructing its
surface. This is chiefly an adapter/parser problem, not evidence that virtualized
WinUI layout takes sixteen seconds. The existing Rust/pulldown-cmark lead
recommendation remains; Rust/Windows integration is still FEAS-006 work.

`RichTextBlock` has continuous native selection, but repeated top/end navigation
times out on all three 1 MiB prose and delimiter trials after reaching the end.
Its long-line trials complete, with a median per-trial maximum navigation/timer
delay around 491 ms. This rejects the measured monolithic strategy for unbounded
documents, not WinUI itself.

`RichEditBox` completes 1 MiB prose navigation, with median end-request time
1.61 ms and median per-trial maximum navigation/timer delay 17.6 ms. Its initial
load is materially slower than block mode. All three 5 MiB trials time out before
a render callback. A phase-checkpoint supplement shows about 15.2 seconds in the
parser, followed by a stall in surface setup. Rich text instead passes surface
setup in about 6.6 seconds after parsing, then remains in initial layout when
the 45-second diagnostic is stopped. Batching/formatting costs, incremental
presentation and cancellation require investigation before production use.

The list keeps about 33 rows realized for 121,362 blocks in the 5 MiB model;
end requests take about 4.06 ms. It does not provide cross-row text selection,
global find/outline behavior or a proven complete accessibility reading order.
View virtualization alone does not solve the document interaction contract.

At 1 MiB prose, median last-checkpoint peak working sets were approximately
713 MiB for rich text, 211 MiB for rich edit and 197 MiB for blocks. The rich-text
trials subsequently time out, so its value is a lower bound on trial-lifetime
peak. Completed 5 MiB block trials peak near 351 MiB and retain the full parsed
and flat models. Working set, private bytes and managed heap are distinct;
these are not cross-platform RSS-equivalent measurements.

## Interaction, accessibility and DPI

- All 34 continuous-text trials that reached selection exported matching text
  after CR/LF and terminal-paragraph normalization. Six of those later timed out
  in navigation. Six other continuous trials never reached selection. Block
  mode's missing cross-row selection is recorded as unsupported.
- Fifty-four trials recorded a successful end check, including six that later
  stalled. Rich text checks scroll offset; list mode checks final-row realization;
  rich edit checks the caret, which is weaker than final-glyph visibility.
- The owner performed native Ctrl+A/Ctrl+C on the public GFM document. The
  probe compared the clipboard with its adapter text and saved a match without
  recording clipboard contents.
- The owner confirmed audible Narrator document reading. Narrator was originally
  off; the owner turned it off afterward and process inspection confirmed it.
- Internal peer inspection found a RichTextBlock Text pattern. Direct peer
  queries returned false for RichEditBox and ListView; that does not prove their
  descendants lack Text providers. Full external UI Automation pattern/reading
  order inspection remains unverified because Computer Use's native pipe was
  unavailable after recovery attempts. Accessibility Insights is installed.
- The corrected sweep moved the GFM probe through work-area origins 0, +3840
  and −3840 pixels, each 3840×2088, observed scale 1.5, then restored geometry.
  Dark and light theme application passed. The initial sweep exposed a WinRT
  `IEnumerable` projection exception; indexed access fixed it. Different-scale
  transitions, clipping inspection, font scaling and large-document Narrator
  behavior remain acceptance work.
- Nine exact-spelling Unicode open checks passed across all three surfaces:
  a spaced/non-ASCII filename and separate NFC/NFD spellings. Source hashes
  matched. This does not establish canonical-equivalent fallback, bookmarks,
  shell associations, network-volume or permission-provider behavior.

The adapter flattens inline formatting, simplifies tables/lists/front matter,
uses inert link/image labels and leaves unsupported math/diagrams as source.
HTML is literal text; no resource fetching or document execution is implemented.
These trials do not establish full semantic conformance.

## Host, debugging and development package

The C++/WinRT literal-source host completed three launch/selection smokes at
scale 1.5. Median entry-to-render was about 205 ms and peak working set about
93 MiB. Its entry timestamp includes explicitly called runtime bootstrap; its
single-paragraph literal model differs from the C# adapter. Treat this as native
host feasibility only. Both hosts require WinUI resource/type metadata setup;
the C# XAML tooling generates the provider used by its controls, while the native
probe supplies one explicitly.

Debug and release managed builds passed. Native `/Zi` source/PDB debugging
passed through the Win32 debugging API and DbgHelp: the intentional entry
breakpoint resolved to `NativeProbe.cpp`. This is not a claim that Visual Studio
UI debugging, managed source stepping or Rust FFI debugging was exercised.

An unsigned framework-dependent folder/ZIP was assembled with fixtures,
Apache provenance, pinned dependency metadata and license files, then launched
locally from that assembled folder. It requires the installed .NET/Windows App
SDK/MSVC runtimes. There was no signing, MSIX registration, installation,
publishing or clean-machine deployment test.

WPR was initially idle. CPU capture failed with `0xc5585011`, “Failed to enable
the policy to profile system performance.” No policy was changed and WPR remained
idle. Consequently there is no WPR/WPA CPU-stack capture to support root-cause
claims; use an appropriately authorized profiling session for that follow-up.

## Follow-up and validation

FEAS-006 can now prove the Rust/native boundary; FEAS-007 can reconcile platform
capabilities. GATE-001 must retain the measured Windows limitations. WIN-001
must establish bounded/off-thread parsing, cancellation, incremental blocks,
cross-block selection and native accessibility before deepening the Windows MVP.
Frame pacing during real wheel/touch input, mixed DPI, restoration, clean-machine
deployment and full semantic/UIA conformance remain unproven.

Checks: locked NuGet restore; debug/release managed builds; native host/PDB
builds; primary 60-trial run; supplementary phases, Unicode, clipboard, Narrator,
DPI/theme, package launch; corpus integrity/deterministic generation; Python
compilation; Markdown lint and whitespace checks. Raw logs, process diagnostics,
packages and build products stay under ignored `build/feas003/`.

API references: [WinUI setup](https://learn.microsoft.com/en-us/windows/apps/get-started/start-here),
[rich text display](https://learn.microsoft.com/en-us/windows/apps/develop/ui/controls/rich-text-block),
[RichEditBox](https://learn.microsoft.com/en-us/windows/windows-app-sdk/api/winrt/microsoft.ui.xaml.controls.richeditbox),
[Windows Performance Toolkit](https://learn.microsoft.com/en-us/windows-hardware/test/wpt/).
