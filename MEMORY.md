# Project Memory

Durable, concise context for future humans and agents. Detailed research and
design belong in `docs/` or `agents/`; runtime transcripts do not belong here.
Run logs, per-phase status notes and superseded handoffs are in
[the memory archive](agents/Memory-Archive.md). It is history, not a required
read.

## Current posture

- Phase: `desktop-apps`; the portable-core shared track is complete through
  CORE-006 (GATE-001 approved, D-018). Native chains are now eligible (D-027).
- Product: local-first, read-only native Markdown reader. Windows, macOS and
  Linux desktop apps come first; iOS and iPadOS follow once the desktop
  contracts are stable.
- Organization: one native monorepo (D-004) with independent native UIs,
  shared specs, fixtures and benchmarks, and a Rust/pulldown-cmark core behind
  a hand-written C ABI (D-008, D-009). Source is Apache-2.0 (D-019).
- Merged shared core (PRs #1 to #13): CORE-001A, CORE-002A (model v1),
  CORE-003A and CORE-003B1 to B4 with B2F (parsing, front matter, opt-in model
  v2 with URI targets, extensions, conformance and budgets), CORE-004A
  (sessions), CORE-005A/A2 and CORE-005B/B2 (find, reading stats and
  workspace search, with C ABI) and CORE-006A (C ABI facade). The CORE-003B
  aggregate is closed.
- Three shared-core reviews ([docs/reviews/](docs/reviews/README.md), PR #16)
  found no correctness, memory-safety or file-access defect. The owner
  accepted D-025 (native 001B packages build `model-v2` and request v2; v1
  stays the Rust default), D-026 (the CORE-FIX phase) and D-027 (one linear
  shared track, then one independent chain per host from CORE-006).
- CORE-FIX (DOC-001, CORE-002B, TOOL-002, CORE-003C, TOOL-003, CORE-003D,
  CORE-005C, CORE-005D, CORE-003E; PRs #17 to #27, D-028 to D-032) is merged
  into `main` through the first milestone review, PR #28, tagged
  `m-core-fix`. Its three reviews (Claude, Grok, Codex) found no blocking
  defect. CORE-003F merged in PR #32 (D-034 amends D-032). The CORE-001
  through CORE-006 aggregates closed in PRs #33 to #38; their acceptance
  evidence is linked from [HANDOFF.md](HANDOFF.md). CORE-005 pinned existing
  reference-syntax differences between the v1 workspace index and v2 find.
  CORE-006 documented the ABI v1 zero content-budget change from 8 to 2 MiB
  and passed fresh all-feature release CI on all three OSes. Native packages
  remain platform 001B work; each 001B sets a session memory bound. The next
  Mac step is the macOS skeleton milestone review (MAC-001 closed, PR #48). `main` now requires the
  `Checks / complete` status (M4, 2026-10-29). See [HANDOFF.md](HANDOFF.md) and
  [TODO.md](TODO.md#core-fix-milestone-follow-ups). HARD-001 sits between
  DESK-001 and DESK-002.
- MAC-001A/B (PRs #39/#40) provide the AppKit shell and matching local
  model-v2 static package/Swift owned-value adapter, required debug/release
  consumer CI and explicit per-call bounds: source 5 MiB, nodes 500k, events
  1M, text 16 MiB, copied payload 128 MiB; one worker/two admitted calls.
  Queued cancellation promptly clears owned input/admission; concurrent close
  awaits shared worker cleanup. Bundle ID: `io.github.awesomejt.markdown`.
  [Measurements](docs/macOS-Core-Integration.md) leave repeated-load RSS open
  for MAC-001G before skeleton acceptance.
- MAC-001C (PR #41) adds the Foundation-only `MarkdownDocument` target. It
  covers Open…/Reload, Launch Services delivery with a Viewer/Alternate
  Markdown document type, path-free typed errors (revoked and disk-unavailable
  are distinguished), serial coordinated bounded reads and a per-window
  newest-request-only loader that records failed reads in the core session.
  `NSFileCoordinator` blocks opening a FIFO, so non-regular items are rejected
  before coordination. Not sandboxed.
- MAC-001D (PR #42) adds the `MarkdownRender` target: an off-main renderer
  from the owned v2 snapshot to one attributed string and heading map, shown
  in one read-only TextKit 2 `NSTextView` (never touch `layoutManager`), with
  an outline sidebar and fragment/external link navigation. Only fragment and
  core-external http/https/mailto links get a `.link`. Decode fragments with
  `url.fragment(percentEncoded: false)`, because anchors keep non-ASCII
  letters.
- MAC-001E (PR #43): TextKit 2 stacks `NSTextTable` cells, so tables are one
  paragraph per row with measured, aligned tab stops and a `TableRowFragment`
  (text-layout-manager delegate) drawing the grid. Copy gives TSV. Relative
  images go through `DocumentResources`, which keeps them in the document's
  folder (`realpath` containment, `O_NOFOLLOW_ANY`, regular raster files up
  to 16 MiB, no SVG; it resolves off-main). They decode off-main through
  ImageIO: 32 Mi source pixels, 2,048 px retained side, 128 images and
  128 MiB retained per document. Remote and denied images never reach the
  loader. Share decoded `CGImage`s, never `NSImage`s: alt text is set on each
  `NSImage`.
- MAC-001F (PR #44): find calls `md_snapshot_find` on the core worker over
  `md_session_current`, refusing any generation but the one on screen
  (`CoreError.superseded`). The renderer records each node's display-text
  range, so core segments map exactly to TextKit ranges. Highlight with TextKit
  2 rendering attributes added eagerly (about 26 ms for 10,000 matches): the
  `renderingAttributesValidator` is not re-run for laid-out fragments by
  invalidation. While the find field has focus, don't select the current
  match; the inactive selection paints over it. Text size re-renders at a
  `scale` (75–250%). Settings live in user defaults; the smoke uses
  `MemoryPreferenceStore`, because deleting a suite domain races Quit and
  leaves a plist. Never put the reading surface in an `NSStackView`: hidden
  views detach and lose their frame.
- MAC-001G (PR #45): on macOS 27, a `realloc` that grows a block in the large
  range leaks dirty "Malloc Large (empty)" regions. `markdown-ffi` has a
  macOS-only global allocator that copies reallocations of 1 MiB and more.
  Measure memory with `phys_footprint`, not RSS. TextKit 2 pitfalls: replacing
  a long text in place takes seconds (clear the view first); a paragraph is
  laid out whole (the renderer bounds them); `writablePasteboardTypes` scans
  long selections (~1 s at 5 MiB) and uses legacy type names; and
  `accessibilityLine(for:)` lays out all earlier text. Named pasteboards don't
  send `pasteboardChangedOwner`, so track `changeCount`. A code-built `NSWindow`
  in tests needs `isReleasedWhenClosed = false`. Measure with
  `apps/macos/tools/measure.py`; open budgets are in TODO.md.
- MAC-001H (PR #46): a kqueue vnode watcher (`O_EVTONLY`, re-armed on the
  path after a rename/delete, falling back to the folder) plus a stat on
  window activation feed `md_session_external_change`; refreshes keep the
  shown revision until the new one is ready. `CoreDocument` owns its
  snapshot handle, so positions are anchored in the old snapshot and
  resolved in the new one; keep that in mind for handle-count checks
  (release is async on the worker). Restoration uses AppKit with a cached
  bookmark; `-NSQuitAlwaysKeepsWindows YES` makes a scripted quit keep
  windows. `Data.range(of:)` indexes its argument from zero: never pass a
  slice.
- MAC-001I (PR #47): `apps/macos/tools/artifact.py` zips a `strip -S` copy
  of the release app (strip reapplies the linker ad hoc signature) and
  verifies it extracted outside the checkout, launched directly and through
  Launch Services. `open -a` registers the temporary copy, so unregister it
  with `lsregister -u`. Every bundle carries `Resources/Licenses` from
  `licenses.py` (43 linked packages, a safe superset). The core builds with
  `--remap-path-prefix` for the checkout and CARGO_HOME. Skeleton
  acceptance: [docs/macOS-Skeleton-Acceptance.md](docs/macOS-Skeleton-Acceptance.md).
- MAC-001 aggregate (PR #48) closed on that page with fresh native checks at
  `96d8aea`. The skeleton is AppKit throughout, Settings included; SwiftUI
  stays allowed for later shell surfaces (D-010). No Mac task is eligible
  until DESK-001; the macOS skeleton milestone review (D-030) is next here.
- Workflow: task PRs target `develop` and merge on a Grok review with no
  serious finding (D-029). `main` takes only owner-approved milestone reviews
  from `review/<milestone>`, tagged `m-<milestone>` (D-030). The shared-core
  milestone boundary is reached: CORE-001 through CORE-006 are closed.
  Hand off the combined packet to Claude, then owner review; task completion
  does not approve a merge into `main`.
- Milestone reviews: Claude writes the packet; Grok and Codex add independent
  reviews in `docs/reviews/` on the review branch, not normal PR reviews.
- Grok reviews come from its GitHub integration (D-033): creating a PR starts
  one, `/grok review` asks for another, and the runner polls the PR for it.
  Grok ignores `review/*` and `patch/*`; `patch/` carries unreviewed docs,
  workflow, state and behavior-free nit changes and merges on green CI.
- The owner confirmed on 2026-10-29 that Cursor/Grok reviews post as
  `awesomejt`, with no GitHub app identity and the heading “🤖 Automated
  first-pass review by Grok Bot” (PR #33). Task PRs merge after Grok and CI;
  only critical/high/important fixes need re-review, within the three-review
  cap. The owner handles combined milestone reviews into `main`.
- Each 001B also measures flat-view and retained RSS on its host, and WIN-001B
  makes the MSVC C smoke a required CI step.
- Node IDs are snapshot-local and block anchors carry position (D-021). ABI v1
  grows only by new functions (D-022), and the flat view is frozen (D-023).
  Parser defaults are 1,000,000 nodes and 2,000,000 events (D-024).
- Real-host feasibility evidence for all three desktops is recorded and
  reconciled in the capability matrix. Deferred engine work is ADV-005, after
  DESK-002 (D-014).

## Host notes

- Mac host setup: the project-state skill/backend and workflow contract have
  been restored from the owner's Linux user-level Codex directory. Supported
  inspection works. Xcode 27.1 is installed and selected. Use the normal pass
  lifecycle and re-inspect rather than relying on an earlier revision. The
  reviewed portability patch is now applied to this installed Mac backend,
  with its original backup; native coordination and fixture/path checks passed.
- Windows host setup: Codex roles/skill were copied from Mac, and the native
  build tools and fixture Python dependencies are installed. State coordination
  is restored: the backend detects native Windows versus POSIX Python, and
  `tools/patch_project_state_backend.py` reproduces the reviewed patch on each
  host. Windows, isolated native Mac, and native Linux regression suites passed
  on 2026-10-25. Use `--platform-info` to verify the selected path.
- Windows host facts from FEAS-008 (2026-10-26):
  - Agent shells are not elevated. Cold-cache runs (standby purge) need the
    owner to run the harness from an elevated terminal.
  - Developer Mode is on, so Python `os.symlink` works; PowerShell 5.1
    `New-Item -ItemType SymbolicLink` still needs admin.
  - The feasibility venv has no pip, and markdownlint-cli2 is not installed.
  - Native AOT linking needs `vswhere` on `PATH`; `build_windows.py` adds it.
  - Displays are three 2560×1440 at 150%.
  - Use the native UIA3 client (`UiaProbe.exe`), not Windows PowerShell's
    managed UIA, which lacks `HeadingLevel`.
  - Use forward slashes in Git Bash commands given to the owner.
- On this Mac, `cargo` is not on `PATH` and `~/.rustup` lacks `std`. Use the
  project toolchain: set `RUSTUP_HOME` and `CARGO_HOME` to
  `build/feas002-macos/toolchains/{rustup,cargo}` and prepend its `cargo/bin`.
- On this Mac, Homebrew `python3` lacks PyYAML. Use
  `.build/feas004-venv/bin/python` for `tools/check.py` and
  `tools/project_state.py`.

## Durable context

- The related `markdown-viewer` repository was reviewed at commit `0ab59c0`
  on 2026-10-24. Its worktree was clean and its `main` was 258 commits ahead of
  `origin/main`; the review therefore describes local behavior that may not be
  published upstream.
- `markdown-viewer` is primarily a requirements and functionality source for
  this project. Existing code or dependencies are reused only when they fit the
  native performance and platform goals.
- The web application remains independently useful for LAN/iPad access. The
  native project does not replace it and must not regress it.
- “Native” currently means a platform-owned application and document UI, not a
  wrapper around the complete React app. Diagram compilation may use a bundled
  non-UI engine if security and App Store constraints allow it.
- Mermaid and D2 are the largest parity risks. D2 is rendered by a Go library
  in the web project; iOS cannot depend on spawning that renderer. Diagram
  support is gated and excluded from the first vertical slice.
- Desktop applications should establish thin native walking skeletons on all
  three operating systems early, before deepening one implementation enough to
  bias the shared contract.
- Moderate platform deviation is acceptable. Limitations must be documented in
  a capability matrix and surfaced clearly to users.
- `project.yaml` is stored as readable block-style YAML. State mutations use
  `tools/project_state.py`, which preserves YAML while delegating the atomic
  project-state contract to the installed backend.
- The independent corpus under `fixtures/` is pinned to `markdown-viewer`
  commit `0ab59c0f7bf91730bf9683b47a5eb2658f331b00`: eight semantic cases,
  four generated performance profiles, provenance/hashes, JSON Schemas in
  YAML, and a validator that does not access the sibling repository.
- Unicode fixture contents now use ASCII filenames in Git. The manifest's
  `runtime_path` generates Unicode/NFC/NFD names in separate ignored run
  directories; `tools/validate_fixtures.py` checks three generated paths.
  `.gitattributes` preserves exact corpus bytes across Windows checkouts.
  Windows, macOS, and Linux checks passed on 2026-10-25.
- Never enable pulldown-cmark's YAML metadata option: it accepts `---` blocks
  mid-document and hides a setext heading. Detect front matter on line 1 in
  shared code, and keep voice scans bounded (FEAS-010).
- On Linux and Windows, one continuous text control is rejected for unbounded
  documents. Virtualized semantic blocks (GTK `GtkListView`, WinUI `ListView`)
  must keep document-wide selection and reading order. macOS leads with
  TextKit 2 (FEAS-003 to FEAS-005, FEAS-008).
- GTK keeps about 205 rows realized, so fragments are bounded (1 KiB, 512 B
  unbreakable) and labels need `width-chars` (FEAS-008 Linux).
- WinUI pitfalls (FEAS-008 Windows): put the heading level on the row's text
  element, not the `ListViewItem`; recycled containers must keep template
  content; use Win32 clipboard formats, because DataPackage stream formats
  fail after Flush; Native AOT needs QueryInterface casts for template parts.
- Per-spike evaluation summaries are in the archive; the full reports are in
  `docs/`.

## Known risks

- A rich native attributed-text document can become slow or inaccessible if
  represented as one monolithic view.
- Virtualized document lists make the app own accessibility structure. Open
  gaps: Orca H heading navigation on Linux; on Windows, Narrator reading each
  row twice, and whole-tree UIA searches exceeding the provider timeout at
  5 MiB.
- Parser source ranges may be insufficient for every web behavior; the core
  model needs stable block identities and explicit source mapping.
- iOS security-scoped bookmarks can become stale and require user recovery.
- Raw HTML, remote resources, local links, and SVG each need separate policy.
- Feasibility FFI and local package consumption pass on Windows, macOS, and
  Linux, but production ABI stability, clean deployment, platform packages and
  Apple XCFramework concerns remain open.
- MAC-001B release consumer repeats retained 112.2 MiB RSS after the first
  5 MiB document and 1,698.1 MiB after twenty sessions, despite zero native
  handles, live malloc about 5.1–5.2 MiB and a separate zero-leak diagnostic.
  The host bound is 5 MiB source; dense 8 MiB core/default and 5 MiB host
  sessions peak at 577.9/361.8 MiB process RSS before NodeLimit, distinct
  from the older counted-heap metric below. No whole-app memory acceptance
  is claimed: MAC-001G must resolve or obtain owner disposition before the
  skeleton aggregate. See the current integration report for methodology.
- Dense token input at the 8 MiB source ceiling peaks at about 1,152 MiB
  before the node limit stops it (D-024): pulldown-cmark's first pass costs
  up to 144 bytes per source byte, and the model a few hundred per node.
  Sessions inherit it. Content search bounds one call to 334 MiB measured
  (420 MiB budget) with a 2 MiB default file budget and a node allowance of
  `max_file_bytes / 4` (CORE-005C); hosts still cap concurrency.
