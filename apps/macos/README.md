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

Folder grants, recents and security-scoped bookmarks remain MAC-002A work.
External changes and relaunch restoration are described in
[External refresh and restoration](#external-refresh-and-restoration-mac-001h).
Images beside the document are read under the
[resource policy](#tables-task-lists-and-images-mac-001e).

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
- Tables, task items and images are described in
  [the next section](#tables-task-lists-and-images-mac-001e). Alerts render
  as a quote with its label, and footnotes as bracketed labels. Raw HTML,
  math and other unsupported syntax stay inert, secondary-coloured source
  text.
- Links follow the core's v2 classification. A fragment link to a heading
  anchor (including non-ASCII anchors such as `#café`) scrolls to that heading; a missing anchor beeps. An `http`, `https`
  or `mailto` link the core marks external opens with the system handler.
  Links to other files need a folder grant (MAC-002A) and denied links
  (unsafe schemes, invalid URIs) carry no link at all: they are dotted and
  explain themselves in a tooltip.
- The sidebar outline lists every heading from the core's outline, indented
  by level. Clicking a row (again, to return to it) or moving through it with
  the arrow keys scrolls
  that heading to the top and selects it, so keyboard and accessibility focus
  follow. Link navigation selects the matching outline row. View → Show/Hide
  Sidebar (Control-Command-S) toggles it, and Edit → Copy/Select All work on
  the document.

Find, text size and appearance are described in
[a later section](#find-text-size-and-appearance-mac-001f), and large
documents, VoiceOver rotors and budgets in
[Large documents and accessibility](#large-documents-and-accessibility-mac-001g).
Reading-position restoration is described in
[External refresh and restoration](#external-refresh-and-restoration-mac-001h).

## Tables, task lists and images (MAC-001E)

- **Tables.** TextKit 2 accepts `NSTextTable` attributes but lays each cell
  out as a stacked paragraph (checked on macOS 27.0.1), so a table is one
  paragraph per row with tab-separated cells. Tab stops come from each
  column's measured width (capped at 360 pt) and its GFM alignment: left,
  centre or right. A centred or right-aligned first column starts with a tab.
  A custom `NSTextLayoutFragment`, supplied by the surface's
  text-layout-manager delegate, draws the header shading, alternate-row
  shading and grid rules behind the text using dynamic system colours. The
  header is bold and GFM pads short rows. Rows stay selectable, and copying
  a table gives tab-separated text that pastes into a spreadsheet. Limits:
  a cell wider than its column overflows into the next one, and a table
  wider than the window wraps its rows. VoiceOver reads a table as lines of
  text; its Tables rotor moves between tables, without cell navigation.
- **Task lists.** Task items show ☑ (accent colour) or ☐ (secondary colour)
  in place of the bullet. They are read-only.
- **Images: resource policy.** Rendering follows the core's v2 resource
  classification ([URI policy](../../docs/URI-Policy.md)):
  - Remote images (`MD_DENIAL_REMOTE_RESOURCE`) are never fetched, and other
    denied targets (unsafe, `data:`, fragment-only) are never read.
  - A relative image (`requires_capability`) is read by
    `DocumentResources` in `MarkdownDocument`. The core's path is already
    decoded once, and it is never decoded again or turned into a URL. It is
    joined to the document's folder and resolved with `realpath`. The result
    must be inside that folder or a subfolder, so `../` and symlinks that
    leave the folder are refused. It is opened with `O_NOFOLLOW_ANY |
    O_NONBLOCK`, so a symlink swapped in after the check fails and a FIFO
    cannot block. It must be a regular file of at most 16 MiB whose
    extension is a raster image type; SVG is refused (ADV-002).
  - `MarkdownRender` decodes in the off-main render. ImageIO identifies the
    content, so SVG or non-image data named `.png` is refused. The pixel
    count from the header must be at most 32 Mi pixels, so a full decode
    (ImageIO cannot subsample PNG and most GIF/TIFF) briefly needs at most
    128 MiB, and images decode one at a time. The retained copy is
    downsampled to at most 2,048 px on its long side. One document holds at
    most 128 distinct images and 128 MiB of retained decoded pixels. Each distinct path
    is read and decoded once per render, and cancellation is checked before
    each image.
  - A loaded image is an inline attachment. It scales down to the line width
    and never up, at one point per pixel. Its accessibility description is
    its alt text, and its tooltip is its title or alt text. Linked images
    keep their link.
  - Any other image shows its alt text in brackets, in italics. A tooltip
    gives the reason: remote images are blocked, the image was blocked, it is
    outside the document's folder, it was not found, permission was denied,
    the format is not supported, it is too large, there are too many images,
    or it couldn't be read. No path is shown.

  This build is not sandboxed. A sandboxed build's single-file grant does not
  cover sibling images (capability matrix CAP-04), so it would need the
  folder grant from MAC-002A. Images are read again whenever the document
  is rendered again: on Reload, or when an external change alters its
  text. A change to an image file alone is not detected.

## Find, text size and appearance (MAC-001F)

- **Find.** Edit → Find → Find… (Command-F) shows a find bar above the
  document: a search field, previous and next buttons, the match status
  (“3 of 12”, “Not found”), Match Case, Whole Words and Done. Find Next
  (Command-G), Find Previous (Shift-Command-G) and Use Selection for Find
  (Command-E) are in the same menu. In the field, Return goes to the next
  match, Shift-Return to the previous one, and Escape closes the bar. Find
  Next and Find Previous show the bar if it is hidden, because the bar is
  what dismisses the highlights.
- **The core searches, not the attributed text.** The window calls
  `md_snapshot_find` through `CoreSession.find`
  ([adapter](../../docs/macOS-Core-Integration.md#package-and-ownership)),
  off the main thread and only for the snapshot generation on screen. Typing
  starts a new search and only the newest result is shown, so a slow search
  never replaces a newer one. The core stops at 10,000 matches, shown as
  “10,000+”. A query over 4,096 bytes shows “Search text is too long”.
- **Exact highlighting.** The renderer records where each node's display
  text lands in the rendered string. Core match segments are UTF-8 ranges
  in that display text, so they convert to exact UTF-16 ranges, including
  across emphasis and links, in code spans and blocks, table cells, alt text
  and raw HTML source. A break or an image attachment is highlighted whole.
  Mapping runs off the main thread.
- **Highlights and the current match.** Highlights are TextKit 2 rendering
  attributes, so the text storage never changes and copied text carries no
  highlight. The current match is drawn in the system find colour with dark
  text, the other matches more lightly. While the find field has the focus,
  the current match is highlighted but not selected, because the inactive
  selection would draw over it. Find Next and Find Previous with the focus
  in the document select the match, and closing the bar selects the current
  match unless the reader has moved the selection since, so Copy and
  VoiceOver follow it. A Return or Command-G during a running search waits
  for it instead of starting another. Each new search result is announced to VoiceOver (“3 of 12”).
- **Navigation.** A search starts at the selection. Next and previous wrap
  around the document and continue from the current match until the reader
  moves the selection. They count matches, not source offsets (specs/search).
  A reloaded document is searched again.
- **Unicode.** Matching follows the [core rules](../../specs/search/README.md#matching-rules):
  simple case folding (ẞ matches ß but not “ss”, and the final sigma matches
  σ), no Unicode normalization, no match inside a combining sequence or emoji
  ZWJ sequence, whole words that treat each ideograph as a word, and logical
  order for right-to-left text. An NFC “café” does not match the decomposed
  spelling, which is how the generated prose profile spells it.
- **Text size.** View → Actual Size (Command-0), Make Text Bigger (Command-+,
  and Command-= through a hidden alternate item)
  and Make Text Smaller (Command-−) step through nine sizes from 75% to 250%.
  Fonts and every text metric (indents, spacing, table columns) scale; images
  keep their pixel size. A change re-renders off the main thread and keeps
  the top line, the selection and the find highlights. Images are read again.
- **Appearance.** View → Appearance → System, Light or Dark sets the
  application appearance. Rendered colours are dynamic, so open documents
  follow without re-rendering.
- **Settings.** Markdown → Settings… (Command-comma) has an Appearance pop-up
  and a Text size slider. Both settings apply to every window at once and
  are stored in user defaults under the bundle ID (`appearance`,
  `textScale`). Missing or invalid values fall back to System and 100%. The
  app smoke keeps settings in memory and never writes user defaults.

Limits: find is literal (no regular expressions) and not
normalization-insensitive; that needs the open core proposal in
specs/search. The outline is not searched. Text size is app-wide, not per
window.

## Large documents and accessibility (MAC-001G)

- **Repeated loads keep memory bounded.** On macOS 27 a `realloc` that grows
  a block in Apple's large-allocation range leaves the old region resident
  and dirty, and later allocations don't reuse it ("Malloc Large (empty)" in
  `vmmap`). The core's vectors grow that way while parsing, so every 5 MiB
  document used to leave about 80 MiB behind after its window closed. On
  macOS `markdown-ffi` now installs a global allocator that forwards
  everything to the system allocator, except that a reallocation of 1 MiB or
  more allocates, copies and frees
  ([details](../../docs/macOS-Core-Integration.md#repeated-session-memory)).
- **One continuous text view, bounded paragraphs.** Selection, Copy, Select
  All and the text accessibility element still span the whole document.
  TextKit 2 lays out and draws a whole paragraph to show any of it, so once a
  paragraph passes 16,384 UTF-16 units the next space or soft break becomes a
  line break, and a run with no space gets a line break inserted at a
  grapheme boundary every 32,768 units. Find maps matches exactly across both
  (an inserted break is skipped; a replaced space is one unit, like the
  break). Only such paragraphs change: copied text has a line break there,
  and a list item's continuation starts at its marker.
- **The start of a long document shows first.** The renderer passes a
  *lead*, the root blocks up to the first 32,768 UTF-16 units, while it
  renders the rest. The window shows it, then appends the remainder in place
  without moving the selection or the scroll position. The outline fills in
  at the same time, and a search started meanwhile runs once the whole text
  is shown.
- **Fast replacement and copy.** Replacing a long text in place took TextKit
  2 about 12 s for 5 MiB, so the surface clears the view first (14 ms); text
  size changes and reloads use this. For a selection over 65,536 UTF-16
  units, AppKit took about 1 s to choose the pasteboard types and 0.3 s to
  make RTF. Such a Copy writes plain text at once and promises RTF (and RTFD
  when the selection holds a loaded image), made only when an app pastes
  them. The promise is a separate owner the app keeps while the pasteboard
  is unchanged, so pasting after its window closes still works; a promise
  whose pasteboard changed is dropped at the next copy. Shorter and multiple
  selections copy as before.
- **VoiceOver rotors.** The text view offers Headings, Heading Level 1–6,
  Tables and Images rotors. Each item is a range of the document's text,
  labelled with the heading title, “Table N” or the image's alt text (“Image”
  without one), and
  the rotor filter matches labels. Headings are not otherwise announced as
  headings while reading continuously, and tables are read as lines.

Limits: the first VoiceOver line query near the end of a 5 MiB document
lays out all the text before it (see the evidence below). Cold-cache launch
measurements need an administrator for `purge` and were not run.

## External refresh and restoration (MAC-001H)

- **Watching.** Each window watches its document with a kqueue vnode source
  (`DocumentWatcher` in `MarkdownDocument`). The descriptor uses
  `O_EVTONLY`, so watching never stops a disk from being ejected. A write,
  growth, rename, deletion or revocation reports a change. After a rename
  or deletion the watcher re-arms on the path: an editor's atomic save
  replaces the file, so the new file is watched; if nothing is there it
  watches the folder and reports when the path comes back. Bursts within
  100 ms are coalesced into one report. When a window becomes key, the app
  also compares the file's revision token (device, inode, size and
  modification time, from one `stat` off the main thread), for changes a
  network volume may not report. Changes to images, and permission changes
  alone, are not watched.
- **The core decides whether to read.** A report hands the file's current
  token to the core session (`md_session_external_change`). It answers
  *unchanged* (the bytes it already has), *pending* (the read in flight has
  it) or *read required*, and only then is the file read again. A report
  that arrives during a load is checked when the load finishes, so a burst
  of saves ends on the last one and an older revision is never shown after
  a newer one.
- **Refreshing in place.** The new revision loads in the background while
  the window keeps showing the current one, with no loading state. Bytes
  identical to the shown revision (a `touch`, or a file restored after
  deletion) are not parsed, copied or rendered again. A long document waits
  for its whole rendering instead of showing its lead, since the reader's
  place may lie past it.
- **Keeping the reader's place.** The line at the top of the view and the
  selection are mapped to source offsets in the shown revision and stored
  as core block anchors (content fingerprint, neighbours, offset within the
  block; [session contract](../../specs/session/README.md#node-identity-and-anchors)).
  They are resolved in the new revision and mapped back to its rendered
  text, so text added or removed above keeps the same line at the top,
  an edited paragraph keeps the reader on it, and the selection follows its
  words. Reload (Command-R) does the same. Anchoring a position in the 5 MiB
  prose profile and resolving it in an edited revision takes about 85 ms,
  off the main thread. Each copied document keeps its core snapshot handle
  for this; the session holds the same snapshot while it is published, so
  this adds no memory until a newer revision replaces it.
- **Stale documents.** If a refresh fails because the file was deleted,
  moved, became unreadable or is no longer valid UTF-8, the window keeps the
  version it last read and shows a notice below it with the reason and a
  Reload button. VoiceOver announces the notice. The core session records a
  deletion or an unreadable file, as for a failed reload. The notice goes
  away by itself when the file comes back. Choosing Reload, or opening the
  file again, tries to read it; if that fails too, the last version and
  the notice stay. A file missing when it was opened loads by itself once
  it appears.
- **Relaunch.** Reader windows use AppKit state restoration. Each saves a
  bookmark to its document (which also finds the file after a move or
  rename on the same volume), its reading position as anchors, and whether
  its sidebar is shown; AppKit saves the frame. The position is saved half
  a second after the reader stops scrolling or selecting. On relaunch the
  document reopens at that line with that selection. A document whose
  bookmark no longer resolves, or one whose load had failed, is not
  restored. Bookmarks are resolved without mounting a volume, so a
  disconnected server never stalls the launch. Restoration follows the
  system setting “Close windows when quitting an application”: with it on
  (the default), windows come back after a restart or a crash, not after
  Quit. Saved state is private document-derived data; it is never logged.
  This build is not sandboxed, so its bookmarks are not security-scoped
  (MAC-002A).
- **Text size during a lead.** A text size change while a long document is
  still showing its lead now keeps the line at the top and the selection
  (the MAC-001G deferred finding); the new size waits for the whole text.

Limits: a move or rename of an open document is reported as the file not
being found, as before; its window does not follow the file (a relaunch
does, through the bookmark). Change detection on network volumes is
unverified (capability matrix CAP-06). A refresh briefly holds both
revisions; see the evidence below.

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
Its rich-block part opens a document whose sibling image loads through the
app's real resource adapter, while a remote image and a `../` image are
refused; the task marker and tab-separated table rows are checked on TextKit 2.
Copy is written to a private pasteboard, never the user's clipboard.
Its MAC-001F part searches a Unicode document through the real find bar and
menu commands: case folding, NFC and NFD spellings, an emoji sequence, CJK
and right-to-left text, the match status, Find Next and Previous with
wrapping, Return and Escape in the field, Match Case, Use Selection for Find
and the current match becoming the selection. It then checks Make Text
Bigger/Smaller and Actual Size (the text re-renders and keeps the
selection), each appearance reaching the window, and the Settings window
following and changing both settings, which a new preferences reader sees.
`MarkdownRenderTests` render every semantic corpus case and compare its
outline levels, titles and anchors with the frozen expectations, check each
heading's rendered range, link policy on the GFM tour, inert unsafe content,
block and inline styling, render cancellation, and navigation and copy on a
real TextKit 2 view; they also render, install and navigate the generated
performance profiles and print their timings. MAC-001E adds table alignment
(right edges and centres line up across rows, grid fragments are used, copy
is tab-separated), table edge cases, task markers, the image policy (each
denial reason, one read per distinct path, no loader call for remote or
denied targets, width fitting on a real view) and the image budgets
(header pixel limit, forged dimensions, downsampling, count and decoded-memory
limits, cancellation). MAC-001F's `FindTests` map core matches for every kind
of display text (inline markup, entities, code spans and blocks, soft and
hard breaks, alt text, raw HTML, table cells, headings) after astral,
combining, emoji and right-to-left text; check navigation wrapping, the text
size steps and scaled fonts, indents and tab stops; check highlights,
current-match selection and a text size change on a real TextKit 2 view; and
time find over the generated profiles. MAC-001G's `LargeDocumentTests` check
paragraph bounding (spaces and soft breaks past the limit, inserted breaks
at grapheme boundaries, unchanged text otherwise and across text sizes),
find across an inserted break, the lead (a prefix of the complete text and
outline, completed in place keeping the selection and top line, none for a
short document), every rotor in both directions with a filter, lazy RTF and
RTFD copy and AppKit's own pasteboard types, and fast text replacement. The
smoke's reading document is long enough to show a lead, and the smoke checks
it completes and that the heading rotor reaches the last heading.
`MarkdownCoreTests` check find over
the current snapshot: superseded and unpublished generations, the Unicode
rules, truncation, an over-long query, cancellation before the worker runs,
a closed session and zero live handles. `MarkdownDocumentTests` checks resource containment:
subfolders, `./`/`../` within the folder, literal `%` names, escaping `../`,
escaping and in-folder symlinks, absolute paths, missing files, SVG and
non-image types, folders, FIFOs, the size limit and denied permission.
`MarkdownDocumentTests` cover the same reader and loader directly, plus a FIFO,
an exact-bound file, cancellation and a real detached disk image (`hdiutil`).

MAC-001H adds: `MarkdownCoreTests` anchor round trips across an insertion,
an old snapshot still resolving after the session moved on, forged
anchors, an emptied document, change-notice dispositions, unchanged
refreshes without a copy, and documents releasing their snapshot handles.
`MarkdownDocumentTests` drive a real watcher through a write, an atomic
save (followed to the new file), an append, deletion, unrelated folder
changes, reappearance, a coalesced burst and cancellation, and the loader
through an external write (with no loading state), a newer modification
time with identical bytes (no new snapshot), an unchanged check (no read),
deletion (stale, with the session's recorded deletion), the same bytes
returning, ten rapid atomic saves (the last one shown, never out of order),
an invalid save (stale, still stale after Reload, then replaced by a good
save) and a missing file that loads
once it appears. `ReadingPositionTests` map rendered locations to source
offsets and back through non-ASCII text, inline markup, code spans and
blocks and list markers; resolve saved positions across an insertion, an
edited paragraph and an emptied document; check the line (not just the
paragraph) at the top of a real TextKit 2 view across a reload; and time
anchoring on the generated profiles. The smoke's MAC-001H part keeps a
reader's place (a heading at the top, a word selected) across an external
atomic save that inserts text above, a deletion (notice shown, text kept,
deletion recorded), the file's return (notice cleared, nothing re-rendered)
and Reload; it then encodes the window's restorable state into a private
archive, closes the window, restores it from the archive at the same place
and selection, and checks that a second restore, another state version, a
deleted document and unreadable anchors are handled. Finally `check.py`
runs a real relaunch: the bundle is launched with
`--relaunch-check prepare` and `-NSQuitAlwaysKeepsWindows YES`, places the
reader and quits, then relaunched with `--relaunch-check verify`, which
expects AppKit to restore that window at that place and selection. The
verify run closes its windows, so the app's saved state is left empty. The
smoke and measurements never restore or save windows.

Menu actions routed directly to a window do not
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
8. In the same document press Command-F and type `table`. Confirm the
   highlights, the status, Return and Shift-Return, Command-G and
   Shift-Command-G, Match Case and Whole Words, then Escape. Select a word
   and press Command-E. Press Command-+, Command-− and Command-0, choose View
   → Appearance → Dark, Light and System, and open Settings with
   Command-comma. Quit and relaunch to confirm the settings persist. With
   VoiceOver already on, confirm that a search announces its result.
9. With VoiceOver already on, open `build/fixtures/performance/prose-5mib.md`
   (generated by the checks). Confirm the text appears within about half a
   second and stays scrollable while the rest loads. In the rotor (VO-U),
   use Headings, a heading level, Tables and Images on the GFM tour, and
   confirm VoiceOver moves to and reads each item. Note any pause when
   VoiceOver first reads near the end of the 5 MiB document. Select All,
   Copy and paste into TextEdit; the text, its styling and images arrive.
   Copy again, close the window and paste; then copy, quit Markdown and
   paste, and record whether the styled text still arrives after quitting.
10. Open a long Markdown file, scroll to the middle and select a word. Edit
    the file in another editor (TextEdit, or `vim`, which saves by
    replacing the file) and save: the change appears without moving the
    text or the selection. Add lines near the top and save again. Delete
    the file in Finder and read the notice; restore it from the Trash and
    confirm the notice goes away. Turn off System Settings → Desktop & Dock
    → “Close windows when quitting an application”, quit Markdown with the
    window open and relaunch: the window returns at the same place. Move
    the file while Markdown is not running and relaunch again.

Inspect the empty-state text/window/menu roles using macOS accessibility
tooling. Full VoiceOver document reading, mixed displays and permission
recovery remain later native checks. Do not
toggle VoiceOver or change user accessibility preferences as part of this smoke.

## Measurements

`apps/macos/tools/measure.py` drives the app's `--measure` modes on the
generated profiles (prose 1 MiB and 5 MiB, long lines 1 MiB). Build the
release app and run `tools/check.py` (or
`tools/generate_performance_fixtures.py`) first:

```sh
.build/feas004-venv/bin/python apps/macos/tools/measure.py launch          # five warm runs each
.build/feas004-venv/bin/python apps/macos/tools/measure.py launch --cold   # needs sudo purge
.build/feas004-venv/bin/python apps/macos/tools/measure.py repeated --count 20
.build/feas004-venv/bin/python apps/macos/tools/measure.py interaction
.build/feas004-venv/bin/python apps/macos/tools/measure.py refresh --count 10
```

- `launch` opens the document through Launch Services (`open -n -a`), as
  Finder does, after one unrecorded warm-up run. The app measures from the
  kernel's start time for its process to the presentation of the first
  frame with document text (D-012): it waits for the run-loop turn whose
  layout, drawing and Core Animation commit include laid-out text in the
  viewport, then for the next display-link frame. It also reports when the
  whole document is shown.
- `repeated` opens, presents and closes the document in one process and
  reports the physical footprint (`phys_footprint`, Activity Monitor's
  Memory column) after each close.
- `interaction` times outline jumps to the last, first and middle headings
  (to the presented frame), 60 page-down steps (main-thread layout and
  drawing per step, not display pacing), Select All and Copy to a private
  pasteboard, a text size change and the accessibility queries VoiceOver
  makes.
- `refresh` puts the middle heading at the top, then saves a temporary copy
  of the profile from outside (an atomic save, alternating two revisions
  that differ by a paragraph at the top). It reports the time from each
  save to the presented frame of the new revision, whether the heading
  stayed at the top and the footprint after each refresh.

Rows hold counts, times and memory only, never text or paths. Settings stay
in memory and the user's clipboard is never touched.

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
`apps/macos/tools/check.py` passed in debug and release, with 8
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
quarter of a second; MAC-001G bounded its paragraphs. The
interactive steps in manual check 7 were not performed by the agent (no
Accessibility or Screen Recording grant); the smoke covers them through the
same controls.

## MAC-001F local evidence

Checked on 2026-10-30 on the same Mac (Apple M5 Max, macOS 27.0.1, Xcode
27.1, Swift 6.4): `apps/macos/tools/check.py` passed in debug and release,
each with 13 `MarkdownRenderTests`, 6 `FindTests`, 8 `MarkdownDocumentTests`
and 11 `MarkdownCoreTests`, strict format, warnings as errors and both
relocated bundle smokes including the find, text size, appearance and
Settings checks. Offscreen snapshots of highlights (light and dark, 100% and
150%) and of the window with its find bar were inspected. Release timings
from `FindTests` (core find on the worker and mapping off the main thread;
highlighting and revealing the last match on it):

| Profile and query | Matches | Core find | Mapping | Highlight and reveal last |
| --- | ---: | ---: | ---: | ---: |
| Prose 5 MiB, decomposed `café` | 10,000+ | 56.1 ms | 0.9 ms | 44.0 ms |
| Prose 5 MiB, `native text layout` | 10,000+ | 45.6 ms | 3.9 ms | 41.7 ms |
| Long lines 1 MiB, `xxxxxxxx` | 10,000+ | 1.3 ms | 0.3 ms | 26.5 ms |
| Delimiter storm 1 MiB, `a` | 10,000+ | 7.8 ms | 0.2 ms | 27.9 ms |

These are single in-process samples, not presented-frame budgets. One find
call stays well under 100 ms on the largest accepted document and runs off
the main thread, so the uncancellable call needs no cancellable ABI variant
(CORE-002B host guidance). Highlighting all 10,000 matches on the main
thread takes about 40 ms; see MAC-001G's interaction budgets. Release prose
5 MiB rendering, which now records each node's text range, took 586–592 ms
in three runs (MAC-001E: about 580 ms). Manual check 8 and VoiceOver
announcements were not performed by the agent (no Accessibility or Screen
Recording grant); the smoke covers the same controls.

## MAC-001G local evidence

Checked on 2026-10-30 on the same Mac (Mac17,6, Apple M5 Max, 48 GiB,
macOS 27.0.1, Xcode 27.1, Swift 6.4, Rust 1.99.1; two 5K displays at 2×,
60 Hz). Release build, warm filesystem caches, no other validation running.

D-012 launch to presented content (`measure.py launch`, five Launch Services
launches after a warm-up; min/median/max ms):

| Profile | First text presented | Whole document shown | D-016 warm target (400 ms) |
| --- | ---: | ---: | --- |
| Prose 1 MiB | 347 / 355 / 404 | 431 / 438 / 459 | missed by one run (404) |
| Prose 5 MiB | 533 / 547 / 573 | 1,143 / 1,155 / 1,177 | missed |
| Long lines 1 MiB | 343 / 354 / 369 | 266 / 277 / 292 | met |

About 250–265 ms pass before `applicationDidFinishLaunching` (AppKit and
window start-up). Prose 5 MiB then reads and parses for about 215 ms, so
its first text cannot reach 400 ms without a faster or incremental core
parse. Before this task, the same direct launches presented prose 1 MiB at
about 400–420 ms, prose 5 MiB at about 1,020 ms and long lines at about
625 ms, of which about 385 ms was laying out and drawing one 1 MiB
paragraph. Cold-cache runs (`--cold`) need `sudo purge` and were not run.

Repeated loads (`measure.py repeated --count 20`, footprint after each
window closes, launch baseline 28–29 MiB): prose 1 MiB 284–291 MiB, long
lines 239–254 MiB and prose 5 MiB 227–581 MiB. A 40-load 5 MiB run stayed
between 461 and 572 MiB, rising slowly and then dropping back as malloc
trims its cache. With the allocator wrapper disabled, the same 20-load run
reached 1,838 MiB and grew about 81 MiB per load. The retained memory is
mostly the system allocator's cache of freed large blocks, which it reuses.

Interaction (`measure.py interaction`, ms):

| Step | Prose 1 MiB | Prose 5 MiB | Long lines 1 MiB |
| --- | ---: | ---: | ---: |
| Jump to last / first / middle heading, to presented frame (target 150) | 16.6 / 24.5 / 29.8 | 37.6 / 32.6 / 46.3 | 25.8 / 31.3 / 28.5 |
| Page-down step, p50 / p95 / max | 1.5 / 1.8 / 2.0 | 1.5 / 1.8 / 1.8 | 0.1 / 4.5 / 4.9 |
| Select All, then Copy | 0.2, 4.8 | 1.6, 25.0 | 1.0, 1.5 |
| Text size 150%, to presented frame | 169.5 | 705.5 | 68.6 |
| Heading rotor, last heading | 0.3 | 1.2 | 0.0 |
| First accessibility line query at the end | 711.3 | 3,605.3 | 131.1 |

Before the copy change, Copy of the whole 5 MiB document blocked the main
thread for 1.3–1.5 s; before the replacement change, the text size change
took 12.6 s. The first `accessibilityLine(for:)` near the end lays out all
the text before it (later queries take under 1 ms), and the 5 MiB footprint
then rises to about 1 GiB while the document stays open. VoiceOver asks
for line numbers while reading, so this first pause and memory remain open
for owner disposition with a real VoiceOver check (manual check 9). Display
pacing, mixed-scale displays and VoiceOver speech were not measured by the
agent.

## MAC-001H local evidence

Checked on 2026-10-31 on the same Mac (Apple M5 Max, macOS 27.0.1 build
26A434, Xcode 27.1, Swift 6.4, Rust 1.99.1). `tools/check.py` passed in
debug and release: 30 render, 11 document and 13 core tests, both bundle
smokes with the refresh and restoration parts, both real relaunch checks
and the relocated release consumer.

External refreshes (`measure.py refresh --count 10`, release, middle
heading at the top; ms from the save to the presented frame of the new
revision, min/median/max; footprint in MiB):

| Profile | Refresh | Place kept | Footprint before → after 10 refreshes |
| --- | ---: | --- | --- |
| Prose 1 MiB | 300 / 310 / 324 | 10 of 10 | 288 → 348–410 |
| Prose 5 MiB | 1,033 / 1,050 / 1,067 | 10 of 10 | 531 → 887–1,043 |
| Long lines 1 MiB | 150 / 167 / 167 | 10 of 10 | 239 → 253–267 |

A refresh reads, parses and renders the whole new revision off the main
thread (for 5 MiB, about 215 ms of read and parse and 600 ms of rendering),
while the window keeps showing the old one. In the 5 MiB profile, saving
the anchors took about 42 ms and resolving them about 45 ms, off the main
thread (`ReadingPositionTests`, release). The footprint rises over the first two
or three refreshes and then stays flat: for a moment both revisions'
models, renderings and layouts are alive, and the system allocator keeps
the freed blocks for reuse, as with repeated loads (MAC-001G). It is not
growing, but a 5 MiB document can stay near 1 GiB after it has been
refreshed; see [TODO.md](../../TODO.md#open-native-budgets). Manual check 10
was not performed by the agent; the smoke and relaunch check cover the same
paths with synthetic files.
