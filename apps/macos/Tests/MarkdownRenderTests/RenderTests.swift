import AppKit
import Foundation
import MarkdownCore
import Testing

@testable import MarkdownRender

@Suite(.serialized)
@MainActor
struct RenderTests {
  private static func parse(_ source: Data) async throws -> CoreDocument {
    let session = try CoreSession(documentID: "render")
    let result = try await session.parse(source)
    try await session.close()
    return try #require(result.document)
  }

  private static func render(_ markdown: String) async throws -> RenderedDocument {
    try DocumentRenderer.render(try await parse(Data(markdown.utf8)))
  }

  private static var root: URL {
    get throws {
      URL(fileURLWithPath: try #require(ProcessInfo.processInfo.environment["MARKDOWN_TEST_ROOT"]))
    }
  }

  private static func attribute<T>(
    _ key: NSAttributedString.Key, of text: String, in rendered: RenderedDocument
  ) -> T? {
    let range = (rendered.text.string as NSString).range(of: text)
    guard range.location != NSNotFound else { return nil }
    return rendered.text.attribute(key, at: range.location, effectiveRange: nil) as? T
  }

  private static func traits(of text: String, in rendered: RenderedDocument)
    -> NSFontDescriptor.SymbolicTraits
  {
    let font: NSFont? = attribute(.font, of: text, in: rendered)
    return font?.fontDescriptor.symbolicTraits ?? []
  }

  /// Every semantic case: the rendered outline matches the frozen expectation,
  /// each heading's range holds its title, and front matter is not content.
  @Test func semanticCorpusOutlines() async throws {
    let fixtures = try Self.root.appendingPathComponent("fixtures")
    let expected = try FileManager.default.contentsOfDirectory(
      at: fixtures.appendingPathComponent("expected"), includingPropertiesForKeys: nil
    ).filter { $0.pathExtension == "yaml" }
    let markdown = try #require(
      FileManager.default.subpaths(atPath: fixtures.appendingPathComponent("markdown").path))
    var sources: [String: URL] = [:]
    for path in markdown where path.hasSuffix(".md") {
      let url = fixtures.appendingPathComponent("markdown").appendingPathComponent(path)
      sources[url.deletingPathExtension().lastPathComponent] = url
    }
    #expect(expected.count == 8)
    for yaml in expected {
      let name = yaml.deletingPathExtension().lastPathComponent
      let source = try Data(contentsOf: try #require(sources[name], "\(name) source"))
      let document = try await Self.parse(source)
      let rendered = try DocumentRenderer.render(document)
      let outline = try Self.outline(String(contentsOf: yaml, encoding: .utf8))
      #expect(rendered.headings.map(\.level) == outline.map(\.level), "\(name) levels")
      #expect(rendered.headings.map(\.anchor) == outline.map(\.slug), "\(name) anchors")
      #expect(rendered.headings.map(\.title) == outline.map(\.text), "\(name) titles")
      let text = rendered.text.string as NSString
      for heading in rendered.headings {
        #expect(text.substring(with: heading.range) == heading.title, "\(name) heading text")
      }
      if document.frontMatterRange != nil {
        #expect(!rendered.text.string.hasPrefix("---"), "\(name) front matter hidden")
      }
    }
  }

  /// Parse the `- {level: N, text: T, slug: S}` outline lines of a case file.
  private static func outline(_ yaml: String) throws -> [(level: Int, text: String, slug: String)] {
    let pattern = try Regex(#"^\s+- \{level: (\d+), text: (.*), slug: (.*)\}$"#)
    var entries: [(Int, String, String)] = []
    var inOutline = false
    for line in yaml.split(separator: "\n", omittingEmptySubsequences: false) {
      if line.hasSuffix("outline:") {
        inOutline = true
        continue
      }
      guard inOutline else { continue }
      guard let match = try pattern.wholeMatch(in: String(line)) else { break }
      func unquote(_ value: Substring) -> String {
        var text = String(value)
        if text.hasPrefix("'"), text.hasSuffix("'") {
          text = String(text.dropFirst().dropLast()).replacingOccurrences(of: "''", with: "'")
        }
        return text
      }
      entries.append(
        (
          Int(match.output[1].substring!)!, unquote(match.output[2].substring!),
          unquote(match.output[3].substring!)
        ))
    }
    return entries
  }

  @Test func gfmTourLinksFollowPolicy() async throws {
    let source = try Data(
      contentsOf: try Self.root.appendingPathComponent("fixtures/markdown/foundation/gfm-tour.md"))
    let rendered = try DocumentRenderer.render(try await Self.parse(source))
    var fragments: [String] = []
    var external: [URL] = []
    rendered.text.enumerateAttribute(.link, in: NSRange(0..<rendered.text.length)) { value, _, _ in
      guard let url = value as? URL else { return }
      switch LinkDestination(link: url) {
      case .fragment(let anchor): fragments.append(anchor)
      case .external(let url): external.append(url)
      case nil: Issue.record("unexpected link \(url)")
      }
    }
    #expect(fragments == ["tables", "fenced-code", "gfm-feature-tour"])
    #expect(fragments.allSatisfy { rendered.heading(anchor: $0) != nil })
    // The core's canonical URI form.
    #expect(external.map(\.absoluteString) == ["https://example.com/"])
    // Relative links need a folder grant: inert, with an explanation.
    for text in ["examples index", "Mermaid page"] {
      let tip: String? = Self.attribute(.toolTip, of: text, in: rendered)
      #expect(tip == "Links to other files are not available yet")
      let link: URL? = Self.attribute(.link, of: text, in: rendered)
      #expect(link == nil)
    }
  }

  @Test func unsafeContentIsInert() async throws {
    for name in ["foundation/unsafe-html.md", "foundation/gfm-tour.md", "advanced/voice-parity.md"]
    {
      let source = try Data(
        contentsOf: try Self.root.appendingPathComponent("fixtures/markdown/\(name)"))
      let rendered = try DocumentRenderer.render(try await Self.parse(source))
      rendered.text.enumerateAttribute(.link, in: NSRange(0..<rendered.text.length)) {
        value, _, _ in
        guard let value else { return }
        let url = value as? URL
        #expect(url.flatMap(LinkDestination.init(link:)) != nil, "\(name) link \(value)")
      }
      #expect(!rendered.text.containsAttachments)
    }
    let rendered = try await Self.render(
      "[a](javascript:alert(1)) [b](data:text/html,x) <script>alert(1)</script>\n")
    var links = 0
    rendered.text.enumerateAttribute(.link, in: NSRange(0..<rendered.text.length)) { value, _, _ in
      if value != nil { links += 1 }
    }
    #expect(links == 0)
    #expect(rendered.text.string.contains("<script>alert(1)</script>"))
  }

  @Test func blocksAndInlines() async throws {
    let rendered = try await Self.render(
      """
      # Title

      Plain *em* **strong** ~~gone~~ `code` [frag](#title)
      hard\\
      break

      3. three
      4. four
         - nested

      > quoted

      ```swift
      let x = 1
      let y = 2
      ```

      ---

      ![alt text](pic.png)
      """)
    #expect(
      rendered.text.string == """
        Title
        Plain em strong gone code frag hard\u{2028}break
        3.\tthree
        4.\tfour
        •\tnested
        quoted
        let x = 1
        let y = 2
        * * *
        [alt text]
        """)
    #expect(rendered.headings.map(\.title) == ["Title"])
    #expect(Self.traits(of: "em", in: rendered).contains(.italic))
    #expect(Self.traits(of: "strong", in: rendered).contains(.bold))
    #expect(Self.traits(of: "Title", in: rendered).contains(.bold))
    #expect(Self.traits(of: "code", in: rendered).contains(.monoSpace))
    #expect(Self.traits(of: "let y", in: rendered).contains(.monoSpace))
    let strike: Int? = Self.attribute(.strikethroughStyle, of: "gone", in: rendered)
    #expect(strike == NSUnderlineStyle.single.rawValue)
    let link: URL? = Self.attribute(.link, of: "frag", in: rendered)
    #expect(link.flatMap(LinkDestination.init(link:)) == .fragment("title"))
    let nested: NSParagraphStyle? = Self.attribute(.paragraphStyle, of: "nested", in: rendered)
    let top: NSParagraphStyle? = Self.attribute(.paragraphStyle, of: "three", in: rendered)
    #expect((nested?.headIndent ?? 0) > (top?.headIndent ?? 0))
    // Tight items sit close together; the list's end keeps a block gap.
    #expect((top?.paragraphSpacing ?? 0) < 10)
    #expect((nested?.paragraphSpacing ?? 0) == 10)
    let quote: NSParagraphStyle? = Self.attribute(.paragraphStyle, of: "quoted", in: rendered)
    #expect((quote?.headIndent ?? 0) > 0)
    let tip: String? = Self.attribute(.toolTip, of: "[alt text]", in: rendered)
    #expect(tip == "Image not shown")
  }

  @Test func nonASCIIAnchorsAndListHeadings() async throws {
    let rendered = try await Self.render("# Café\n\n[back](#café)\n\n- ## Listed\n")
    let link: URL? = Self.attribute(.link, of: "back", in: rendered)
    let destination = link.flatMap(LinkDestination.init(link:))
    #expect(destination == .fragment("café"))
    #expect(rendered.heading(anchor: "café") != nil)
    // A heading that opens a list item excludes the item's marker.
    let text = rendered.text.string as NSString
    #expect(rendered.headings.map { text.substring(with: $0.range) } == ["Café", "Listed"])
  }

  @Test func renderingIsCancellable() async throws {
    var markdown = ""
    for index in 0..<20_000 { markdown += "Paragraph \(index) with *emphasis*.\n\n" }
    let document = try await Self.parse(Data(markdown.utf8))
    let task = Task.detached {
      withUnsafeCurrentTask { $0?.cancel() }
      return try DocumentRenderer.render(document)
    }
    await #expect(throws: CancellationError.self) { try await task.value }
  }

  /// Outline navigation, fragment links, external links and copy on a real
  /// TextKit 2 view in an offscreen window.
  @Test func surfaceNavigationAndCopy() async throws {
    var markdown = "# Start\n\n[to end](#end) and [site](https://example.com/x)\n\n"
    for index in 0..<400 {
      markdown +=
        "## Section \(index)\n\n" + String(repeating: "Words in a line. ", count: 40)
        + "\n\n"
    }
    markdown += "## End\n\nLast paragraph.\n"
    let rendered = try await Self.render(markdown)
    let window = NSWindow(
      contentRect: NSRect(x: 0, y: 0, width: 700, height: 500), styleMask: [.titled],
      backing: .buffered, defer: false)
    let surface = ReaderSurface()
    surface.scrollView.frame = window.contentView!.bounds
    surface.scrollView.autoresizingMask = [.width, .height]
    window.contentView!.addSubview(surface.scrollView)
    #expect(surface.textView.textLayoutManager != nil)
    surface.show(rendered)
    window.contentView!.layoutSubtreeIfNeeded()

    let last = try #require(rendered.headings.last)
    #expect(last.anchor == "end")
    #expect(!surface.isVisible(last.range))
    var navigated: [Int] = []
    surface.onNavigate = { navigated.append($0) }
    #expect(surface.reveal(heading: rendered.headings.count - 1))
    #expect(surface.isVisible(last.range))
    #expect(surface.textView.selectedRange() == last.range)
    #expect(!surface.isVisible(rendered.headings[0].range))

    // A fragment link navigates; an external link goes to the injected opener.
    #expect(surface.reveal(heading: 0))
    let text = rendered.text.string as NSString
    let fragment = text.range(of: "to end")
    let fragmentURL = try #require(
      rendered.text.attribute(.link, at: fragment.location, effectiveRange: nil))
    #expect(surface.textView(surface.textView, clickedOnLink: fragmentURL, at: fragment.location))
    #expect(surface.isVisible(last.range))
    #expect(navigated == [rendered.headings.count - 1, 0, rendered.headings.count - 1])
    var opened: [URL] = []
    surface.openExternal = { opened.append($0) }
    let site = text.range(of: "site")
    let siteURL = try #require(
      rendered.text.attribute(.link, at: site.location, effectiveRange: nil))
    _ = surface.textView(surface.textView, clickedOnLink: siteURL, at: site.location)
    #expect(opened.map(\.absoluteString) == ["https://example.com/x"])
    #expect(!surface.reveal(anchor: "missing"))

