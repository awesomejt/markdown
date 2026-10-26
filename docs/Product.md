# Product Definition

## Product statement

Build a fast, private, offline native Markdown reader that feels at home on
each supported operating system while interpreting the same document
consistently across platforms.

The first product is not a native wrapper for the LAN web application. It is a
local-document application with native file access, navigation, commands,
accessibility, typography, state restoration, and presentation.

## Primary users and jobs

- A Windows, macOS, or Linux user opens a file or folder and reads technical or
  long-form Markdown with a useful outline, links, code, properties, and
  search, using conventions and integrations native to that desktop.
- An iPad/iPhone user chooses documents through the system picker and reads
  them offline with adaptive navigation and excellent accessibility.
- A user moves between platforms without semantic surprises in headings,
  links, tables, task lists, front matter, and supported extensions.

## Definition of native

Required:

- Platform lifecycle, windows/scenes, menus/commands, navigation, settings,
  file permissions, accessibility tree, text interaction, sharing, and state
  restoration are platform-owned.
- The main document is presented by native UI/text components, potentially
  using lower-level native text/layout APIs where a high-level framework does
  not meet performance or accessibility requirements.
- Core behavior is available in-process and offline.

Allowed after an explicit decision:

- A bundled non-UI engine may compile a diagram or math expression into a safe
  intermediate form displayed by native UI.
- Small platform bridges may use C/C++ or Objective-C ABI layers.

Not the default:

- Loading the entire React application in a WebView.
- Starting a localhost server solely to make a native window display the web
  UI.
- Requiring the existing LAN server for ordinary local document reading.

## Parity tiers

### Tier 0 — feasibility contract

CommonMark/GFM semantics, source spans, stable outline, links/images, front
matter, unsafe-content policy, long-document rendering, and accessibility.

### Tier 1 — first useful release

- Open files and approved folders.
- Read-only GFM with tables, task lists, fenced code, alerts, and safe links.
- Syntax-highlighted code where it meets performance and accessibility goals.
- Outline, document properties, find/search, theme and text controls.
- Recent documents, state restoration, external-change handling.
- Offline operation and no telemetry by default.
- Native packaging and launch behavior on Windows, macOS, and Linux.

### Tier 2 — reading experience

Multiple reading styles, per-section timing, focus mode, print/share/export,
project/workspace concepts where native users need them, and selected
voice-over features.

### Tier 3 — advanced parity

Mermaid, D2, math, teleprompter, complete voice-over syntax, cross-root
projects, and the remaining web feature set. Each item needs a native product
case; parity is not automatic.

## Explicit non-goals for the first release

- Markdown editing or file management operations.
- Public-network serving, authentication, or collaboration.
- One shared UI toolkit across all operating systems.
- Pixel parity with the web application.
- Identical UI implementations or pixel parity across operating systems.
- Full HTML/CSS compatibility inside Markdown.

## Quality attributes

- **Privacy:** no document upload, telemetry, or runtime CDN dependency.
- **Safety:** capability-scoped file access; raw HTML disabled or strictly
  represented; safe URL/SVG policy.
- **Accessibility:** semantic headings, lists, tables, links, controls, and
  useful VoiceOver order are release requirements.
- **Performance:** native startup and input responsiveness, measured parsing and
  layout, incremental work for long documents, cancellation and stale-result
  rejection, and no full-document UI rebuild for a small state change.
- **Consistency:** parser behavior is tested through shared semantic goldens.
- **Recoverability:** stale permissions, moved files, parse errors, and external
  changes have user-visible recovery paths.

## Owner answers (GATE-001, 2026-10-25)

1. MVPs are deepened in the order macOS, Linux, Windows (D-013).
2. Whether a non-UI engine may compile diagrams or math is decided from the
   FEAS-009 comparison; the owner wants the best-performing safe path, even a
   custom implementation.
3. v1 contains core Markdown only. After it: voice-over and expanded syntax,
   teleprompter, Mermaid, math, then D2 (D-013).
4. v1 opens files and folders through the system only; `*.mdv.yaml` projects
   come later in ADV-003.
