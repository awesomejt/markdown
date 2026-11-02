# Windows native shell

WIN-001A, WIN-001B, WIN-001C, and WIN-001D provide an unpackaged Windows 11 x64 WinUI 3
application in C# Native AOT linked against the local shared-core C ABI package
(`markdown_ffi.dll`, model v2). It opens a native window with a File menu,
Open… (Ctrl+O), Reload (Ctrl+R), New Window (Ctrl+N), Close Window (Ctrl+W),
and Exit (Ctrl+Shift+W). Alt+F opens the File menu; Alt+F4 retains the Windows
close-window behavior. Closing the final window ends the process. Each window
follows the system theme and declares PerMonitorV2 DPI awareness.

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
detached-volume promotion (`DocumentError.VolumeUnavailable`). No document
contents, full file paths, recent files, settings, network requests or resource
permissions are logged or persisted.

The native presentation layer (`apps/windows/Render/`) renders model-v2 documents
in a virtualized WinUI 3 `ListView` over an off-thread bounded presentation plan
(`RenderPlan.cs`). Model nodes are split into 512-byte fragments with accounted budget
ceilings (128 MiB default) and cooperative cancellation checks. Headings, prose,
ordered and bullet lists, code blocks and fallback raw HTML are rendered into
recycled `RowItem` containers using `RichTextBlock` (`IsTextSelectionEnabled = true`),
styled inlines, Cascadia Mono fonts for code/fallback, blockquote borders and
UIA heading level automation properties (`AutomationProperties.SetHeadingLevel`).
Link targets are resolved to internal heading fragment indices, safe external URIs
(`http`, `https`, `mailto`), or inert text. `ReaderControl` provides a two-column
layout with an outline sidebar (`ListView`) and document virtualized list, outline jump
navigation, and safe link activation. Tables, images, full multi-row selection, and
Narrator continuous reading proof remain later tasks.

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
both configurations, and copies the `Release` output to a temporary directory
outside the checkout to run relocated `--document-tests`, `--render-tests`, `--smoke`,
`check_association.py`, and `--core-probe single` without a repository at
runtime.

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
clean-machine distribution are WIN-001I work, and no release approval is
claimed.

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
