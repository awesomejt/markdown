// swift-tools-version:6.2
import PackageDescription

let package = Package(
    name: "SwiftMarkdownProbe",
    platforms: [.macOS(.v13)],
    dependencies: [
        .package(url: "https://github.com/swiftlang/swift-markdown.git",
                 revision: "25cb61d3482054b09ae76ca4f281b1bfe7fe5a43"), // 0.9.0
    ],
    targets: [
        .executableTarget(name: "swift-markdown-probe", dependencies: [
            .product(name: "Markdown", package: "swift-markdown"),
        ]),
    ]
)
