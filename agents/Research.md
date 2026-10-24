# Research Notes

## 2026-10-24 — initial ecosystem check

Primary sources consulted for the planning baseline:

- [Swift Markdown](https://github.com/swiftlang/swift-markdown): Swift package
  for parsing/building/analyzing Markdown; its parser is powered by
  `cmark-gfm`, and its markup tree uses immutable thread-safe value types.
- [Comrak](https://github.com/kivikakk/comrak): Rust CommonMark/GFM parser and
  renderer with an AST, extensions, safe defaults, and pluggable highlighting.
- [cmark-gfm](https://github.com/github/cmark-gfm): GitHub's portable C99 GFM
  parser, AST, and renderers, with unsafe HTML/links scrubbed by default.
- [UniFFI guide](https://mozilla.github.io/uniffi-rs/latest/): generated
  bindings for Rust libraries, including production-quality Swift support. The
  Swift guide currently describes Swift 6 support as partial and identifies
  async `Sendable` limitations.

These sources establish feasibility, not selection. FEAS-002 and FEAS-004 must
test the exact versions pinned at implementation time.

## 2026-10-24 — native desktop framework check

- [WinUI 3](https://learn.microsoft.com/en-us/windows/apps/winui): Microsoft's
  recommended native framework for new Windows desktop applications, with C#
  and C++ support through the Windows App SDK.
- [AppKit](https://developer.apple.com/documentation/AppKit): macOS-native UI,
  document, printing, accessibility, and large-data facilities; interoperates
  with SwiftUI.
- [SwiftUI/UIKit integration](https://developer.apple.com/documentation/swiftui/uikit-integration):
  supports mixing native UIKit and SwiftUI presentation on iOS/iPadOS.
- [GTK 4](https://docs.gtk.org/gtk4/getting_started.html): leading candidate
  native widget toolkit for the Linux application spike.
- [Cargo workspaces](https://doc.rust-lang.org/cargo/reference/workspaces.html):
  support coordinated Rust packages, shared lock state, and common commands
  inside the proposed monorepo.

Framework choice remains gated by native rendering and packaging measurements.

## Research hygiene

- Prefer official repositories, specifications, and vendor documentation.
- Record access date, pinned version/commit, platform/toolchain, and unresolved
  claims in each feasibility report.
- Store measurements with reproducible commands and fixtures.
- Do not convert an ecosystem claim into an architectural decision without
  project-specific evidence.
