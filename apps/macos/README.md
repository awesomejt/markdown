# macOS application

MAC-001A provides the native AppKit shell: an empty reader window, standard
application commands, File → New Window/Close Window, and Window →
Minimize/Zoom/Bring All to Front. Command-N/W/M/Q use the normal menu shortcuts.
Closing the last window keeps the app running; reopening from the Dock creates
a window or restores an existing minimized one. The shell follows system
appearance and uses a native, accessibility-exposed empty-state label.

MAC-001B adds the model-v2 static core package and owned Swift session adapter.
The app validates the ABI at startup. See the
[core integration evidence and memory limits](../../docs/macOS-Core-Integration.md). The product target remains macOS 27 on Apple
Silicon (D-018); other OS versions and architectures are not claimed.

## Opening documents (MAC-001C)

File → Open… (Command-O) shows an open panel for Markdown and plain-text
files, with multiple selection. The bundle declares a Markdown **Viewer**
document type at `Alternate` rank, and imports `net.daringfireball.markdown`
(conforming to `public.plain-text`; `.md`/`.markdown`, `text/markdown`).
Finder, Launch Services and `open -a` deliver URLs to
`application(_:open:)`. A document already shown is focused, and retried if
its load failed; otherwise the first empty window is reused, or a new window
opens. Documents delivered at launch do not add a stray empty window. File → Reload (Command-R) rereads the
current document. A failed load shows the typed error and its recovery
suggestion in the window; a loaded document is rendered as described in
[Reading documents](#reading-documents-mac-001d).

The Foundation-only `MarkdownDocument` target holds the platform side of the
document flow:

- `DocumentCapability` is one user-granted file URL. Following its
  symlinks/aliases is the grant; its description is redacted, and the core
  receives only bytes, a random session document ID and an opaque
  device/inode/size/mtime revision token, never the path. Security-scoped
  access is bracketed around each read, so the same code works if a later
  task enables App Sandbox. This build is not sandboxed.
- Reads run one at a time on a serial queue, off the main thread, through a
  coordinated read (`NSFileCoordinator`, `.withoutChanges`). The item must be
  a regular file before coordination, because the coordinator itself would
  block opening a FIFO; `open(O_NONBLOCK)` and `fstat` on the descriptor then
  guard a swap. Files over the 5 MiB host source bound are rejected from their
  size, and growth during a read stops at the bound. Reads check cancellation
  between 256 KiB chunks.
- Typed errors with a message and a recovery suggestion, naming no path:
  not found, disk unavailable, permission denied, access revoked, folder or
  non-regular item, unsupported type (an extension whose type is not plain
  text), too large, not UTF-8, too complex (core node/event/depth/text
  limits), busy and unreadable.
- `DocumentLoader` (main actor, one per window) gives every open or reload a
  new request number. Only the newest request publishes; an older result,
  even a completed one, is dropped and counted. Opening a different file
  replaces the window's core session; closing the window cancels work and
  awaits native cleanup. A failed read of the current request is recorded
  in the core session as `Deleted` or `Unreadable` (permission, revoked,
  I/O), so in-flight work is superseded and status stays truthful. A core
  admission timeout (busy) never touched the file and records nothing.
  Older requests never record availability, so a late failure cannot
  supersede a newer successful load.
- Permission lost after a successful read is reported as **revoked**. A
  missing file on a volume no longer mounted where it was read is
  **disk unavailable**, not deleted. Core admission is shared by all windows
  (one running, one queued); a load waits up to 30 seconds for a slot.
  Reload after a failed session creation retries the whole open.

Folder grants, recents, security-scoped bookmarks, external-change watching
and sandboxed sibling-image access remain MAC-002A, MAC-001H and MAC-001E
work.

## Reading documents (MAC-001D)

A loaded document is shown as one continuous, read-only, selectable TextKit 2
text view (`NSTextView` with a `textLayoutManager`; nothing asks for its
TextKit 1 `layoutManager`). One text view keeps selection, Copy and the text
accessibility element continuous across the whole document. The
`MarkdownRender` target holds the rendering:

- `DocumentRenderer` turns the owned model-v2 snapshot into one immutable
  attributed string and a heading map, **off the main thread**. The window
  starts a render for each loaded result; only the newest render installs,
  and a newer state or closing the window cancels it. Cancellation is checked
  every 1,024 nodes. The core's depth limit (128) bounds the recursion.
- Styled: headings (six sizes, bold), paragraphs, emphasis, strong,
  strikethrough, inline code, soft and hard breaks, ordered (with start
  number), bullet and nested lists, block quotes, thematic breaks and fenced
  or indented code in a monospaced font. Colours are dynamic system colours,
  so light and dark appearance follow the window.
- Readable fallbacks until MAC-001E: tables become tab-separated rows with a
  bold header row, images show their alt text in brackets (“Image not
  shown”), and task items show ☐/☑ markers. Alerts render as a quote with
  its label, and footnotes as bracketed labels. Raw HTML, math and other
  unsupported syntax stay inert, secondary-coloured source text. No
  attachment, image or remote resource is loaded.
- Links follow the core's v2 classification. A fragment link to a heading
  anchor scrolls to that heading; a missing anchor beeps. An `http`, `https`
  or `mailto` link the core marks external opens with the system handler.
  Links to other files need a folder grant (MAC-002A) and denied links
  (unsafe schemes, invalid URIs) carry no link at all: they are dotted and
  explain themselves in a tooltip.
- The sidebar outline lists every heading from the core's outline, indented
  by level. Clicking a row or moving through it with the arrow keys scrolls
  that heading to the top and selects it, so keyboard and accessibility focus
  follow. Link navigation selects the matching outline row. View → Show/Hide
  Sidebar (Control-Command-S) toggles it, and Edit → Copy/Select All work on
  the document.

Heading-level accessibility roles, find, text size and theme controls,
document-wide performance budgets and reading-position restoration remain
MAC-001E to MAC-001H work.

## Build and launch

Use Xcode 27 with the macOS 27 SDK, Swift 6 and the pinned Rust toolchain.
There are no external Swift packages; Rust dependencies remain the reviewed
core graph. On the current Mac, first set the isolated Rust environment from
[Development.md](../../docs/Development.md).
On that isolated installation, prefix the Python commands below with
`RUSTUP_TOOLCHAIN=stable` after verifying it reports the repository's pinned
Rust version. The package tool rejects a compiler version mismatch.

The development bundle ID is `io.github.awesomejt.markdown`, under the
repository owner's GitHub namespace. No released identity or user-data
migration exists yet; signing and distribution identity review remain later work.

From the repository root:

```sh
python3 apps/macos/tools/build.py --configuration debug
open build/macos/debug/Markdown.app
```

The build tool prepares `markdown-ffi` with `model-v2` and `panic=unwind`, stages
its selected profile in ignored `CorePackage/`, invokes native SwiftPM and
assembles the app under `build/macos/`. Use `--configuration release` for the
release app. The public header and archive hashes/profile are recorded in the
package manifest; the module fingerprint forces rebuilds when the archive changes.
The selected static library is linked into the executable, with no Rust dylib
at runtime. Both supported build configurations target macOS 27/arm64.

The underlying native commands remain available:

```sh
python3 apps/macos/tools/package_core.py --configuration debug
swift build --package-path apps/macos --configuration debug -Xswiftc -warnings-as-errors
```

Prepare the **matching** profile before running Swift directly or opening
`Package.swift` in Xcode. Do not stage debug and link release, or vice versa.
Only one `libmarkdown_ffi.a` is staged at a time. Source/package build outputs
stay ignored; no binary, generated header or environment is committed.

These are local development bundles, without Developer ID signing,
notarization, sandbox entitlements, installer or distribution approval. The
toolchain may attach an ad hoc Mach-O signature during linking; no explicit
codesign step, signing identity or credential is used. The assembly tool only
replaces its generated bundle and does not install or register file types.

## Checks

Run on a logged-in macOS graphical session:

```sh
.build/feas004-venv/bin/python apps/macos/tools/check.py
```

Use a Python environment with the root fixture-tool requirements installed
(the existing Mac environment above is suitable; a fresh environment follows
[Development.md](../../docs/Development.md)). The suite generates the frozen
performance inputs, prepares each Rust profile, runs Swift package tests against
the staged artifact, and verifies its actual fingerprint and configuration.
It runs `swift format lint --strict`, builds debug/release with warnings as
errors and Swift 6 concurrency checking, validates Info.plist, then launches
each **assembled bundle executable** with `--smoke-test`. Each child has a
30-second timeout and must emit its success marker and exit zero through the
native Quit action. No success marker or a timeout fails the check. The relocated
app is launched with loader overrides removed, and a relocated standalone release
consumer parses synthetic input without repository/runtime library dependencies.

The smoke runs the AppKit event loop and verifies bundle identity, a visible
resizable window/native empty state, menu shortcuts/actions, a second window,
close/controller cleanup, last-window close, Dock-reopen delegate behaviour,
minimize/restore and Quit. Its document part uses synthetic files in a private
temporary directory: the declared document type, Launch Services delivery
reusing the empty window, focus instead of a duplicate, typed missing/folder/
unsupported/oversized/invalid-UTF-8 errors in their own windows, Reload after
withdrawn and restored permission (with the core session's recorded
revocation), a slow document replaced before it finishes never being
presented, and zero native handles after all document windows close. Its
reading part opens a 202-heading document and checks the rendered text, the
outline rows, outline and fragment-link navigation (heading revealed and
selected, outline following), an external link routed to a stub instead of
the browser, the sidebar command and Copy enabled on the document selection.
Copy is written to a private pasteboard, never the user's clipboard.
`MarkdownRenderTests` render every semantic corpus case and compare its
outline levels, titles and anchors with the frozen expectations, check each
heading's rendered range, link policy on the GFM tour, inert unsafe content,
block and inline styling, render cancellation, and navigation and copy on a
real TextKit 2 view; they also render, install and navigate the generated
performance profiles and print their timings.
`MarkdownDocumentTests` cover the same reader and loader directly, plus a FIFO,
an exact-bound file, cancellation and a real detached disk image (`hdiutil`). Menu actions routed directly to a window do not
prove physical keyboard/responder-chain interaction; perform the manual check
below as well. This is shell and basic-reading acceptance, not heading-role,
screen-reader, startup-to-presented-content, RSS or distribution acceptance.

CI uses the path-filtered `macos-native` job on GitHub's
[`xcode-27` image](https://github.com/actions/runner-images#available-images),
because `macos-latest` still uses macOS 26. A selected native job must succeed
for `Checks / complete`; failure, cancellation or an unexpected skip fails the
gate. Only an unselected job may be skipped. Manual/shared/workflow changes
select the job; app-only macOS paths select only macOS host work. Common
regressions exercise the gate's actual shell command for failed/cancelled/
skipped native results and earlier failed jobs.

If GitHub retires or renames the preview image, check the linked official image
list and update `runs-on` to its available macOS 27/Apple Silicon replacement
(for example `macos-27` only once GitHub lists it). Update these instructions
and obtain a fresh hosted debug/release smoke before merging that change.
Do not lower the deployment target or bypass the native gate. A job timeout
does not bound runner queue time; a job stuck waiting for a retired label must
be cancelled and its runner configuration fixed.

## Manual shell check

1. Launch the development bundle above and confirm the Markdown window and
   native menu bar appear, with no document loaded.
2. Press Command-N for a second window; press Command-W to close it. Resize
   the remaining window, minimize it with Command-M, and reopen via the Dock.
3. Exercise Window → Zoom and the About Markdown panel. Close every reader
   window, confirm the app still runs, and reopen from the Dock.
4. Press Command-Q and relaunch. Confirm one empty window appears.

5. Choose File → Open…, select a Markdown file and confirm its title appears.
   Open the same file again and confirm its window is focused instead of a
   duplicate. Open a `.png` and a missing file (for example, delete one after
   choosing it) and read the error and recovery text.
6. With Markdown not running, run `open -a build/macos/debug/Markdown.app
   some.md` and confirm one document window and no extra empty window. Use
   Finder → Open With to confirm Markdown is listed for `.md` files without
   becoming the default.
7. Open `fixtures/markdown/foundation/gfm-tour.md`. Read it in light and dark
   appearance, click outline rows and use the arrow keys in the outline,
   click “the table section”, drag a selection across several blocks and
   paste it into a text editor, and press Control-Command-S twice.

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
MAC-001B consumer/cost/RSS evidence is in
[the integration report](../../docs/macOS-Core-Integration.md). Repeated-load
RSS remains unresolved; per-call limits are not a whole-app memory budget.
CI and review disposition are recorded in the task PR and completion pass.

The first hosted [native job](https://github.com/awesomejt/markdown/actions/runs/41086966487/job/124170589212)
passed debug/release at `d57667a558c262484061f616661a34e0c6d1043e` on
`xcode-27-arm64`: macOS 27.0.1, Xcode 27.1 and Swift 6.4. This confirms the
preview label was available; it does not guarantee future label availability.

## MAC-001C local evidence

Checked on 2026-10-30 on the same Mac (macOS 27.0.1, Xcode 27.1, Swift 6.4):
`apps/macos/tools/check.py` passed in debug and release, with 6
`MarkdownDocumentTests` and 10 `MarkdownCoreTests` in each and both relocated
bundle smokes including the document checks. `open -a` with the debug bundle
delivered a Markdown file at launch and an invalid-UTF-8 file to the running
app; the window server listed exactly two windows (one per document, no empty
launch window). Window titles and contents were not inspected there, because
the agent shell has no Accessibility or Screen Recording grant; the smoke
checks them through the same delegate path. `lsregister -dump` showed the
bundle claiming `net.daringfireball.markdown` as Viewer at Alternate rank,
so the system default handler was not changed. The interactive open panel
and Finder Open With remain the manual steps above. Not sandboxed, signed or
distributed.

## MAC-001D local evidence

Checked on 2026-10-30 on the same Mac (macOS 27.0.1, Xcode 27.1, Swift 6.4):
`apps/macos/tools/check.py` passed in debug and release, with 7
`MarkdownRenderTests`, 7 `MarkdownDocumentTests` and 10 `MarkdownCoreTests`
in each and both relocated bundle smokes including the reading checks.
Offscreen snapshots of the GFM tour were inspected in light and dark
appearance. Release-build timings from the profile test (render off the main
thread; install and navigation on it):

| Profile | Headings | Render | Install | Reveal last heading |
| --- | ---: | ---: | ---: | ---: |
| Prose 1 MiB | 4,855 | 97.7 ms | 7.8 ms | 1.4 ms |
| Delimiter storm 1 MiB | 12,337 | 186.7 ms | 7.0 ms | 1.1 ms |
| Long lines 1 MiB | 1 | 1.7 ms | 246.3 ms | 1.9 ms |
| Prose 5 MiB | 24,273 | 539.9 ms | 15.6 ms | 1.3 ms |

These are single in-process samples, not presented-frame, scrolling or memory
budgets. Installing the long-line profile blocks the main thread for about a
quarter of a second; MAC-001G owns large-document layout budgets. The
interactive steps in manual check 7 were not performed by the agent (no
Accessibility or Screen Recording grant); the smoke covers them through the
same controls.
