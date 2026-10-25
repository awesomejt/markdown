# FEAS-003 Windows native rendering probe

Disposable WinUI 3 evaluation, not production application scaffolding. C#/.NET
hosts three native document surfaces; a bounded C++/WinRT smoke proves the other
host can build and launch. No shared-core FFI or production stack is accepted.

## Build

Requires a real x64 Windows host with Developer Mode, .NET 10 SDK, Visual Studio
C++ tools, Windows SDK 10.0.26100, Windows App SDK 2.5 runtime, and the fixture
Python dependencies. The project pins Windows App SDK 2.5.1, SDK BuildTools
10.0.28000.2705 and Markdig 1.5.0. Restore uses `csharp/packages.lock.json`.

From the repository root in Git Bash:

```sh
dotnet restore spikes/feas003/csharp/WindowsRenderingProbe.csproj --locked-mode -p:Platform=x64
dotnet build spikes/feas003/csharp/WindowsRenderingProbe.csproj --no-restore -c Release -p:Platform=x64
.build/feasibility-venv/Scripts/python.exe tools/generate_performance_fixtures.py
.build/feasibility-venv/Scripts/python.exe spikes/feas003/build_native.py
.build/feasibility-venv/Scripts/python.exe spikes/feas003/measure.py
.build/feasibility-venv/Scripts/python.exe spikes/feas003/package.py
```

The native builder uses the installed MSVC x64 environment, pinned NuGet
metadata already restored by the C# project, and C++/WinRT 3.0.260818.1. Its tool
archive is downloaded to ignored build output and checked against a pinned
SHA-256 before execution. Microsoft package licenses remain in NuGet; the
development package includes dependency licensing metadata. Referencing WebView2
metadata during header generation does not create a WebView document surface.

Outputs are under `build/feas003/`. C# restore intermediates are ignored under
`csharp/obj/`. Build Debug with `-c Debug`; native `/Zi` builds retain source PDBs
even with `/O2`. Set `FEAS003_DEBUG_BREAK=1` only when launching the native probe
under a debugger; it breaks at native entry before runtime bootstrap.

## Run a probe

The C# executable is under
`build/feas003/csharp/bin/x64/Release/net10.0-windows10.0.26100.0/win-x64/`.
Arguments are surface, input file, optional output JSON and `--interactive`.

```sh
build/feas003/csharp/bin/x64/Release/net10.0-windows10.0.26100.0/win-x64/WindowsRenderingProbe.exe \
  richedit fixtures/markdown/foundation/gfm-tour.md
```

Surfaces:

- `richtext`: one selectable `RichTextBlock` inside a `ScrollViewer`.
- `richedit`: one `RichEditBox`, populated before setting it read-only.
- `blocks`: a virtualized `ListView` of individually selectable text blocks.
  It has no cross-row document selection implementation.

Markdig is solely a spike input adapter. Headings get different sizes, code is
monospaced in continuous controls, and inline markup is flattened. List bullets,
tables, front matter, math and diagrams are simplified or left as source; there
is no full semantic conformance claim. HTML remains literal text. Links/images
are inert labels; there is no HTML importer, resource fetch or executable
document content. The Rust/pulldown-cmark recommendation remains unchanged.

`NativeProbe.exe INPUT [OUTPUT_JSON]` displays literal source as one native rich
text paragraph. It does not parse Markdown, match the C# paragraph model or
support all three renderer variants. Its host startup/memory observations are
smoke evidence, not an apples-to-apples language performance comparison.

## Measurement and interpretation

`measure.py` launches serial fresh processes for eight semantic cases once and
four performance profiles three times per surface. Each trial has a 45-second
timeout; only that launched process is killed on a stall. Results checkpoint to
ignored YAML/JSON. The runner refuses to overwrite a previous result directory;
use `--output` for a new run. Trial errors and timeouts remain in the results.

- Entry-to-render is measured from authored C# `Main` to a composition rendering
  callback after adding the document. It excludes earlier CLR/auto-bootstrap
  work and is not a GPU-present timestamp or cold launch. The native entry
  timestamp includes its explicitly called bootstrap, so it has a different scope.
- Launch-to-first-render-checkpoint includes process startup and file-observation
  delay; it is not direct display presentation timing. Background caches are
  not flushed, so trials are fresh-process, not cold-cache launches.
- Read, parser, adapter and surface construction are separate synchronous UI
  operations. This spike lacks cancellation and incremental publication.
- Selection compares normalized CR/LF and trailing paragraph separators against
  the adapter's plain text. It tests text-control selection, not semantic fidelity.
- End requests use native scroll/caret APIs. Rich text verifies scroll offset;
  block mode checks realization of the final row. Rich edit's caret check is
  weaker than proof that the final glyph is visible.
- Ten alternating top/end requests with 16 ms delays measure UI-thread delay,
  not sustained wheel scrolling or GPU/display frame pacing.
- Memory uses Windows working-set, peak-working-set and private-byte counters;
  managed heap size is supplementary, not whole-process memory. Timeouts can
  precede a final memory checkpoint.

The validator generates Unicode filesystem paths separately. Feed paths from
`tools/generate_path_fixtures.py` to a native probe for Unicode integration
checks; the manifest's ASCII `input` names alone do not prove Unicode lookup.

The checked-in 60-trial run used an assembly recorded by SHA-256 in the results.
It predates interactive clipboard/DPI hooks and extra phase checkpoints; its
renderer/model/navigation algorithms are unchanged. Supplementary checks use
the final instrumentation. Early renderer trials overlapped native compilation;
these are exploratory observations, not controlled-host performance budgets.

## Manual checks

Pass an output path and `--interactive` to keep the window open while writing
observations. Use native keyboard selection and copy before pressing **F8**;
the app compares clipboard text with the fixture and saves only the result,
never clipboard contents. **F9** moves the probe to each display work area,
records actual DPI scale/client dimensions, then restores its original geometry.
It does not change system scaling. A single-scale sweep cannot prove mixed-DPI
behavior. The toolbar also offers top/end navigation and a local theme toggle.
`--dpi-sweep` and `--theme-cycle` invoke those same test hooks automatically.
The sweep uses indexed access to the WinRT display collection because its
`IEnumerable` projection threw on the measured runtime.

Use Narrator and external UI Automation inspection to complement the internal
automation-peer name. The peer name alone does not prove reading order, Text
pattern coverage or audible screen-reader behavior. Accessibility settings must
be restored after an authorized temporary check.

`package.py` makes an unsigned, framework-dependent local folder and ZIP with
fixtures, their provenance/license and dependency metadata. Test launch from the
assembled folder. It does not sign, install, register MSIX, publish or establish
clean-machine deployment. Identity signing and distribution remain owner tasks.

`build/feas003/native/DebugProbe.exe NATIVE_EXE INPUT OUTPUT_JSON` performs a
bounded Win32 debugger/PDB smoke. It launches the native probe with the entry
break flag, resolves that breakpoint to a source line through DbgHelp, then
terminates that debuggee before it creates a window. This verifies native source
debug information; it does not establish Visual Studio UI debugging, managed
debugging or Rust FFI debugging. WPR capture must be reported separately if
the current token cannot enable system profiling policy.
