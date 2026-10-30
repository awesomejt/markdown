# Platform Capability Evaluation — FEAS-007

## Status and scope

Date: 2026-10-25. This is the feasibility-phase capability matrix for Windows,
macOS, Linux and later iOS/iPadOS. It reconciles the measured FEAS-003 through
FEAS-006 native evidence with official platform and engine documentation that
was read on that date. It doesn't accept a production baseline: GATE-001 still
owns the stack, baseline OS versions, package formats, budgets, advanced-feature
scope and the owner's answers to the product questions in
[Product.md](Product.md).

The matrix classifies whether a platform can provide each capability. It is not
a list of implemented, promised or release-tier features. Advanced capabilities
remain individually prioritized under R3 and the ADV milestones.

## Classification rules

Each capability has exactly one status per platform:

- **Supported** — a platform-owned mechanism exists that fits the product's
  native, offline and read-only constraints. It does not mean the product
  implements it today.
- **Alternative** — the requirement is reachable only through a different
  mechanism or user flow than the desktop baseline. The fallback column names
  it.
- **Deferred** — classification waits on missing evidence or a gate decision.
  The capability stays visible and must be reclassified by the named owner.
- **Unavailable** — the platform does not provide the capability within the
  product's constraints. The fallback names the user-visible behavior instead.

Each status cites one evidence level:

- **Measured** — a repository measurement on a named host. Measured cells may
  add authoritative sources as supplements.
- **Authoritative** — official vendor, project or specification documentation.
  It establishes feasibility, never measured product behavior.
- **Unverified** — an explicit evidence gap. Only a Deferred status may rest on
  unverified evidence.

The iOS/iPadOS column combines both families. Where iPad behavior differs, the
cell says so. No iOS/iPadOS device or simulator measurement exists yet, so
every Measured cell is a desktop cell.

`tools/validate_capabilities.py` enforces these rules mechanically: canonical
platform rows, overview/detail agreement, source-kind rules, local paths,
HTTPS references and next-check owners that exist in `project.yaml`.

## Overview

| ID | Capability | Requirement | Windows | macOS | Linux | iOS/iPadOS |
| --- | --- | --- | --- | --- | --- | --- |
| CAP-01 | Open a single file through picker or association | R1.1 | Supported | Supported | Supported | Supported |
| CAP-02 | Open and browse a user-selected folder | R1.1 | Supported | Supported | Supported | Supported |
| CAP-03 | Retain file and folder access across relaunch | R1.1, R2 | Supported | Supported | Supported | Supported |
| CAP-04 | Load relative images beside a single opened file | R1.1, R5 | Supported | Alternative | Alternative | Alternative |
| CAP-05 | Watch local files while running | R1.1, R2 | Supported | Supported | Supported | Alternative |
| CAP-06 | Detect changes in network or provider folders | R1.1, R2 | Supported | Deferred | Alternative | Alternative |
| CAP-07 | Open exact Unicode filename spellings | R1.1, R5 | Supported | Supported | Supported | Deferred |
| CAP-08 | Continue bounded work when not frontmost | R2, R4 | Supported | Deferred | Supported | Alternative |
| CAP-09 | Full-screen reading presentation | R2 | Supported | Supported | Supported | Alternative |
| CAP-10 | Inhibit display sleep | R3 | Supported | Supported | Supported | Supported |
| CAP-11 | Unbounded long-document presentation | R1.3, R4 | Supported | Supported | Supported | Deferred |
| CAP-12 | Document-wide selection and copy | R1.3 | Supported | Supported | Supported | Deferred |
| CAP-13 | Screen-reader document reading | R1.3 | Supported | Supported | Supported | Deferred |
| CAP-14 | Mixed and fractional display scaling | R1.3 | Deferred | Deferred | Deferred | Deferred |
| CAP-15 | KaTeX-compatible math presentation | R3 | Deferred | Deferred | Deferred | Deferred |
| CAP-16 | Mermaid diagram compilation | R3 | Deferred | Deferred | Deferred | Deferred |
| CAP-17 | D2 diagram compilation | R3 | Supported | Supported | Supported | Alternative |
| CAP-18 | Bundled helper subprocess | R3 | Supported | Supported | Supported | Deferred |
| CAP-19 | Generated SVG and math output sanitization | R3, R5 | Supported | Supported | Supported | Supported |
| CAP-20 | Static SVG display | R3 | Supported | Alternative | Alternative | Alternative |
| CAP-21 | Diagram pan and zoom | R3 | Supported | Supported | Supported | Supported |
| CAP-22 | Diagram and image export to file | R3 | Supported | Supported | Supported | Supported |
| CAP-23 | Print document | R2 | Supported | Supported | Supported | Supported |
| CAP-24 | Standalone PDF export | R2, R3 | Deferred | Supported | Supported | Supported |
| CAP-25 | System share | R2 | Supported | Supported | Unavailable | Supported |
| CAP-26 | Voice-over script presentation | R3 | Supported | Supported | Supported | Deferred |
| CAP-27 | Teleprompter timed auto-scroll | R3 | Supported | Supported | Supported | Deferred |
| CAP-28 | Unsigned local development package | R1.5 | Supported | Supported | Supported | Deferred |
| CAP-29 | Signed direct distribution | R1.5 | Supported | Supported | Deferred | Alternative |
| CAP-30 | Store or sandboxed package | R1.5 | Supported | Supported | Supported | Supported |

## Files and access

### CAP-01 — Open a single file through picker or association

| Platform | Status | Evidence | Basis and user impact | Fallback | Next check | Sources |
| --- | --- | --- | --- | --- | --- | --- |
| Windows | Supported | Measured | Native WinUI processes opened every frozen corpus file. The Windows App SDK file picker and shell file associations are documented, but neither was exercised. | Command-line and drag-open paths | WIN-001: picker, association and revoked-file errors through the accepted shell | M01, A01, A04 |
| macOS | Supported | Measured | AppKit processes opened every corpus file. Open panels extend the sandbox to the chosen file, and bundle document types provide Finder association. MAC-001C declares a Markdown Viewer type at Alternate rank; Launch Services recorded the claim, and `open -a` delivered documents to the running app on macOS 27.0.1. Typed missing, permission, revoked, unavailable-disk, folder, type, size and UTF-8 errors pass native tests and the app smoke. | Open panel only, if association registration fails review | MAC-001: interactive open panel and Finder Open With; sandboxed launch if App Sandbox is adopted | M02, A20, A21 |
| Linux | Supported | Measured | GTK processes opened every corpus file. `GtkFileDialog` uses the FileChooser portal under sandboxing, and desktop entries declare MIME associations. | Command-line open | LNX-001: dialog, portal and MIME association on GNOME and KDE | M03, A57, A59, A58 |
| iOS/iPadOS | Supported | Authoritative | The document picker returns security-scoped URLs for external documents. Reads must be coordinated and presented through a file presenter. | Copy-on-import when a provider refuses in-place access | IOS-001: picker, provider and coordinated read on a named device | A40, A23 |

### CAP-02 — Open and browse a user-selected folder

