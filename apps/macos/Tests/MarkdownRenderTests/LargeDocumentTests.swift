import AppKit
import Foundation
import Testing

@testable import MarkdownCore
@testable import MarkdownRender

/// Large documents (MAC-001G): bounded TextKit paragraphs with exact find
/// mapping, the lead shown while the rest renders, VoiceOver rotors, and copy
/// and text replacement on long selections.
@Suite(.serialized)
@MainActor
struct LargeDocumentTests {
  private static func parse(_ markdown: String) async throws -> (CoreSession, CoreDocument) {
    let session = try CoreSession(documentID: "large")
    let document = try #require(try await session.parse(Data(markdown.utf8)).document)
    return (session, document)
  }

  /// AppKit tells a pasteboard's previous owner on a later run-loop turn.
  private static func released(_ pasteboard: NSPasteboard) async throws -> Bool {
    for _ in 0..<100 where RichTextPromise.isHeld(for: pasteboard) {
      try await Task.sleep(for: .milliseconds(10))
    }
    return !RichTextPromise.isHeld(for: pasteboard)
  }

  private static func find(
    _ query: String, _ session: CoreSession, _ rendered: RenderedDocument
  ) async throws -> [NSRange] {
    rendered.matches(
      for: try await session.find(
        query, options: CoreFindOptions(), generation: rendered.generation)
    ).ranges
  }

  /// A real view in an offscreen window, laid out with `rendered`.
  private static func surface(_ rendered: RenderedDocument) -> (NSWindow, ReaderSurface) {
    let window = NSWindow(
      contentRect: NSRect(x: 0, y: 0, width: 700, height: 500), styleMask: [.titled],
      backing: .buffered, defer: false)
    // Closing must not release it a second time under ARC.
    window.isReleasedWhenClosed = false
    let surface = ReaderSurface()
    surface.scrollView.frame = window.contentView!.bounds
    surface.scrollView.autoresizingMask = [.width, .height]
    window.contentView!.addSubview(surface.scrollView)
    surface.show(rendered)
    window.contentView!.layoutSubtreeIfNeeded()
    return (window, surface)
  }

  /// Many short paragraphs, so the renderer passes a lead.
  private static func longProse(sections: Int) -> String {
    (0..<sections).map { section in
      "## Section \(section)\n\n"
        + (0..<8).map { _ in "A sentence of ordinary prose for the long document test." }
        .joined(separator: " ") + "\n"
    }.joined(separator: "\n")
  }

