# Project Memory

Durable, concise context for future humans and agents. Detailed research and
design belong in `docs/` or `agents/`; runtime transcripts do not belong here.
Run logs, per-phase status notes and superseded handoffs are in
[the memory archive](agents/Memory-Archive.md). It is history, not a required
read.

## Current posture

- Phase: `portable-core`, the shared track (GATE-001 approved, D-018).
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
- CORE-FIX order: DOC-001, CORE-002B, CORE-003C, CORE-003D, CORE-005C,
  CORE-005D, then CORE-003E (owner-added from the PR #17 review). The CORE-001 to CORE-006 aggregates follow and close on shared
  acceptance. HARD-001 sits between DESK-001 and DESK-002.
- DOC-001, CORE-002B and TOOL-002 (CI tiers, D-028) merged in PRs #17 to
  #19. Now: CORE-003C. See
  [HANDOFF.md](HANDOFF.md) and [TODO.md](TODO.md).
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
- Repeated native-only 5 MiB parses on the Mac grew RSS to about 583 MiB
  after twenty loads with zero native owners and no leaked bytes. It looks
  like macOS allocator retention; Linux/glibc stays flat. No acceptable
  repeated-session budget is established.
- Dense delimiter input at the 8 MiB source ceiling peaks at about 1,152 MiB
  before the node limit stops it (D-024). The content-search guard is
  CORE-005C; hosts still cap concurrency.