| Platform | Status | Evidence | Basis and user impact | Fallback | Next check | Sources |
| --- | --- | --- | --- | --- | --- | --- |
| Windows | Supported | Authoritative | The folder picker returns a folder, and full-trust desktop apps have ordinary user file access. | Recent-path entry | WIN-002: folder picker, large-folder enumeration and denied subfolders | A02, A05 |
| macOS | Supported | Authoritative | A user-selected folder extends the sandbox recursively to nested items. Discretionary permissions can still deny individual items. | Per-item permission errors in the browser | MAC-002: sandboxed folder grant and nested denied-item behavior | A20 |
| Linux | Supported | Authoritative | `GtkFileDialog` folder selection exists, and the FileChooser portal supports directory selection for sandboxed packages. | Native-package path access | LNX-002: portal folder grant inside the selected package format | A57, A59 |
| iOS/iPadOS | Supported | Authoritative | Since iOS 13, the document picker can return a security-scoped directory URL with recursive access. That includes third-party File Provider locations. | Single-file mode | IOS-002: folder pick on local, iCloud and third-party providers | A41 |

### CAP-03 — Retain file and folder access across relaunch

| Platform | Status | Evidence | Basis and user impact | Fallback | Next check | Sources |
| --- | --- | --- | --- | --- | --- | --- |
| Windows | Supported | Authoritative | Full-trust desktop apps keep normal path access. Packaged apps virtualize some AppData writes, which affects where recents are stored, not document access. | Re-pick a moved item | WIN-002: recents across move, rename and unavailable volume | A05 |
| macOS | Supported | Authoritative | Security-scoped bookmarks restore sandboxed access after relaunch. Stale bookmarks must be recreated, and each access is bracketed by start and stop calls. | Recovery prompt to re-grant | MAC-002: stale and moved-bookmark recovery | A20 |
| Linux | Supported | Authoritative | Files added to the Documents portal through FileChooser stay accessible across sessions. Native packages keep plain paths. | Re-grant through the portal | LNX-002: portal persistence and moved-file behavior per package format | A59, A60 |
| iOS/iPadOS | Supported | Authoritative | A bookmark saved from a security-scoped picker URL restores access on the next launch. Bookmarks can go stale and need user recovery. | Recovery prompt to re-pick | IOS-002: stale bookmark and provider sign-out recovery | A40, A41 |

### CAP-04 — Load relative images beside a single opened file

| Platform | Status | Evidence | Basis and user impact | Fallback | Next check | Sources |
| --- | --- | --- | --- | --- | --- | --- |
| Windows | Supported | Authoritative | Full-trust desktop access covers sibling files, subject to the resource policy in R5. | Inert placeholder when policy denies | WIN-001: resource-policy denial and traversal checks | A05 |
| macOS | Alternative | Authoritative | A sandboxed single-file grant doesn't cover arbitrary siblings. Related-item types only cover declared ancillary types, so images beside a lone file stay inaccessible. | Prompt for the containing folder, or ship a non-sandboxed Developer ID build | MAC-001: sandboxed sibling-image denial and folder-upgrade flow | A20 |
| Linux | Alternative | Authoritative | Inside Flatpak, the document portal exposes only the files the app was granted. Native packages see siblings. | Prompt for the folder, or a static filesystem permission recorded in the package | LNX-001: sibling access per package format | A60, A65 |
| iOS/iPadOS | Alternative | Authoritative | A picked file's security scope covers only that document. | Ask the user to open the containing folder | IOS-002: relative image behavior from file and folder picks | A40, A41 |

### CAP-05 — Watch local files while running

| Platform | Status | Evidence | Basis and user impact | Fallback | Next check | Sources |
| --- | --- | --- | --- | --- | --- | --- |
| Windows | Supported | Authoritative | `ReadDirectoryChangesW` reports subtree changes. On overflow, the buffer is discarded and the app must re-enumerate. | Re-enumerate and reread on overflow or focus | WIN-001: replace, rename and overflow storms | A06 |
| macOS | Supported | Authoritative | FSEvents reports directory-hierarchy changes. File presenters receive coordinated changes from other processes. | Reread on focus | MAC-001: atomic-replace and coordinated-write refresh | A22, A23 |
| Linux | Supported | Authoritative | inotify reports local changes. Its queue can overflow, and watches are capped per user. | Poll after `IN_Q_OVERFLOW` or watch exhaustion | LNX-001: replace, rename, overflow and watch-limit behavior | A55 |
| iOS/iPadOS | Alternative | Authoritative | File presenters are notified only of coordinated changes. They must be removed before backgrounding, or coordinated access elsewhere can deadlock. | Re-check on scene activation | IOS-002: presenter lifecycle and foreground refresh | A23 |

### CAP-06 — Detect changes in network or provider folders

| Platform | Status | Evidence | Basis and user impact | Fallback | Next check | Sources |
| --- | --- | --- | --- | --- | --- | --- |
| Windows | Supported | Authoritative | SMB 3 shares support change notification. Network watches need buffers of 64 KB or less, and redirectors without support return `ERROR_INVALID_FUNCTION`. | Polling when the share refuses notification | WIN-002: SMB share watch and unsupported-redirector fallback | A06 |
| macOS | Deferred | Unverified | No official statement was found on whether FSEvents reports changes made by other machines on network volumes. | Poll and reread on focus | MAC-002: SMB and NFS change detection on a named NAS | U01, A22 |
| Linux | Alternative | Authoritative | inotify doesn't catch remote events on network filesystems, and the man page tells apps to poll. | Bounded polling with visible staleness | LNX-002: polling cost and staleness on SMB and NFS | A55 |
| iOS/iPadOS | Alternative | Authoritative | Network locations arrive through File Provider extensions, not mounted filesystems. Only coordinated changes notify presenters. | Foreground refresh and manual reload | IOS-002: third-party provider refresh behavior | A41, A23 |

### CAP-07 — Open exact Unicode filename spellings

| Platform | Status | Evidence | Basis and user impact | Fallback | Next check | Sources |
| --- | --- | --- | --- | --- | --- | --- |
| Windows | Supported | Measured | Nine exact-spelling opens passed across all three surfaces, including separate NFC and NFD names. Canonical-equivalent fallback wasn't tested. | Report not-found with the requested spelling | WIN-001: canonical-equivalent lookup policy | M01 |
| macOS | Supported | Measured | Unicode and NFD corpus files opened in all four AppKit variants. Canonical lookup on other volume formats wasn't tested. | Report not-found with the requested spelling | MAC-001: APFS and external-volume normalization lookups | M07 |
| Linux | Supported | Measured | All eight semantic documents, including the Unicode and NFD inputs, opened in GTK. Byte-exact names on ext4-class filesystems are expected. | Report not-found with the requested spelling | LNX-001: canonical-equivalent lookup policy | M03 |
| iOS/iPadOS | Deferred | Unverified | No iOS/iPadOS filename measurement exists. | Report not-found with the requested spelling | IOS-001: corpus filenames through local and provider storage | U02 |

## Lifecycle and presentation

### CAP-08 — Continue bounded work when not frontmost

