import AppKit
import Foundation
import Testing

@testable import MarkdownCore
@testable import MarkdownRender

/// Document find (MAC-001F): core matches mapped to the rendered text,
/// navigation, highlighting on a real TextKit 2 view and text size.
@Suite(.serialized)
@MainActor
struct FindTests {
  /// A parsed and rendered document whose session stays open for finding.
  private struct Fixture {
    let session: CoreSession
    let rendered: RenderedDocument

    init(_ markdown: String, scale: CGFloat = 1) async throws {
      try await self.init(Data(markdown.utf8), scale: scale)
    }

    init(_ source: Data, scale: CGFloat = 1) async throws {
      session = try CoreSession(documentID: "find")
      let document = try #require(try await session.parse(source).document)
      rendered = try DocumentRenderer.render(document, scale: scale)
    }

    func find(_ query: String, _ options: CoreFindOptions = CoreFindOptions()) async throws
      -> FindMatches
    {
      rendered.matches(
        for: try await session.find(query, options: options, generation: rendered.generation))
    }

    func texts(_ matches: FindMatches) -> [String] {
      let text = rendered.text.string as NSString
      return matches.ranges.map { text.substring(with: $0) }
    }

    func close() async throws { try await session.close() }
  }

  /// Every kind of display text maps exactly: text across inline markup,
  /// entities, code spans and blocks, breaks, alt text, raw HTML, table cells,
  /// headings, and Unicode after astral, combining and right-to-left text.
  @Test func matchesMapToRenderedText() async throws {
    let fixture = try await Fixture(
      """
      # Find 𝒳 heading

      Plain foo **bar** baz, 𝒳𝒴 target, AT&amp;T and `code span`.
      soft
      break and hard\\
      line.

      > ```
      > quoted code
      > second line
      > ```

      ![alt words](missing.png) and <span>raw html</span>

      | Cell one | Cell two |
      | - | - |
      | table value | 中文 |

      Café and cafe\u{301}; 👨\u{200D}👩\u{200D}👧 family; עברית שלום.
      """)
    let cases: [(String, [String])] = [
      ("foo bar", ["foo bar"]),
      ("target", ["target"]),
      ("𝒳", ["𝒳", "𝒳"]),
      ("AT&T", ["AT&T"]),
      ("code span", ["code span"]),
      ("soft break", ["soft break"]),
      ("hard line", ["hard\u{2028}line"]),
      ("code second", ["code\nsecond"]),
      ("alt words", ["alt words"]),
      ("<span>raw", ["<span>raw"]),
      ("table value", ["table value"]),
      ("中文", ["中文"]),
      ("café", ["Café"]),
      ("cafe\u{301}", ["cafe\u{301}"]),
      ("family", ["family"]),
      ("שלום", ["שלום"]),
      ("heading", ["heading"]),
      ("missing", []),
    ]
    for (query, expected) in cases {
      let matches = try await fixture.find(query)
      #expect(fixture.texts(matches) == expected, "\(query)")
      #expect(
        zip(matches.ranges, matches.ranges.dropFirst()).allSatisfy { $0.location < $1.location })
    }
    let sensitive = try await fixture.find("CAFÉ", CoreFindOptions(caseSensitive: true))
    #expect(sensitive.ranges.isEmpty)
    let whole = try await fixture.find("cell", CoreFindOptions(wholeWord: true))
    #expect(fixture.texts(whole) == ["Cell", "Cell"])
    // A result for another snapshot never maps onto this rendering.
    let stale = CoreFindResult(
      generation: fixture.rendered.generation + 1,
      matches: [CoreFindMatch(unit: 0, source: 0..<1, exact: true, segments: 0..<0)],
      segments: [], truncated: false)
    #expect(fixture.rendered.matches(for: stale).ranges.isEmpty)
    try await fixture.close()
  }

  @Test func navigationWrapsAndReportsStatus() {
    let matches = FindMatches(
      generation: 1, ranges: [NSRange(location: 5, length: 2), NSRange(location: 20, length: 2)],
      truncated: false)
    #expect(matches.index(atOrAfter: 0) == 0)
    #expect(matches.index(atOrAfter: 5) == 0)
    #expect(matches.index(atOrAfter: 6) == 1)
    #expect(matches.index(atOrAfter: 21) == 0, "wraps to the first match")
    #expect(matches.index(before: 21) == 1)
    #expect(matches.index(before: 20) == 0)
    #expect(matches.index(before: 5) == 1, "wraps to the last match")
    #expect(matches.index(after: 1) == 0)
    #expect(matches.index(after: 0, backwards: true) == 1)
    #expect(matches.status(current: 1) == "2 of 2")
    #expect(matches.status(current: nil) == "2 matches")
    let truncated = FindMatches(
      generation: 1, ranges: Array(repeating: NSRange(location: 0, length: 1), count: 10_000),
      truncated: true)
    #expect(truncated.status(current: 0) == "1 of \(10_000.formatted())+")
    let none = FindMatches(generation: 1, ranges: [], truncated: false)
    #expect(none.index(atOrAfter: 0) == nil && none.index(before: 0) == nil)
    #expect(none.status(current: nil) == "Not found")
  }

  @Test func textScaleSteps() {
    #expect(TextScale.clamped(1) == 1)
    #expect(TextScale.clamped(1.04) == 1)
    #expect(TextScale.clamped(9) == TextScale.steps.last)
    #expect(TextScale.clamped(-1) == TextScale.steps.first)
    #expect(TextScale.clamped(.nan) == TextScale.standard)
    #expect(TextScale.larger(1) == 1.15 && TextScale.smaller(1) == 0.85)
    #expect(TextScale.larger(TextScale.steps.last!) == nil)
    #expect(TextScale.smaller(TextScale.steps.first!) == nil)
  }

  /// The text size multiplies fonts and text metrics; the text is unchanged,
  /// so find ranges and selections stay valid across sizes.
  @Test func scaledRendering() async throws {
    let markdown = "# Title\n\nBody `code`\n\n- item\n\n| A | B |\n| - | - |\n| 1 | 2 |\n"
    let normal = try await Fixture(markdown)
    let large = try await Fixture(markdown, scale: 2)
    #expect(normal.rendered.text.string == large.rendered.text.string)
    #expect(large.rendered.scale == 2)
    func attribute<T>(_ key: NSAttributedString.Key, _ text: String, _ fixture: Fixture) -> T? {
      let location = (fixture.rendered.text.string as NSString).range(of: text).location
      return fixture.rendered.text.attribute(key, at: location, effectiveRange: nil) as? T
    }
    for text in ["Title", "Body", "code", "item", "A"] {
      let small: NSFont? = attribute(.font, text, normal)
      let big: NSFont? = attribute(.font, text, large)
      #expect(big?.pointSize == (small?.pointSize).map { $0 * 2 }, "\(text) font")
    }
    let item: NSParagraphStyle? = attribute(.paragraphStyle, "item", normal)
    let bigItem: NSParagraphStyle? = attribute(.paragraphStyle, "item", large)
    #expect(bigItem?.headIndent == (item?.headIndent).map { $0 * 2 })
    let cell: NSParagraphStyle? = attribute(.paragraphStyle, "A", normal)
    let bigCell: NSParagraphStyle? = attribute(.paragraphStyle, "A", large)
    // Column widths are measured from the scaled text, so they scale only nearly linearly.
    let stops = zip(bigCell?.tabStops ?? [], cell?.tabStops ?? [])
    #expect(bigCell?.tabStops.count == cell?.tabStops.count && bigCell?.tabStops.isEmpty == false)
    #expect(stops.allSatisfy { abs($0.location - $1.location * 2) <= 4 })
    try await normal.close()
    try await large.close()
  }

  private static func surface(_ rendered: RenderedDocument) -> (NSWindow, ReaderSurface) {
    let window = NSWindow(
      contentRect: NSRect(x: 0, y: 0, width: 700, height: 500), styleMask: [.titled],
      backing: .buffered, defer: false)
    let surface = ReaderSurface()
    surface.scrollView.frame = window.contentView!.bounds
    surface.scrollView.autoresizingMask = [.width, .height]
    window.contentView!.addSubview(surface.scrollView)
    surface.show(rendered)
    window.contentView!.layoutSubtreeIfNeeded()
    return (window, surface)
  }

  /// The rendering background colour TextKit 2 will draw at `location`.
  private static func highlight(at location: Int, in surface: ReaderSurface) -> NSColor? {
    guard let layout = surface.textView.textLayoutManager, let content = layout.textContentManager,
      let start = content.location(content.documentRange.location, offsetBy: location),
      let end = content.location(start, offsetBy: 1),
      let range = NSTextRange(location: start, end: end)
    else { return nil }
    layout.ensureLayout(for: range)
    var color: NSColor?
    layout.enumerateRenderingAttributes(from: start, reverse: false) { _, attributes, found in
      if found.contains(start) { color = attributes[.backgroundColor] as? NSColor }
      return false
    }
    return color
  }

  /// Highlights are TextKit 2 rendering attributes on a real view: the
  /// current match is selected, revealed and drawn more strongly, the text
  /// storage is never changed, and a text size change keeps the position.
  @Test func surfaceHighlightsAndRestyle() async throws {
    var markdown = "# Start\n\n"
    for index in 0..<300 { markdown += "Paragraph \(index) holds a needle in the text.\n\n" }
    let fixture = try await Fixture(markdown)
    let (window, surface) = Self.surface(fixture.rendered)
    let matches = try await fixture.find("needle")
    #expect(matches.count == 300)
    surface.highlight(matches.ranges)
    #expect(Self.highlight(at: matches.ranges[0].location, in: surface) == ReaderSurface.matchColor)
    #expect(Self.highlight(at: matches.ranges[0].location - 2, in: surface) == nil)
    surface.showMatch(299)
    let last = matches.ranges[299]
    #expect(surface.currentMatch == 299)
    #expect(surface.textView.selectedRange() == last)
    #expect(surface.isVisible(last))
    #expect(!surface.isVisible(matches.ranges[0]))
    #expect(Self.highlight(at: last.location, in: surface) == ReaderSurface.currentMatchColor)
    #expect(
      surface.textView.textStorage?.attribute(
        .backgroundColor, at: last.location, effectiveRange: nil)
        == nil, "highlights never change the text storage")
    #expect(surface.textView.textLayoutManager != nil)

    // A larger text size keeps the selection, the highlights and the top line
    // (mid-document: near the end the view cannot scroll a line to the top).
    surface.showMatch(150)
    #expect(Self.highlight(at: last.location, in: surface) == ReaderSurface.matchColor)
    let middle = matches.ranges[150]
    let top = surface.topLocation()
    let large = try DocumentRenderer.render(
      try #require(try await fixture.session.parse(Data(markdown.utf8)).document), scale: 1.5)
    surface.restyle(large)
    window.contentView!.layoutSubtreeIfNeeded()
    #expect(surface.document?.scale == 1.5)
    #expect(surface.textView.selectedRange() == middle)
    #expect(surface.currentMatch == 150 && surface.matches == matches.ranges)
    let font = surface.textView.textStorage?.attribute(
      .font, at: middle.location, effectiveRange: nil)
    #expect((font as? NSFont)?.pointSize == 21)
    let paragraph = (surface.textView.string as NSString).paragraphRange(
      for: NSRange(location: top, length: 0))
    #expect(paragraph.contains(surface.topLocation()), "the top line stays in view")
    #expect(Self.highlight(at: middle.location, in: surface) == ReaderSurface.currentMatchColor)
    #expect(Self.highlight(at: last.location, in: surface) == ReaderSurface.matchColor)

    surface.highlight([])
    #expect(Self.highlight(at: middle.location, in: surface) == nil)
    surface.show(fixture.rendered)
    #expect(surface.matches.isEmpty && surface.currentMatch == nil)
    try await fixture.close()
  }

  /// Find over the generated profiles: core find, mapping, then highlighting
  /// and revealing the last match. Timings are printed as evidence.
  @Test func performanceProfilesFind() async throws {
    let directory = URL(
      fileURLWithPath: try #require(ProcessInfo.processInfo.environment["MARKDOWN_TEST_ROOT"])
    ).appendingPathComponent("build/fixtures/performance")
    for (name, query) in [
      ("prose-5mib", "cafe\u{301}"), ("prose-5mib", "native text layout"),
      ("long-lines-1mib", "xxxxxxxx"), ("delimiter-storm-1mib", "a"),
    ] {
      let fixture = try await Fixture(
        try Data(contentsOf: directory.appendingPathComponent("\(name).md")))
      let clock = ContinuousClock()
      var result: CoreFindResult?
      let core = try await clock.measure {
        result = try await fixture.session.find(query, generation: fixture.rendered.generation)
      }
      let found = try #require(result)
      if query == "cafe\u{301}" {
        // The profile spells café decomposed; find never normalizes (specs/search).
        let composed = try await fixture.find("café")
        #expect(composed.ranges.isEmpty, "NFC café does not match NFD text")
      }
      var matches: FindMatches?
      let map = clock.measure { matches = fixture.rendered.matches(for: found) }
      let mapped = try #require(matches)
      try #require(!mapped.ranges.isEmpty, "\(name) \(query) has matches")
      #expect(mapped.count == found.matches.count, "\(name) every match is shown")
      let text = fixture.rendered.text.string as NSString
      #expect(
        mapped.ranges.allSatisfy {
          text.substring(with: $0).compare(query, options: [.caseInsensitive]) == .orderedSame
        }, "\(name) highlights hold the query")
      let (_, surface) = Self.surface(fixture.rendered)
      let reveal = clock.measure {
        surface.highlight(mapped.ranges)
        surface.showMatch(mapped.count - 1)
      }
      #expect(surface.isVisible(mapped.ranges[mapped.count - 1]), "\(name) last match visible")
      print(
        "find-profile: \(name) query=\(query) matches=\(mapped.count)"
          + " truncated=\(mapped.truncated) core=\(Self.ms(core)) map=\(Self.ms(map))"
          + " highlight-last=\(Self.ms(reveal))")
      try await fixture.close()
    }
  }

  private static func ms(_ duration: Duration) -> String {
    let (seconds, attoseconds) = duration.components
    return String(format: "%.1fms", Double(seconds) * 1000 + Double(attoseconds) / 1e15)
  }
}

extension NSRange {
  fileprivate func contains(_ location: Int) -> Bool {
    location >= self.location && location < self.location + length
  }
}
