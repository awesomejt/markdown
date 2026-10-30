import AppKit
import Foundation
import ImageIO
import MarkdownCore
import Testing
import UniformTypeIdentifiers
import zlib

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

  /// A real view in an offscreen window, laid out with `rendered`.
  private static func surface(_ rendered: RenderedDocument, width: CGFloat = 700) -> (
    NSWindow, ReaderSurface
  ) {
    let window = NSWindow(
      contentRect: NSRect(x: 0, y: 0, width: width, height: 500), styleMask: [.titled],
      backing: .buffered, defer: false)
    let surface = ReaderSurface()
    surface.scrollView.frame = window.contentView!.bounds
    surface.scrollView.autoresizingMask = [.width, .height]
    window.contentView!.addSubview(surface.scrollView)
    surface.show(rendered)
    window.contentView!.layoutSubtreeIfNeeded()
    return (window, surface)
  }

  /// Tables keep their alignment on TextKit 2: one paragraph per row, measured
  /// tab stops, a grid-drawing fragment, and tab-separated copy.
  @Test func tablesAlignAndCopy() async throws {
    let source = try Data(
      contentsOf: try Self.root.appendingPathComponent("fixtures/markdown/foundation/gfm-tour.md"))
    let rendered = try DocumentRenderer.render(try await Self.parse(source))
    let text = rendered.text.string as NSString
    let rows = ["Name\tType\tScore", "Alpha\tstring\t10", "Beta\tnumber\t200", "Gamma\tboolean\t3"]
    #expect(text.range(of: rows.joined(separator: "\n")).location != NSNotFound)
    #expect(Self.traits(of: "Score", in: rendered).contains(.bold))
    #expect(!Self.traits(of: "Gamma", in: rendered).contains(.bold))
    let style: NSParagraphStyle? = Self.attribute(.paragraphStyle, of: "Alpha", in: rendered)
    #expect(style?.tabStops.map(\.alignment) == [.left, .center, .right])
    let header: TableRowStyle? = Self.attribute(.tableRow, of: "Name", in: rendered)
    let body: TableRowStyle? = Self.attribute(.tableRow, of: "Beta", in: rendered)
    #expect(header?.header == true && header?.first == true)
    #expect(body?.header == false && body?.shaded == true && body?.edges.count == 4)

    let (window, surface) = Self.surface(rendered)
    _ = window
    #expect(surface.textView.textLayoutManager != nil)
    var fragments = 0
    let layout = try #require(surface.textView.textLayoutManager)
    layout.enumerateTextLayoutFragments(
      from: layout.documentRange.location, options: [.ensuresLayout]
    ) {
      if let row = $0 as? TableRowFragment {
        fragments += 1
        #expect(row.gridRect.width > 0 && row.renderingSurfaceBounds.contains(row.gridRect))
      }
      return true
    }
    #expect(fragments == 4)
    // Right-aligned scores share a right edge; centred types share a centre;
    // each row is one line.
    func frame(_ string: String) throws -> NSRect {
      try #require(surface.frame(of: text.range(of: string)))
    }
    let scores = try ["Score", "\t10", "200", "\t3"].map { try frame($0).maxX }
    #expect(scores.allSatisfy { abs($0 - scores[0]) < 1 })
    let types = try ["Type", "string", "number", "boolean"].map { try frame($0).midX }
    #expect(types.allSatisfy { abs($0 - types[0]) < 1 })
    #expect(try frame("Name").minX == frame("Gamma").minX)
    #expect(try frame("Score").maxX < frame("Name").minX + 400)

    let span = text.range(of: rows.joined(separator: "\n"))
    surface.textView.setSelectedRange(span)
    let pasteboard = NSPasteboard(name: NSPasteboard.Name("markdown-render-\(UUID().uuidString)"))
    defer { pasteboard.releaseGlobally() }
    #expect(
      surface.textView.writeSelection(
        to: pasteboard, types: surface.textView.writablePasteboardTypes))
    #expect(pasteboard.string(forType: .string) == rows.joined(separator: "\n"))
  }

  @Test func tableEdgeCases() async throws {
    // A right-aligned first column leads with a tab; a short row keeps its
    // cells; a table inside a quote is indented with it.
    let rendered = try await Self.render(
      "| # | Item |\n| -: | --- |\n| 1 | one |\n| 22 |\n\n> | q |\n> | - |\n> | x |\n")
    let text = rendered.text.string
    // GFM pads the short row with an empty cell.
    #expect(text.hasPrefix("\t#\tItem\n\t1\tone\n\t22\t\n"))
    let first: NSParagraphStyle? = Self.attribute(.paragraphStyle, of: "\t1", in: rendered)
    #expect(first?.tabStops.first?.alignment == .right)
    let quoted: TableRowStyle? = Self.attribute(.tableRow, of: "x", in: rendered)
    let plain: TableRowStyle? = Self.attribute(.tableRow, of: "one", in: rendered)
    #expect((quoted?.edges.first ?? 0) > (plain?.edges.first ?? 0))
  }

  @Test func taskMarkers() async throws {
    let rendered = try await Self.render("- [x] done\n- [ ] open\n- plain\n")
    #expect(rendered.text.string == "☑\tdone\n☐\topen\n•\tplain")
    let done: NSColor? = Self.attribute(.foregroundColor, of: "☑", in: rendered)
    let open: NSColor? = Self.attribute(.foregroundColor, of: "☐", in: rendered)
    #expect(done == .controlAccentColor && open == .secondaryLabelColor)
    let item: NSColor? = Self.attribute(.foregroundColor, of: "done", in: rendered)
    #expect(item == .labelColor)
  }

  /// A PNG of `width` by `height` pixels.
  private static func png(width: Int, height: Int) throws -> Data {
    let context = try #require(
      CGContext(
        data: nil, width: width, height: height, bitsPerComponent: 8, bytesPerRow: 0,
        space: CGColorSpace(name: CGColorSpace.sRGB)!,
        bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue))
    context.setFillColor(CGColor(red: 0.2, green: 0.4, blue: 0.8, alpha: 1))
    context.fill(CGRect(x: 0, y: 0, width: width, height: height))
    let image = try #require(context.makeImage())
    let data = NSMutableData()
    let destination = try #require(
      CGImageDestinationCreateWithData(data, UTType.png.identifier as CFString, 1, nil))
    CGImageDestinationAddImage(destination, image, nil)
    #expect(CGImageDestinationFinalize(destination))
    return data as Data
  }

  /// A small PNG whose header claims `width` by `height` pixels.
  private static func pngClaiming(width: UInt32, height: UInt32) throws -> Data {
    var data = try png(width: 4, height: 4)
    // IHDR data starts at byte 16; its CRC covers the type and data (12..<29).
    withUnsafeBytes(of: width.bigEndian) { data.replaceSubrange(16..<20, with: $0) }
    withUnsafeBytes(of: height.bigEndian) { data.replaceSubrange(20..<24, with: $0) }
    let crc = data[12..<29].withUnsafeBytes {
      UInt32(crc32(0, $0.bindMemory(to: UInt8.self).baseAddress, 17))
    }
    withUnsafeBytes(of: crc.bigEndian) { data.replaceSubrange(29..<33, with: $0) }
    return data
  }

  /// Local images load through the injected loader; remote, denied, missing,
  /// SVG and corrupt images show their alt text with the reason.
  @Test func imagesFollowResourcePolicy() async throws {
    let picture = try Self.png(width: 1600, height: 800)
    let requests = Requests()
    let loader: ImageLoader = { path, limit throws(ImageFailure) in
      requests.append(path)
      #expect(limit == ImageBudget().maxFileBytes)
      switch path {
      case "img/pic.png", "dir name/é.png": return picture
      case "vector.svg": throw .unsupportedType
      case "disguised.png": return Data("<svg xmlns='http://www.w3.org/2000/svg'/>".utf8)
      case "corrupt.png": return Data("not an image".utf8)
      case "../up.png": throw .outsideFolder
      default: throw .notFound
      }
    }
    let markdown = """
      # Images

      ![a picture](img/pic.png "The title") and again ![again](img/pic.png)

      ![spaced](<dir name/é.png>) ![remote](https://example.com/x.png) ![data](data:image/png;base64,AA==)

      ![](missing.png) ![up](../up.png) ![svg](vector.svg) ![disguised](disguised.png) ![bad](corrupt.png) ![frag](#images)

      [![linked](img/pic.png)](https://example.com/)
      """
    let document = try await Self.parse(Data(markdown.utf8))
    let rendered = try DocumentRenderer.render(document, images: loader)
    // Each distinct path is read once; remote and denied targets never reach the loader.
    #expect(
      requests.values == [
        "img/pic.png", "dir name/é.png", "missing.png", "../up.png", "vector.svg",
        "disguised.png", "corrupt.png",
      ])
    let text = rendered.text.string as NSString
    #expect(
      rendered.images.map(\.failure) == [
        nil, nil, nil, .remote, .blocked, .notFound, .outsideFolder, .unsupportedType,
        .undecodable, .undecodable, .blocked, nil,
      ])
    for image in rendered.images {
      let tip = rendered.text.attribute(.toolTip, at: image.range.location, effectiveRange: nil)
      if let failure = image.failure {
        #expect(tip as? String == failure.message)
        #expect(!rendered.text.containsAttachments(in: image.range))
      } else {
        let attachment = try #require(
          rendered.text.attribute(.attachment, at: image.range.location, effectiveRange: nil)
            as? ImageAttachment)
        #expect(attachment.naturalSize == CGSize(width: 1600, height: 800))
        #expect(text.substring(with: image.range) == "\u{FFFC}")
      }
    }
    let first = try #require(
      rendered.text.attribute(
        .attachment, at: rendered.images[0].range.location, effectiveRange: nil)
        as? ImageAttachment)
    #expect(first.image?.accessibilityDescription == "a picture")
    let title = rendered.text.attribute(
      .toolTip, at: rendered.images[0].range.location, effectiveRange: nil)
    #expect(title as? String == "The title")
    #expect(text.substring(with: rendered.images[5].range) == "[image]")
    #expect(text.substring(with: rendered.images[3].range) == "[remote]")
    // A linked image keeps the link.
    let link = rendered.text.attribute(
      .link, at: rendered.images[11].range.location, effectiveRange: nil)
    #expect((link as? URL)?.absoluteString == "https://example.com/")

    // Without a loader nothing is read and local images keep their alt text.
    let plain = try DocumentRenderer.render(document)
    #expect(!plain.text.containsAttachments)
    #expect(plain.images[0].failure == .notLoaded)

    // On a real view the image scales down to the line width, never up.
    let (window, surface) = Self.surface(rendered, width: 500)
    _ = window
    let frame = try #require(surface.frame(of: rendered.images[0].range))
    let line = surface.textView.textContainer!.size.width
    #expect(frame.width <= line && frame.width > 200)
    #expect(abs(frame.width / frame.height - 2) < 0.05)
    #expect(
      ImageAttachment.fitted(CGSize(width: 40, height: 20), width: 500).size
        == CGSize(width: 40, height: 20))
  }

  /// Decoding is bounded: oversized pixel counts are refused from the header,
  /// decoded copies are downsampled, and the per-document budgets hold.
  @Test func imageBudgetsBoundDecoding() async throws {
    let small = try Self.png(width: 64, height: 64)
    let huge = try Self.pngClaiming(width: 50_000, height: 50_000)
    let wide = try Self.png(width: 4000, height: 100)
    let images: [String: Data] = ["huge.png": huge, "wide.png": wide]
    let loader: ImageLoader = { path, _ in images[path] ?? small }
    var markdown = "![h](huge.png) ![w](wide.png)\n\n"
    for index in 0..<6 { markdown += "![\(index)](i\(index).png) " }
    let document = try await Self.parse(Data(markdown.utf8))
    var budget = ImageBudget()
    budget.maxImages = 5
    let rendered = try DocumentRenderer.render(document, images: loader, budget: budget)
    // The forged header is refused before any pixels are allocated.
    let bomb = try #require(rendered.images.first?.failure)
    #expect(bomb == .tooLarge || bomb == .undecodable)
    #expect(
      rendered.images.dropFirst().map(\.failure) == [nil, nil, nil, nil, .limit, .limit, .limit])
    var pixels = ImageBudget()
    pixels.maxSourcePixels = 4000 * 100 - 1
    let refused = try DocumentRenderer.render(document, images: loader, budget: pixels)
    #expect(refused.images[1].failure == .tooLarge && refused.images[2].failure == nil)
    // Decoded memory: two small images fit, the wide one and the rest do not.
    var memory = ImageBudget()
    memory.maxDecodedBytes = 64 * 64 * 4 * 2
    let bounded = try DocumentRenderer.render(document, images: loader, budget: memory)
    #expect(
      bounded.images.dropFirst().map(\.failure) == [
        .limit, nil, nil, .limit, .limit, .limit, .limit,
      ])
    let wideImage = try #require(
      rendered.text.attribute(
        .attachment, at: rendered.images[1].range.location, effectiveRange: nil)
        as? ImageAttachment)
    #expect(wideImage.naturalSize.width == 2048)
    #expect(wideImage.naturalSize.height < 60)
    // Rendering with images is still cancellable.
    let task = Task.detached {
      withUnsafeCurrentTask { $0?.cancel() }
      return try DocumentRenderer.render(document, images: loader)
    }
    await #expect(throws: CancellationError.self) { try await task.value }
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

/// Records loader calls from the render thread.
private final class Requests: @unchecked Sendable {
  private let lock = NSLock()
  private var paths: [String] = []
  func append(_ path: String) { lock.withLock { paths.append(path) } }
  var values: [String] { lock.withLock { paths } }
}

extension NSRange {
  fileprivate var upperBound: Int { location + length }
}
