import AppKit

/// The read-only TextKit 2 document surface: one continuous, selectable text
/// view, so selection, copy and the text accessibility element span the whole
/// document. Never touch `layoutManager`, which would fall back to TextKit 1.
@MainActor
public final class ReaderSurface: NSObject, NSTextViewDelegate, NSTextLayoutManagerDelegate {
  public let scrollView: NSScrollView
  public let textView: NSTextView
  public private(set) var document: RenderedDocument?
  /// Opens an external link; replaced by checks so nothing leaves the app.
  public var openExternal: (URL) -> Void = { NSWorkspace.shared.open($0) }
  /// Called after navigation to a heading, with its outline index.
  public var onNavigate: ((Int) -> Void)?
  /// Highlighted find matches, in reading order, and the current one.
  public private(set) var matches: [NSRange] = []
  public private(set) var currentMatch: Int?

  static let matchColor = NSColor.findHighlightColor.withAlphaComponent(0.4)
  static let currentMatchColor = NSColor.findHighlightColor

  public override init() {
    scrollView = NSTextView.scrollableTextView()
    textView = scrollView.documentView as! NSTextView
    super.init()
    precondition(textView.textLayoutManager != nil, "TextKit 2 text view expected")
    textView.textLayoutManager?.delegate = self
    textView.isEditable = false
    textView.isSelectable = true
    textView.isRichText = true
    textView.importsGraphics = false
    textView.usesFindBar = false
    textView.drawsBackground = true
    textView.backgroundColor = .textBackgroundColor
    textView.textContainerInset = NSSize(width: 32, height: 24)
    textView.linkTextAttributes = [
      .foregroundColor: NSColor.linkColor,
      .underlineStyle: NSUnderlineStyle.single.rawValue,
      .cursor: NSCursor.pointingHand,
    ]
    textView.delegate = self
    textView.setAccessibilityIdentifier("document-text")
    textView.setAccessibilityLabel("Document")
    scrollView.hasVerticalScroller = true
    scrollView.autohidesScrollers = true
    scrollView.borderType = .noBorder
  }

  /// Replace the presented text and return to the top, clearing highlights.
  public func show(_ document: RenderedDocument?) {
    self.document = document
    textView.textStorage?.setAttributedString(document?.text ?? NSAttributedString())
    highlight([])
    textView.setSelectedRange(NSRange(location: 0, length: 0))
    textView.scroll(.zero)
  }

  /// Replace the presented text with a new rendering of the same text (a
  /// text size change), keeping the selection, highlights and the line at the
  /// top of the view.
  public func restyle(_ document: RenderedDocument) {
    guard let previous = self.document, previous.text.string == document.text.string else {
      show(document)
      return
    }
    let top = topLocation()
    let selection = textView.selectedRange()
    self.document = document
    textView.textStorage?.setAttributedString(document.text)
    textView.setSelectedRange(selection)
    let line = NSRange(location: top, length: min(1, document.text.length - top))
    scroll(to: line)
    textView.layoutSubtreeIfNeeded()
    scroll(to: line)
    // Re-apply the highlights to the new text.
    highlight(matches, current: currentMatch)
  }

  /// The UTF-16 location of the line at the top of the visible area, below
  /// the inset that `scroll(to:)` leaves above a revealed line.
  public func topLocation() -> Int {
    guard let layout = textView.textLayoutManager, let content = layout.textContentManager else {
      return 0
    }
    // One point lower, so a line starting exactly at the top is not resolved
    // to the paragraph that ends there.
    let y =
      textView.visibleRect.minY - textView.textContainerOrigin.y
      + textView.textContainerInset.height + 1
    let point = CGPoint(x: 0, y: max(y, 0))
    guard let fragment = layout.textLayoutFragment(for: point) else { return 0 }
    return content.offset(
      from: content.documentRange.location, to: fragment.rangeInElement.location)
  }

  /// Highlight find matches (rendered ranges in reading order). `current` is
  /// drawn more strongly; use `showMatch` to select and reveal it. Highlights
  /// are TextKit 2 rendering attributes: they never change the text storage,
  /// so copied text carries none.
  public func highlight(_ ranges: [NSRange], current: Int? = nil) {
    if let layout = textView.textLayoutManager {
      layout.removeRenderingAttribute(.backgroundColor, for: layout.documentRange)
      layout.removeRenderingAttribute(.foregroundColor, for: layout.documentRange)
    }
    matches = ranges
    currentMatch = current.flatMap { ranges.indices.contains($0) ? $0 : nil }
    for (index, range) in ranges.enumerated() { paint(range, current: index == currentMatch) }
  }