| Platform | Status | Evidence | Basis and user impact | Fallback | Next check | Sources |
| --- | --- | --- | --- | --- | --- | --- |
| Windows | Supported | Authoritative | Windows App SDK desktop apps are either running or not running, and they aren't suspended like UWP apps. Off-thread parse and cancellation are measured. | Pause indexing on battery saver | WIN-002: indexing while minimized under power policy | A07, M04 |
| macOS | Deferred | Unverified | Off-thread parse and cancellation are measured. Throttling of hidden apps wasn't verified from current official documentation. | Resume indexing on activation | MAC-002: indexing while hidden and occluded | U04, M05 |
| Linux | Supported | Authoritative | Native processes keep running. Flatpak apps can request background permission through the Background portal. Off-thread work is measured. | Pause until refocused when permission is denied | LNX-002: portal denial and background indexing | A61, M06 |
| iOS/iPadOS | Alternative | Authoritative | After backgrounding, an app has about five seconds before suspension. Extensions are short, and processing tasks are system-scheduled. | Foreground-only indexing, resumable at checkpoints | IOS-002: suspension and resumable index checkpoints | A42, A43 |

### CAP-09 — Full-screen reading presentation

| Platform | Status | Evidence | Basis and user impact | Fallback | Next check | Sources |
| --- | --- | --- | --- | --- | --- | --- |
| Windows | Supported | Authoritative | `AppWindow` provides a full-screen presenter. | Maximized focus mode | DESK-003: full-screen focus mode | A08 |
| macOS | Supported | Authoritative | `toggleFullScreen` is the standard window full-screen mechanism. | Zoomed window | DESK-003: full-screen focus mode and Spaces | A24 |
| Linux | Supported | Authoritative | GTK can request full screen, but window managers may refuse or undo it. | Maximized focus mode | DESK-003: GNOME and KDE full-screen behavior | A66 |
| iOS/iPadOS | Alternative | Authoritative | Apps already fill their scene. iPad presents multiple windows through scenes rather than a desktop full-screen toggle. | Hide chrome within the scene | IOS-002: focus mode in iPad multitasking layouts | A45 |

### CAP-10 — Inhibit display sleep

| Platform | Status | Evidence | Basis and user impact | Fallback | Next check | Sources |
| --- | --- | --- | --- | --- | --- | --- |
| Windows | Supported | Authoritative | `PowerRequestDisplayRequired` keeps the display on. A system-required request is also needed to prevent sleep. | Tell the user to adjust power settings | ADV-004: teleprompter wake request and release | A09 |
| macOS | Supported | Authoritative | Process activities with `idleDisplaySleepDisabled` keep the screen on. | Tell the user to adjust Energy settings | ADV-004: wake activity and release | A25 |
| Linux | Supported | Authoritative | `gtk_application_inhibit` and the Inhibit portal's idle flag request inhibition. Desktop backends decide whether to honor it. | Show that inhibition was refused | ADV-004: GNOME and KDE idle inhibition | A67, A62 |
| iOS/iPadOS | Supported | Authoritative | `isIdleTimerDisabled` prevents screen sleep. Apple limits it to apps that display content with minimal interaction, which a teleprompter is. | Show that the device may sleep | ADV-004: idle-timer reset on scene exit | A44 |

## Reading surface

### CAP-11 — Unbounded long-document presentation

