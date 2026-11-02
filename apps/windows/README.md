# Windows native shell

WIN-001A provides an unpackaged Windows 11 x64 WinUI 3 application in C#
Native AOT. It opens a native window with a File menu, New Window (Ctrl+N),
Close Window (Ctrl+W), and Exit (Ctrl+Shift+W). Alt+F opens the File menu;
Alt+F4 retains the Windows close-window behavior. Closing the final window
ends the process. Each window follows the system theme and declares
PerMonitorV2 DPI awareness.

This increment is a shell. Core integration is WIN-001B; native Open/Reload
and file delivery are WIN-001C. No document contents, recent files, settings,
network requests or resource permissions are stored or requested here.
Rendering, Narrator document reading, mixed-DPI validation and distribution
acceptance remain later Windows tasks.

## Build and check

Install .NET SDK 10.0.402, Visual Studio C++ x64 build tools and the Windows
SDK (26100 or newer), plus native Python. The project targets Windows 11
build 22000 or later; actual older-OS compatibility is unverified. NuGet
package versions and hashes are in `packages.lock.json`; restore uses locked
mode. The App SDK runtime is copied locally, so the published app does not
require a separately installed .NET or Windows App SDK runtime.

From the repository root:

```powershell
python apps/windows/tools/check.py
& ./build/windows/Release/Markdown.Windows.exe
```

The checker verifies formatting, publishes **Native AOT in Debug and Release**, and runs each
executable's `--smoke` mode with a 60-second timeout. It checks the initial
window/accessibility name, New Window, Close, Exit and last-window process
termination using the command delegates. It rejects CoreCLR output. The smoke
opens only its own windows and neither edits preferences nor accesses files.
Unknown arguments exit with status 2. A failed smoke returns nonzero.

The helper adds Visual Studio Installer to PATH for Native AOT's `vswhere`.
For a direct `dotnet publish`, add that directory to PATH yourself:

```powershell
dotnet restore apps/windows/Markdown.Windows.csproj --locked-mode -p:Platform=x64
dotnet publish apps/windows/Markdown.Windows.csproj --no-restore -c Release -p:Platform=x64 -o build/windows/Release
dotnet format apps/windows/Markdown.Windows.csproj --no-restore --verify-no-changes --severity warn
```

WinUI generates the app PRI and `App.xbf` after publish items are collected;
the project explicitly copies them after publish. Omitting either can cause
a XAML startup failure. Do not distribute just the executable: the entire
publish directory is required. MSIX, signing and clean-machine distribution
are WIN-001I work, and no release approval is claimed.

The path-filtered `windows-native` CI job runs the same checker on
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

## WIN-001A local evidence

Validated on Windows 11 Pro build 26300, Ryzen 9 5950X x64, .NET SDK
10.0.402/runtime 10.0.13, MSVC 14.51.36231, and WinUI package 2.3.9
(App SDK 2.5.1). Debug and Release Native AOT publishes and lifecycle
smokes passed. `dotnet format ... --no-restore --verify-no-changes --severity warn`
passed. NuGet vulnerability inspection of the locked transitive graph reported
no known vulnerable packages on 2026-11-01; it is not a permanent guarantee.

Native UI inspection confirmed the welcome/window tree, native File menu,
New Window by menu and Ctrl+N with the menu closed, Ctrl+W closing only its
window, Alt+F and visible shortcut hints, and File > Exit terminating the
process. The original title-bar close also terminated the process. No
Narrator, touch, mixed-DPI, minimum-OS or clean-machine proof is claimed.
The initial menu-only accelerators failed before being moved to the root;
the final root accelerator checks passed.

`python tools/check.py all` passed on this Windows host, including debug
default/all-features/model-v2, all-features release guards, dependency policy,
fixtures, lint, links and the three-host CI completion-gate regressions.
The Windows MSVC C consumer remains intentionally deferred to WIN-001B;
the core suite's Windows C smoke policy does not establish that consumer proof.
