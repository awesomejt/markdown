# Windows native shell

WIN-001A through WIN-001I provide an unpackaged Windows 11 x64 WinUI 3
application in C# Native AOT linked against the local shared-core C ABI package
(`markdown_ffi.dll`, model v2), WIN-002A adds folder capability grants,
persistent recent files/folders, and inline access recovery, WIN-002B adds
the native workspace browser sidebar (`WorkspaceTree`, `WorkspaceSidebar`,
`md_workspace_classify_path` classification with invisible-character warnings,
`All` / `.*` / `↻` / `×` controls, and `Ctrl+B` sidebar toggle), and WIN-002C
adds bounded workspace search (`md_workspace_find_names`,
`md_workspace_content_admit`, `md_workspace_content_search`,
`WorkspaceSearch`, `WorkspaceSearchPanel` with `Files` / `Search` mode toggle,
`Edit > Find in Workspace…` `Ctrl+Shift+F`, `Stop`, and source/anchor result
navigation). It opens a native window with a menu bar: File
(Open… Ctrl+O, Open Folder… Ctrl+Shift+O, Open Recent, Close Folder, Reload Ctrl+R, New Window Ctrl+N, Close Window Ctrl+W, Exit Ctrl+Shift+W),
Edit (Copy Ctrl+C, Select All Ctrl+A, Find… Ctrl+F, Find Next F3, Find Previous Shift+F3, Find in Workspace… Ctrl+Shift+F), View (Reset Text Size Ctrl+0,
Increase Text Size Ctrl++, Decrease Text Size Ctrl+-, Theme: System, Light, Dark, and Toggle Workspace Sidebar Ctrl+B),
and Settings… (Ctrl+,). Alt+F opens the File menu; Alt+F4 retains the Windows
close-window behavior. Closing the final window ends the process. Each window
follows the system or user-chosen theme and declares PerMonitorV2 DPI awareness.

At startup, the host verifies the bundled `markdown_ffi.dll` path, build
configuration and SHA-256 fingerprint against the compiled-in package identity
and negotiates C ABI v1 / model v2 (`CoreRuntime.Validate()`). The UI-free
core adapter (`apps/windows/Core/`) owns session lifecycle, explicit host
limits, bounded background parse admission (`MaxAdmitted = 2` on the
`markdown-core` worker thread), cancellation, close/dispose cleanup and owned
`CoreDocument` snapshot copies; see
[Windows core integration](../../docs/Windows-Core-Integration.md).

