# Linux application

LNX-001A provides the native GTK 4 shell, written in Rust with gtk-rs: an
empty reader window with a menu bar, File → New Window/Close Window/Quit and
Help → About Markdown. Ctrl+N, Ctrl+W and Ctrl+Q are the standard
accelerators. The window shows a native, accessible "No document open" label
and uses the desktop's GTK theme.

The shell behaves like a normal GTK application, which differs from the macOS
shell in two places:

- Closing the last window ends the application. Linux desktops have no Dock
  that keeps a windowless app running.
- The application is single-instance through D-Bus, under the application ID
  `io.github.awesomejt.markdown` (the macOS bundle ID). Launching it again
  opens another window in the running process.

Since LNX-001B the app links the model-v2 core library through a UI-free Rust
adapter, `apps/linux/core` ([integration report](../../docs/Linux-Core-Integration.md)),
and refuses to start against an incompatible core. LNX-001C adds native Open…/Reload, local-file capabilities and background
loading through the core. LNX-001D presents the owned semantic model as native
virtualized GTK text with an outline, links and basic selection.
LNX-001D1 verified AT-SPI semantics and recorded D-035 MVP scope.
LNX-001E delivers native tables, task list markers, and folder-contained image resource policy.
LNX-001F delivers native document find with an interactive FindBar, match highlighting, case/word options, text scaling across 9 discrete steps (75%–250%, Ctrl++/Ctrl--/Ctrl+0), and theme switching (System, Light, Dark) with persistent preferences. The accepted baseline is
x86-64 KDE Plasma on Wayland (D-018); libadwaita is not used, and other
desktops and architectures are not claimed.

## Build and launch

Install Rust 1.99.1 (the root `rust-toolchain.toml`), the GTK 4 development
files (GTK 4.14 or newer: `gtk4` on Arch, `libgtk-4-dev` on Debian and Ubuntu)
and `pkg-config`.
The app is its own Cargo workspace with its own `Cargo.lock`. The root
workspace and its dependency policy cover only the portable core, and gtk-rs
is not a core dependency. The app does not depend on the core's Rust crates
either: it links the core's C ABI library from a staged package.

From the repository root, stage the core package for the configuration you
build, then build and launch:

```sh
python3 apps/linux/tools/package_core.py --configuration debug
cargo build --manifest-path apps/linux/Cargo.toml --workspace --locked --target-dir build/linux/target
build/linux/target/debug/markdown-linux
```

For an optimized build, stage with `--configuration release` and add
`--release`; the output is at `build/linux/target/release/`. A build refuses
a core package of the other configuration. Re-stage after changing core
sources. Build output and packages stay under the ignored `build/linux/`; the
binaries find `libmarkdown_ffi.so` through a `RUNPATH` relative to their own
directory, so keep the `build/linux/` layout together. This is a local
development binary, not a package: a source desktop-entry template supports
isolated association checks, but no icon, Flatpak or Arch package is supplied,
and nothing is installed or registered. The binary name
`markdown-linux` is a development name; packaging (LNX-001I and later) picks
the installed name.

## Checks

Run in a graphical session (Wayland or X11):

```sh
python3 apps/linux/tools/check.py
```

