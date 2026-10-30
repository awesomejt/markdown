// swift-tools-version: 6.0
import PackageDescription

let package = Package(
  name: "Markdown",
  platforms: [.macOS("27.0")],
  products: [.executable(name: "Markdown", targets: ["Markdown"])],
  targets: [.executableTarget(name: "Markdown")]
)
