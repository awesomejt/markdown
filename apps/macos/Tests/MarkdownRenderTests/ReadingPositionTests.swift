import AppKit
import Foundation
import Testing

@testable import MarkdownCore
@testable import MarkdownRender

/// Reading positions (MAC-001H): rendered locations map to source offsets
/// and back, and a position anchored in one revision lands on the same text
/// in the next, on a real TextKit 2 view.
@Suite(.serialized)
@MainActor
struct ReadingPositionTests {
  private static func render(_ markdown: String, session: CoreSession) async throws
    -> RenderedDocument
  {
    let document = try #require(try await session.parse(Data(markdown.utf8)).document)
    return try DocumentRenderer.render(document)
  }

  private static func surface(_ rendered: RenderedDocument) -> (NSWindow, ReaderSurface) {
    let window = NSWindow(
      contentRect: NSRect(x: 0, y: 0, width: 700, height: 500), styleMask: [.titled],
      backing: .buffered, defer: false)
    window.isReleasedWhenClosed = false
    let surface = ReaderSurface()
    surface.scrollView.frame = window.contentView!.bounds
    surface.scrollView.autoresizingMask = [.width, .height]
    window.contentView!.addSubview(surface.scrollView)
    surface.show(rendered)
    window.contentView!.layoutSubtreeIfNeeded()
    return (window, surface)
  }

  private static func sections(_ count: Int, prefix: String = "") -> String {
    prefix
      + (0..<count).map { "## Part \($0)\n\nProse for part \($0), *emphasised* and plain.\n" }
      .joined(separator: "\n")
  }

