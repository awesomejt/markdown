# Proposed Architecture

## Status

Proposed desktop-first baseline. GATE-001 must confirm the core language,
native UI stacks, interop, performance budgets, and platform scope before
production scaffolding.

## System shape

```text
            shared requirements, semantic specs, fixtures, benchmarks
                                  |
                    candidate portable semantic core
             parse / model / search / settings / diagnostics
                    /              |              \
          stable facade      stable facade      stable facade
                |                  |                  |
        Windows native       macOS native        Linux native
        app + adapters       app + adapters      app + adapters
                                  |
                         iOS/iPadOS later
```

The applications share no UI framework and do not use a localhost HTTP API.
Each target owns its native rendering, filesystem, lifecycle, accessibility,
commands, persistence, packaging, and profiling. The shared core is optional
until feasibility evidence shows that it improves the product.

## Portable contract

The portable contract is more important than the portable implementation. It
defines versioned, serialization-tested values for:

- Document metadata and diagnostics.
- Blocks and inlines with stable IDs and source spans.
- Outline, front-matter properties, links, images, and code metadata.
- Safety classification and unsupported-feature nodes.
- Search results, bounded snippets, read metadata, and settings.
- Capability/version negotiation between the app and any shared core.

The contract does not expose parser arenas, raw pointers, UI types, platform
paths, fonts, colors, or operating-system handles.

## Candidate core responsibilities

- Validate bounded UTF-8 input and parse the accepted Markdown dialect.
- Produce immutable semantic snapshots and diagnostics.
- Apply extension passes deterministically.
- Compute outline, properties, anchors, word/section data, and link intents.
- Search approved document snapshots within caller-supplied budgets.
- Resolve portable settings semantics.
- Reject or classify unsafe HTML, URLs, and resources.

The core never opens an arbitrary platform path, creates windows, persists OS
preferences, watches the filesystem, or chooses a native presentation.

## Platform application responsibilities

Each application owns:

- Process/window/scene lifecycle and state restoration.
- Native file/folder selection, permission persistence, and file identity.
- File coordination, change watching, network-volume/provider behavior.
- Document session scheduling, cancellation, and stale-result rejection.
- Native text/block layout, selection, accessibility, links, images, and code.
- Outline/search/properties UI and platform conventions.
- Print, share/export, clipboard, drag/drop, settings, and recent items.
- Packaging, signing, update integration, crash/privacy policy, and profiling.

Platform adapters pass document bytes, logical base identity, options, and
budgets into the semantic layer; they receive values and diagnostics back.

## Document flow

1. A native picker, shell association, or recent item identifies a file or
   workspace under the target OS permission model.
2. The platform document store reads a bounded byte snapshot with a logical
   identity and revision token.
3. The document session assigns a generation and schedules parse/model work
   away from the UI thread.
4. The semantic layer returns an immutable snapshot or typed failure.
5. The session drops obsolete generations and publishes the newest snapshot.
6. The native renderer presents semantic blocks lazily/incrementally.
7. External changes trigger a coordinated reread and restore position using a
   stable block ID plus intra-block offset.

## Native rendering

Do not represent a large document as one monolithic attributed string or one
native view per trivial inline token without measurement. Each platform spike
must compare block virtualization, native text layout, selection,
accessibility, and incremental update strategies.

Specialized native block presenters handle tables, code, images, math, and
future diagrams. Platforms may implement them differently while semantic
fixtures remain consistent.

Likely platform surfaces:

- Windows: WinUI controls plus lower-level Windows text/rendering APIs where
  profiling requires them. FEAS-003 supports a WinUI shell, bounded read-only
  rich edit and virtualized-block investigation; a monolithic RichTextBlock is
  not the universal large-document strategy. See the
  [Windows evaluation](Windows-Rendering-Evaluation.md) for measured limits.
- macOS: AppKit document/text/list primitives, with SwiftUI for suitable shell
  and settings surfaces.
- Linux: GTK 4/libadwaita widgets and native text/layout facilities.
- iOS/iPadOS: UIKit document/text primitives, with SwiftUI where suitable.

## Interop

If a shared core is selected, prefer a narrow stable C ABI or generated
language binding over exposing C++ or Rust implementation types. Operations
should be coarse enough to avoid per-node FFI chatter and unnecessary copies.

- Immutable snapshots or chunked result handles.
- Typed status/error values.
- Explicit lifetime and cancellation handles.
- Contract/core version negotiation.
- Panic/exception containment at the boundary.
- Benchmarks that isolate conversion and copy cost.

The Linux app may call a Rust core directly, but it must still respect the same
public facade used to test the other hosts.

The FEAS-006 checkpoints exercise one experimental C ABI on
[Windows](Shared-Core-Windows-Evaluation.md),
[macOS](Shared-Core-macOS-Evaluation.md), and
[Linux](Shared-Core-Linux-Evaluation.md) with two owned value buffers,
negotiated layouts, matching-allocator release and cooperative cancellation.
All three native hosts copied values and released core owners before UI work;
normal-library relocation and native source debugging passed on the named
systems. This supports the GATE-001 decision, but the flat model, snapshot-local
IDs, package constraints, and spike ABI are not the production semantic contract.

## Platform capability deviations

The app queries a static/build-time capability set and, where needed, runtime
OS capabilities. Unsupported features remain explicit semantic nodes so the UI
can explain the limitation, show source, or offer an alternative. They must not
silently disappear or trigger a network fallback.

Feasibility capability entries live in
[Platform-Capability-Evaluation.md](Platform-Capability-Evaluation.md), with a
compact summary in `docs/Requirements.md`. They move to versioned specs once
implementation begins.

## Security

- Treat every document, path, URL, image, SVG, and diagram as untrusted.
- Access only OS-approved files/folders and pass bytes—not ambient filesystem
  authority—to portable logic.
- Default to no executable raw HTML and no implicit remote resources.
- Bound file size, nesting, search, image decode, highlighting, diagram work,
  and cross-boundary payloads.
- Sanitize generated SVG and prevent external resource resolution.
- Keep document contents and private full paths out of logs and project state.

## Monorepo boundaries

The repository layout and dependency rules are defined in
`docs/Repository-Strategy.md`. A source-level monorepo does not create one
runtime binary or force one release artifact. Each platform application is
independently buildable and packages only its required core and resources.
