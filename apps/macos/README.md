# macOS application

MAC-001A provides the native AppKit shell: an empty reader window, standard
application commands, File → New Window/Close Window, and Window →
Minimize/Zoom/Bring All to Front. Command-N/W/M/Q use the normal menu shortcuts.
Closing the last window keeps the app running; reopening from the Dock creates
a window or restores an existing minimized one. The shell follows system
appearance and uses a native, accessibility-exposed empty-state label.

There is no document loader, file association, core adapter or renderer yet.
Those begin in MAC-001B/001C/001D. The product target remains macOS 27 on Apple
Silicon (D-018); other OS versions and architectures are not claimed.

## Build and launch

Use Xcode 27 with the macOS 27 SDK and Swift 6. The package has no third-party
dependencies. From the repository root:

```sh
swift build --package-path apps/macos --configuration debug -Xswiftc -warnings-as-errors
python3 apps/macos/tools/build.py --configuration debug
open build/macos/debug/Markdown.app
```

The native `swift build` command compiles the executable. The Python assembly
step repeats that incremental build and copies the executable plus checked-in
Info.plist into `build/macos/debug/Markdown.app`. Use `--configuration release`
for a release development bundle. Generated files stay under ignored `build/`
and `apps/macos/.build/`. To open the package in Xcode, open `Package.swift`.

These are local development bundles, without Developer ID signing,
notarization, sandbox entitlements, installer or distribution approval. The
toolchain may attach an ad hoc Mach-O signature during linking; no explicit
codesign step, signing identity or credential is used. The assembly tool only
replaces its generated bundle and does not install or register file types.

## Checks

Run on a logged-in macOS graphical session:

```sh
python3 apps/macos/tools/check.py
```

It runs `swift format lint --strict`, builds debug/release with warnings as
errors and Swift 6 concurrency checking, validates Info.plist, then launches
each **assembled bundle executable** with `--smoke-test`. Each child has a
30-second timeout and must emit its success marker and exit zero through the
native Quit action. No success marker or a timeout fails the check.

The smoke runs the AppKit event loop and verifies bundle identity, a visible
resizable window/native empty state, menu shortcuts/actions, a second window,
close/controller cleanup, last-window close, Dock-reopen delegate behaviour,
minimize/restore and Quit. Menu actions routed directly to a window do not
prove physical keyboard/responder-chain interaction; perform the manual check
below as well. This is shell acceptance, not renderer, screen-reader,
startup-to-presented-content, RSS or distribution acceptance.

CI uses the path-filtered `macos-native` job on GitHub's
[`xcode-27` image](https://github.com/actions/runner-images#available-images),
because `macos-latest` still uses macOS 26. A selected native job must succeed
for `Checks / complete`; failure, cancellation or an unexpected skip fails the
gate. Only an unselected job may be skipped. Manual/shared/workflow changes
select the job; app-only macOS paths select only macOS host work. Common
regressions exercise the gate's actual shell command for failed/cancelled/
skipped native results and earlier failed jobs.

## Manual shell check

1. Launch the development bundle above and confirm the Markdown window and
   native menu bar appear, with no document loaded.
2. Press Command-N for a second window; press Command-W to close it. Resize
   the remaining window, minimize it with Command-M, and reopen via the Dock.
3. Exercise Window → Zoom and the About Markdown panel. Close every reader
   window, confirm the app still runs, and reopen from the Dock.
4. Press Command-Q and relaunch. Confirm one empty window appears.

Inspect the empty-state text/window/menu roles using macOS accessibility
tooling. Full VoiceOver document reading, large documents, mixed displays,
state restoration and permission recovery remain later native checks. Do not
toggle VoiceOver or change user accessibility preferences as part of this smoke.

## MAC-001A local evidence

Checked on 2026-10-30: Apple M5 Max/arm64, macOS 27.0.1 (26A434), Xcode 27.1
(27A9275), Swift 6.4, macOS SDK 27.0. Debug and release bundle smoke passed,
including format, warnings/concurrency and close/controller-lifetime checks.
The visible release app showed native dark appearance, a centred empty state
that stayed centred after resizing, and exposed text/window/menu accessibility
elements. Command-N produced two Window-menu entries; Command-W returned to
one. Zoom, Command-M, the standard About panel (version 0.1.0), Command-Q exit
and relaunch to one empty window were exercised through native UI automation.
Finder launch/reopen was exercised; Dock-reopen delegate cases are automated,
not a claim of a physical Dock-icon click. No user accessibility setting changed.

The installed actionlint version does not yet recognize the official preview
`xcode-27` label. Workflow lint passed with only that exact unknown-label
diagnostic suppressed; actual hosted execution remains the required evidence.
CI and review disposition are recorded in the task PR and completion pass.