  /// Inside display text the mapping is exact both ways, including after
  /// non-ASCII text, inline markup and code; between nodes it snaps forward.
  @Test func renderedLocationsMapToSource() async throws {
    let session = try CoreSession(documentID: "positions")
    let markdown = "# Café ☕\n\n- *item* `code` 😀 end\n\n```\nlet x = 1\n```\n"
    let rendered = try await Self.render(markdown, session: session)
    let source = Array(markdown.utf8)
    let string = rendered.text.string as NSString
    for word in ["Café", "☕", "item", "code", "😀", "end", "let x"] {
      let location = string.range(of: word).location
      let offset = rendered.sourceOffset(at: location)
      #expect(
        String(decoding: source[offset..<offset + word.utf8.count], as: UTF8.self) == word,
        "\(word)")
      #expect(rendered.location(atSource: offset) == location, "\(word) maps back")
    }
    // The list marker has no source text of its own: it snaps to the item.
    let marker = string.range(of: "•").location
    #expect(
      rendered.sourceOffset(at: marker)
        == rendered.sourceOffset(at: string.range(of: "item").location))
    #expect(rendered.location(atSource: source.count + 10) == rendered.text.length)
    try await session.close()
  }

  /// A position saved in one revision resolves in an edited one: text
  /// inserted above moves it down, an edited paragraph keeps it nearby, and
  /// the selection follows its words.
  @Test func savedPositionsSurviveEdits() async throws {
    let session = try CoreSession(documentID: "edits")
    let old = try await Self.render(Self.sections(40), session: session)
    let oldString = old.text.string as NSString
    let target = oldString.range(of: "Prose for part 30")
    let selection = oldString.range(
      of: "emphasised", range: NSRange(target.location..<old.text.length))
    let saved = try old.save(ReadingPosition(top: target.location + 4, selection: selection))

    let new = try await Self.render(
      Self.sections(40, prefix: "# Inserted\n\nA new paragraph above.\n\n"), session: session)
    let newString = new.text.string as NSString
    let restored = try #require(new.restore(saved))
    let moved = newString.range(of: "Prose for part 30")
    #expect(restored.top == moved.location + 4)
    #expect(
      newString.substring(with: restored.selection) == "emphasised"
        && restored.selection.location > moved.location)
    // The old document still resolves its own position exactly.
    #expect(old.restore(saved)?.top == target.location + 4)

    // Editing the paragraph itself relocates to it, clamped.
    let edited = try await Self.render(
      Self.sections(40).replacingOccurrences(of: "Prose for part 30,", with: "Rewritten part,"),
      session: session)
    let editedTop = try #require(edited.restore(saved)).top
    let paragraph = (edited.text.string as NSString).range(of: "Rewritten part")
    #expect(
      (paragraph.location..<paragraph.location + 60).contains(editedTop),
      "an edited paragraph keeps the reader nearby")

    // An emptied document loses the position.
    let empty = try await Self.render("", session: session)
    #expect(empty.restore(saved) == nil)
    try await session.close()
  }

  /// The surface reports the line, not just the paragraph, at the top, and
  /// shows a document at a saved position across a reload.
  @Test func surfaceKeepsTheLineAcrossAReload() async throws {
    let session = try CoreSession(documentID: "surface")
    let long = (0..<200).map { "Sentence \($0) of one long paragraph." }.joined(separator: " ")
    let markdown = "# Top\n\n" + long + "\n\n" + Self.sections(30)
    let old = try await Self.render(markdown, session: session)
    let (window, surface) = Self.surface(old)
    defer { window.close() }
    let string = old.text.string as NSString
    let inside = string.range(of: "Sentence 150")
    surface.show(old, at: ReadingPosition(top: inside.location, selection: inside))
    let top = surface.topLocation()
    let paragraph = string.paragraphRange(for: inside)
    #expect(top > paragraph.location, "the top is a line inside the long paragraph")
    #expect(abs(top - inside.location) < 200, "the top line holds the shown position")
    #expect(surface.textView.selectedRange() == inside)

    let saved = try old.save(surface.readingPosition)
    let new = try await Self.render("Added first.\n\n" + markdown, session: session)
    let position = try #require(new.restore(saved))
    surface.show(new, at: position)
    let newString = new.text.string as NSString
    let newTop = surface.topLocation()
    let shift = newString.range(of: "Sentence 150").location - inside.location
    #expect(abs(newTop - (top + shift)) < 120, "the same line stays at the top")
    #expect(newString.substring(with: surface.textView.selectedRange()) == "Sentence 150")
    #expect(surface.isVisible(newString.range(of: "Sentence 150")))
    try await session.close()
  }

  /// Anchoring a position in one revision of each generated profile and
  /// resolving it in an edited revision stays off the main thread's way.
  /// Timings are printed as evidence; the bound is generous.
  @Test func profilesTransferPositionsQuickly() async throws {
    let root = URL(
      fileURLWithPath: try #require(ProcessInfo.processInfo.environment["MARKDOWN_TEST_ROOT"]))
    let directory = root.appendingPathComponent("build/fixtures/performance")
    for name in ["prose-1mib", "delimiter-storm-1mib", "long-lines-1mib", "prose-5mib"] {
      let source = try Data(contentsOf: directory.appendingPathComponent("\(name).md"))
      let session = try CoreSession(documentID: "profile")
      let old = try DocumentRenderer.render(
        try #require(try await session.parse(source).document))
      // Insert a paragraph, dropping the last line so 5 MiB stays in bounds.
      let cut = try #require(source[..<(source.count - 64)].lastIndex(of: 0x0A))
      let edited = Data("Edited first.\n\n".utf8) + source[..<cut]
      let new = try DocumentRenderer.render(
        try #require(try await session.parse(edited).document))
      let middle = old.text.length * 3 / 4
      let position = ReadingPosition(top: middle, selection: NSRange(location: middle, length: 20))
      let clock = ContinuousClock()
      var saved: SavedPosition?
      let save = try clock.measure { saved = try old.save(position) }
      var restored: ReadingPosition?
      let restore = clock.measure { restored = new.restore(try! #require(saved)) }
      let moved = try #require(restored)
      let expected = (new.text.string as NSString).substring(with: moved.selection)
      #expect(
        expected == (old.text.string as NSString).substring(with: position.selection), "\(name)")
      print("reading-position: \(name) save=\(save) restore=\(restore)")
      #expect(save + restore < .seconds(2), "\(name)")
      try await session.close()
    }
  }
}
