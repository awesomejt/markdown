# Windows MVP acceptance

WIN-002F completes the development packaging and R1 presentation slice on
Windows 11 x64. The WIN-002 aggregate remains a separate task after the
WIN-002F reviewed merge. This is an unsigned development build; production
signing, publishing and clean-machine release acceptance are separate work.

## R1 coverage

| Requirement | Implementation and evidence |
| --- | --- |
| R1.1 files, folders, recents, recovery | WIN-001C and WIN-002A/B/E; native document tests, association smoke, workspace change/access-loss tests and relaunch verification |
| R1.1 local navigation | Relative links resolve against the current document directory through the active `FolderCapability`, including canonical containment and junction checks. Missing folder access produces an actionable message; outside-folder links retain the current document and show an error. Images retain the existing bounded raster resource policy. |
| R1.2 Markdown and safety | Core model-v2 corpus, renderer semantic tests, inert raw HTML and denied schemes; native tables, task markers, code, images and explicit alert labels |
| R1.2 code highlighting | Realized code rows only, at most 512 UTF-16 code units, 128 tokens and 2 ms per row. Rust, Python, JS/TS, JSON, C/C++, C#, Go, shell and SQL lexical hints. No regex, engine, execution or fetching. Unknown languages and budget exhaustion use plain code. Multi-line lexical state across virtualized fragments is intentionally approximate. |
| R1.2 front matter | Typed null/boolean/integer/string/string-list properties in a native collapsible Properties expander; excluded from body, find and copy as before; cleared when a plain document replaces it |
| R1.3 native reader | WIN-001D–H and WIN-002D/E; WinUI menus, outline, selection, dual-format clipboard, find, settings, lazy rows and restoration |
| R1.4 search | WIN-001F and WIN-002C; document find and bounded/cancellable workspace filename/content search |
| R1.5 diagnostics | Help > About Markdown and `--version` validate the linked DLL and identify app version, model v2, ABI v1, anchor v1, core configuration and SHA-256 |
| R1.5 installation | ZIP includes `install.ps1` and `uninstall.ps1`; installed copy runs all artifact checks outside the checkout, with hash/containment and unowned-file preservation tests |

## Development installation

Extract the ZIP, then run from its `Markdown` directory in PowerShell:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File ./install.ps1
& "$env:LOCALAPPDATA/MarkdownDev/Markdown.Windows.exe"
```

Use `-Destination 'C:/chosen/new/folder'` to choose another new directory
under an existing parent. Relative destinations follow the PowerShell location;
trailing separators are normalized. Files stage beside the destination and the
complete directory is renamed into place; a copy failure removes its private
staging directory without publishing a partial installation.
The installer refuses any existing destination and reparse-point payload or
destination. It copies the self-contained Native AOT application, core DLL,
runtime assets, license inventory and build record. It does not need elevation
or a separately installed .NET/App SDK runtime. Launch the installed executable
from Explorer or the command above. No file associations are registered.

Quit every installed app window before uninstalling:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File "$env:LOCALAPPDATA/MarkdownDev/uninstall.ps1"
```

Uninstall validates the installation identity, every owned file hash, and all
absolute targets before deleting individual files. Modified payloads stop removal;
unknown files and application preferences/recents/restoration remain. Reinstall
into a new directory or uninstall the previous development build first.

## Validation and measurement contract

```powershell
python apps/windows/tools/check.py
python tools/check.py common
python apps/windows/tools/measure.py window-launch --runs 5
python apps/windows/tools/measure.py jump-frame --runs 5
python apps/windows/tools/measure.py repeated --count 20
```

The historical `launch` mode measures an in-process read/parse/plan pipeline,
without constructing a reader window. Its compositor flush does not prove
presented document content. `window-launch` launches the actual app and
measures the parent's QPC immediately before process creation through WinUI
layout, two rendering callbacks and `DwmFlush`. This is a presentation proxy,
not physical display latency. `jump-frame` includes the same frame proxy after
far/end/start/middle navigation. Measurements isolate preferences, recents,
restoration and instance identity in temporary state. Warm runs use one warmup;
no cold-cache result is inferred. D-016 still applies to 5 MiB stress documents;
an exceeded budget is recorded, not reinterpreted as a pass.

## Remaining manual evidence

Clean second-machine installation, downloaded-file SmartScreen behavior,
physical mixed-DPI monitor moves, cold-cache launches, frame pacing and a full
Narrator pass over the properties expander remain unrun by the automated checks.
The hosted restoration smoke is intermittently unstable: `develop` run
44010960619 failed its first attempt by two rows and passed its second attempt.
Local baseline Debug/Release/installed-equivalent checks passed on the same source.
These limitations do not establish production release readiness.

## WIN-002F local evidence

Host: AMD Ryzen 9 5950X, Windows 11 Pro build 26300, .NET SDK 10.0.402,
Rust 1.99.1, native Python 3.14, Windows App SDK 2.5.1, x64 Native AOT.
Five warm runs per profile after one warmup (uniform display scale):

| Profile | Launch proxy min / median / max (ms) | Jump proxy min / median / max (ms) | Peak RSS median (MiB) | Realized rows |
| --- | --- | --- | --- | --- |
| Prose 1 MiB | 445.11 / 456.95 / 477.96 | 32.79 / 33.46 / 34.19 | 174.72 | 9-11 |
| Prose 5 MiB | 1233.42 / 1258.03 / 1296.16 | 31.06 / 33.59 / 35.12 | 403.29 | 9-11 |
| Long lines 1 MiB | 299.82 / 301.78 / 314.21 | 32.54 / 32.92 / 33.87 | 118.78 | 3-4 |

The two prose profiles miss the 400 ms warm target; long lines passes. All
measured jumps meet 150 ms. Cold launches, physical display latency and frame
pacing remain unmeasured. These numeric gaps stay in TODO.md for DESK-002;
WIN-002F does not waive them or claim complete performance/release acceptance.

Native validation: `python apps/windows/tools/check.py` passes Debug, Release
and installed Release (22 core, 22 document and 23 render tests each), WinUI
smoke with the R1 scenarios, association, two-phase relaunch, Debug/Release
window-frame probes, license inventory (42 Rust + 19 NuGet), installed diagnostics,
zero-handle core probe and install/uninstall safety checks. `python tools/check.py common` passes fixture/schema/capability/conformance, tool tests, local links
and Markdown lint. No core dependency or ABI change is included.

The existing in-process `repeated --count 20` harness reports final RSS of
51.18 MiB (1 MiB prose), 199.70 MiB (5 MiB prose) and 17.27 MiB (long lines),
from 14/18/14 MiB baselines. Its per-load RSS samples include a live plan/document
before collection. This is not a whole-window retained-memory or leak proof;
the installed core probe separately verifies zero native handles. No numeric
whole-app memory budget is accepted by these measurements.
