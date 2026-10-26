# FEAS-008 virtualized renderer spike

Disposable FEAS-008 spike. It tests whether a native virtualized list can
serve as the production document surface when it presents a semantic block
model, and compares it with a WebView document baseline:

- **Linux:** GTK 4 `GtkListView` and WebKitGTK. See
  [the Linux evaluation](../../docs/Virtualized-Renderer-Linux-Evaluation.md)
  and [its results](results-linux-2026-10-25.yaml).
- **Windows:** WinUI 3 `ListView` in C# (JIT and Native AOT) and C++/WinRT,
  and WebView2. See
  [the Windows evaluation](../../docs/Virtualized-Renderer-Windows-Evaluation.md)
  and [its results](results-windows-2026-10-26.yaml), and the
  [Windows section](#windows) below.

## Layout

- `docmodel/` — owned-value block model over pulldown-cmark: rendered text
  with styled runs, block source spans, outline and slugs, display fragments,
  find, and link/image policy. It is linked in-process; FEAS-006 already
  proved the C ABI, and CORE-002 replaces this model.
- `native/` — gtk-rs host. Only display fragments near the viewport become
  widgets. Selection, copy, find, outline jumps, links and images all work
  on the model, so they reach rows that were never realized.
- `webview/` — the same model rendered to HTML in WebKitGTK 6 with page
  scripts disabled, a CSP, a `mdv-res:` image scheme reusing the host
  containment checks, and navigation gated by the link policy.
- `measure_linux.py` — D-012 cold/warm launch to presented content.
- `atspi_probe.py` — live AT-SPI tree inspection.
- `make_media_fixture.py` — synthesized image and link policy fixture.

## Build and run

```sh
python3 tools/generate_performance_fixtures.py   # if build/fixtures is absent
python3 spikes/feas008/make_media_fixture.py
cargo test --release --manifest-path spikes/feas008/Cargo.toml -p feas008-docmodel
cargo build --release --manifest-path spikes/feas008/Cargo.toml --workspace
spikes/feas008/target/release/feas008-native build/fixtures/performance/prose-5mib.md
```

`--automated` runs the scripted checks and prints one `REPORT` JSON line. It
covers select-all copy, sampled row text against the copy, find, outline and
fragment jumps, and the link and image policy. `--control` accepts line
commands on stdin (`copy-all`, `item N`, `outline N`, `texts A B`, `quit`) for
the external probes. `FEAS008_FRAGMENT_BYTES` overrides the fragment size
used in the sweep.

```sh
python3 spikes/feas008/measure_linux.py --engine native --out build/feas008/native-d012.json
python3 spikes/feas008/measure_linux.py --engine webview --out build/feas008/webview-d012.json
python3 spikes/feas008/atspi_probe.py > build/feas008/atspi.json
```

The WebView crate needs `webkitgtk-6.0`. It is outside `default-members`, so
the native build doesn't require it.

## Interaction model

- Pointer: a capture-phase drag on the list hit-tests Pango layouts into model
  positions. Dragging past an edge auto-scrolls; Shift+click extends.
- Keyboard: Ctrl+A, Ctrl+C, Shift+Left/Right (character), Shift+Up/Down
  (fragment) and Shift+Ctrl+Home/End. Plain arrows move row focus through
  `GtkListView`, which scrolls and moves accessibility focus.
- Copy puts rendered text (`text/plain`) and the Markdown source of the
  selected blocks (`text/markdown`) on the clipboard. A whole-document
  selection copies the source exactly.
- Find (Ctrl+F) searches the model, then scrolls to and selects the match.
  The outline sidebar and `#fragment` links jump and move focus.

## Measured design constraint

GTK keeps about 205 list rows realized however tall they are. The cost of a
launch or jump is therefore about 205 fragments of layout. The same is true of
any wrapped `GtkLabel` row without `width-chars`: GTK spent about 18× the raw
Pango cost measuring the label's minimum width. The spike therefore sets
`width-chars`/`max-width-chars` to 40. Fragments are at most 1 KiB at a
newline or space, and unbreakable runs are cut at 512 bytes and shown with
character wrapping. Continuation rows drop their padding, but lines still
break early at fragment edges.

## Windows

Use native Windows Python, Rust, the .NET 10 SDK, Visual Studio with the C++
x64 tools, and the Windows App Runtime 2.5 and WebView2 runtime. Developer
Mode lets `make_media_fixture.py` create its symlink.

- `ffi/` — disposable C ABI over `docmodel` (`feas008.h`), shared by the
  Windows hosts. The model stays in Rust memory; hosts decode only the
  fragments of realized rows.
- `windows/csharp/` — the C# WinUI host. `jit/` builds a default JIT app and
  `aot/` publishes Native AOT from the same sources.
- `windows/cpp/` — the same surface in C++/WinRT.
- `windows/webview/` — the WebView2 baseline (Native AOT).
- `windows/uia/` — a UIA3 client used by `uia_probe.py`.
- `build_windows.py` — builds all of these into `build/feas008/windows/`.
  It fetches the pinned C++/WinRT generator and checks its hash.
- `measure_windows.py` — D-012 cold and warm launch to presented content.
  Cold runs need an elevated process; `--warm-only` skips them.
- `uia_probe.py` — UIA reading order, set size and heading levels at the top,
  middle and end of a document.
- `clipboard_check.py` — compares the clipboard, read from a separate
  process, with the source and model text after a manual Ctrl+A, Ctrl+C.

```sh
python tools/generate_performance_fixtures.py        # if build/fixtures is absent
python spikes/feas008/make_media_fixture.py
python spikes/feas008/build_windows.py               # csharp-jit csharp-aot cpp webview, or name targets
python spikes/feas008/build_windows.py uia
build/feas008/windows/csharp-aot/Feas008WinUIAot.exe build/fixtures/performance/prose-5mib.md
python spikes/feas008/measure_windows.py --runs 5 --out build/feas008/d012.json   # elevated
python spikes/feas008/uia_probe.py csharp-aot
python spikes/feas008/clipboard_check.py build/fixtures/performance/prose-5mib.md
```

On this checkout, `python` is `.build/feasibility-venv/Scripts/python.exe`.
The hosts take the same `--automated`, `--report PATH` and `--control`
arguments as the GTK host. Control mode also accepts `focus N`,
`select B1 O1 B2 O2` and `geom`. Set `FEAS008_TRACE=path` to log the C++
host's pointer and key handling. The WebView2 host uses
`WEBVIEW2_USER_DATA_FOLDER`; the harness stops its engine processes after
each run.