| Platform | Status | Evidence | Basis and user impact | Fallback | Next check | Sources |
| --- | --- | --- | --- | --- | --- | --- |
| Windows | Supported | Measured | A WinUI 3 `ListView` over the same bounded fragments presented 1 MiB, 5 MiB and long-line input in 240–283 ms warm and 309–356 ms cold (C# Native AOT and C++/WinRT), with off-screen jumps under 70 ms. It realizes about 15 rows. Continuous controls still time out at 5 MiB. | Bounded rich edit for small documents | WIN-001: fragment seams, tables and code within accepted budgets | M01, M04, M09 |
| macOS | Supported | Measured | TextKit 2 reached the end of every input, including 5 MiB, with sub-millisecond end requests. Memory near 388 MiB and RSS retention across repeated parses remain open risks. | Bounded blocks for pathological lines | MAC-001: memory and repeated-session RSS budget | M02, M05 |
| Linux | Supported | Measured | Virtualized `GtkListView` fragments presented 1 MiB, 5 MiB and long-line input in 289–373 ms warm and 528–623 ms cold, with off-screen jumps under 150 ms. This requires bounded fragments and a fixed label minimum width. Monolithic `GtkTextView` is rejected. | Small-document text view only | LNX-001: fragment seams, tables and code within accepted budgets | M03, M06, M08 |
| iOS/iPadOS | Deferred | Unverified | No mobile rendering measurement exists, and mobile memory limits are tighter than desktop. | Bounded documents | IOS-001: UIKit text layout and memory on a named device | U02 |

### CAP-12 — Document-wide selection and copy

| Platform | Status | Evidence | Basis and user impact | Fallback | Next check | Sources |
| --- | --- | --- | --- | --- | --- | --- |
| Windows | Supported | Measured | An app-owned model selection spans virtualized rows by pointer and keyboard, drawn with `TextHighlighter`. The owner's Ctrl+A copy of 5 MiB matched the source (registered `text/markdown`) and model text (`CF_UNICODETEXT`) byte for byte when read by a separate process. | Per-block copy, disclosed to the user | WIN-001: selection during incremental updates, IME and touch | M01, M09 |
| macOS | Supported | Measured | All 60 continuous-text runs exported matching selections, including the lead TextKit 2 surface. | Not needed | MAC-001: selection during scrolling and incremental updates | M02 |
| Linux | Supported | Measured | An app-owned model selection spans virtualized rows by pointer and keyboard. The owner's Ctrl+A copy of 5 MiB matched the source and model text byte for byte through `wl-paste`. | Per-block copy, disclosed to the user | LNX-001: selection during incremental updates and IME | M03, M08 |
| iOS/iPadOS | Deferred | Unverified | No mobile selection measurement exists. | Per-block copy | IOS-001: touch selection across blocks | U02 |

### CAP-13 — Screen-reader document reading

| Platform | Status | Evidence | Basis and user impact | Fallback | Next check | Sources |
| --- | --- | --- | --- | --- | --- | --- |
| Windows | Supported | Measured | UIA3 inspection shows rows in model order at the top, middle and end of 5 MiB with the true set size. With the heading level on each row's text element, the owner confirmed Narrator reads rows in order, moves between headings with H, and keeps reading past the screen. Each row is read twice, and whole-tree UIA searches time out at 5 MiB. | Outline navigation | WIN-001: single exposure per row and whole-tree UIA queries at scale | M01, M09 |
| macOS | Supported | Measured | The owner confirmed audible VoiceOver reading. Rotor heading navigation and table semantics are unverified. | Outline navigation | MAC-001: semantic roles and large-document speech | M02 |
| Linux | Supported | Measured | AT-SPI exposes on-screen rows in model order with heading roles and levels, and the owner confirmed Orca reads rows in order. Orca heading navigation (H) doesn't work, and Say All past the screen is unconfirmed. | Outline navigation | LNX-001: Orca structural navigation and Say All on GNOME and KDE | M03, M08 |
| iOS/iPadOS | Deferred | Unverified | No VoiceOver measurement exists on iOS/iPadOS. | Outline navigation | IOS-001: VoiceOver on a named device | U02 |

### CAP-14 — Mixed and fractional display scaling

| Platform | Status | Evidence | Basis and user impact | Fallback | Next check | Sources |
| --- | --- | --- | --- | --- | --- | --- |
| Windows | Deferred | Measured | Only a uniform 150% three-display sweep was measured. Transitions between different scales are untested. | Single-scale support statement | WIN-001: mixed-scale transitions and font scaling | M01 |
| macOS | Deferred | Measured | Only backing scale 2 on one display was measured. | Single-scale support statement | MAC-001: mixed-display transitions | M02 |
| Linux | Deferred | Measured | Only compositor scale 1 was measured, and forced GDK scale variables had no effect under Wayland. | Integer-scale support statement | LNX-001: fractional scaling on GNOME and KDE | M03 |
| iOS/iPadOS | Deferred | Unverified | No device scaling or Dynamic Type measurement exists. | System text size only | IOS-001: Dynamic Type and external displays | U02 |

## Math and diagrams

### CAP-15 — KaTeX-compatible math presentation

| Platform | Status | Evidence | Basis and user impact | Fallback | Next check | Sources |
| --- | --- | --- | --- | --- | --- | --- |
| Windows | Deferred | Authoritative | KaTeX outputs only HTML or MathML, and no native presenter for either is established. The spikes left math as source. | Show TeX source with an unsupported notice | GATE-001: decide math engine and native output form | A75, U08, M01 |
| macOS | Deferred | Authoritative | KaTeX could run in JavaScriptCore, but its HTML or MathML still needs a presenter. Native text does not render MathML. | Show TeX source with an unsupported notice | GATE-001: decide math engine and native output form | A75, A33, U08 |
| Linux | Deferred | Authoritative | Same as Windows. A JavaScript engine would also be a new implementation language under the AGENTS.md rule. | Show TeX source with an unsupported notice | GATE-001: decide math engine and native output form | A75, U08, M03 |
| iOS/iPadOS | Deferred | Authoritative | JavaScriptCore is available, and bundled code is allowed, while downloaded code that changes features is not. Presentation is unresolved. | Show TeX source with an unsupported notice | GATE-001: decide math engine and native output form | A75, A33, A50, U08 |

### CAP-16 — Mermaid diagram compilation

| Platform | Status | Evidence | Basis and user impact | Fallback | Next check | Sources |
| --- | --- | --- | --- | --- | --- | --- |
| Windows | Deferred | Authoritative | Mermaid targets browsers, and its official CLI renders through Puppeteer and Chromium. Native compilation without a browser engine is unavailable. | Show diagram source with an unsupported notice | GATE-001: owner decision on an offscreen web engine as a non-UI compiler | A77, A78 |
| macOS | Deferred | Authoritative | An offscreen `WKWebView` could compile and snapshot diagrams, but that is still a browser engine. | Show diagram source with an unsupported notice | GATE-001: owner decision on an offscreen web engine as a non-UI compiler | A77, A34 |
| Linux | Deferred | Authoritative | It needs a bundled browser engine or a headless-browser helper. | Show diagram source with an unsupported notice | GATE-001: owner decision on an offscreen web engine as a non-UI compiler | A77, A78 |
| iOS/iPadOS | Deferred | Authoritative | Only WebKit may browse the web, and no helper process exists, so an offscreen `WKWebView` is the only candidate. | Show diagram source with an unsupported notice | GATE-001: owner decision on an offscreen web engine as a non-UI compiler | A77, A34, A50 |

### CAP-17 — D2 diagram compilation

| Platform | Status | Evidence | Basis and user impact | Fallback | Next check | Sources |
| --- | --- | --- | --- | --- | --- | --- |
| Windows | Supported | Authoritative | D2 v0.9.0 is a pure Go module whose layout engines come from Go modules (`dagro`, `elk-go`), with no JavaScript runtime dependency. Go can build C shared libraries, and helper processes also work. Adopting it needs a second-language decision. | Show diagram source with an unsupported notice | GATE-001: accept or reject Go for an in-process or helper D2 engine | A81, A79, A82, A17 |
| macOS | Supported | Authoritative | Same module, as an in-process C archive or a sandbox-inheriting helper. | Show diagram source with an unsupported notice | GATE-001: accept or reject Go for an in-process or helper D2 engine | A81, A82, A29 |
| Linux | Supported | Authoritative | Same module, as an in-process library or a helper. | Show diagram source with an unsupported notice | GATE-001: accept or reject Go for an in-process or helper D2 engine | A81, A82, A70 |
| iOS/iPadOS | Alternative | Authoritative | Go targets iOS and can build a static C archive, but only in-process use is possible. The web project's server rendering isn't available offline. | Show diagram source with an unsupported notice | IOS-001: static Go archive size, memory and App Review fit | A81, A82, A83, A50 |

### CAP-18 — Bundled helper subprocess

| Platform | Status | Evidence | Basis and user impact | Fallback | Next check | Sources |
| --- | --- | --- | --- | --- | --- | --- |
| Windows | Supported | Authoritative | Full-trust desktop apps can create processes. | In-process library | GATE-001: helper security and packaging review | A17, A05 |
| macOS | Supported | Authoritative | A sandboxed app can run an embedded helper that is signed with only the sandbox and inherit entitlements. | In-process library | MAC-002: inherited-sandbox helper launch | A29 |
| Linux | Supported | Authoritative | Native processes can spawn bundled helpers. Child behavior inside Flatpak wasn't verified. | In-process library | LNX-002: helper launch inside the selected package format | A70, U09 |
| iOS/iPadOS | Deferred | Unverified | Apple's product documentation reviewed here doesn't state the child-process limit. Apple forum answers say apps can't launch child processes, so treat it as unavailable until proven. | In-process library only | IOS-001: confirm child-process restriction | U05 |

### CAP-19 — Generated SVG and math output sanitization

| Platform | Status | Evidence | Basis and user impact | Fallback | Next check | Sources |
| --- | --- | --- | --- | --- | --- | --- |
| Windows | Supported | Authoritative | This is a portable core duty: disable KaTeX trust, bound macro expansion, keep Mermaid at strict security, confine D2 imports and render only static SVG with no network or local hrefs. | Drop the output and show source | CORE-003: hostile diagram and math corpus | A76, A77, A80, A85, A86 |
| macOS | Supported | Authoritative | The same portable policy applies. | Drop the output and show source | CORE-003: hostile diagram and math corpus | A76, A77, A80, A85, A86 |
| Linux | Supported | Authoritative | The same portable policy applies. | Drop the output and show source | CORE-003: hostile diagram and math corpus | A76, A77, A80, A85, A86 |
| iOS/iPadOS | Supported | Authoritative | The same portable policy applies. | Drop the output and show source | CORE-003: hostile diagram and math corpus | A76, A77, A80, A85, A86 |

### CAP-20 — Static SVG display

| Platform | Status | Evidence | Basis and user impact | Fallback | Next check | Sources |
| --- | --- | --- | --- | --- | --- | --- |
| Windows | Supported | Authoritative | `SvgImageSource` displays SVG from a URI or a stream. Its handling of external references is undocumented, so feed it only sanitized SVG. | Rasterize in the core with resvg | WIN-001: sanitized SVG through a stream source | A10, A85 |
| macOS | Alternative | Authoritative | `NSImage` lists formats without SVG, and loose SVG support is undocumented. resvg is pure Rust and renders the static subset. | Core-side resvg raster at display scale | ADV-002: resvg raster quality and memory | A30, A85, U03 |
| Linux | Alternative | Authoritative | GTK SVG loading through its image stack wasn't verified. resvg needs no system libraries. | Core-side resvg raster at display scale | ADV-002: resvg raster quality and memory | A85, U03 |
| iOS/iPadOS | Alternative | Authoritative | Same as macOS. resvg notes rough edges with system font loading on mobile. | Core-side resvg raster at display scale | IOS-001: resvg font loading on device | A30, A85, U03 |

### CAP-21 — Diagram pan and zoom

| Platform | Status | Evidence | Basis and user impact | Fallback | Next check | Sources |
| --- | --- | --- | --- | --- | --- | --- |
| Windows | Supported | Authoritative | `ScrollViewer` exposes a zoom mode. | Fixed-scale scroll | ADV-002: wheel, touch and keyboard zoom | A11 |
| macOS | Supported | Authoritative | `NSScrollView` supports user magnification. | Fixed-scale scroll | ADV-002: trackpad and keyboard zoom | A26 |
| Linux | Supported | Authoritative | `GtkGestureZoom` reports pinch scale for a scrolled view. | Fixed-scale scroll | ADV-002: touchpad and keyboard zoom | A68 |
| iOS/iPadOS | Supported | Authoritative | `UIScrollView` provides pinch zoom and panning. | Fixed-scale scroll | ADV-002: pinch zoom and VoiceOver | A46 |

### CAP-22 — Diagram and image export to file

| Platform | Status | Evidence | Basis and user impact | Fallback | Next check | Sources |
| --- | --- | --- | --- | --- | --- | --- |
| Windows | Supported | Authoritative | The file save picker chooses the destination, and the core writes sanitized SVG or a raster. | Copy image to clipboard | ADV-002: SVG and PNG export | A03, A85 |
| macOS | Supported | Authoritative | A save panel grants write access to the chosen destination. | Copy image to clipboard | ADV-002: SVG and PNG export | A20, A85 |
| Linux | Supported | Authoritative | `GtkFileDialog` saves through the portal when sandboxed. | Copy image to clipboard | ADV-002: SVG and PNG export | A57, A85 |
| iOS/iPadOS | Supported | Authoritative | The document picker's export mode places a file at a user-chosen destination. | Share sheet | ADV-002: SVG and PNG export | A40, A85 |

## Output and sharing

### CAP-23 — Print document

| Platform | Status | Evidence | Basis and user impact | Fallback | Next check | Sources |
| --- | --- | --- | --- | --- | --- | --- |
| Windows | Supported | Authoritative | `PrintManager` is supported on Windows 11 but not yet on Windows 10 in the Windows App SDK. Desktop windows use the print-manager interop interface. | Export a file and print it elsewhere | GATE-001: Windows baseline; then DESK-003 print | A12, A13 |
| macOS | Supported | Authoritative | `NSPrintOperation` prints a view or generates PDF. | Not needed | DESK-003: paginated native print | A27 |
| Linux | Supported | Authoritative | `GtkPrintOperation` hides the Print portal's round trips for sandboxed apps. | Export PDF | DESK-003: print through the portal | A63, A69 |
| iOS/iPadOS | Supported | Authoritative | `UIPrintInteractionController` prints formatters or page renderers. | Share sheet | IOS-002: print with a page renderer | A47 |

### CAP-24 — Standalone PDF export

| Platform | Status | Evidence | Basis and user impact | Fallback | Next check | Sources |
| --- | --- | --- | --- | --- | --- | --- |
| Windows | Deferred | Unverified | No native WinUI PDF generation API was identified, and print-to-PDF through the print UI wasn't verified. | Print through the system dialog | DESK-003: choose a native or core PDF writer | U07 |
| macOS | Supported | Authoritative | `NSPrintOperation` generates PDF from a view. | Print dialog PDF option | DESK-003: PDF export of long documents | A27 |
| Linux | Supported | Authoritative | `GtkPrintOperation` export-filename generates PDF without a dialog. | Print dialog PDF printer | DESK-003: PDF export of long documents | A69 |
| iOS/iPadOS | Supported | Authoritative | `UIGraphicsPDFRenderer` creates PDF files. | Print to PDF through the share sheet | IOS-002: PDF export of long documents | A48 |

### CAP-25 — System share

| Platform | Status | Evidence | Basis and user impact | Fallback | Next check | Sources |
| --- | --- | --- | --- | --- | --- | --- |
| Windows | Supported | Authoritative | Desktop apps, packaged or unpackaged, share through `IDataTransferManagerInterop` per window. | Save a copy | DESK-003: share a file and a selection | A14 |
| macOS | Supported | Authoritative | `NSSharingServicePicker` presents the system sharing services. | Save a copy | DESK-003: share a file and a selection | A28 |
| Linux | Unavailable | Authoritative | The portal API lists Email, OpenURI and File Transfer, but no general share portal. | Save a copy, copy to clipboard and open with another app | DESK-003: export and open-with flows | A64 |
| iOS/iPadOS | Supported | Authoritative | `UIActivityViewController` offers system services. It must be a popover on iPad. | Save a copy through export | IOS-002: share sheet on iPhone and iPad | A49 |

## Voice-over and teleprompter

### CAP-26 — Voice-over script presentation

| Platform | Status | Evidence | Basis and user impact | Fallback | Next check | Sources |
| --- | --- | --- | --- | --- | --- | --- |
| Windows | Supported | Measured | Script semantics belong to the portable core, and presentation is native text blocks. WinUI rendered the corpus blocks, but the spike adapter flattened inline formatting, so styled runs weren't measured. | Plain styled paragraphs | ADV-004: speaker, direction and line-number layout | M01 |
| macOS | Supported | Measured | Same split; styled TextKit content rendered. | Plain styled paragraphs | ADV-004: speaker, direction and line-number layout | M02 |
| Linux | Supported | Measured | Same split; tagged `GtkTextBuffer` content rendered. | Plain styled paragraphs | ADV-004: speaker, direction and line-number layout | M03 |
| iOS/iPadOS | Deferred | Unverified | No mobile text presentation measurement exists. | Plain styled paragraphs | IOS-001: styled block presentation | U02 |

### CAP-27 — Teleprompter timed auto-scroll

| Platform | Status | Evidence | Basis and user impact | Fallback | Next check | Sources |
| --- | --- | --- | --- | --- | --- | --- |
| Windows | Supported | Measured | Programmatic far navigation and composition render callbacks were measured. Smooth timed scrolling and frame pacing weren't. | Stepped scroll | ADV-004: constant-speed scroll frame pacing | M01 |
| macOS | Supported | Measured | 60 programmed scroll steps had p95 timer gaps of about 17 ms on prose. Those are timer delays, not display pacing. | Stepped scroll | ADV-004: constant-speed scroll frame pacing | M02 |
| Linux | Supported | Measured | Frame-clock callbacks and end scrolling were measured. Far-scroll convergence failed in the monolithic view. | Stepped scroll | ADV-004: constant-speed scroll over virtualized blocks | M03 |
| iOS/iPadOS | Deferred | Unverified | No mobile scroll measurement exists. | Stepped scroll | IOS-001: display-link scroll pacing | U02 |

## Packaging and distribution

### CAP-28 — Unsigned local development package

| Platform | Status | Evidence | Basis and user impact | Fallback | Next check | Sources |
| --- | --- | --- | --- | --- | --- | --- |
| Windows | Supported | Measured | Unsigned framework-dependent folders launched locally with the normal core DLL. Clean-machine deployment wasn't tested. | Run from build output | WIN-001: relocated launch on a clean machine | M01, M04 |
| macOS | Supported | Measured | Ad-hoc signed bundles with the normal dylib relocated and launched. | Run from build output | MAC-001: relocated bundle with the accepted floor | M02, M05 |
| Linux | Supported | Measured | A relocated normal-library package passed lifecycle, console and GTK smokes with loader overrides removed. | Run from build output | LNX-001: package on a second distribution | M06 |
| iOS/iPadOS | Deferred | Unverified | No iOS build exists. Device installs need development signing. | Simulator build, labeled as simulator evidence | IOS-001: device install of the packaged core | U02 |

### CAP-29 — Signed direct distribution

| Platform | Status | Evidence | Basis and user impact | Fallback | Next check | Sources |
| --- | --- | --- | --- | --- | --- | --- |
| Windows | Supported | Authoritative | MSIX packages must be signed by a trusted certificate. Downloaded apps also face SmartScreen reputation checks. | Self-signed development install | DESK-002: owner-authorized signing task | A15, A16 |
| macOS | Supported | Authoritative | Developer ID signing plus notarization is the path for distribution outside the App Store. | Local ad-hoc builds only | DESK-002: owner-authorized notarization task | A31 |
| Linux | Deferred | Unverified | Package formats aren't selected, and signing depends on the format and repository. | Unsigned archive for development | GATE-001: select Linux package formats | U06 |
| iOS/iPadOS | Alternative | Authoritative | Distribution normally goes through App Review, with no general direct-download path. Regional alternative distribution wasn't evaluated. | TestFlight or development provisioning | IOS-002: App Review readiness | A50 |

### CAP-30 — Store or sandboxed package

| Platform | Status | Evidence | Basis and user impact | Fallback | Next check | Sources |
| --- | --- | --- | --- | --- | --- | --- |
| Windows | Supported | Authoritative | MSIX full-trust desktop apps keep normal file access, with some AppData virtualization. | Unpackaged distribution | GATE-001: packaging model choice | A05, A15 |
| macOS | Supported | Authoritative | The Mac App Store requires App Sandbox, which brings the CAP-04 and CAP-18 constraints with it. | Developer ID distribution | GATE-001: sandboxed versus Developer ID scope | A32, A20 |
| Linux | Supported | Authoritative | Flatpak blocks host files and the network by default and routes access through portals. | Distribution-native packages | GATE-001: select Linux package formats | A65 |
| iOS/iPadOS | Supported | Authoritative | Apps are self-contained in their bundles and container under App Review, and the sandbox applies to every distribution path. | Not applicable | IOS-002: App Review readiness | A50 |

## iOS/iPadOS synthesis

iOS and iPadOS are not desktop ports. Every iOS/iPadOS cell above is
authoritative or unverified; IOS-001 must replace them with named-device
measurements before the mobile scope is accepted.

- **File access is grant-based.** Files and folders come from the document
  picker as security-scoped URLs, possibly from File Provider extensions.
  Relative images need a folder grant, and bookmarks can go stale (CAP-01–04).
- **Refresh is foreground-only.** No desktop-style watcher or background
  indexer exists; coordinated presenter notifications and foreground re-checks
  replace them (CAP-05, CAP-06, CAP-08).
- **Engines must be in-process.** Child processes are expected to be
  unavailable, and downloaded code is forbidden, so D2 requires an in-process
  Go archive and Mermaid an offscreen WebKit view (CAP-16–18).
- **Presentation risk is unmeasured.** Long-document layout, selection,
  VoiceOver, Dynamic Type and memory have no mobile evidence (CAP-11–14).
- **Output paths exist.** Print, PDF, share and export are platform-provided
  (CAP-22–25).

## Retained measured failures and deviations

These measured results remain open and are not reclassified by this matrix:

- Windows continuous controls time out at 5 MiB, and virtualized rows lack
  document-wide selection and a proven UIA reading order (FEAS-003, FEAS-006).
- Linux monolithic `GtkTextView` doesn't converge on far-scroll, and a 1 MiB
  logical line takes about 11.4 seconds (FEAS-005, FEAS-006).
- macOS twenty native-only 5 MiB parses grew RSS to about 583 MiB with zero
  native owners; there is no leak proof or accepted budget (FEAS-006).
- Mixed or fractional display scaling is unmeasured on every platform (CAP-14).
- No signing, notarization, store, clean-machine or distribution-floor test was
  run on any platform (CAP-28–30).

## Implications for GATE-001

GATE-001 must decide, or explicitly defer, the following before the advanced
scope or mobile scope is accepted:

1. Whether “native” permits a bundled non-UI engine. Mermaid needs a browser
   engine, and KaTeX produces HTML or MathML that no native text stack
   presents (CAP-15, CAP-16).
2. Whether Go is accepted as a second implementation language for D2, in-process
   or as a desktop helper. AGENTS.md requires an accepted decision record
   (CAP-17, CAP-18).
3. The Windows baseline: `PrintManager` printing isn't yet available on
   Windows 10 in the Windows App SDK (CAP-23).
4. Mac distribution scope: sandboxed App Store builds can't read images beside a
   lone file, or run unentitled helpers (CAP-04, CAP-18, CAP-30).
5. Linux package formats, which determine sibling access, helper behavior and
   signing (CAP-04, CAP-18, CAP-29).
6. Tier 1 continues to exclude Mermaid and D2. Math remains deferred; all three
   show source with an unsupported notice until ADV-002.

## Sources

Sources were read on 2026-10-25. Measured paths are relative to this file.

| ID | Kind | Reference | Scope |
| --- | --- | --- | --- |
| M01 | Measured | [Windows rendering evaluation](Windows-Rendering-Evaluation.md) | FEAS-003 WinUI surfaces, Narrator, Unicode, scaling and package |
| M02 | Measured | [macOS rendering evaluation](macOS-Rendering-Evaluation.md) | FEAS-004 TextKit surfaces, VoiceOver, scrolling and bundles |
| M03 | Measured | [Linux rendering evaluation](Linux-Rendering-Evaluation.md) | FEAS-005 GTK surface, AT-SPI, scaling and package |
| M04 | Measured | [Windows core integration](Shared-Core-Windows-Evaluation.md) | FEAS-006 Windows facade, virtualized timing and package |
| M05 | Measured | [macOS core integration](Shared-Core-macOS-Evaluation.md) | FEAS-006 macOS facade, memory retention and bundles |
| M06 | Measured | [Linux core integration](Shared-Core-Linux-Evaluation.md) | FEAS-006 Linux facade, long line and relocated package |
| M07 | Measured | [FEAS-004 results](../spikes/feas004/results-2026-10-24.yaml) | macOS Unicode and NFD corpus runs |
| M08 | Measured | [Linux virtualized renderer evaluation](Virtualized-Renderer-Linux-Evaluation.md) | FEAS-008 GTK virtualized blocks, selection, Orca and WebKitGTK baseline |
| M09 | Measured | [Windows virtualized renderer evaluation](Virtualized-Renderer-Windows-Evaluation.md) | FEAS-008 WinUI virtualized blocks in C# JIT, Native AOT and C++/WinRT, selection, Narrator, UIA and WebView2 baseline |
| A01 | Authoritative | [FileOpenPicker](https://learn.microsoft.com/en-us/windows/windows-app-sdk/api/winrt/microsoft.windows.storage.pickers.fileopenpicker) | Windows App SDK file picker |
| A02 | Authoritative | [FolderPicker](https://learn.microsoft.com/en-us/windows/windows-app-sdk/api/winrt/microsoft.windows.storage.pickers.folderpicker) | Windows App SDK folder picker |
| A03 | Authoritative | [FileSavePicker](https://learn.microsoft.com/en-us/windows/windows-app-sdk/api/winrt/microsoft.windows.storage.pickers.filesavepicker) | Windows App SDK save picker |
| A04 | Authoritative | [File types and associations](https://learn.microsoft.com/en-us/windows/win32/shell/fa-intro) | Windows shell associations |
| A05 | Authoritative | [MSIX desktop app behavior](https://learn.microsoft.com/en-us/windows/msix/desktop/desktop-to-uwp-behind-the-scenes) | Full-trust packaged file system and AppData virtualization |
| A06 | Authoritative | [ReadDirectoryChangesW](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-readdirectorychangesw) | Overflow, SMB support and network buffer limit |
| A07 | Authoritative | [Windows App SDK app lifecycle](https://learn.microsoft.com/en-us/windows/apps/windows-app-sdk/applifecycle/applifecycle) | Desktop apps aren't suspended |
| A08 | Authoritative | [AppWindowPresenterKind](https://learn.microsoft.com/en-us/windows/windows-app-sdk/api/winrt/microsoft.ui.windowing.appwindowpresenterkind) | Full-screen presenter |
| A09 | Authoritative | [PowerSetRequest](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-powersetrequest) | Display-required power requests |
| A10 | Authoritative | [SvgImageSource](https://learn.microsoft.com/en-us/windows/windows-app-sdk/api/winrt/microsoft.ui.xaml.media.imaging.svgimagesource) | WinUI SVG image source |
| A11 | Authoritative | [ScrollViewer.ZoomMode](https://learn.microsoft.com/en-us/windows/windows-app-sdk/api/winrt/microsoft.ui.xaml.controls.scrollviewer.zoommode) | WinUI zoom |
| A12 | Authoritative | [Windows App SDK support table](https://learn.microsoft.com/en-us/windows/apps/windows-app-sdk/migrate-to-windows-app-sdk/what-is-supported) | PrintManager is Windows 11 only |
| A13 | Authoritative | [IPrintManagerInterop](https://learn.microsoft.com/en-us/windows/win32/api/printmanagerinterop/nn-printmanagerinterop-iprintmanagerinterop) | Per-window print UI for desktop apps |
| A14 | Authoritative | [Windows share integration](https://learn.microsoft.com/en-us/windows/apps/develop/windows-integration/integrate-sharesheet-send) | Desktop share through IDataTransferManagerInterop |
| A15 | Authoritative | [MSIX signing overview](https://learn.microsoft.com/en-us/windows/msix/package/signing-package-overview) | MSIX signing and trust requirement |
| A16 | Authoritative | [Microsoft Defender SmartScreen](https://learn.microsoft.com/en-us/windows/security/operating-system-security/virus-and-threat-protection/microsoft-defender-smartscreen/) | Download reputation checks |
| A17 | Authoritative | [CreateProcessW](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-createprocessw) | Windows process creation |
| A20 | Authoritative | [Accessing files from the macOS App Sandbox](https://developer.apple.com/documentation/security/accessing-files-from-the-macos-app-sandbox) | User-selected access, folders, bookmarks and related items |
| A21 | Authoritative | [CFBundleDocumentTypes](https://developer.apple.com/documentation/bundleresources/information-property-list/cfbundledocumenttypes) | macOS document type associations |
| A22 | Authoritative | [File System Events](https://developer.apple.com/documentation/coreservices/file_system_events) | macOS directory change notification |
| A23 | Authoritative | [NSFilePresenter](https://developer.apple.com/documentation/foundation/nsfilepresenter) | Coordinated change notification and background deadlock |
| A24 | Authoritative | [NSWindow.toggleFullScreen](https://developer.apple.com/documentation/appkit/nswindow/togglefullscreen%28_%3A%29) | macOS full screen |
| A25 | Authoritative | [idleDisplaySleepDisabled](https://developer.apple.com/documentation/foundation/processinfo/activityoptions/idledisplaysleepdisabled) | Keep the screen powered on |
| A26 | Authoritative | [NSScrollView.allowsMagnification](https://developer.apple.com/documentation/appkit/nsscrollview/allowsmagnification) | macOS magnification |
| A27 | Authoritative | [NSPrintOperation](https://developer.apple.com/documentation/appkit/nsprintoperation) | Print and PDF generation |
| A28 | Authoritative | [NSSharingServicePicker](https://developer.apple.com/documentation/appkit/nssharingservicepicker) | macOS share picker |
| A29 | Authoritative | [Embedding a helper tool in a sandboxed app](https://developer.apple.com/documentation/xcode/embedding-a-helper-tool-in-a-sandboxed-app) | Inherited-sandbox helpers |
| A30 | Authoritative | [NSImage](https://developer.apple.com/documentation/appkit/nsimage) | Documented image formats omit SVG |
| A31 | Authoritative | [Notarizing macOS software](https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution) | Developer ID notarization |
| A32 | Authoritative | [App Sandbox](https://developer.apple.com/documentation/security/app-sandbox) | Mac App Store sandbox requirement |
| A33 | Authoritative | [JavaScriptCore](https://developer.apple.com/documentation/javascriptcore) | In-app JavaScript evaluation on macOS and iOS |
| A34 | Authoritative | [WKWebView](https://developer.apple.com/documentation/webkit/wkwebview) | WebKit view, snapshot and PDF |
| A40 | Authoritative | [UIDocumentPickerViewController](https://developer.apple.com/documentation/uikit/uidocumentpickerviewcontroller) | iOS open and export, security scope and coordination |
| A41 | Authoritative | [Providing access to directories](https://developer.apple.com/documentation/uikit/providing-access-to-directories) | iOS folder access and providers |
| A42 | Authoritative | [Extending background execution time](https://developer.apple.com/documentation/uikit/extending-your-app-s-background-execution-time) | Five-second background window and suspension |
| A43 | Authoritative | [BGProcessingTaskRequest](https://developer.apple.com/documentation/backgroundtasks/bgprocessingtaskrequest) | System-scheduled processing |
| A44 | Authoritative | [isIdleTimerDisabled](https://developer.apple.com/documentation/uikit/uiapplication/isidletimerdisabled) | iOS idle timer |
| A45 | Authoritative | [Supporting multiple windows on iPad](https://developer.apple.com/documentation/uikit/supporting-multiple-windows-on-ipad) | iPad scenes |
| A46 | Authoritative | [UIScrollView](https://developer.apple.com/documentation/uikit/uiscrollview) | iOS zoom and pan |
| A47 | Authoritative | [UIPrintInteractionController](https://developer.apple.com/documentation/uikit/uiprintinteractioncontroller) | iOS printing |
| A48 | Authoritative | [UIGraphicsPDFRenderer](https://developer.apple.com/documentation/uikit/uigraphicspdfrenderer) | iOS PDF creation |
| A49 | Authoritative | [UIActivityViewController](https://developer.apple.com/documentation/uikit/uiactivityviewcontroller) | iOS share sheet |
| A50 | Authoritative | [App Review Guidelines](https://developer.apple.com/app-store/review/guidelines/) | 2.5.2 self-contained code and 2.5.6 WebKit |
| A55 | Authoritative | [inotify(7)](https://man7.org/linux/man-pages/man7/inotify.7.html) | No remote events, overflow and watch limits |
| A57 | Authoritative | [GtkFileDialog](https://docs.gtk.org/gtk4/class.FileDialog.html) | GTK open, folder and save dialogs |
| A58 | Authoritative | [Desktop Entry recognized keys](https://specifications.freedesktop.org/desktop-entry-spec/latest/recognized-keys.html) | MIME type associations |
| A59 | Authoritative | [FileChooser portal](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.FileChooser.html) | Sandboxed file and folder selection |
| A60 | Authoritative | [Documents portal](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.Documents.html) | Per-file sandbox exposure |
| A61 | Authoritative | [Background portal](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.Background.html) | Background activity permission |
| A62 | Authoritative | [Inhibit portal](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.Inhibit.html) | Idle inhibition |
| A63 | Authoritative | [Print portal](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.Print.html) | Sandboxed printing |
| A64 | Authoritative | [Portal API reference](https://flatpak.github.io/xdg-desktop-portal/docs/api-reference.html) | Portal list, with no share portal |
| A65 | Authoritative | [Flatpak sandbox permissions](https://docs.flatpak.org/en/latest/sandbox-permissions.html) | Default host-file and network isolation |
| A66 | Authoritative | [gtk_window_fullscreen](https://docs.gtk.org/gtk4/method.Window.fullscreen.html) | Full-screen requests window managers may refuse |
| A67 | Authoritative | [gtk_application_inhibit](https://docs.gtk.org/gtk4/method.Application.inhibit.html) | GTK session inhibition |
| A68 | Authoritative | [GtkGestureZoom](https://docs.gtk.org/gtk4/class.GestureZoom.html) | Pinch zoom |
| A69 | Authoritative | [GtkPrintOperation export-filename](https://docs.gtk.org/gtk4/property.PrintOperation.export-filename.html) | PDF export |
| A70 | Authoritative | [posix_spawn(3)](https://man7.org/linux/man-pages/man3/posix_spawn.3.html) | Linux child processes |
| A75 | Authoritative | [KaTeX options](https://katex.org/docs/options) | HTML or MathML output, trust and maxExpand |
| A76 | Authoritative | [KaTeX security](https://katex.org/docs/security) | Untrusted input and output sanitization |
| A77 | Authoritative | [Mermaid usage](https://mermaid.js.org/config/usage.html) | Browser targets and securityLevel |
| A78 | Authoritative | [mermaid-cli package manifest](https://github.com/mermaid-js/mermaid-cli/blob/master/package.json) | CLI rendering depends on Puppeteer |
| A79 | Authoritative | [D2 layout engines](https://d2lang.com/tour/layouts/) | dagre, ELK and proprietary TALA |
| A80 | Authoritative | [D2 imports](https://d2lang.com/tour/imports/) | File imports to confine |
| A81 | Authoritative | [D2 v0.9.0 module manifest](https://proxy.golang.org/github.com/d2lang/d2/@v/v0.9.0.mod) | Pure Go dependencies, no JavaScript runtime |
| A82 | Authoritative | [go build modes](https://pkg.go.dev/cmd/go) | c-archive and c-shared |
| A83 | Authoritative | [Installing Go from source](https://go.dev/doc/install/source) | macOS/iOS (Darwin) targets |
| A84 | Authoritative | [Rust platform support](https://doc.rust-lang.org/rustc/platform-support.html) | aarch64-apple-ios is Tier 2 |
| A85 | Authoritative | [resvg](https://github.com/linebender/resvg) | Pure Rust static SVG, no scripts or animation |
| A86 | Authoritative | [usvg ImageHrefResolver](https://docs.rs/usvg/latest/usvg/struct.ImageHrefResolver.html) | Local-file hrefs allowed by default; no network |
| U01 | Unverified | No official statement on FSEvents for remote changes on network volumes | macOS network-folder watching |
| U02 | Unverified | No iOS/iPadOS device or simulator measurement exists | All mobile presentation and packaging claims |
| U03 | Unverified | Loose SVG decoding by NSImage, UIImage or GTK image loading isn't documented in the sources read | Native SVG display |
| U04 | Unverified | Throttling of hidden macOS apps wasn't confirmed from current official documentation | macOS background work |
| U05 | Unverified | The iOS child-process limit appears only in Apple Developer Forums answers | iOS helper subprocess |
| U06 | Unverified | No Linux distribution package or signing prototype exists | Linux signed distribution |
| U07 | Unverified | No WinUI PDF writer or print-to-PDF path was verified | Windows PDF export |
| U08 | Unverified | No native presenter for KaTeX HTML or MathML output is established on any target | Math presentation |
| U09 | Unverified | Child-process behavior inside Flatpak wasn't verified | Linux sandboxed helper |

A84 supports the GATE-001 Apple packaging question: the Rust iOS device target
is guaranteed to build but not guaranteed to pass tests. An XCFramework proof
remains IOS-001 work.

## Validation

```sh
python3 tools/validate_capabilities.py docs/Platform-Capability-Evaluation.md
python3 tools/test_validate_capabilities.py
```
