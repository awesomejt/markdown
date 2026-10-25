# FEAS-004 macOS rendering probe

Disposable native AppKit comparison, not production application scaffolding.
Swift Markdown 0.9.0 is pinned solely as a presentation-spike input adapter;
this does not replace the Rust/pulldown-cmark core recommendation or perform
FEAS-006 FFI work.

## Build and measure

Requires macOS, Xcode/Swift, and the Python dependencies in
`tools/requirements-fixtures.txt`. From the repository root:

```sh
swift build --package-path spikes/feas004 -c release
python3 tools/generate_performance_fixtures.py
python3 spikes/feas004/measure.py
python3 spikes/feas004/package.py
```

Run a single automatically closing probe:

```sh
spikes/feas004/.build/release/MacRenderingProbe tk2 \
  fixtures/markdown/foundation/gfm-tour.md \
  --automated build/feas004/example.json
```

Use `tk1`, `tk2`, or `blocks`; add `--swiftui` to host the same AppKit
surface in an `NSHostingView`. Without `--automated`, the app remains open
for keyboard, selection, accessibility and lifecycle checks. `package.py`
assembles four local development `.app` bundles under ignored `build/feas004/`
with the GFM fixture, its Apache-2.0 license and origin commit metadata. It
does not invoke signing, install, notarization or publishing commands.

## What is compared

- TextKit 1: one attributed `NSTextView` backed by `NSLayoutManager`.
- TextKit 2: one attributed `NSTextView` backed by `NSTextLayoutManager`.
  The probe avoids accessing `.layoutManager` on this path and records whether
  TextKit 2 remains active.
- Blocks: `NSTableView` reuses text cells, each targeting at most 8,192 UTF-16
  units with composed-character boundaries preserved. Row heights are computed
  eagerly; the full document and attributed blocks remain in memory. This is
  view reuse, not a lazy semantic model or production virtualization solution.
- SwiftUI host: the TextKit 2/AppKit controls are embedded through
  `NSViewRepresentable`. This measures hosting overhead, not an all-SwiftUI
  document renderer or a completed SwiftUI navigation/settings design.

Headings, bold, italic, strikethrough and code receive basic native styling.
Tables become tab-separated text; list numbering and task checkboxes are
simplified. Front matter is not extracted. Links are inert labels, images are
omitted with a marker, HTML is literal text, and math/diagrams remain source.
There is no HTML importer, WebView, resource fetch or document execution path.
This adapter does not establish full semantic conformance.

## Metric definitions

The runner executes semantic cases once per variant and performance profiles
three times per variant, serially. It checkpoints each result, captures Darwin
`/usr/bin/time -l` peak RSS in bytes, and kills only the launched process group
on the configured timeout. Partial checkpoints survive a navigation stall.
Raw results and standard error stay under `build/feas004/measurements/`.

- Entry-to-first-draw starts at the Swift executable's top-level timestamp and
  ends after a native draw callback. It excludes earlier dyld/runtime startup;
  it is not a cold-launch or display-presentation measurement.
- Read, parse, attributed adapter and surface setup times are separate. This
  prototype performs them on the main thread and does not provide cancellation.
- End navigation checks the final insertion index by visible-point hit testing
  or the final table row's visibility after 300 ms. The table check is a row
  visibility check, not proof that every pixel of its final text is visible.
- Selection export uses the view's advertised pasteboard types on a private
  pasteboard and compares text without logging it. Reusable rows do not provide
  cross-row text selection or heading navigation in this spike.
- Sixty 60 Hz timer intervals during programmed scrolling measure main-thread
  delay, not actual GPU/display frame pacing or wheel/trackpad behavior.
- Native accessibility attributes and external AX inspection establish
  exposure only; VoiceOver needs a separate check.

Use the evaluation report and checked-in results for conclusions. An unrun,
timed-out or incomplete check must never be reported as a pass.
