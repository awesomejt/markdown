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
its load failed (the same file is found by resolved URL or, off the main
thread, by device and inode, so a hard link or a case variant of the name
focuses it too: MAC-002B); otherwise the first empty window is reused, or a new window
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

Folder grants, recents and access recovery are described in
[Folders, recents and access recovery](#folders-recents-and-access-recovery-mac-002a).
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
  A relative link to another Markdown or text document opens it
  ([Workspace browser and links](#workspace-browser-and-links-mac-002b));
  denied links
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
  folder grant ([MAC-002A](#folders-recents-and-access-recovery-mac-002a)).
  Images are read again whenever the document
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
  MAC-002D adds versioning and two more settings; see
  [Settings and preferences](#settings-and-preferences-mac-002d).

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
  This build is not sandboxed, so its bookmarks are not security-scoped.
  A window with a folder also restores the folder and reopens its document
  through the folder's grant ([MAC-002E](#workspace-watching-and-restoration-mac-002e)).
- **Text size during a lead.** A text size change while a long document is
  still showing its lead now keeps the line at the top and the selection
  (the MAC-001G deferred finding); the new size waits for the whole text.

Limits: a move or rename of an open document is reported as the file not
being found, as before; its window does not follow the file (a relaunch
does, through the bookmark). Change detection on network volumes is
unverified (capability matrix CAP-06). A refresh briefly holds both
revisions; see the evidence below.

## Folders, recents and access recovery (MAC-002A)

- **Folder grants.** File → Open Folder… (Shift-Command-O) grants a folder
  to a window: the key empty window, another empty one, or a new one. A
  window already showing that folder is focused instead. The folder is
  validated off the main thread (it must exist, be a folder and be
  listable). The window then names the folder, and its subtitle keeps
  naming it while a document is shown. File → Open… then starts in the key window's folder.
  A chosen document that resolves, through symlinks, inside the folder
  opens through the folder's grant, in that window while it is empty and in
  a new window of the same folder otherwise; a document outside it (`..`
  or a symlink escape) opens by itself, without the grant.
- **Revocation.** `FolderCapability` holds one shared, revocable
  `FolderGrant`; every document opened through the folder carries it.
  File → Close Folder revokes it: that window's document closes, and every
  other window reading through the folder loses access at once (its read
  fails as **access revoked**, which the core session records). A revoked
  grant has no current revision, so watchers and Reload report it rather
  than serving stale bytes. Permission lost after the folder was read is
  also reported as revoked; a folder on a volume no longer mounted where it
  was read is **disk unavailable**. Errors name no path.
- **Recents.** File → Open Recent lists up to 10 documents, then up to 10
  folders, newest first, and Clear Menu. Each entry keeps a bookmark (the
  persistent grant), its name, last path and device/inode, so a hard link
  or case variant of a recent file is the same entry. Entries are checked
  off the main thread when the list changes and when the app becomes
  active; one that can't be opened is shown with a warning symbol and a
  short reason (“Can’t be found”, “Disk not available”, “Permission
  denied”). The list is stored as JSON at
  `~/Library/Application Support/io.github.awesomejt.markdown/recents.json`
  (written through a 0600 temporary file and renamed, in a folder kept at
  0700; `MARKDOWN_RECENTS_PATH` overrides the location and leaves its folder's
  permissions alone; a failed save keeps the list in memory). It is untrusted input:
  files over 1 MiB are ignored before they are read, and entries with a
  wrong kind, a relative path, oversized fields or duplicates are dropped.
  A list written by a newer version is used but never overwritten. The
  smoke, measurements and relaunch check keep recents in memory.
- **Moved, missing and denied items.** Opening a recent item resolves its
  bookmark without UI or mounting. A moved or renamed item opens from its
  new location, and its entry follows it. An item moved to the Trash counts
  as deleted. One that can't be opened gets a window with its typed error
  and recovery buttons: **Locate…** (missing; the chosen item replaces the
  entry), **Grant Access…** (denied or revoked; a document opened through a
  folder asks for the folder again and reopens the document through the new
  grant), **Try Again** (disk unavailable) and **Remove from Recents**. The
  same buttons follow a failed open or Reload of a document or folder.
- **Sandboxing.** This build is not sandboxed: a plain bookmark is the
  persistent grant, and security-scoped access is bracketed around reads
  anyway. A sandboxed process makes and resolves security-scoped bookmarks
  instead; that path is not exercised until a sandboxed build exists.

Limits: a restored window reopens its document without the folder grant
(MAC-002E).

## Workspace browser and links (MAC-002B)

- **Files sidebar.** A window with a folder shows a **Files | Outline**
  switch above its sidebar (View → Files, Option-Command-1; View → Outline,
  Option-Command-2); without a folder the sidebar is the outline alone.
  The files are an `NSOutlineView` source list over an immutable listing,
  so only visible rows are made and a folder costs nothing until expanded.
  Folders come first, then documents, each sorted as Finder sorts names.
- **Listing.** `WorkspaceListing` enumerates the folder off the main thread,
  under its `FolderGrant` (a Close Folder during the listing stops it as
  access revoked), and asks the core to classify each display path through
  `md_workspace_classify_path`: names the core rejects (invalid UTF-8,
  controls, bidi formatting, too long) are skipped and counted in the
  sidebar's footer; hidden names (a component starting with `.`) are left
  out, with their folders not even read, unless View → Show Hidden Files
  (Shift-Command-.) is on; only Markdown (`md`, `markdown`, `mdx`) is listed,
  and folders without any are left out, unless View → Show All Files is on.
  A name with invisible characters is kept, with a warning symbol and
  “The name contains invisible characters” in its tooltip and VoiceOver
  label. A folder that can’t be read is shown, marked, with no contents.
  Symlinked folders and packages (an app, a bundle) are not descended; a
  symlinked file is listed only when it resolves inside the folder. The
  listing stops after 100,000 directory entries or 255 levels and says so.
  View → Refresh Files lists again (watching is MAC-002E); a new listing
  keeps the expanded folders and the selection.
- **Keyboard and opening.** The arrow keys move through the rows, Right and
  Left expand and collapse (Left on a document moves to its folder), typing
  selects by name, and Return or Enter opens the selected document or
  toggles the selected folder. A click on a document opens it. A document
  opens through the folder’s grant, in the same window, replacing the
  document shown; if another window already shows it, that window comes
  forward instead. The shown document stays selected in the sidebar.
- **Links between documents.** A link the core classifies as needing a
  capability (a relative path, decoded by the core exactly once, and an
  optional heading anchor) is a real link. Clicking it resolves the path
  against the shown document’s folder, off the main thread, without
  decoding it again: a document opened through a folder may link anywhere
  inside that folder, and the target shares the grant; one opened by itself
  may link inside its own folder, as its images may. The target opens as the
  sidebar opens documents, then shows the anchored heading (a missing anchor
  beeps). A link that resolves outside, through `..` or a symlink, or to a
  file that isn’t Markdown or plain text, is refused with a sheet that says
  why; a missing target (even in missing folders) opens and reports “can’t be
  found” with Locate….
- **Finding the open window.** Every open (Finder, Open…, Open Recent, the
  sidebar, links) looks for a window already showing the file: by resolved
  URL, else by device and inode, so a hard link or a case variant of the
  name finds it. Each window records its file's identity off the main thread
  when the file loads; an open reads only its target's, off the main thread
  and for at most 2 seconds, so a stalled volume can't hold up later opens
  (it then falls back to the URL match). Opens are placed one at a time, in
  the order asked.

Limits: no file operations (rename, move, Trash) and no Back/Forward history.
A folder’s listing changes only on Refresh Files or a new grant until
watching (MAC-002E). Workspace search is described in
[Workspace search](#workspace-search-mac-002c).

## Workspace search (MAC-002C)

- **Opening it.** Edit → Find → Find in Folder… (Shift-Command-F) or
  View → Search (Option-Command-3) shows the sidebar's **Search** pane, the
  third segment beside Files and Outline, and puts the cursor in its search
  field. It needs a folder, like the files.
- **What is searched.** Exactly the documents the files sidebar lists, so
  Show Hidden Files and Show All Files apply, symlinks never lead outside the
  folder and an unreadable subfolder is skipped. File names are searched
  first, over the whole listing (`md_workspace_find_names`, in NFC, so a
  decomposed name on disk matches a typed one). Then each Markdown document
  is read through the folder's grant and searched by the core
  (`md_workspace_content_admit`/`_search`), in the sidebar's order. Document
  text is matched as written, under the core's rules: literal, simple case
  folding (Match Case turns it off), Whole Words for document text only, and
  the visible text, not raw Markdown. `.mdx` documents, which the reader can't
  open as text, are not searched. Nothing is indexed or kept between
  searches.
- **Running.** The search starts when typing pauses, on Return, or when Match
  Case, Whole Words or the listing changes (Refresh Files, a listing option, a
  new folder), and the new search supersedes the running one. It runs off
  the main thread; results appear as documents are searched. Stop (or Escape
  in the field while searching) ends it between documents. Close Folder and
  closing the window stop it too. Revoking the folder mid-search stops it
  with **access revoked**.
- **Bounds.** The core's defaults (specs/workspace-search): documents over
  2 MiB are skipped unread, at most 10,000 documents and 256 MiB are read,
  100 matches are kept per document and 2,000 in all, and 200 file names are
  shown. Each document is one bounded core call, and every window shares one
  permit, so one parse runs at a time in the whole app (the CORE-005C memory
  guard bounds one call). The scan reads without file coordination (the same
  bounded, non-blocking read and descriptor checks as an open, about 0.6 ms
  per file cheaper); opening a result is a normal coordinated read.
- **Results.** An outline: **File Names** first, then one group per
  document (its path and match count, `+` when more weren't kept), each match
  a row with its line number and a one-line snippet of visible text, the
  match in bold on the find highlight colour and `…` where the block goes on.
  The line under the results counts what was found and states every reason
  it may be incomplete: a document, reading or match limit, Stop, file names
  not all shown, documents with more matches than kept, documents too large,
  too complex (the core's parse allowance) or not UTF-8, documents that
  couldn't be read, and a listing that was itself cut short. VoiceOver reads
  each row (“Line 12 of docs/guide.md: …”) and hears the summary when the
  search ends.
- **Keyboard and opening.** Down arrow in the field moves to the first result;
  the arrow keys move through them and Return, Enter or a click opens the
  selected one, in this window (or the window already showing the document,
  as for the files sidebar). A file name opens its document; a match opens
  its document with the match selected and scrolled into view, with the find
  indicator. If the document is unchanged since the search (same length and
  SHA-256), the hit's source span applies directly. If it was edited, the
  query is found again in the opened document and the match nearest where
  the hit's anchor resolves is selected; when the query is gone, the cursor
  goes to the anchor's position, and a hit whose anchor is lost beeps. The
  results stay in the sidebar, so the next one is one key away.

Limits: no replace, regular expressions or search history; results don't
update while documents change (watching is MAC-002E), and an open document
isn't re-searched until the search runs again. Workspace hits come from the
core's v1 text index, so a reference-style link label can match differently
than Find in the opened (v2) document (specs/workspace-search).

## Settings and preferences (MAC-002D)

- **Settings window.** Markdown → Settings… (Command-comma) holds every
  setting: Appearance (System, Light, Dark), Text size (75%–250%, the View
  menu's steps), **Show the sidebar in new windows** and **Reopen windows
  from last time**, with **Restore Defaults** (enabled when any setting
  differs from its default). Each change applies at once and is stored at
  once; there is no Apply or OK. The View menu's text size and appearance
  commands change the same settings, and the window follows them.
- **Sidebar.** A new window shows or hides its sidebar as the setting says.
  Open windows keep theirs, and a restored window keeps the sidebar state it
  was saved with.
- **Reopening windows.** On (the default), windows are saved at quit and
  reopened at launch as in MAC-001H, still subject to macOS's own
  “Close windows when quitting an application” (System Settings → Desktop &
  Dock), which the window notes. Off, open windows stop being restorable,
  so AppKit drops their saved state (and its document bookmarks) at its next
  save, and any state saved earlier is declined at launch.
- **Storage and versions.** Settings live in user defaults under the bundle
  ID: `preferencesVersion` (2), `appearance`, `textScale`, `showsSidebar` and
  `reopensWindows`. A missing setting means its default, so Restore Defaults
  removes the four setting keys and keeps the version. MAC-001F's
  unversioned settings are version 1: the first launch upgrades them in
  place, keeping a valid appearance, snapping a text size to its nearest
  step and removing malformed values (a non-Boolean sidebar or reopen value,
  an unknown appearance, a text size that isn't a finite number). A
  malformed `preferencesVersion` counts as version 1. A newer version,
  written by a later build, is never rewritten or downgraded: its known
  settings are read and can be changed, and its other keys are left alone.
  Every read validates the stored value, so a hand-edited setting falls back
  to its default.
- **Isolation.** The smoke, measurements and relaunch check keep settings in
  memory. The preferences check below uses its own throwaway defaults
  suite, deleted afterwards; none of them reads or writes the user's
  settings.

Limits: settings are app-wide, not per document or folder; there is no
font choice, reading width or default sidebar pane yet (ADV-001 reading
refinement).

## Workspace watching and restoration (MAC-002E)

- **Watching the folder.** A window with a folder watches its whole tree
  with an FSEvents stream (`WorkspaceWatcher` in `MarkdownDocument`) and
  lists it again when something changes: a document added, removed or
  renamed, or a folder created, appears in the files sidebar by itself. The
  refresh keeps the expanded folders, the selection and the scroll
  position, and the search is run again over the new listing, unless the
  reader stopped it.
  FSEvents watches by path and holds no file descriptor, so it never keeps
  a disk from being ejected. Events name each changed item, so changes
  only to hidden items (a `.git` folder, an editor's `.swp` file beside a
  visible document) are ignored unless View → Show Hidden Files is on. A dropped or overflowed batch of events still lists the
  folder again, since every refresh is a full bounded listing (MAC-002B).
- **Change storms.** Events are coalesced: a refresh runs once changes have
  been quiet for 150 ms, or 1 s after the first change of a burst if they
  never stop, so a long storm refreshes about once a second. One listing
  runs at a time; changes during it cause exactly one more listing when it
  finishes, never a queue. Reload Files and the View toggles still restart
  the listing at once. In the smoke, 1,500 documents written in about
  150 ms cost one listing, shown about 370 ms after the first write, and
  200 writes inside `.git` and 20 to a `.swp` file in a visible folder
  cost none.
- **Access loss and recovery.** The stream watches the folder's own path,
  so moving, renaming or deleting the folder, or its disk being unmounted,
  lists it again; the listing fails, and the files sidebar and the search
  say why (“The folder can’t be found.”, “The disk containing the folder is
  no longer available.”, “Access to the folder was lost.”). The window keeps
  the folder, so Close Folder still works, and remembers which folders were
  expanded. When the folder comes back at its path (moved back, the disk
  reconnected, permission restored) it lists again by itself: on the
  stream's next event, when the window becomes key (for a volume that
  delivers no events), or with Reload Files. The open document is checked
  again at the same time, so its stale notice clears if it came back too. A
  folder moved somewhere else is not followed while the app runs; reopen it
  from Open Recent, which finds it through its bookmark (MAC-002A).
- **Restoring the workspace.** A window's saved state (MAC-001H) is now
  version 2. It adds a bookmark to the window's folder, the sidebar pane
  (Files, Outline or Search), the expanded folders as display paths (at
  most 512, each at most 4 KiB) and the Show Hidden Files and Show All
  Files options. On relaunch the folder is validated off the main thread
  and listed with those folders expanded and that pane shown, and the
  document reopens through the folder's grant at its saved line and
  selection, so its relative links and images work as before the quit. A
  folder window showing no document restores the folder alone. If the
  folder's bookmark no longer resolves, the document reopens by itself; if
  the folder resolves but can't be read, the window shows the MAC-002A
  recovery (Locate…, Grant Access…, Try Again). A window already showing the
  document, or the folder without a document, isn't restored twice. Version
  1 state, saved before this change, still restores its document. Saved
  state is untrusted: sizes are bounded, an unknown pane falls back to
  Files, and malformed folder state drops the folder, never the launch. The
  expanded paths are only looked up in the fresh listing, never used as
  access. Like the rest of the saved state they are private and never
  logged. Restoration still follows Settings → Reopen windows from last time
  (MAC-002D) and macOS's “Close windows when quitting an application”.

Limits: a search's query and results are not restored, and the search
re-runs on every refresh of a folder whose documents keep changing (it is
bounded and cancellable, MAC-002C). Event delivery on network and provider
volumes is unverified (capability matrix CAP-06); becoming key covers
them. A folder larger than the listing limits refreshes in full each time,
within those limits.

## Packaging and R1 conformance (MAC-002F)

- **Front matter as properties.** A document's front matter is never part
  of its text, so find, copy and reading positions ignore it. A
  **Properties (N)** row above the text, collapsed at first, expands to the
  keys and values the core published, in the core's order (sorted by key);
  a list shows as its items separated by commas. When the core published
  none for the front matter it found (a duplicate key, an unsupported value
  or a limit discards the whole map, [Front-Matter.md](../../docs/Front-Matter.md#atomic-failure-and-diagnostics)),
  the row reads **Properties (not read)** and shows the front matter as
  written, so it stays readable. The row shows this only when the core
  reports one of those discard warnings (`front_matter_duplicate`,
  `front_matter_unsupported`, `front_matter_limit`); empty or comment-only
  front matter is a valid empty mapping and shows no row. The row is
  hidden for a document without front matter, keeps the reader's expanded or collapsed choice across
  documents in the window, scrolls past 180 points and stacks below the
  find bar. VoiceOver reads it as the “Document properties” group, with
  each value labelled by its key.
- **Syntax highlighting.** A fenced code block whose info string names a
  known language has its keywords, type-like names, strings, comments and
  numbers coloured (`SyntaxHighlighter` in `MarkdownRender`): Rust, Python,
  JavaScript/TypeScript, C/C++/Objective-C, Go, Swift, shell, JSON,
  YAML/TOML, SQL and HTML/XML/SVG/CSS, as in the Linux reader (LNX-002F)
  plus Swift. It is a lexical tokenizer, not a parser: a misread token only
  changes a colour. It runs in the off-main, cancellable render, never on
  the main thread, and is bounded: a block over 16,384 UTF-16 units, the
  tokens past 2,048 in a block, and a block that would take the document
  past 524,288 highlighted units all stay plain monospace. Code in a block
  quote takes the quote's colour. The colours are foreground colours with
  light and dark variants, so the find highlights (rendering attributes)
  still show over them and copied rich text keeps them. Unlike Linux,
  which highlights only the rows on screen, the Mac highlights while
  rendering, because TextKit 2 lays out from the finished text; the
  per-document limit bounds that cost instead.
- **Build diagnostics.** `Markdown --version` prints one line naming the
  app version, the semantic contract the app negotiated with its core
  (`model-v2 (ABI v1, anchor v1)`), the core package's configuration and
  fingerprint, and the source commit `build.py` recorded, for example
  `Markdown 0.1.0 (1); Semantic contract: model-v2 (ABI v1, anchor v1);
  Core package: release dd0c327d3a24; Source: 7f2dd11cc395`. Markdown →
  About Markdown shows the same lines in its credits. No path is included.
- **Installable disk image.** `artifact.py` (and the release part of
  `check.py`) now also writes
  `build/macos/artifacts/Markdown-<version>-<commit>[-dirty]-macos-arm64.dmg`:
  a compressed, read-only disk image holding the same stripped
  `Markdown.app` as the zip and an `Applications` link, the usual macOS
  drag-to-install layout. Its size and SHA-256 are in the JSON record
  beside it. The verification mounts it read-only and hidden from Finder,
  checks that it holds only the app and the link to `/Applications`, copies
  the app out as a Finder drag does into a stand-in Applications folder in
  a temporary directory, checks that the installed bundle matches the zip's
  byte for byte and passes the same bundle checks (identity, arm64 and
  macOS 27.0, system libraries only, no build-machine path, unsigned,
  licenses), checks `--version`, runs the full smoke directly and through
  Launch Services, unregisters it and removes it again. It never writes to
  the real `/Applications`. To install by hand, open the disk image and
  drag Markdown to Applications; to uninstall, move `Markdown.app` to the
  Trash (saved settings and window state stay in the user's preferences
  and saved application state, as for any Mac app).
- **Budgets.** See [MAC-002F local evidence](#mac-002f-local-evidence).

Limits: the disk image is unsigned and not notarized, like the zip, so a
copy downloaded on another Mac is quarantined and Gatekeeper refuses it
until it is allowed in System Settings → Privacy & Security (CAP-28/29);
a clean second Mac is untested. The install verification uses a stand-in
Applications folder, so a real install into `/Applications` is a manual
check.

## Development artifact and licenses (MAC-001I)

```sh
.build/feas004-venv/bin/python apps/macos/tools/artifact.py
```

builds the release app and writes
`build/macos/artifacts/Markdown-<version>-<commit>[-dirty]-macos-arm64.zip`
(about 1.1 MB) with a JSON record beside it: the archive's SHA-256 and
size, the source commit and whether the working tree had changes, the
toolchain versions and the core archive's hash. It then verifies the
artifact relocated, as described below. `--no-verify` skips that.

- **What the zip holds.** One `Markdown.app` for macOS 27 on Apple Silicon
  (arm64 only). The core is linked statically; the app needs only the Swift
  runtime and frameworks that macOS supplies. Its debug symbols are
  stripped (`strip -S`), because the debug map names the build's object
  and source paths; local builds under `build/macos/` keep them.
- **No build-machine paths.** The core package builds with
  `--remap-path-prefix`, so the source locations in Rust panic messages
  read `/cargo/registry/…` and `./core/…` instead of the builder's home
  folder and checkout.
- **License inventory.** Every bundle `build.py` assembles carries
  `Contents/Resources/Licenses/`: the project `LICENSE` (Apache-2.0),
  `THIRD-PARTY-NOTICES.txt` and `inventory.json`. The inventory lists the
  Rust packages the app actually links: the normal dependencies of
  `markdown-ffi` with `model-v2` for `aarch64-apple-darwin`, without
  build, development or proc-macro crates (those run only in the compiler).
  `cargo metadata` unifies features across the workspace, so the list is a
  safe superset: a feature another member enables can add a package the
  app does not link, never drop one.
  That is 43 packages: the two workspace crates and 41 from crates.io. Each
  crates.io package's own license files are copied in full; markdown-core
  adds the Unicode License V3 for its Unicode data. Each license must pass
  the repository's [dependency policy](../../docs/Dependencies.md), and a
  package without license text fails the build. Swift and Apple frameworks
  come with macOS and are noted, not copied.
  `Contents/Resources/BuildInfo.json` records the same build facts as the
  JSON record, with no paths.
- **Unsigned.** Only the linker's ad hoc signature is present: no signing
  identity, no bundle seal, no notarization, no sandbox entitlements, no
  installer. `strip` reapplies the linker signature, so the stripped
  executable still runs on Apple Silicon. This is a development artifact,
  not a distribution.

**Relocated launch.** The verification extracts the zip with `ditto` into
a new temporary folder outside the checkout and checks that it holds only
`Markdown.app`, the bundle ID and executable, arm64 only with a macOS 27.0
minimum, only `/usr/lib` and `/System/Library` libraries and no absolute
run path, no file naming the checkout, the home folder or `CARGO_HOME`, no
signing identity or seal, the bundled `LICENSE` and a notice and license
text for every inventoried package. It then runs the full app smoke
twice from the extracted copy: once by executing the bundle directly, and
once through Launch Services (`open -n -W -a`), as Finder launches it.
That launch registers the temporary copy with Launch Services, so the tool
unregisters it afterwards (`lsregister -u`), and stops the app if the
launch times out. `open` does not report the app's exit status, so the
smoke's marker must be its last line of output.
`check.py` runs this for the release configuration; the debug bundle is
still copied to a temporary folder and smoke-tested there.

Limits: the artifact was verified on the Mac that built it. A copy
downloaded on another Mac is quarantined, and Gatekeeper refuses an
unsigned app until it is allowed in System Settings → Privacy & Security;
that path and a clean second Mac are untested. Signed distribution
(CAP-29) needs owner approval.

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
For a single file to hand to a tester, use the
[development artifact](#development-artifact-and-licenses-mac-001i).

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
In release, the relocated app is the
[development artifact](#development-artifact-and-licenses-mac-001i), extracted
outside the checkout, verified and launched directly and through Launch Services,
and the [disk image](#packaging-and-r1-conformance-mac-002f) is installed into a
stand-in Applications folder, launched both ways and uninstalled. Each
configuration's relocated bundle must also print the expected `--version` line.

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
MAC-002A part replaces the open panel with scripted choices and keeps
recents in memory: Open Folder, Open… starting in the folder (inside through
the grant, outside by itself), a second window sharing the grant, Close
Folder revoking access in the other window and Grant Access… reopening it
through a new grant, the Open Recent menu order, a moved folder opening
where it is now with its entry following, a deleted document offering
Locate… (the located file replaces the entry) and Remove from Recents, a
denied folder marked in the menu and recovered with Grant Access…, and
Clear Menu. Its MAC-002B part lists a folder in the files sidebar (Markdown
only, folders first, an invisible-character name marked for VoiceOver),
expands, moves and opens with the arrow keys and Return, follows a relative
link to its anchored heading through the grant, refuses links outside the
folder and to an image, reports a missing target, finds the open window
through a hard link and a case variant, toggles hidden and all files, and
lists and expands a 5,000-document folder. Its MAC-002C part checks the
Find in Folder and View → Search menu items, focuses the search field,
finds file names and grouped content matches over a 404-document folder
with the status counts, moves to the results with Down arrow, opens a match
deep in a long document (selected and in view) with Return and another by
the click action, opens a match in a document edited since the search,
supersedes a running query, stops one, reports a match budget, and clears
the search with Close Folder. Its MAC-002E part watches a folder: an added
and a removed document appear without losing the expanded folders or the
selection, a storm of 1,500 new documents costs a bounded number of
listings (one in practice), 200 writes inside `.git` and 20 to a swap file
beside a visible document cost none, and the
folder moved away is reported in the sidebar and recovers with its
expansion and document when moved back. It then saves a folder window
showing the search pane, hidden files and a document scrolled to a heading
into a private archive, closes it and restores it (folder, pane, option,
expanded folders, the document through the folder's grant and its place,
and watching again), and checks a folder window without a document, a
second restore, a folder bookmark that no longer resolves and forged
expansion and pane values. Its
reading part opens a 202-heading document and checks the rendered text, the
outline rows, outline and fragment-link navigation (heading revealed and
selected, outline following), an external link routed to a stub instead of
the browser, the sidebar command and Copy enabled on the document selection.
Its rich-block part opens a document whose sibling image loads through the
app's real resource adapter, while a remote image and a `../` image are
refused; the task marker and tab-separated table rows are checked on TextKit 2.
Its MAC-002F part checks that a document without front matter shows no
properties, that one with front matter shows **Properties (2)** collapsed,
keeps the front matter out of the text and lists each key and value when
expanded, that the text starts below the expanded properties and the find
bar together, that fenced Swift code is coloured in the live text view, and
that the About credits name the semantic contract and the bundle its source
commit.
Copy is written to a private pasteboard, never the user's clipboard.
Its MAC-001F part searches a Unicode document through the real find bar and
menu commands: case folding, NFC and NFD spellings, an emoji sequence, CJK
and right-to-left text, the match status, Find Next and Previous with
wrapping, Return and Escape in the field, Match Case, Use Selection for Find
and the current match becoming the selection. It then checks Make Text
Bigger/Smaller and Actual Size (the text re-renders and keeps the
selection), each appearance reaching the window, and the Settings window
following and changing both settings, which a new preferences reader sees.
MAC-002D extends it: the Settings sidebar checkbox hides the sidebar of a new
window but not of the open one, and Restore Defaults resets every setting,
re-renders at 100%, follows the system appearance and disables itself.
`MarkdownRenderTests`' `PreferencesTests` cover the schema: a new store,
the upgrade of version 1 settings (kept, snapped and malformed values), a
malformed version, a newer version left untouched while its known settings
are read and changed, validation of hand-edited values, change
notifications and Restore Defaults. Real user defaults are left to the
two-launch preferences check below.
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
expects AppKit to restore that window at that place and selection. It then
does the same for a folder (MAC-002E): `prepare` opens it, expands `docs`,
opens `docs/live.md` from the files sidebar and shows the search pane, and
`verify` expects the folder back with that pane and expansion and the
document through the folder's grant at its place. Each verify run closes
its windows, so the app's saved state is left empty. The
smoke and measurements never restore or save windows.

MAC-002D adds a second two-launch check of settings. `check.py` seeds a
throwaway user defaults suite (`io.github.awesomejt.markdown.preferences-check.<uuid>`)
with `defaults write`, as MAC-001F would have stored it plus a malformed
value, and launches the bundle with `--preferences-check prepare <suite>
<document>` and `-ApplePersistenceIgnoreState YES`, so the user's own saved
windows are neither restored nor rewritten. `prepare` expects the upgrade
(version 2, the text size snapped, the bad value dropped, Dark kept),
changes every setting through the Settings window's controls, checks that
the open window stops being restorable and a new window hides its sidebar,
and quits. `check.py` then reads the suite back with `defaults export`
and requires exactly the five stored values. `verify` expects the settings
at launch (Light applied, the Settings controls, the launch window's
sidebar hidden and not restorable), declines saved window state for a real
document while reopening is off, restores it once the checkbox turns it on
(with its saved sidebar), presses Restore Defaults and checks the stored
keys are gone and the version kept, then deletes the suite.
`check.py` deletes it again, with its property list, even on failure.

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
   In Settings, move through the controls with Tab and Space (with Keyboard
   navigation on) and with VoiceOver; turn off Show the sidebar in new
   windows and press Command-N; turn off Reopen windows from last time, quit
   with a document open and relaunch (no window returns); then press Restore
   Defaults.
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
11. Open a folder, expand a subfolder and open a document from the files
    sidebar. In Finder, add, rename and delete Markdown files in it: the
    sidebar follows without collapsing the subfolder. Unzip a large archive
    or run `git checkout` of another branch in it: the sidebar settles once,
    without flicker. Move the folder to the Desktop and back, and eject and
    reconnect a disk holding a folder: the sidebar reports the loss, then
    lists again by itself. With the search pane showing, quit with the
    window open (with “Close windows when quitting an application” off) and
    relaunch: the folder, the pane, the expanded subfolder and the document
    at its place return. Check the loss message with VoiceOver.

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

## MAC-001I local evidence

Checked on 2026-10-31 on the same Mac (Apple M5 Max, macOS 27.0.1 build
26A434, Xcode 27.1, Swift 6.4, Rust 1.99.1). `tools/check.py` passed in
debug and release: 30 render, 11 document and 13 core tests, the debug
bundle smoke from a copied folder, both real relaunch checks, the relocated
release consumer and the release artifact verification. The artifact was
1,115,214 bytes; its 43 inventoried packages (41 from crates.io) each had a
notice and license text. Extracted into a temporary folder outside the
checkout, it linked only `/usr/lib` and `/System/Library` libraries
(`libiconv`, `libresolv`, `libobjc`, `libSystem`, the Swift runtime and
AppKit, Foundation, CoreGraphics, ImageIO, QuartzCore and CoreFoundation),
had only the linker's ad hoc signature and no run path but
`@loader_path`, and passed the full smoke when executed directly and when
launched through Launch Services. Before the path remapping and `strip -S`,
the release executable named the build user's home folder 23 times in
Rust panic locations and 66 times in the debug map; the artifact names
it nowhere. Stripping took the executable from 3.15 MB to 2.88 MB.
A clean second Mac and a quarantined download were not available.

## MAC-002A local evidence

2026-11-14, macOS 27.0.1 on Apple Silicon, Xcode 27.1, Swift 6.4:
`.build/feas004-venv/bin/python apps/macos/tools/check.py` passed in debug
and release: `swift format lint --strict`, 62 Swift tests per configuration
(including 8 new `FolderTests`: containment and symlink escape, revocation
across documents, the loader's revoked reload, folder validation errors,
moved/missing/denied recents, bounded and de-duplicated persistence,
untrusted stored lists and file storage), the bundle smoke with its MAC-002A
part, relaunch restoration, the release development artifact and the
relocated `CoreProbe` consumer. Not run by the agent: the interactive open
panel for Open Folder…, Locate… and Grant Access… (the smoke scripts the
choices), a real disconnected volume, and VoiceOver on the recovery buttons.

## MAC-002B local evidence

2026-11-15, macOS 27.0.1 on Apple Silicon, Xcode 27.1, Swift 6.4:
`.build/feas004-venv/bin/python apps/macos/tools/check.py` passed in debug
and release: `swift format lint --strict`, 71 Swift tests per configuration
(including 8 new `WorkspaceTests`: core classification, Markdown-only and
all-file listings and their order, symlinks, packages and invisible names,
an unreadable subfolder and root errors, entry/depth limits and
cancellation, a 20,000-document folder listed in about 1.8 s with its
creation, links kept inside their folder with literal `%` names, and file
identity through a hard link; and a document-link URL round trip in
`RenderTests`), the bundle smoke with its MAC-002B part (listing, keyboard
navigation with arrow keys and Return, sidebar opening, links with anchors,
refused and missing links, hard-link and case-variant window lookup,
hidden/all-file toggles, Close Folder, and a 5,000-document folder listed in
46–56 ms and expanded to 5,050 rows), relaunch restoration, the release
development artifact and the relocated `CoreProbe` consumer. Not run by the
agent: VoiceOver on the files sidebar, a folder on a network volume, and
mouse clicks (the smoke drives the click action and key events directly).

## MAC-002C local evidence

2026-11-15, macOS 27.0.1 on Apple Silicon, Xcode 27.1, Swift 6.4:
`.build/feas004-venv/bin/python apps/macos/tools/check.py` passed in debug
and release: `swift format lint --strict`, 81 Swift tests per configuration
(including 3 new core `WorkspaceSearchTests`: name ranks and UTF-16 ranges,
content snippets, lines, spans and anchors, skips and stops; 6
`WorkspaceSearchAdapterTests`: listing order and filters, symlink escape and
`.mdx`, NFC names, budgets, invalid UTF-8 and unreadable files, cancellation,
revocation, and 1,000 documents of about 9 KiB of dense markup searched in
about 0.56 s with no match and 0.61 s with 1,000 matches; and a
`ReadingPositionTests` case locating hits in searched and edited revisions),
the bundle smoke with its MAC-002C part (404 documents searched in 44 ms in
release), relaunch restoration, the release development artifact and the
relocated `CoreProbe` consumer. Not run by the agent: VoiceOver on the search
results and its summary announcement, typing in the field with real key
events (the smoke sets the field and drives the action), and a folder on a
network volume.

## MAC-002D local evidence

2026-11-15, macOS 27.0.1 on Apple Silicon, Xcode 27.1, Swift 6.4:
`.build/feas004-venv/bin/python apps/macos/tools/check.py` passed in debug
and release: `swift format lint --strict`, 88 Swift tests per configuration
(including 7 new `PreferencesTests`), the bundle smoke with its extended
Settings part, relaunch restoration, the new two-launch preferences check
(upgrade, persistence read back with `defaults export`, restoration
declined and allowed by the reopen setting, Restore Defaults, suite
deleted), the release development artifact and the relocated `CoreProbe`
consumer. Not run by the agent: keyboard and VoiceOver use of the Settings
window, and a real quit and relaunch with Reopen windows from last time
off (the check covers the declining path directly, with saved state ignored
at launch).

## MAC-002E local evidence

2026-11-15, macOS 27.0.1 on Apple Silicon, Xcode 27.1, Swift 6.4:
`.build/feas004-venv/bin/python apps/macos/tools/check.py` passed in debug
and release: `swift format lint --strict`, 90 Swift tests per configuration
(including 2 new `WorkspaceWatcherTests`: the coalescer's quiet and maximum
delays, and a real FSEvents stream reporting a new file, ignoring then
reporting changes in a hidden folder and to a swap file beside a document, reporting the folder moved away and back,
coalescing 2,000 writes and stopping when cancelled), the bundle smoke with
its MAC-002E part (1,500 documents written in 133–151 ms, listed once,
about 372 ms after the first write, in both configurations), the existing
document relaunch check and the new folder relaunch check, the preferences
check, the release development artifact and the relocated `CoreProbe`
consumer. Not run by the agent: a disk ejected and reconnected or a network
folder, permission removed from a watched folder in Finder, VoiceOver on
the files sidebar's loss message, and a real quit and relaunch outside the
check's `-NSQuitAlwaysKeepsWindows YES` launches.

## MAC-002F local evidence

2026-11-15, Apple M5 Max, macOS 27.0.1, Xcode 27.1, Swift 6.4:
`.build/feas004-venv/bin/python apps/macos/tools/check.py` passed in debug
and release: `swift format lint --strict`, 97 Swift tests per configuration
(including 7 new `ConformanceTests`: each token kind, common misreads such
as `${#a}`, apostrophes and Rust lifetimes, UTF-16 ranges past non-ASCII
text, the block, token and document limits, quoted and unlabelled blocks
left plain, properties in the core's order with lists, booleans, integers
and null, unread front matter shown as written, and the diagnostics'
contract), the bundle smoke with its MAC-002F part, both relaunch checks,
the preferences check, `--version` from the relocated debug bundle, the
release zip verified relocated, the release disk image (about 1.7 MB)
installed into a stand-in Applications folder, launched directly and
through Launch Services and uninstalled, and the relocated `CoreProbe`
consumer.

Budgets, release, `apps/macos/tools/measure.py` (five warm runs, ten
refreshes), against the D-016 targets of 400 ms to first text and 150 ms
per jump:

| Profile | Warm launch to first text (median, min–max) | Whole document | Outline jumps (first/middle/last) | Refresh (median), place kept |
| --- | --- | --- | --- | --- |
| prose 1 MiB | 419 ms (388–426) | 501 ms | 26 / 30 / 17 ms | 317 ms, 10/10 |
| prose 5 MiB | 605 ms (596–609) | 1,242 ms | 33 / 45 / 51 ms | 1,058 ms, 10/10 |
| long lines 1 MiB | 398 ms (395–407) | 310 ms | 27 / 32 / 32 ms | 167 ms, 10/10 |

Page-down steps stay under 2 ms at the 95th percentile (5 ms for long lines),
Copy of the whole 5 MiB document takes 40 ms, and a text size change
171 ms (1 MiB) and 740 ms (5 MiB). The jump target is met on every profile.
The launch target is met only by long lines 1 MiB, by 2 ms: prose 1 MiB
launches in 419 ms, where MAC-001G measured 355 ms. The same measurement on
the `develop` head this branch starts from (`7f2dd11`) gives 417 ms
(384–428), so MAC-002F adds nothing. The difference from MAC-001G is AppKit
start-up, which now reaches `applicationDidFinishLaunching` about 290 ms
after the process starts (about 255 ms then), before any document work.
The prose profiles contain no fenced code, so highlighting costs them
nothing. The refresh footprint matches MAC-001H (5 MiB: flat at about
1,074 MiB after the fourth refresh). These stay open in
[TODO.md](../../TODO.md#open-native-budgets). Cold launches (`--cold`, which
needs `sudo purge`) were not run.

Not run by the agent: dragging the app from the mounted disk image into the
real `/Applications` in Finder and launching it from there, a quarantined
download on a second Mac, VoiceOver on the properties row, and the About
panel's appearance.