The UI-free document access and loading layer (`apps/windows/Document/`) wraps
user-selected files in `DocumentCapability` (`ToString()` redacted as
`DocumentCapability(<private>)`; `DisplayName` sanitized against control and
Unicode bidi characters and bounded to 160 scalars; non-local URIs and UNC
network shares rejected with `DocumentError.NonLocalFile`). `DocumentReader`
opens regular local disk files via Win32 `CreateFileW`, rejects device paths
(`\\.\`, `\\?\pipe\`, and DOS device names `CON`, `PRN`, `AUX`, `NUL`,
`COM1`–`COM9`, `LPT1`–`LPT9`) and directories (`DocumentError.NotRegularFile`),
enforces the 5 MiB (`MaxSourceBytes`) cap before and during 64 KiB chunked reads
(`DocumentError.TooLarge`), and builds an opaque path-free revision token from
handle metadata. `DocumentLoader` serializes background reads and 30-second
`CoreErrorKind.Busy` admission retries on a dedicated `markdown-files` thread
with per-window queue deduplication, generation/serial guards
(`DiscardedResults`), explicit capability revocation (`Revoke()`) and post-read
NTFS ACL withdrawal promotion (`DocumentError.AccessRevoked`), and
detached-volume promotion (`DocumentError.VolumeUnavailable`). Document content
is not logged. Recents and restoration persist private file/folder paths locally;
preferences persist settings under the user's application-data directory.

The native presentation layer (`apps/windows/Render/`) renders model-v2 documents
in a virtualized WinUI 3 `ListView` over an off-thread bounded presentation plan
(`RenderPlan.cs`). Model nodes are split into 512-byte fragments with accounted budget
ceilings (128 MiB default) and cooperative cancellation checks. Headings, prose,
ordered and bullet lists, code blocks and fallback raw HTML are rendered into
recycled `RowItem` containers using `RichTextBlock` (`IsTextSelectionEnabled = false`; document-wide selection is host-owned),
styled inlines, Cascadia Mono fonts for code/fallback, blockquote borders and
UIA heading level automation properties (`AutomationProperties.SetHeadingLevel`).
Link targets are resolved to internal heading fragment indices, safe external URIs
(`http`, `https`, `mailto`), folder-capability relative links, or inert text. `ReaderControl` provides a two-column
layout with an outline sidebar (`ListView`) and document virtualized list, outline jump
navigation, and safe link activation. Global multi-fragment selection across virtualized rows is supported with pointer drag, edge auto-scrolling, Shift+arrow/Home/End keyboard extension, Escape to clear, Ctrl+A to select all, and Ctrl+C dual-format native clipboard copy (CF_UNICODETEXT plain text and registered "text/markdown" UTF-8 source bytes). Narrator double-reading is eliminated by setting AutomationProperties.Name on the row container while setting AccessibilityView.Raw on the child RichTextBlock, accompanied by semantic ItemType, PositionInSet, SizeOfSet, and heading levels.

## Build and check

Install .NET SDK 10.0.402, Rust 1.99.1 (`x86_64-pc-windows-msvc`), Visual
Studio C++ x64 build tools and the Windows SDK (26100 or newer), plus native
Python with `tools/requirements-fixtures.txt`. The project targets Windows 11
build 22000 or later; actual older-OS compatibility is unverified. NuGet
package versions and hashes are in `packages.lock.json`; restore uses locked
mode. The App SDK runtime and `markdown_ffi.dll` are copied locally, so the
published app does not require a separately installed .NET, Rust or Windows App
SDK runtime.

From the repository root:

```powershell
python apps/windows/tools/check.py
& ./build/windows/Release/Markdown.Windows.exe
```

`apps/windows/tools/check.py` generates the performance fixtures, stages the
`debug` and `release` core packages via `apps/windows/tools/package_core.py`,
verifies C# formatting, publishes **Native AOT in Debug and Release**, verifies
the staged core manifests and PE export tables, runs `--core-tests`,
`--document-tests`, `--render-tests`, `--smoke`, and `apps/windows/tools/check_association.py` in
both configurations, and in `Release` packages the local unsigned development
artifact, verifies it outside the checkout, and runs relocated
`--document-tests`, `--render-tests`, `--smoke`, `check_association.py`,
relaunch check, and `--core-probe single` without a repository at runtime.

For direct staging and `dotnet publish` (adding Visual Studio Installer to
`PATH` for Native AOT's `vswhere`):

```powershell
python apps/windows/tools/package_core.py --configuration debug
python apps/windows/tools/package_core.py --configuration release
dotnet restore apps/windows/Markdown.Windows.csproj --locked-mode -p:Platform=x64
dotnet publish apps/windows/Markdown.Windows.csproj --no-restore -c Release -p:Platform=x64 -o build/windows/Release
dotnet format apps/windows/Markdown.Windows.csproj --no-restore --verify-no-changes --severity warn
& ./build/windows/Release/Markdown.Windows.exe --core-tests | Out-String -Stream
& ./build/windows/Release/Markdown.Windows.exe --document-tests | Out-String -Stream
& ./build/windows/Release/Markdown.Windows.exe --render-tests | Out-String -Stream
python apps/windows/tools/check_association.py build/windows/Release/Markdown.Windows.exe
& ./build/windows/Release/Markdown.Windows.exe --core-probe single | Out-String -Stream
```

WinUI generates the app PRI and `App.xbf` after publish items are collected;
the project explicitly copies them after publish. Omitting either can cause
a XAML startup failure. Do not distribute just the executable: the entire
publish directory (including `markdown_ffi.dll`) is required. MSIX, signing and
clean-machine distribution require owner authorization; WIN-001I provides the local
unsigned development artifact with complete license notices.

The smoke logs each stage and reports captured output on timeout. Its stale-load
supersession check uses a bounded worker rendezvous without pumping the UI
dispatcher, so a queued watcher/activation refresh cannot cancel the controlled
reload before its read barrier is entered. This synchronization applies only to
`--smoke`; normal reading and refresh remain asynchronous. Position assertions
wait up to three seconds for WinUI layout without relaxing their position or
selection checks. The smoke arms its failure watchdog before closing the last
window and uses the existing smoke-only process-detach bypass immediately after
verifying closed windows and zero owned core handles.

## Development artifact and licenses (WIN-001I)

```powershell
python apps/windows/tools/artifact.py
```

packages the Release Native AOT build into an unsigned local development zip archive
under `build/windows/artifacts/` (e.g. `Markdown-<version>-<commit>[-dirty]-windows-x64.zip`)
alongside a cryptographic JSON record containing the artifact SHA-256 hash, byte size,
source commit, dirty flag, toolchain versions (Rust, .NET SDK, Windows App SDK), and core
library hash.

Every assembled package and development artifact carries:

- `Licenses/LICENSE`: project Apache-2.0 license.
- `Licenses/THIRD-PARTY-NOTICES.txt`: third-party license notices for all 42 statically
  linked Rust packages (`x86_64-pc-windows-msvc`, `model-v2`) and 19 locked NuGet platform
  packages from `packages.lock.json`.
- `Licenses/inventory.json`: machine-readable inventory of linked Rust and NuGet packages.
- `BuildInfo.json`: build record with configuration, versions, hashes, and unsigned status.

`artifact.py` verifies the development artifact by extracting it to a temporary directory
outside the checkout and verifying:

1. Directory layout and PE32+ x64 architecture for `Markdown.Windows.exe` and `markdown_ffi.dll`.
2. Exact 24 exported `md_` symbols in `markdown_ffi.dll`.
3. Unsigned status (no Authenticode certificate in the PE security directory).
4. Zero build-machine path leaks across all packaged files.
5. Completeness of license notices and inventory.
6. Execution of relocated unit tests (`--document-tests`, `--render-tests`), Native AOT smoke
   (`--smoke`), multi-process association check, two-phase relaunch restoration
   (`--relaunch-check`), and single-session core probe without repository dependencies.

The path-filtered `windows-native` CI job runs `apps/windows/tools/check.py` on
`windows-latest`. The completion gate requires success whenever Windows is
selected. Shared workflow changes select all three host jobs.

## Native interaction checks

1. Launch the published app; inspect the native title bar, File menu and welcome text.
2. Invoke File > New Window and verify two independent windows.
3. With the menu closed, use Ctrl+N, then Ctrl+W; verify only the focused window closes.
4. Use Alt+F to inspect the command labels/shortcut hints; invoke Exit and verify all windows close.
5. Relaunch and use the title-bar close button or Alt+F4 on the last window; verify the process exits.

UIA should expose one welcome text element named "Markdown reader", a File
menu and the native window controls. This is shell evidence only; Narrator
document semantics and the FEAS-008 duplicate-reading/large-tree defects
remain open. Physical touch, mixed scaling, minimum-OS and clean-machine
tests are not covered by the in-process smoke.

## Dependencies

Source is Apache-2.0 under the repository license. The NuGet lock includes the
Microsoft Windows App SDK 2.5.1 graph and Windows SDK build tools
10.0.28000.2705. Their Microsoft license terms differ from the portable Rust
license allowlist; see the [dependency inventory](../../docs/Dependencies.md#windows-shell-nuget-graph).
The shell uses native WinUI controls; the App SDK's transitive WebView2 and AI
packages do not mean the application constructs a browser or invokes AI.

## Local evidence

Validated on Windows 11 Pro build 26300.9711, Ryzen 9 5950X x64, Rust 1.99.1,
.NET SDK 10.0.402/runtime 10.0.13, MSVC 14.51.36231, and WinUI package 2.3.9
(App SDK 2.5.1):

- **WIN-001A**: Debug and Release Native AOT publishes and lifecycle smokes
  passed. `dotnet format ... --no-restore --verify-no-changes --severity warn`
  passed. NuGet vulnerability inspection of the locked transitive graph
  reported no known vulnerable packages on 2026-11-01. Native UI inspection
  confirmed the welcome/window tree, native File menu, New Window by menu and
  Ctrl+N with the menu closed, Ctrl+W closing only its window, Alt+F and
  visible shortcut hints, and File > Exit and title-bar close terminating the
  process.
- **WIN-001B**: Debug and Release core packages (`x86_64-pc-windows-msvc`,
  `model-v2`, `panic=unwind`, 24 exported `md_` symbols, zero leaked build
  paths), 17 consumer tests (`--core-tests`), in-app core parse `--smoke`,
  relocated `--smoke` and `--core-probe single`, and MSVC C smoke (`c_abi.rs` /
  `smoke.c` with `MD_REQUIRE_C_SMOKE=1`) passed. Release 5 MiB prose timing,
  20-session repeated RSS/heap retention, and 8 MiB / 5 MiB dense peak working
  set measurements are recorded in
  [Windows core integration](../../docs/Windows-Core-Integration.md).
- **WIN-001C**: Debug, Release, and relocated Release Native AOT builds passed
  12 document access/loader unit tests (`--document-tests`), the in-app WinUI
  smoke (`--smoke`: 5 File menu accelerators, native `GetOpenFileNameW` chooser
  Cancel and Select+Accept, Reload after edit, missing-file `NotFound` and
  recovery, post-open NTFS ACL read withdrawal `AccessRevoked` and recovery,
  non-local URI rejection, stale slow-load supersession, and zero leaked core
  handles on exit), and isolated multi-process file-association activation
  (`apps/windows/tools/check_association.py` against
  `apps/windows/resources/io.github.awesomejt.markdown.reg.in` for `.md`,
  `.markdown`, `.mdown`, `.mkd`, and `.txt`, including paths with spaces,
  Unicode filenames, and existing-window reuse).
- **WIN-001D**: Debug, Release, and relocated Release Native AOT builds passed
  9 render and navigation unit tests (`--render-tests`: styled inlines, lists,
  fenced code, html fallback, safe link target resolution, token resolution,
  unicode fragmentation, budget ceiling exhaustion, cooperative cancellation,
  8 frozen corpus outline regressions matching model-v2 contracts, empty list markers,
  control character replacement, soft/hard line break flow, long heading fragmentation
  preserving single semantic start, and plan barrier cancellation/close suppression).
  The Native AOT smoke (`--smoke`) was extended to verify `ReaderControl` presentation,
  outline hierarchy, outline heading jump navigation, safe link interception delegate,
  and within-fragment text selection with zero leaked core handles.
- **WIN-001E**: Debug, Release, and relocated Release Native AOT builds passed
  13 render unit tests (`--render-tests`: including GFM table layout with tab-separated cell rows for native TSV clipboard copying, monospace font,
  zebra shading, and border styling, task list checkbox markers [checked and unchecked] with
  accent styling, resource policy enforcement restricting image loads to canonical folder
  containment without escaping or traversal, raster-only format validation [PNG, JPEG, GIF, BMP,
  WebP] with header dimension extraction, dimension / pixel side / 32 MP bomb caps, 128 MiB memory
  budget tracking, rejection of SVG / XML vectors, remote HTTP/HTTPS URIs, data: URIs, directory paths,
  and corrupt files with user-friendly alt-text status messages, off-thread decoding with UI-thread
  `BitmapImage` surface caching and lifecycle clearing). The Native AOT smoke (`--smoke`) was extended
  to verify rendering of rich blocks (table, task list, local image) with zero leaked core handles or crashes.
- **WIN-001F**: Debug, Release, and relocated Release Native AOT builds passed
  18 core consumer tests (`--core-tests`, including exact find, case sensitivity, NFD vs NFC,
  simple folding without normalization, emoji ZWJ sequences, CJK, RTL Hebrew, `maxMatches`
  truncation, 4096-byte query rejection, superseded generation, cancellation, and handle leak audits),
  12 document tests (`--document-tests`), 16 render tests (`--render-tests`, including text scale clamping
  and 9 steps, in-memory and local settings preferences persistence, and document find Unicode
  byte-to-char mapping, segment handling, status, and wrapping), and the extended Native AOT smoke
  (`--smoke`: native FindBar with search input, Previous/Next match cycling, match status,
  text scaling across 9 steps with Ctrl++/Ctrl--/Ctrl+0 and scroll retention, appearance switching
  [System/Light/Dark], and Settings window).

- **WIN-001G**: Debug, Release, and relocated Release Native AOT builds passed
  all 18 core consumer tests (`--core-tests`), 12 document access tests
  (`--document-tests`), 18 render tests (`--render-tests`, including
  `global_selection_and_text_between` and `native_clipboard_roundtrip`), and
  the extended Native AOT smoke (`--smoke`: 15 accelerators verified,
  Select All [Ctrl+A], Copy [Ctrl+C] with `CF_UNICODETEXT` and `text/markdown`
  clipboard formats, clipboard read-back verification against document source,
  Clear Selection [Escape], UIA double-reading elimination on `RowItem` containers,
  and independent selection vs find text highlighters). Automated performance
  measurements on Release Native AOT (`apps/windows/tools/measure.py`):
  - **Pipeline (read, parse, plan)**: `prose-1mib` median 170.19ms (min 154.57ms, max 170.97ms, D-016 <= 1 MiB target <= 400ms: PASS);
    `long-lines-1mib` median 22.56ms (target <= 400ms: PASS); `prose-5mib` median 993.20ms (5 MiB stress fixture beyond 1 MiB target).
  - **Outline target resolution**: `prose-1mib` 0.03ms, `prose-5mib` 0.03ms, `long-lines-1mib` 0.02ms (budget <= 150ms: PASS).
  - **Copy selection**: 1 MiB prose text/markdown extraction in 7.47ms, Win32 clipboard write in 1.37ms;
    5 MiB prose in 67.22ms, clipboard write in 8.54ms.
  - **Memory retention**: repeated 5x load/close cycles demonstrated complete memory reclamation
    with no unbounded growth (final RSS ~19.1 MiB for 1 MiB prose, ~23.9 MiB for 5 MiB prose,
    ~17.3 MiB for 1 MiB long lines).
- **WIN-001H**: Debug, Release, and relocated Release Native AOT builds passed
  all 20 core consumer tests (`--core-tests`, adding anchor creation, resolution,
  external change copyUnchanged modes, and snapshot release verification), 14 document
  tests (`--document-tests`, adding DocumentWatcher debounced event delivery, atomic
  replacement, deletion, reappearance, and external refresh dispositions), 20 render
  tests (`--render-tests`, adding reading position anchor mapping and restoration store
  JSON roundtrip/tamper safety), the 13-stage Native AOT smoke (`--smoke`, covering
  external edits, anchor restoration, reading position retention across edits, external
  deletion preserving stale view with warning banner, file reappearance recovery,
  and zero leaked core or snapshot handles), association check, and real two-phase
  relaunch restoration (`--relaunch-check prepare` and `--relaunch-check verify` in
  `apps/windows/tools/check.py`).
  Automated refresh benchmark measurements on Release Native AOT (`apps/windows/tools/measure.py refresh --count 5`):
  - **prose-1mib**: 168.91ms median refresh, `kept_place: true`, 95.25 MiB peak footprint.
  - **long-lines-1mib**: 24.70ms median refresh, `kept_place: true`, 28.60 MiB peak footprint.
  - **prose-5mib**: 996.00ms median refresh, `kept_place: true`, 363.02 MiB peak footprint.
- **WIN-001I**: Debug and Release Native AOT builds passed all 20 core consumer tests,
  14 document tests, 20 render tests, the 13-stage Native AOT smoke, file association checks,
  and two-phase relaunch restoration. In Release, apps/windows/tools/licenses.py generated
  the complete license inventory (42 statically linked Rust packages and 19 locked NuGet
  platform packages) and third-party notices under Licenses/, alongside BuildInfo.json.
  apps/windows/tools/artifact.py packaged the release build into an unsigned local
  development zip artifact (`Markdown-0.1.0-<commit>-windows-x64.zip`), and verified it
  relocated outside the checkout: confirming PE32+ x64 architecture, 24 exported `md_`
  symbols, unsigned development status, zero leaked build-machine paths, license notices
  integrity, relocated document tests (14 passed), render tests (20 passed), Native AOT
  smoke, association check, real relaunch restoration, and core probe with 0 leaked handles.
  Full walking skeleton acceptance across WIN-001A through WIN-001I is documented in
  docs/Windows-Skeleton-Acceptance.md.
- **WIN-002A**: Adds UI-free `FolderCapability` / `FolderGrant` / `FolderError`
  (`apps/windows/Document/FolderAccess.cs`) with Win32 canonical root validation,
  symlink/junction and prefix containment (`OutsideFolder`), and grant revocation
  propagation to bound `DocumentCapability` instances; persistent JSON `RecentsStore`
  (`apps/windows/Document/Recents.cs`, capped at 10 files + 10 folders, case-insensitive
  dedup, untrusted entry filtering, future-version preservation, off-main availability
  checks); native `IFileOpenDialog` (`FOS_PICKFOLDERS`) folder picking; `File > Open Folder…`
  (`Ctrl+Shift+O`), `Open Recent`, `Close Folder`, `RecentItemsWindow`, and inline
  `ErrorBanner` recovery actions (`Locate…`, `Grant Access…`, `Try Again`,
  `Remove from Recents`). Debug, Release, and relocated Release Native AOT builds
  passed 20 core consumer tests, 18 document tests, 20 render tests, the extended
  Native AOT smoke (`--smoke`), association checks, two-phase relaunch restoration,
  and relocated core probe with 0 leaked handles.
- **WIN-002D**: Adds versioned persistent preferences (`preferences.json` schema v2 in
  `apps/windows/ReaderPreferences.cs` with `MARKDOWN_PREFERENCES_PATH` isolation,
  `showOutlineByDefault`, `restoreSessionOnLaunch`, automatic v1-to-v2 migration on load
  with text-scale snapping and malformed-field fallback, and future-version/unknown-field
  preservation on save); expands `SettingsWindow` (`apps/windows/SettingsWindow.cs`,
  `Settings…` / `Ctrl+,`) with `Show document outline in new windows`, `Restore previous
  session on launch`, and `Reset to Defaults`; wires outline default and session restoration
  gating in `ReaderApp`/`ReaderWindow`/`ReaderControl`; and extends `RenderTests`, `--smoke`,
  and two-phase `--relaunch-check` (in `check.py` and `artifact.py`) to verify v1-to-v2
  migration and preference persistence across processes.

## Development installation and R1 conformance (WIN-002F)

The [Windows MVP acceptance](../../docs/Windows-MVP-Acceptance.md) maps R1 to
native evidence and documents development installation/uninstallation. The ZIP
now includes per-user `install.ps1` and `uninstall.ps1`; the artifact checker
installs into a fresh path outside the checkout, runs the installed app and
checks uninstall containment, modified-file refusal and unowned-file preservation.

The reader adds a native collapsible Properties expander for typed front matter,
explicit alert labels, lazy bounded code colors for Rust/Python/JS/TS/JSON/C/C++/
C#/Go/shell/SQL, and relative document links through the active folder capability.
The new code colors preserve text and coexist with find and selection. Unknown
languages, lexical continuations and exhausted highlight budgets retain plain
code. Help > About Markdown and `--version` report the app and negotiated core
contract, configuration and DLL SHA-256.

Real-window `window-launch` and `jump-frame` probes use the production reader
and loader, with a WinUI rendering/compositor presentation proxy and isolated
state. The older `launch` mode is an in-process pipeline measurement only.
See the acceptance document for measured spreads and remaining native budgets.