  /// Make match `index` current and scroll it into view away from the edges.
  /// With `select`, it also becomes the selection, so copy and accessibility
  /// follow; while the find field has the focus the selection would draw
  /// over the current-match colour, so the window selects only later.
  public func showMatch(_ index: Int, select: Bool = true) {
    guard matches.indices.contains(index) else { return }
    if let previous = currentMatch, previous != index, matches.indices.contains(previous) {
      paint(matches[previous], current: false)
    }
    currentMatch = index
    let range = matches[index]
    paint(range, current: true)
    if select { textView.setSelectedRange(range) }
    let visible = textView.visibleRect
    if let rect = frame(of: range), !visible.insetBy(dx: 0, dy: visible.height / 6).contains(rect) {
      // Same settling as heading navigation, but leave context above the match.
      let lead = NSRange(location: range.location, length: 0)
      scroll(to: lead, margin: visible.height / 3)
      textView.layoutSubtreeIfNeeded()
      scroll(to: lead, margin: visible.height / 3)
    }
    if select { NSAccessibility.post(element: textView, notification: .selectedTextChanged) }
  }

  private func paint(_ range: NSRange, current: Bool) {
    guard let layout = textView.textLayoutManager, let textRange = textRange(for: range) else {
      return
    }
    layout.removeRenderingAttribute(.foregroundColor, for: textRange)
    layout.addRenderingAttribute(
      .backgroundColor, value: current ? Self.currentMatchColor : Self.matchColor, for: textRange)
    // The find colour is light in both appearances; keep the current match's text dark.
    if current {
      layout.addRenderingAttribute(.foregroundColor, value: NSColor.black, for: textRange)
    }
  }

  private func textRange(for range: NSRange) -> NSTextRange? {
    guard let content = textView.textLayoutManager?.textContentManager,
      let start = content.location(content.documentRange.location, offsetBy: range.location),
      let end = content.location(start, offsetBy: range.length)
    else { return nil }
    return NSTextRange(location: start, end: end)
  }

  /// Scroll a heading to the top of the view and select it, so the keyboard
  /// and accessibility focus follow the navigation.
  @discardableResult
  public func reveal(heading index: Int) -> Bool {
    guard let heading = document?.headings[safe: index] else { return false }
    reveal(heading.range)
    onNavigate?(index)
    return true
  }

  public func reveal(anchor: String) -> Bool {
    guard let index = document?.headings.firstIndex(where: { $0.anchor == anchor }) else {
      return false
    }
    return reveal(heading: index)
  }

  /// The rect of `range` in text-view coordinates, laying out what it needs.
  public func frame(of range: NSRange) -> NSRect? {
    guard let layout = textView.textLayoutManager, let textRange = textRange(for: range) else {
      return nil
    }
    layout.ensureLayout(for: textRange)
    var rect: NSRect?
    layout.enumerateTextSegments(in: textRange, type: .standard, options: []) {
      _, segment, _, _ in
      rect = rect.map { $0.union(segment) } ?? segment
      return true
    }
    let origin = textView.textContainerOrigin
    return rect?.offsetBy(dx: origin.x, dy: origin.y)
  }

  /// Whether `range` currently lies inside the visible part of the document.
  public func isVisible(_ range: NSRange) -> Bool {
    guard let rect = frame(of: range) else { return false }
    return textView.visibleRect.intersects(rect)
  }

  private func reveal(_ range: NSRange) {
    textView.setSelectedRange(range)
    // Positions above the target can change as estimated layout firms up;
    // scroll once, then again after the layout pass settles.
    scroll(to: range)
    textView.layoutSubtreeIfNeeded()
    scroll(to: range)
    NSAccessibility.post(element: textView, notification: .selectedTextChanged)
  }

  private func scroll(to range: NSRange, margin: CGFloat = 0) {
    guard let rect = frame(of: range) else {
      textView.scrollRangeToVisible(range)
      return
    }
    let top = max(rect.minY - textView.textContainerInset.height - margin, 0)
    textView.scroll(NSPoint(x: 0, y: top))
  }

  /// Table rows get a fragment that draws their grid; everything else is default.
  public nonisolated func textLayoutManager(
    _ textLayoutManager: NSTextLayoutManager, textLayoutFragmentFor location: any NSTextLocation,
    in textElement: NSTextElement
  ) -> NSTextLayoutFragment {
    let range = textElement.elementRange
    if let paragraph = textElement as? NSTextParagraph, paragraph.attributedString.length > 0,
      let style = paragraph.attributedString.attribute(.tableRow, at: 0, effectiveRange: nil)
        as? TableRowStyle
    {
      return TableRowFragment(style: style, textElement: textElement, range: range)
    }
    return NSTextLayoutFragment(textElement: textElement, range: range)
  }

  public func textView(_ textView: NSTextView, clickedOnLink link: Any, at charIndex: Int) -> Bool {
    guard let url = link as? URL ?? (link as? String).flatMap(URL.init(string:)),
      let destination = LinkDestination(link: url)
    else { return true }
    switch destination {
    case .fragment(let anchor):
      if !reveal(anchor: anchor) { NSSound.beep() }
    case .external(let url):
      openExternal(url)
    }
    return true
  }
}

extension Array {
  fileprivate subscript(safe index: Int) -> Element? {
    indices.contains(index) ? self[index] : nil
  }
}
