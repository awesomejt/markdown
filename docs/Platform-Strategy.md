# Platform and Language Strategy

## Recommendation

Use platform-native presentation and OS integration over a small candidate
portable core. Treat the core language and each UI stack as feasibility
decisions driven by measured performance, semantic fit, packaging, and
maintenance—not by a desire to maximize code reuse.

The leading baseline is:

| Target | Native presentation candidate | Platform integration | Core boundary |
| --- | --- | --- | --- |
| Windows | WinUI 3 with C++/WinRT or C# host | Windows App SDK, shell, file APIs, MSIX | C ABI or generated binding |
| macOS | AppKit document/text surface with SwiftUI where useful | Foundation/AppKit, sandbox/bookmarks, menus, notarization | C ABI or generated Swift binding |
| Linux | GTK 4/libadwaita with Rust or C++ host | GLib/GIO, portals, desktop packaging | Direct Rust or C ABI |
| iOS/iPadOS | UIKit text/document surface with SwiftUI where useful | Document picker, security scope, scenes, share/print | C ABI or generated Swift binding |

Microsoft identifies WinUI 3 as its current native framework for new Windows
desktop applications, supporting C# and C++. Apple supports mixing AppKit or
UIKit with SwiftUI, which is important when lower-level text/layout control is
needed. GTK 4 is the leading Linux-native toolkit candidate. These are starting
points for spikes, not preapproved dependencies.

Primary references:

Windows FEAS-003 has now exercised C# WinUI surfaces and a C++/WinRT literal
host on real Windows. Both hosts are feasible; the native smoke is not a
language-performance comparison. Preserve the large-document, global selection,
mixed-DPI and accessibility limits in the
[Windows evaluation](Windows-Rendering-Evaluation.md) until FEAS-006/GATE-001.

- [WinUI 3](https://learn.microsoft.com/en-us/windows/apps/winui)
- [AppKit](https://developer.apple.com/documentation/AppKit)
- [SwiftUI/UIKit integration](https://developer.apple.com/documentation/swiftui/uikit-integration)
- [GTK 4](https://docs.gtk.org/gtk4/getting_started.html)

## Portable core candidates

### Rust

Strong candidate for parsing, semantic analysis, search, settings resolution,
and bounded concurrent work. It can be called directly by a Rust Linux host and
through a stable C ABI or generated bindings elsewhere. Cargo workspaces offer
coordinated packages and one lock file inside the monorepo.

Risks: binding generation, debugger experience, Apple XCFramework packaging,
Windows toolchain integration, and ABI design. These must be proven on all
desktop targets, not only on Apple.

References: [Cargo workspaces](https://doc.rust-lang.org/cargo/reference/workspaces.html),
[Comrak](https://github.com/kivikakk/comrak), and
[UniFFI](https://mozilla.github.io/uniffi-rs/latest/).

### C++/C

C++ is viable for a portable high-performance core, and C provides the most
universal ABI. GitHub's [`cmark-gfm`](https://github.com/github/cmark-gfm) is a
portable C99 parser and compatibility candidate.

Advantages include direct C++/WinRT integration, mature native debuggers, and a
straight path to Objective-C/Swift and GTK hosts. Costs include manual memory
and error boundaries, a larger unsafe surface, and more care around concurrency
and package management.

### Platform-specific cores

If shared-core abstraction or FFI measurably harms performance or feature fit,
platform-specific implementations are allowed. They must consume the same
semantic fixtures and expose equivalent behavior at the application boundary.
Duplication can be cheaper than a poor universal abstraction.

## What should be shared

- Requirements and capability matrix.
- Markdown/extension semantic fixtures and expected results.
- Stable semantic model and safety policy.
- Performance corpus and metric definitions.
- Parser/search/settings logic when measurements justify it.
- Portable low-level utilities with clear ownership and profiling evidence.

## What should remain platform-specific

- UI component tree, document layout, text selection, accessibility mapping.
- Windows/scenes, menus, commands, shortcuts, drag/drop, and clipboard.
- File pickers, permissions, persistent file identity, and change watchers.
- Native search presentation, print/share/export, settings storage.
- Packaging, signing, updates, crash integration, and release tooling.
- Platform performance optimizations and feature fallbacks.

## Performance strategy

Language choice alone does not guarantee speed. The dominant risks are likely
to be document modeling, text layout, number of native views, syntax
highlighting, image decode, search I/O, and unnecessary cross-FFI copying.

Every platform spike must measure:

- Cold/warm launch and first useful content.
- Parse/model conversion time and allocation/peak memory.
- Initial and incremental layout.
- Long-document scroll frame pacing and main-thread stalls.
- Selection, find, outline navigation, and theme/font changes.
- Cross-boundary serialization/copy costs.
- Package size and idle memory.

Optimize at the layer shown by profiles. Do not move presentation into a shared
engine merely because parsing is shared.

## Selection gate

GATE-001 accepts:

- Core implementation language and parser.
- Stable interop strategy per platform.
- Windows, macOS, and Linux presentation stacks.
- Baseline OS versions, architectures, and package formats.
- Performance budgets and named test hardware.
- Supported/deferred advanced features per platform.

No stack is accepted solely because it minimizes the number of languages.