    // Selection spans blocks and copies as plain text through the native path.
    let span = NSRange(location: 0, length: text.range(of: "Section 1\n").upperBound)
    surface.textView.setSelectedRange(span)
    let pasteboard = NSPasteboard(name: NSPasteboard.Name("markdown-render-\(UUID().uuidString)"))
    defer { pasteboard.releaseGlobally() }
    #expect(
      surface.textView.writeSelection(
        to: pasteboard, types: surface.textView.writablePasteboardTypes))
    let copied = pasteboard.string(forType: .string)
    #expect(copied == text.substring(with: span))
    #expect(copied?.hasPrefix("Start\nto end and site\nSection 0\n") == true)
  }

  /// The generated performance profiles render, install and navigate to their
  /// last heading on TextKit 2. Timings are printed as evidence, not asserted.
  @Test func performanceProfilesRenderAndNavigate() async throws {
    let directory = try Self.root.appendingPathComponent("build/fixtures/performance")
    for name in ["prose-1mib", "delimiter-storm-1mib", "long-lines-1mib", "prose-5mib"] {
      let source = try Data(contentsOf: directory.appendingPathComponent("\(name).md"))
      let document = try await Self.parse(source)
      let clock = ContinuousClock()
      var rendered: RenderedDocument?
      let render = try clock.measure { rendered = try DocumentRenderer.render(document) }
      let output = try #require(rendered)
      let window = NSWindow(
        contentRect: NSRect(x: 0, y: 0, width: 900, height: 640), styleMask: [.titled],
        backing: .buffered, defer: false)
      let surface = ReaderSurface()
      surface.scrollView.frame = window.contentView!.bounds
      window.contentView!.addSubview(surface.scrollView)
      let install = clock.measure {
        surface.show(output)
        window.contentView!.layoutSubtreeIfNeeded()
      }
      #expect(surface.textView.textLayoutManager != nil)
      var navigate = Duration.zero
      if let last = output.headings.indices.last {
        navigate = clock.measure { #expect(surface.reveal(heading: last)) }
        #expect(surface.isVisible(output.headings[last].range), "\(name) last heading visible")
      }
      print(
        "render-profile: \(name) bytes=\(source.count) headings=\(output.headings.count)"
          + " utf16=\(output.text.length) render=\(Self.ms(render)) install=\(Self.ms(install))"
          + " last-heading=\(Self.ms(navigate))")
    }
  }

  private static func ms(_ duration: Duration) -> String {
    let (seconds, attoseconds) = duration.components
    return String(format: "%.1fms", Double(seconds) * 1000 + Double(attoseconds) / 1e15)
  }
}

extension NSRange {
  fileprivate var upperBound: Int { location + length }
}
