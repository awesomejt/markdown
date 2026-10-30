import Foundation
// swift-tools-version: 6.0
import PackageDescription

let coreLibraryPath = URL(fileURLWithPath: #filePath).deletingLastPathComponent()
  .appendingPathComponent("CorePackage/lib").path

let package = Package(
  name: "Markdown",
  platforms: [.macOS("27.0")],
  products: [
    .executable(name: "Markdown", targets: ["Markdown"]),
    .executable(name: "CoreProbe", targets: ["CoreProbe"]),
  ],
  targets: [
    .systemLibrary(name: "CMarkdown", path: "CorePackage"),
    .target(
      name: "MarkdownCore", dependencies: ["CMarkdown"],
      linkerSettings: [
        .unsafeFlags(["-L", coreLibraryPath]),
        .linkedLibrary("markdown_ffi"),
        .linkedLibrary("iconv"), .linkedLibrary("resolv"),
      ]),
    .executableTarget(name: "Markdown", dependencies: ["MarkdownCore"]),
    .executableTarget(name: "CoreProbe", dependencies: ["MarkdownCore", "CMarkdown"]),
    .testTarget(name: "MarkdownCoreTests", dependencies: ["MarkdownCore", "CMarkdown"]),
  ]
)
