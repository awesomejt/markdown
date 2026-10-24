# Review of `markdown-viewer`

## Review basis

Reviewed 2026-10-24 at local commit `0ab59c0` on `main`. The worktree was clean
and 258 commits ahead of `origin/main`, so this review is pinned to the local
commit rather than the remote repository.

The primary goal was to extract requirements, feature behavior, edge cases,
and validation assets—not to identify code that must be reused. Evidence
included the repository instructions, requirements, architecture,
implementation plan, technical stack, decisions, desktop-wrapper study, Go
packages, React module layout, tests, and build manifests. This was a design
review; the web application's full test suite was not rerun.

## What the existing project is

`markdown-viewer` is a mature LAN application with two coupled halves:

- A Go service owns configured filesystem roots, guarded path access, folder
  listing, settings/project parsing and controlled writes, full-text search,
  D2 rendering, file watching, SSE, and an HTTP API.
- A React/TypeScript application owns Markdown/Mermaid/math/code rendering,
  routing, adaptive three-column layout, preferences, project/workspace state,
  search presentation, export/print, focus mode, teleprompter behavior, and
  voice-over extensions.

Its design is strong for its stated purpose: one server can expose local and
mounted files to iPads and desktops with no runtime internet dependency. Its
security model, NAS behavior, browser quirks, and reading UX have unusually
detailed contracts and tests.

## Architectural strengths worth preserving

1. **Behavior is documented.** Requirements and architecture describe edge
   cases such as Unicode normalization, symlink escape, front-matter limits,
   settings precedence, source-line behavior, live-reload recovery, and
   streamed search completion.
2. **Security boundaries are explicit.** Files are allowlisted, raw content is
   isolated/sanitized, D2 imports cannot read the host, roots constrain access,
   and optional writes use optimistic concurrency and atomic replacement.
3. **The reading experience has real validation.** The project tests multiple
   engines and device layouts, accessibility-adjacent keyboard behavior,
   scrolling anchors, print/export, offline assets, and large inputs.
4. **Domain modules are visible.** Search, settings, projects, file access,
   watching, diagrams, Markdown extensions, and reading modes already have
   conceptual boundaries even where their code is platform-specific.
5. **The fixture/test corpus is high value.** It encodes product semantics more
   reliably than a rewrite based only on screenshots or feature lists.

## Constraints for a native project

### The code is not a portable core

Go packages are under `internal/`, and the UI pipeline is built around React,
the unified syntax tree, DOM behavior, CSS, browser storage, and browser APIs.
There is no stable library interface to import into Swift or Rust. Reusing it
directly means either retaining a local HTTP server/sidecar or embedding the
web application in a WebView.

### “Markdown rendering” contains several products

The web pipeline includes GFM, raw HTML sanitization, GitHub alerts, KaTeX,
Shiki, Mermaid, D2, front matter, outline/read-time analysis, link rewriting,
search highlights, and custom voice-over syntax. A native attributed-text
renderer does not inherit these behaviors from a Markdown parser.

### Filesystem assumptions change on Apple platforms

The server owns absolute configured roots and mounted shares. Native Apple
apps operate through document pickers, sandbox entitlements, security-scoped
URLs/bookmarks, coordinated file access, and lifecycle suspension. Path safety
principles transfer; the concrete Go root implementation does not.

### Diagrams are an architectural fork

- Mermaid is a JavaScript/browser renderer in the existing client.
- D2 is a Go library in the server and intentionally isolated from host files.
- macOS could spawn or embed auxiliary tooling, but iOS cannot use an arbitrary
  subprocess. A portable iOS story therefore needs an embeddable renderer, a
  server dependency, or an explicit feature gap.

## Requirement and implementation classification

| Asset | Native-project use | Notes |
| --- | --- | --- |
| Requirements and edge cases | Primary input | Re-rank into basic, refinement, and advanced tiers. |
| Markdown/voice-over fixtures | Copy with provenance | Create semantic goldens independent of DOM markup. |
| Project/settings schemas | Version or adapt | Preserve compatibility only when it benefits users. |
| Search semantics | Specify and reimplement | Rust port is plausible; do not bind to the HTTP stream. |
| Go filesystem/search code | Optional reference | Reimplement or reuse only if native profiling supports it. |
| React Markdown pipeline | Behavior oracle | Extract semantics; do not assume DOM architecture. |
| CSS themes/layout | Design input | Translate tokens and intent, not selectors. |
| Playwright tests | Acceptance input | Replace with core goldens plus XCUITest/platform checks. |
| D2 Go renderer | Desktop fallback candidate | Not a solution for standalone iOS. |
| API route shapes | Historical contract | Native in-process APIs should be coarser and typed. |

## Review conclusion

A fresh repository is the correct choice. The native applications have
different process, lifecycle, filesystem, rendering, and release boundaries.
The review's main outputs are the requirements baseline, semantic fixtures,
edge cases, and acceptance tests. Code, algorithms, or dependencies are reused
only when they fit the performance and platform architecture.

The web application should remain the behavior oracle during feasibility. Once
the native product intentionally differs, this repository must own its own
versioned contract so that the web project does not become an accidental
permanent dependency.