  /// Long paragraphs break at a space once past the limit, unbroken runs get
  /// breaks at grapheme boundaries, and the text is otherwise unchanged.
  @Test func longParagraphsAreBounded() async throws {
    let words = (0..<6_000).map { String(format: "w%04d", $0) }.joined(separator: " ")
    let run = String(repeating: "x", count: 70_000)
    let combining = String(repeating: "e\u{301}", count: 20_000)
    let (session, document) = try await Self.parse("# T\n\n\(words)\n\n\(run)\n\n\(combining)\n")
    let rendered = try DocumentRenderer.render(document)
    try await session.close()
    let text = rendered.text.string as NSString
    for paragraph in rendered.text.string.components(separatedBy: "\n") {
      #expect(paragraph.utf16.count <= RenderStyle.hardParagraphLimit)
    }
    // Words: spaces past the limit became line breaks, one unit for one.
    let start = text.range(of: "w0000").location
    let shown = text.substring(with: NSRange(location: start, length: (words as NSString).length))
    #expect(shown.contains("\n"))
    #expect(shown.replacingOccurrences(of: "\n", with: " ") == words)
    #expect(
      shown.components(separatedBy: "\n").allSatisfy {
        $0.utf16.count <= RenderStyle.paragraphLimit + 6
      })
    // The unbroken run gets two inserted breaks and the combining run one,
    // between whole "é" sequences.
    #expect(rendered.insertedBreaks.count == 3)
    let last = try #require(rendered.insertedBreaks.last)
    #expect(text.character(at: last - 1) == 0x0301)
    #expect(text.character(at: last + 1) == 0x65)
    let original = text.mutableCopy() as! NSMutableString
    for location in rendered.insertedBreaks.reversed() {
      original.deleteCharacters(in: NSRange(location: location, length: 1))
    }
    #expect((original as String).contains(run))
    #expect((original as String).contains(combining))
    // Breaks don't depend on the text size, so a size change restyles in place.
    #expect(try DocumentRenderer.render(document, scale: 2).text.string == rendered.text.string)
  }

  /// A match across an inserted break covers it, and a match starting just
  /// after one starts after it.
  @Test func findMapsAcrossInsertedBreaks() async throws {
    let hard = RenderStyle.hardParagraphLimit
    let run = String(repeating: "x", count: hard - 2) + "MARKER" + String(repeating: "y", count: 99)
    let (session, document) = try await Self.parse("\(run)\n\nafter MARKER\n")
    let rendered = try DocumentRenderer.render(document)
    let text = rendered.text.string as NSString
    #expect(rendered.insertedBreaks == [hard])
    #expect(text.substring(with: NSRange(location: hard - 2, length: 7)) == "MA\nRKER")
    let later = text.range(of: "after MARKER").location + 6
    #expect(
      try await Self.find("marker", session, rendered) == [
        NSRange(location: hard - 2, length: 7), NSRange(location: later, length: 6),
      ])
    #expect(
      try await Self.find("rker", session, rendered).first == NSRange(location: hard + 1, length: 4)
    )
    #expect(
      try await Self.find("xma", session, rendered) == [NSRange(location: hard - 3, length: 3)])
    try await session.close()
  }

  /// A long document passes a lead whose text and headings are a prefix of
  /// the complete rendering; completing it in place keeps the selection and
  /// the line at the top. A short document passes none.
  @Test func leadIsPrefixAndCompletesInPlace() async throws {
    let (session, document) = try await Self.parse(Self.longProse(sections: 400))
    try await session.close()
    let leads = Leads()
    let complete = try DocumentRenderer.render(document, lead: { leads.append($0) })
    let lead = try #require(leads.values.first)
    #expect(leads.values.count == 1)
    #expect(!lead.isComplete && complete.isComplete)
    #expect(lead.text.length >= RenderStyle.leadLength && lead.text.length < complete.text.length)
    #expect(complete.text.string.hasPrefix(lead.text.string))
    #expect(lead.text.string.hasSuffix("\n"))
    #expect(Array(complete.headings.prefix(lead.headings.count)) == lead.headings)
    #expect(complete.text.isEqual(to: try DocumentRenderer.render(document).text))

    let (window, surface) = Self.surface(lead)
    defer { window.close() }
    let selection = NSRange(location: 200, length: 40)
    surface.textView.setSelectedRange(selection)
    surface.reveal(heading: 3)
    surface.textView.setSelectedRange(selection)
    let top = surface.topLocation()
    surface.complete(complete)
    window.contentView!.layoutSubtreeIfNeeded()
    #expect(surface.textView.string == complete.text.string)
    #expect(surface.document?.isComplete == true)
    #expect(surface.textView.selectedRange() == selection)
    #expect(surface.topLocation() == top)
    #expect(surface.reveal(heading: complete.headings.count - 1))

    let (shortSession, short) = try await Self.parse("# Short\n\nOne paragraph.\n")
    try await shortSession.close()
    let none = Leads()
    #expect(try DocumentRenderer.render(short, lead: { none.append($0) }).isComplete)
    #expect(none.values.isEmpty)
  }

  /// VoiceOver rotors move by heading, heading level, table and image, in
  /// both directions and with a filter, to exact ranges of the text view.
  @Test func rotorsNavigateStructure() async throws {
    let (session, document) = try await Self.parse(
      """
      # One

      Text ![a diagram](missing.png) here.

      ## Two

      | A | B |
      | - | - |
      | 1 | 2 |

      ### Three

      ## Four
      """)
    try await session.close()
    let rendered = try DocumentRenderer.render(document)
    let (window, surface) = Self.surface(rendered)
    defer { window.close() }
    let rotors = surface.textView.accessibilityCustomRotors()
    #expect(
      rotors.map(\.type) == [
        .heading, .headingLevel1, .headingLevel2, .headingLevel3, .headingLevel4, .headingLevel5,
        .headingLevel6, .table, .image,
      ])
    func search(
      _ type: NSAccessibilityCustomRotor.RotorType, from: NSRange? = nil,
      backwards: Bool = false, filter: String = ""
    ) -> NSAccessibilityCustomRotor.ItemResult? {
      let rotor = rotors.first { $0.type == type }!
      let parameters = NSAccessibilityCustomRotor.SearchParameters()
      parameters.searchDirection = backwards ? .previous : .next
      parameters.filterString = filter
      if let from {
        let current = NSAccessibilityCustomRotor.ItemResult(targetElement: surface.textView)
        current.targetRange = from
        parameters.currentItem = current
      }
      return rotor.itemSearchDelegate?.rotor(rotor, resultFor: parameters)
    }
    let headings = rendered.headings
    #expect(search(.heading)?.targetRange == headings[0].range)
    #expect(search(.heading)?.customLabel == "One")
    #expect(search(.heading)?.targetElement === surface.textView)
    #expect(search(.heading, from: headings[0].range)?.targetRange == headings[1].range)
    #expect(search(.heading, from: headings[3].range) == nil)
    #expect(search(.heading, backwards: true)?.targetRange == headings[3].range)
    #expect(
      search(.heading, from: headings[2].range, backwards: true)?.targetRange == headings[1].range)
    #expect(search(.heading, from: headings[0].range, backwards: true) == nil)
    #expect(search(.headingLevel2)?.customLabel == "Two")
    #expect(search(.headingLevel2, from: headings[1].range)?.customLabel == "Four")
    #expect(search(.headingLevel3)?.customLabel == "Three")
    #expect(search(.headingLevel4) == nil)
    #expect(search(.heading, filter: "thr")?.customLabel == "Three")
    let text = rendered.text.string as NSString
    let table = try #require(search(.table))
    #expect(text.substring(with: table.targetRange) == "A\tB\n1\t2")
    #expect(table.customLabel == "Table 1")
    let image = try #require(search(.image))
    #expect(image.customLabel == "a diagram")
    #expect(image.targetRange == rendered.images[0].range)
  }

  /// Copying a long selection writes plain text at once and rich text only
  /// when asked; a short selection keeps AppKit's own types and RTF.
  @Test func longSelectionsCopyRichTextLazily() async throws {
    let (session, document) = try await Self.parse(Self.longProse(sections: 400))
    try await session.close()
    let rendered = try DocumentRenderer.render(document)
    let (window, surface) = Self.surface(rendered)
    defer { window.close() }
    let text = surface.textView
    // The fixed long-selection types match AppKit's for this view, for a
    // short selection without and with an image attachment.
    let reader = try #require(text as? ReaderTextView)
    text.setSelectedRange(NSRange(location: 0, length: 10))
    #expect(text.writablePasteboardTypes == ReaderTextView.types(withImages: false))
    let image = NSMutableAttributedString(attributedString: rendered.text)
    let attachment = NSTextAttachment()
    attachment.image = NSImage(size: NSSize(width: 4, height: 4))
    image.insert(NSAttributedString(attachment: attachment), at: 1)
    text.textStorage?.setAttributedString(image)
    text.setSelectedRange(NSRange(location: 0, length: 10))
    #expect(text.writablePasteboardTypes == ReaderTextView.types(withImages: true))
    // A long selection over a loaded image offers RTFD too.
    reader.imageRanges = [NSRange(location: 1, length: 1)]
    text.selectAll(nil)
    #expect(text.writablePasteboardTypes == ReaderTextView.types(withImages: true))
    let withImage = NSPasteboard(name: NSPasteboard.Name("large-\(UUID().uuidString)"))
    defer { withImage.releaseGlobally() }
    #expect(text.writeSelection(to: withImage, types: text.writablePasteboardTypes))
    #expect(withImage.data(forType: .rtfd) != nil)
    surface.show(rendered)
    #expect(reader.imageRanges.isEmpty)

    let pasteboard = NSPasteboard(name: NSPasteboard.Name("large-\(UUID().uuidString)"))
    defer { pasteboard.releaseGlobally() }
    text.selectAll(nil)
    #expect(text.selectedRange().length > ReaderTextView.eagerRichTextLimit)
    #expect(text.writeSelection(to: pasteboard, types: text.writablePasteboardTypes))
    #expect(pasteboard.string(forType: .string) == rendered.text.string)
    // Reading the promised type asks the view for it.
    let rtf = try #require(pasteboard.data(forType: .rtf))
    let pasted = try #require(NSAttributedString(rtf: rtf, documentAttributes: nil))
    #expect(pasted.string == rendered.text.string)

    let short = NSRange(location: 0, length: 100)
    text.setSelectedRange(short)
    #expect(text.writeSelection(to: pasteboard, types: text.writablePasteboardTypes))
    #expect(
      pasteboard.string(forType: .string)
        == (rendered.text.string as NSString).substring(with: short))
    #expect(pasteboard.data(forType: .rtf) != nil)
    #expect(try await Self.released(pasteboard))

    // A multiple selection keeps AppKit's own types and copy.
    text.setSelectedRanges(
      [
        NSValue(range: NSRange(location: 0, length: 70_000)),
        NSValue(range: NSRange(location: 80_000, length: 10)),
      ],
      affinity: .downstream, stillSelecting: false)
    #expect(text.selectedRanges.count == 2)
    #expect(text.writablePasteboardTypes.count >= 2)
    #expect(text.writeSelection(to: pasteboard, types: text.writablePasteboardTypes))
    #expect(try await Self.released(pasteboard))
  }

  /// The promised rich text outlives the window it was copied from.
  @Test func promisedRichTextOutlivesItsView() async throws {
    let (session, document) = try await Self.parse(Self.longProse(sections: 400))
    try await session.close()
    let rendered = try DocumentRenderer.render(document)
    let pasteboard = NSPasteboard(name: NSPasteboard.Name("large-\(UUID().uuidString)"))
    defer { pasteboard.releaseGlobally() }
    weak var released: NSTextView?
    autoreleasepool {
      let (window, surface) = Self.surface(rendered)
      surface.textView.selectAll(nil)
      #expect(
        surface.textView.writeSelection(
          to: pasteboard, types: surface.textView.writablePasteboardTypes))
      released = surface.textView
      window.close()
    }
    for _ in 0..<100 where released != nil { try await Task.sleep(for: .milliseconds(10)) }
    #expect(released == nil)
    #expect(RichTextPromise.isHeld(for: pasteboard))
    let rtf = try #require(pasteboard.data(forType: .rtf))
    #expect(NSAttributedString(rtf: rtf, documentAttributes: nil)?.string == rendered.text.string)
    // A new owner releases it.
    pasteboard.clearContents()
    #expect(try await Self.released(pasteboard))
  }

  /// Showing or restyling a long document over another replaces its text
  /// quickly: in place, TextKit 2 took seconds (`replaceText`).
  @Test func replacingLongTextIsFast() async throws {
    let (session, document) = try await Self.parse(Self.longProse(sections: 3_000))
    try await session.close()
    let rendered = try DocumentRenderer.render(document)
    let larger = try DocumentRenderer.render(document, scale: 1.5)
    let (window, surface) = Self.surface(rendered)
    defer { window.close() }
    let clock = ContinuousClock()
    let restyle = clock.measure {
      surface.restyle(larger)
      window.contentView!.layoutSubtreeIfNeeded()
    }
    let reshow = clock.measure {
      surface.show(rendered)
      window.contentView!.layoutSubtreeIfNeeded()
    }
    #expect(surface.textView.string == rendered.text.string)
    print("large-document: utf16=\(rendered.text.length) restyle=\(restyle) show-over=\(reshow)")
    #expect(restyle < .seconds(2) && reshow < .seconds(2))
  }
}

/// Collects leads from the render thread.
private final class Leads: @unchecked Sendable {
  private let lock = NSLock()
  private var items: [RenderedDocument] = []
  func append(_ lead: RenderedDocument) { lock.withLock { items.append(lead) } }
  var values: [RenderedDocument] { lock.withLock { items } }
}
