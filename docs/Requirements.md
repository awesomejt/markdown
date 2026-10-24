# Requirements Baseline

## Purpose

This document extracts the product behavior demonstrated and specified by
`markdown-viewer` and reorganizes it for performance-oriented native
applications. It is a requirements source, not a promise of immediate feature
parity and not an instruction to reuse the existing implementation.

Platform behavior may differ when the operating system imposes different file,
lifecycle, rendering, or distribution constraints. Every material deviation
must be recorded in the capability matrix with rationale and fallback behavior.

## Delivery principles

1. Ship basic, coherent functionality before advanced rendering and workflow
   features.
2. Treat Windows, macOS, and Linux as first-class desktop targets.
3. Establish a thin native application on all three desktops early, then deepen
   them against one semantic conformance suite.
4. Add iOS/iPadOS after desktop contracts and performance budgets stabilize.
5. Optimize the UI, text layout, file access, watchers, packaging, and OS
   integration separately for each target.
6. Share code only when it improves consistency or maintenance without
   compromising native performance.

## R1 — Desktop foundation

### R1.1 Local files and folders

- Open a Markdown file through native file-open flows and OS associations.
- Open an approved folder as a workspace and browse nested folders.
- Navigate relative Markdown links and images within the accessible scope.
- Show clear errors for missing, unreadable, oversized, or unsupported files.
- Preserve recent files/workspaces using platform-appropriate persistent
  identity, with recovery when a file moves or permission expires.
- Detect external file changes and refresh without losing reading position.

### R1.2 Basic Markdown

- CommonMark plus GitHub Flavored Markdown tables, task lists,
  strikethrough, and autolinks.
- Headings with stable anchors, paragraphs, emphasis, lists, blockquotes,
  thematic breaks, links, images, inline code, and fenced code.
- Safe handling of raw HTML and dangerous URLs; no executable document content
  in the application process.
- Syntax highlighting for a useful initial language set, performed lazily and
  within explicit time/memory budgets.
- YAML front matter separated from document content and exposed as properties.
- GitHub-style alerts if the chosen parser/model supports them cleanly.

### R1.3 Native reading experience

- Native window, menu, command, keyboard, pointer, selection, clipboard,
  accessibility, theme, and high-DPI behavior for each desktop.
- Document outline/table of contents with heading navigation.
- Light/dark/system appearance and text-size controls.
- Responsive long-document scrolling and incremental/lazy block presentation.
- Restore window, workspace, document, outline, and reading-position state.
- Operate offline with no telemetry or runtime CDN dependency by default.

### R1.4 Basic search

- Find within the open document with source-accurate highlighting/navigation.
- Workspace file-name and content search may ship in the second desktop
  increment if it would delay the coherent single-document MVP.

### R1.5 Distribution

- Native installable artifacts for supported Windows, macOS, and Linux targets.
- Platform-specific signing and store/package formats are release tasks, not
  prerequisites for local development.
- Builds identify their semantic contract and core version for diagnostics.

## R2 — Desktop refinement

- Recent documents and workspaces with missing-item recovery.
- Virtualized browsing for folders with thousands of entries.
- Workspace-wide full-text search with bounded results and progress.
- Multiple reading styles, fonts, line height, and content width.
- Properties panel, outline, and platform-appropriate adaptive layout.
- Word count, estimated document time, and per-section timing.
- Focus/distraction-free reading mode and platform-appropriate full screen.
- Print and safe offline export where the target OS supports them.
- Live refresh for files, folder listings, and search results.
- Preferences and per-workspace/project settings with documented precedence.

## R3 — Advanced functionality

- KaTeX-compatible math semantics.
- Mermaid and D2 diagrams with source, errors, theme, pan/zoom, and export.
- Project files compatible with or explicitly migrated from `*.mdv.yaml`.
- Cross-root projects where the platform permission model permits them.
- Voice-over document opt-in, speakers, directions, notes, pronunciation,
  target timing, source-line numbering, and A/V layouts.
- Teleprompter auto-scroll, speed controls, wake behavior, and session state.
- Diagram/image export and standalone document export.
- Controlled project-setting writes only if editing is deliberately added.

Advanced items are individually prioritized; R3 is not one release gate.

## R4 — Performance requirements

- Define baseline hardware and representative corpora per platform.
- Measure cold/warm startup, time to first useful content, full parse time,
  peak memory, layout cost, scroll frame pacing, search throughput, and package
  size.
- Parsing, highlighting, search, image decode, and diagrams must be cancellable
  or strictly bounded.
- Results carry a document generation; stale results are never presented.
- Small state changes must not rebuild or relayout the entire document.
- Platform applications may choose different data structures or rendering
  strategies while producing semantically equivalent results.

Initial numeric budgets are hypotheses in `docs/Validation.md`; feasibility
tasks replace them with measurements on named systems.

## R5 — Security and privacy

- Treat document bytes, front matter, HTML, URLs, images, SVG, diagram source,
  and filenames as untrusted.
- Restrict access to user-selected files/folders and OS-granted capabilities.
- Resolve traversal, symbolic-link, alias, junction, and case/normalization
  behavior according to a documented platform policy.
- Disable remote resources by default; never fetch implicitly while offline.
- Do not record document contents or full private paths in task state,
  telemetry, or crash annotations.
- Put limits on file size, nesting, search scope, rendered resources, and work
  duration.

## Platform capability matrix

| Capability | Windows | macOS | Linux | iOS/iPadOS |
| --- | --- | --- | --- | --- |
| Open arbitrary local folder | Native picker/shell; normal desktop access | Native picker; sandbox rules depend on distribution | Native chooser/portal; sandbox rules depend on packaging | Files/document picker only; security-scoped/provider access |
| Persistent workspace access | Path plus platform identity/permissions | Security-scoped bookmark when sandboxed | Path/portal grant varies by package | Security-scoped bookmark/provider; may become stale |
| Continuous file watching | Supported; watcher must handle replace/rename | Supported; coordinate sandboxed files | Supported; backend varies by filesystem | Foreground/best effort; background execution is limited |
| Network-mounted folders | OS-mounted shares | OS-mounted shares | OS-mounted shares | Files providers; not equivalent to mounted desktop filesystems |
| Child-process diagram tool | Possible with packaging/security review | Possible outside restrictive sandbox; review required | Possible with packaging/security review | Not a general solution; arbitrary subprocess unavailable |
| Background indexing | Normal desktop service/task options | Constrained by sandbox/energy policy | Desktop/service options vary | Strong lifecycle/background limits |
| Full screen/focus | Native desktop behavior | Native desktop behavior | Desktop-environment dependent | Scene/device-specific; no promise of desktop equivalence |
| Prevent display sleep | OS API/power policy | OS API/power assertion | Desktop-environment/API dependent | Platform APIs and user policy; behavior may be restricted |
| Print/export/share | Windows print/share/file dialogs | AppKit print/share/file export | Print stack and desktop portals vary | System share/print/document export |

The matrix records expected constraints, not completed support. Feasibility and
platform tasks must replace assumptions with tested behavior and versions.

## Conformance policy

- Semantic fixtures define portable behavior for parsing, source spans,
  outline, properties, link classification, search, and advanced extensions.
- Platform acceptance tests define native interaction and presentation.
- Visual layout may differ; semantic meaning, safety policy, and user-visible
  diagnostics should match unless the capability matrix records a deviation.
- No platform is held back solely to imitate an OS behavior it does not have.