It generates the performance fixtures, stages the debug and release core
packages, then runs `cargo fmt --check`, Clippy with warnings denied over the
workspace, and the
[dependency/license policy](../../docs/Dependencies.md#linux-app-graph-lnx-001a)
for the Linux graph. Then, for debug and then release, it builds the
workspace with `--locked`, runs 12 document tests, 16 renderer tests and the
16 core consumer tests against the
matching core package (negotiation, typed errors, owned v2 values, parse
failures, copy budget, queued and running cancellation, bounded admission,
close, non-blocking `close_then` and drop, availability, repeated lifecycle and every corpus and
performance profile), re-verifies the package hash and runs each binary's two
smoke modes as child processes, with a 30-second timeout each:

- `--smoke-test` checks the application ID, one mapped, resizable window with
  a menu bar, the empty-state label (text, accessible role, mapped), every
  menu item and its accelerator against a list written separately from the
  app's own, New Window, Close Window with the closed window released, About
  (transient for the window, with the program name and version) and Quit
  with two windows open. It also exercises the native chooser fallback
  (programmatic selection and Cancel), Open, Reload after edits, missing-file
  recovery, rejection of a nonlocal URI, and responsive window creation/close
  while a large load is pending. The native reader check verifies heading/list/code
  text and escaping, a fragment link to a far heading, near/far outline activation,
  within-label selection bounds, bounded realized rows and recycling/reload cleanup.
  It never writes the clipboard or launches an external URI.
- `--smoke-test=last-window` closes the only window and expects the
  application to exit.
- The commands smoke also parses a heading off the main thread through the
  core adapter, checks the published model-v2 outline and the package
  configuration, and expects zero live native handles afterwards.

Last, it runs the release measurement probe once over the 1 MiB prose
profile; the [integration report](../../docs/Linux-Core-Integration.md#reproduce)
has the full measurement commands.

An isolated association check then launches the generated development desktop
entry with `gio launch`, delivering two files to an already-running primary
process on a private D-Bus session. It verifies startup delivery and all three
files' exact parsed bytes, including spaces, Unicode and a non-UTF-8 Linux
filename. The executable symlink includes spaces, dollar/backtick and quotation
characters; Exec is desktop-entry escaped without a shell. Temporary XDG
applications/config/cache directories and processes are cleaned up; no MIME
defaults or persistent registrations change. This proves delivery rather than
a system-default association or an installed package.

Each run must print its pass marker, then report that the GTK main loop ended,
and exit zero. A smoke run uses `NON_UNIQUE`, so it never hands its checks to
a reader already running on the desktop. Commands are activated as actions;
this proves the actions and the accelerators installed for them, not physical
key presses. Run the manual check below as well. These are scoped shell/reader
checks; screen-reader, whole-app startup and memory acceptance remain separate.

CI runs the path-filtered `linux-native` job on `ubuntu-24.04`, which installs
`libgtk-4-dev` (GTK 4.14) and runs the check under `dbus-run-session` and
`xvfb-run` with `GDK_BACKEND=x11`, because hosted runners have no display or
session bus. That proves the
build and smoke on GTK's X11 backend; Wayland and KDE evidence comes from the
named host below. A selected native job must succeed for `Checks / complete`;
only an unselected job may be skipped. Changes under `apps/linux/` select only
Linux host work; shared, tool and workflow changes select every host.

## Manual shell check

1. Launch the binary above and confirm the Markdown window and its menu bar
   appear, with no document loaded.
2. Press Ctrl+N for a second window and Ctrl+W to close it. Resize, maximize
   and minimize the remaining window with the window controls.
3. Open Help → About Markdown and close it. Launch the binary again from a
   terminal and confirm a second window opens in the running app.
4. Press Ctrl+Q. Relaunch, close the window with Ctrl+W and confirm the
   process exits.

Inspect the window, menu bar and empty-state label with an AT-SPI tool such as
Accerciser. Orca reading, heading navigation and large documents are later
native checks (LNX-001D1, LNX-001G). Do not change user accessibility settings
as part of this smoke.

## LNX-001A local evidence

Checked on 2026-10-31: AMD Ryzen 9 5950X (x86-64), CachyOS, kernel 7.3.0, KDE
Plasma 6.8.2 on Wayland (KWin 6.8.2), GTK 4.24.1, GLib 2.90.1, Pango 1.58.2,
gtk-rs (gtk4 0.11.5), Rust 1.99.1, Python 3.14.8.

- `python3 apps/linux/tools/check.py` passed: format, Clippy, the Linux
  license policy (65 packages) and both smoke modes in debug and release on
  the Wayland backend.
- Both smoke modes also passed on the X11 backend through Xwayland
  (`GDK_BACKEND=x11`), the backend CI uses.
- A deliberately wrong Close Window accelerator made the smoke fail with
  `accelerator for Close Window`.
- A live AT-SPI inspection of the release binary showed an application
  `markdown-linux` with a `frame` "Markdown" (active, showing), a `label`
  "No document open" and a `menu bar` with focusable `menu item`s File and
  Help.
- A second launch exited 0 and the first process then had two Markdown
  frames.

Physical key presses, the window controls, the About dialog's appearance and
Orca were not exercised in this pass; the manual check above covers them.

## LNX-001B local evidence

Checked on 2026-10-31 on the host above (CachyOS, kernel 7.3.0, KDE Plasma
6.8.2 Wayland, GTK 4.24.1, glibc 2.44, Rust 1.99.1, Python 3.14.8):

- `python3 apps/linux/tools/check.py` passed on Wayland, and under
  `GDK_BACKEND=x11 dbus-run-session -- xvfb-run -a` as CI runs it: package
  staging and audit (24 exports), format, Clippy, the license policy, 15
  consumer tests in debug and in release, both smoke modes with the core
  check in each configuration, and the release probe.
- A debug build pointed at the release package (`MARKDOWN_CORE_PACKAGE`)
  failed in the build script with a configuration mismatch.
- The release app's `RUNPATH` is `$ORIGIN`-relative only, and the packaged
  library contains no checkout or home path.
- Measurements, host limits and the dense 8 MiB peak are in the
  [integration report](../../docs/Linux-Core-Integration.md#measurements).

## Local files and loading (LNX-001C)

Open… (Ctrl+O) uses `GtkFileChooserNative`, which can use the desktop portal.
Cancel leaves the selected document unchanged. Open replaces the invoking
window's selection; association delivery opens one window per file, reusing
an active empty window for the first file. New Window remains empty. Reload
(Ctrl+R) is available after selection, including after a read failure. The
Markdown/text picker filter is a discovery hint: All files permits any
explicitly selected local regular UTF-8 file, including extensionless files or
other suffixes. Remote URIs, directories, devices and FIFOs are unsupported.
File arguments use Gio's OS-string API so non-UTF-8 Linux names remain exact.
The title shows only a sanitized, bounded basename; failure messages contain
no path or raw I/O diagnostic.

A capability grants one selected path and its symlink target, without sibling
or parent-directory access. Reload follows the selected path again, so atomic
saves and symlink retargeting select the current target. It does not pin an
inode. This unsandboxed development binary has ordinary process permissions;
the capability is an application boundary, with an explicit revocation token,
rather than OS sandbox enforcement. Grants are not persisted. Portal grant
revocation, physical removable-volume unplug and Flatpak persistence have not
been tested by this slice. Simulated ENODEV/ENXIO/ESTALE/ETIMEDOUT errors test
only unavailable-storage mapping. Real chmod-denied reads are tested when
running unprivileged; root cannot provide that evidence.

The host opens read-only/nonblocking with O_NOCTTY (verified Linux x86-64 flag
values), checks the opened descriptor is regular, then reads chunks with a
5 MiB source limit plus one sentinel byte. Actual bytes are bounded even if a
file grows after metadata was checked. UTF-8 and complexity errors come from
the core. Missing, denied, explicitly revoked and unavailable storage have
separate typed recovery messages; only current read-access failures update
core availability. A failed current load clears the retained model and shows
the error. Reload recovers once access returns.

One host dispatcher serializes reads and core-admission retries, without
waiting for parsing to finish. Each window keeps at most one pending metadata
request; replacement removes obsolete queued work. One original source buffer
and one submission clone may be retained before admission; two parses remain
the process-wide core limit. Busy retries wake on cancellation or every 50 ms,
with a 30-second admission deadline. Loaded models and queue metadata scale
with windows; these bounds are not a whole-app memory budget.

Every selection/reload/close advances a host request serial before cancelling
read/parse work. Publication and error handling check that serial, so a stale
read error cannot alter the replacement session's availability. Cancellation
handles callback-before-install and synchronous queued-cancel races. Close
and Quit remove queued work, cancel running requests and asynchronously release
sessions; GTK never waits for the core worker. Completion polling runs only
while a load is pending, and both window close and programmatic destruction
detach its state and cancel work. A failed replacement-session allocation
preserves the prior selection and Reload lineage. Large replaced/stale owned
results are disposed off the GTK thread.

Cancellation is cooperative between reads. A regular-file kernel read on a
stalled FUSE/network mount may block despite O_NONBLOCK, delaying dispatcher
reclamation and subsequent reads. UI publication is invalidated immediately;
there is no claim of a bounded disk-unplug or I/O interruption deadline. This
slice adds no watcher, restoration, recents, resource loader or renderer.

The automated picker smoke uses the GTK native fallback with per-child
`GDK_DEBUG=no-portals`; on older GTK without that flag it requires the normal
fallback (as on the hosted Xvfb session). It responds to the fallback dialog,
which forwards through the real native chooser response callback. Chooser
configuration uses temporary XDG directories and an in-memory settings backend.
Normal app launches retain the desktop's portal behavior. Physical Ctrl+O and
Ctrl+R, interactive portal selection/cancellation and real grant revocation
remain manual checks. The smoke's load-status label is a native accessible
label; document reading/heading navigation still require LNX-001D1 evidence.

To run only the isolated development association check after building:

```sh
python3 apps/linux/tools/check_association.py build/linux/target/debug/markdown-linux
```

Extend the manual shell check by opening an ordinary Markdown file through the
desktop picker, editing it in another app and pressing Ctrl+R, removing it and
confirming the recovery message, then restoring it and reloading. Try All files
with an extensionless UTF-8 file. Confirm headings, prose, lists and code are
readable; activate outline entries and a same-document link. Select text within
one fragment using pointer and keyboard and copy it manually if desired.

## LNX-001C local evidence

Checked on 2026-11-01 (America/Denver), x86-64 AMD Ryzen 9 5950X, CachyOS
kernel 7.3.0-4-cachyos, KDE Plasma/KWin 6.8.2 Wayland, GTK 4.24.1,
GLib 2.90.1, Pango 1.58.2, Rust 1.99.1, Python 3.14.8, unprivileged uid 1000:

- `python3 apps/linux/tools/check.py` passed on Wayland.
- `GDK_BACKEND=x11 dbus-run-session -- xvfb-run -a python3 apps/linux/tools/check.py`
  passed on X11. Each command stages/audits both core packages, checks format,
  Clippy and license policy, and runs 11 document tests plus 15 core consumer
  tests in both debug and release, both shell smoke modes in each configuration,
  isolated desktop-entry delivery in each configuration and the release probe.
- Document tests cover real denied reads, explicit capability revocation,
  missing-file recovery, FIFO/device rejection, bounded growth, invalid UTF-8,
  complexity limits, unchanged reloads, stale availability/results, a held-read
  replacement/close barrier, callback installation/cancellation races and an
  observed Busy retry before admission is released. Storage-error mapping uses
  injected errno values; no physical unplug or portal revocation is claimed.
- Chooser Cancel must dismiss the actual mapped fallback dialog before the smoke
  reopens it. The smoke checks native actions and installed accelerators; physical
  keys and interactive portal selection remain the manual checks above.
- A live AT-SPI inspection could not run: the existing accessibility bus refused
  connection, and isolated-bus retries could not expose the app because host
  service activation failed. GTK smoke role checks pass; no new AT-SPI or Orca
  result is claimed. LNX-001D1 must still prove the document accessibility gate.

Close during an actively running parse and admission-timeout reclamation have
no direct host-level barrier test in this slice. Existing core consumer tests
cover running cancellation; host tests cover held reads, stale publication,
queued cancellation and cancellation before handle installation. Whole-app
startup, rendering and memory acceptance remain later tasks.

The single host dispatcher also imposes cross-window delays: one stalled
FUSE/network read prevents reads for every other window, even for local files.
Waiting for a core admission slot can likewise delay the next window's read
for up to the 30-second retry deadline. GTK remains responsive, but those
windows stay at “Loading document…” until the dispatcher advances. Bounded
parallel reads or fair admission scheduling require LNX-001G measurements;
they are not implemented by this slice.

## Native reader (LNX-001D)

The reader uses a lazy `GtkListView` of bounded UTF-8 text fragments, with a
separate native outline. Each heading's first fragment has its native heading
role/level; continuations retain heading styling with paragraph roles. Body
labels also have paragraph roles. Ordinary soft line breaks flow as spaces;
explicit Markdown hard breaks remain newlines. Headings, prose, ordered/unordered lists, nested
items, emphasis/strong/strike, inline code and code blocks are presented from
owned model-v2 values. Code wraps in monospace. Outline entries use core titles,
levels and anchors; activation reveals and focuses the heading, including
unrealized distant rows. Scrolling reveals the row minimally, without promising
heading-at-top alignment. Images use readable alt text only; unsupported blocks
and raw HTML retain escaped source fallback. No HTML is executed or resources
loaded. NUL and XML-invalid controls become visible replacement characters.
Tables and richer extensions retain basic readable content; deeper presentation
belongs after the accessibility gate.

A native label supports pointer/keyboard selection and plain-text copy within
one fragment. Selection does not cross fragment seams and is reset when the row
is recycled or a document is reloaded/replaced. Full document/global selection
remains LNX-001G. Fragment seams can introduce early line breaks; every fragment
is conservatively limited to 512 UTF-8 bytes, within D-017's 1024-byte whitespace
and 512-byte unbreakable bounds. Width hints are 40 characters. There is no
whole-document `GtkTextView` or eager widget population.

Links use generated plan-epoch/fragment/intent tokens in escaped Pango markup.
The native handler always suppresses GTK's default activation. Only a core
same-document fragment or canonical external `http:`, `https:` or `mailto:`
target can activate. External targets launch through GTK's URI launcher with
the window parent. Relative/capability-requiring, denied and missing-anchor
links stay inert: selecting a file grants no folder or sibling-document access.
The automated checks do not launch a browser/mail client or access the network.

The pure plan is prepared on successful core-worker completion before the
serial-tagged result is sent to GTK. It checks shared cancellation at node,
allocation and fragment boundaries and before publication. Plan admission caps
accounted owned UTF-8/record payload at 128 MiB, conservatively charging temporary
runs, copied traversal indices and anchor-map entries as well. This is an
allocation-policy bound, not exact allocator RSS or a whole-app memory budget.
Budget exhaustion maps to the existing complexity message. Cyclic/deep invalid
models and out-of-range link indices fail without indexing panics. The core
worker serializes plan construction, so a large plan can delay other windows'
core work; scheduling/performance tuning remains LNX-001G. Large stale plans
and replaced models are released off the GTK thread.

Packaged-core renderer tests cover all eight corpus outlines, inline/list/code
semantics, escaped raw HTML, alt text, URI policy and stale/forged token rejection,
UTF-8/style/link clipping, injected budget exhaustion and cancellation. A held
completed-plan barrier tests replacement, reload and close before publication;
existing held-read and lifecycle tests remain in place. Native smoke selection
checks inspect bounds only and leave the user's clipboard untouched.

LNX-001D1 is still mandatory before deeper renderer work: native roles and
headless/native smoke navigation do not prove AT-SPI whole-document access,
Orca continuous reading across virtualized rows, or off-screen heading access.
No new Orca proof is claimed here. GTK 4.14 is the compatibility floor; local
checks on newer GTK do not replace CI evidence at that floor.

### LNX-001D local evidence

Checked on 2026-11-01 (America/Denver), AMD Ryzen 9 5950X x86-64,
CachyOS kernel 7.3.0-4-cachyos, KDE Plasma/KWin 6.8.2 Wayland, GTK 4.24.1,
Rust 1.99.1 and Python 3.14.8:

- `python3 apps/linux/tools/check.py` and
  `GDK_BACKEND=x11 dbus-run-session -- xvfb-run -a python3 apps/linux/tools/check.py`
  both passed with the final GTK `v4_14` feature configuration. Each runs
  12 document/loader, 5 renderer and 15 core consumer tests in debug and release,
  format, Clippy, Linux dependency policy, package audits, reader/shell/last-window
  and isolated association smokes, and the release probe.
- Native smoke checks distant outline/fragment navigation and focus, selectable
  text bounds, exactly one visible text label per row, recycling cleanup and
  document replacement. A synthetic 1,602-heading document stays below the
  smoke's 1,000-realized-row limit; an exact row count is not emitted. This
  proves lazy realization for that case, not a 5 MiB performance budget.
- Tests use packaged core parses for all eight corpus outlines, nested-only
  and empty list markers, escaped fallback/control characters, styled UTF-8
  fragments, link policy/tokens, plan budgets and cancellation. A completed-plan
  barrier proves replacement/reload/close suppression before publication.
- Live AT-SPI inspection remains unavailable: Registry activation reports a
  failed unit. Private-bus address discovery is not an application-tree proof;
  no unrelated apps were enumerated and no services or user accessibility
  settings were changed. LNX-001D1 still needs AT-SPI and owner-confirmed Orca.

No browser/mail client was launched and the user's clipboard was untouched.
Physical input, continuous/global selection, cold/warm content presentation,
scroll pacing, complete memory budgets and mixed scaling remain unmeasured.

After Grok review 1 (PR #52), both full native commands passed again in debug
and release with 12 document/loader, 8 renderer and 15 core tests per backend
and profile (140 test executions). New checks distinguish soft/hard breaks,
prove mixed-case source schemes become core canonical targets, and verify a
1,103-byte heading occupies three fragments with one semantic Heading role and
styled Paragraph continuations. Native fragment limits remain unchanged.
Plan disposal now reports thread-creation failure without panicking; resource
exhaustion was not fault-injected. Reload position preservation is recorded
for LNX-001H. These role checks do not replace the LNX-001D1 AT-SPI/Orca gate.

## Early document accessibility gate (LNX-001D1)

The [production accessibility inspection](../../docs/Linux-Accessibility-Gate.md)
verified native AT-SPI document and heading semantics: exposed headings report
`Role.HEADING` with attribute `level=1`, body fragments report `Role.PARAGRAPH`,
and exposed text strictly matches the model reading order.

Under owner decision **D-035**, MVP delivery prioritizes document presentation,
tables, task lists, and images, minimizing screen-reader troubleshooting. The
`LNX-001D1` early gate is satisfied for MVP on the proven native GTK semantics
and outline navigation, while off-screen Orca structural navigation (`H`) and
unbroken continuous reading (Say All) across virtualized rows are formally
documented and deferred to post-MVP (`ADV-001`). Deeper Linux renderer work
(`LNX-001E` and onward) is unblocked.

Run `python3 apps/linux/tools/probe_accessibility.py` after building the release
app to reproduce the AT-SPI inspection. It generates public text and uses a
private D-Bus session and temporary registry on the current display, without
changing desktop accessibility settings/services or launching Orca. Exit 0
means inspection completed; read the reported failures. Use `--configuration
debug` for debug, or `--prepare-only` for a persistent public owner-test fixture.

## Tables, task lists and images (LNX-001E)

- **Tables:** GFM tables render as one virtual row fragment per table row with
  tab-separated cells for native TSV clipboard copying. Cells use a monospace font,
  bold header styling, and alternate-row zebra striping with separator borders.
  Short rows are padded with tabs up to the table's column count. Images in table
  cells render their alt text into cell runs without leaking into subsequent paragraphs.
- **Task lists:** Task list items display read-only ☑ (`\u{2611}`) markers with
  accent styling for checked items and ☐ (`\u{2610}`) markers with muted styling
  for unchecked items.
- **Image resource policy:** Local image loading follows the core's v2 resource
  classification and URI policy:
  - Remote images (`MD_TARGET_DENIED` with `MD_DENIAL_REMOTE_RESOURCE` or external)
    and unsupported schemes (`data:`, fragment-only) are blocked.
  - Relative images beside the document are read via `DocumentResources` from the
    document's folder. Relative paths are normalized lexically and canonicalized;
    any path escaping the document folder (`..` or symlink escapes), absolute paths,
    or paths with null bytes are rejected (`OutsideFolder`).
  - Only regular files are opened; directories, FIFOs, and devices are rejected.
  - Image files are size-capped at 16 MiB.
  - Format validation rejects SVG / XML vectors as `UnsupportedType` (ADV-002).
  - Raster format inspection validates PNG, JPEG, GIF, BMP, and WebP headers, enforcing
    dimension bomb caps (32 MP = 32 × 1024 × 1024 pixels) and side caps (2048 px).
  - Per-document budgets enforce at most 128 distinct images and 128 MiB of decoded
    pixel memory. Identical image paths are deduplicated to a single read.
  - Succeeded images render via native `gtk::Picture` backed by `gdk::Texture` with
    per-reader texture caching.
  - Blocked or failed images show their alt text with italic status messages (e.g.
    `[alt (Remote images are blocked)]`, `[alt (Images outside the document’s folder are not shown)]`).
    Messages name no filesystem paths.
- **Validation:** Debug and release suites pass 13 renderer tests, 12 document/loader
  tests, and 15 core consumer tests per profile. The GTK native shell smoke (`--smoke-test`)
  verifies rendering of rich blocks (table rows with TSV, task list markers, and local
  image rendering through real `DocumentResources` while remote and escaping images are blocked).

## Find, text scaling and appearance (LNX-001F)

- **Document Find**: `md_snapshot_find` C ABI binding in `apps/linux/core/src/ffi.rs`,
  non-blocking worker find with `FindGate` cancellation, dynamic buffer sizing with retry on
  `MD_ERR_BUFFER_TOO_SMALL`, generation validation against `MD_ERR_SUPERSEDED`, and consumer
  tests in `apps/linux/core/tests/consumer.rs`.
- **Render match mapping**: `FindMatches`, `FindMatch`, `FindMatchRange`, `RenderPlan::map_matches`,
  and `Fragment::markup_with_highlights` mapping core node matches into virtualized fragment
  runs with Pango orange (`#ff7800`) active and yellow (`#f6d32d`) passive highlights across
  inline boundaries and soft/hard breaks.
- **FindBar widget**: GTK 4 `FindBar` widget (`apps/linux/src/find_bar.rs`) with search entry,
  next/prev navigation buttons, match status label, Match Case / Whole Words check buttons,
  close button, and keyboard shortcuts (`Ctrl+F`, `F3`, `Shift+F3`, Return/Shift+Return, and
  `stop-search` Escape dismissal).
- **Text scaling**: 9 discrete zoom steps (75% to 250%) with scroll position retention across
  scale changes via visible fragment anchoring, keyboard shortcuts (`Ctrl++`/`Ctrl+=`, `Ctrl+-`,
  `Ctrl+0`), and per-reader scoped CSS class isolation (`.reader-scale-{reader_id}`) with
  cleanup on reader drop.
- **Theme switching & preferences**: System, Light, and Dark themes with dynamic
  `gtk-interface-color-scheme` resetting/setting and `gtk-application-prefer-dark-theme` fallback.
  Atomic persistent JSON preferences store under `$XDG_CONFIG_HOME/markdown/preferences.json`
  with NaN safety and `GSETTINGS_BACKEND=memory` test isolation.

## Large documents, global selection and accessibility (LNX-001G)

- **Global selection model**: Document-level `DocumentPosition` and `DocumentSelection` spanning
  virtualized rows and fragment seams, with plain rendered text slicing (`text_between`) inserting
  paragraph/table separators, and Markdown source slicing (`source_between`) returning exact
  source bytes for whole-document selection, node spans for whole-block selections, and exact plain-text bytes for sub-fragment selections.
- **Selection presentation**: Dynamic Pango markup generation with GNOME/GTK blue accent highlight
  (`<span background="#3584e4" foreground="#ffffff">`) composing cleanly with active/passive
  find match highlights.
- **Gestures and keyboard selection**:
  - `GtkGestureDrag` with capture-phase propagation on `GtkListView`, Pango layout hit testing
    via `layout.xy_to_index`, and edge auto-scrolling during drag. Gesture sequence is claimed only
    once the 4 px drag threshold is crossed so plain link clicks pass through cleanly. Weak references
    prevent controller/timer closure cycles and ensure prompt memory release on window drop.
  - Keyboard navigation: Shift+arrows for character/fragment extension (starting from reading caret
    anchor when no selection exists), Shift+Ctrl+Home/End for document start/end extension, Escape
    to clear selection, Select All (`Ctrl+A`), and Copy (`Ctrl+C`).
- **Dual-format native clipboard**: Sets both `text/plain` rendered text and `text/markdown`
  UTF-8 source bytes via `gdk::ContentProvider::new_union` on the display clipboard.
- **Accessibility**: Preserved AT-SPI document/heading semantics, verified with
  `apps/linux/tools/probe_accessibility.py`.
- **Performance budgets**: Automated measurement tool `apps/linux/tools/measure.py` measuring
  `launch`, `interaction`, and `repeated` modes:
  - **Launch pipeline (D-016 $\le 400\,\text{ms}$ warm)**:
    - Measures core read, parse, and render-plan build pipeline before window presentation.
    - `prose-1mib`: median pipeline 89.86 ms (parse 78.13 ms, plan 11.76 ms, target $\le 400\,\text{ms}$: PASS).
    - `long-lines-1mib`: median pipeline 9.20 ms (parse 5.94 ms, plan 2.79 ms, target $\le 400\,\text{ms}$: PASS).
    - `prose-5mib`: median pipeline 578.36 ms (parse 521.88 ms, plan 54.66 ms, stress fixture).
  - **Outline target fragment resolution (D-016 $\le 150\,\text{ms}$)**:
    - 0.01 ms median across 1 MiB prose, 5 MiB prose, and 1 MiB long lines (PASS).
  - **Copy selection**:
    - 1 MiB prose: text/markdown extraction in 4.93 ms, clipboard write in 8.41 ms ($< 15\,\text{ms}$ total).
    - 5 MiB prose: text/markdown extraction in 24.49 ms, clipboard write in 10.65 ms.
    - Verified exact byte-identical readback against source Markdown.
  - **Repeated loads & memory reclamation**:
    - 10 repeated loads evaluated glibc allocator caching and `malloc_trim(0)`.
    - 1 MiB prose trimmed RSS is flat at $\sim 51.1\,\text{MiB}$ (untrimmed glibc cache $\sim 88.0\,\text{MiB}$).
    - 5 MiB prose trimmed RSS is flat at $\sim 153.4\,\text{MiB}$ (untrimmed glibc cache $\sim 266.4\,\text{MiB}$).
