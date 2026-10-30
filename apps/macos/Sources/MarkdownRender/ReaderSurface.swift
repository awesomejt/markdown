import AppKit

/// The read-only TextKit 2 document surface: one continuous, selectable text
/// view, so selection, copy and the text accessibility element span the whole
/// document. Never touch `layoutManager`, which would fall back to TextKit 1.
@MainActor
public final class ReaderSurface: NSObject, NSTextViewDelegate {
  public let scrollView: NSScrollView
  public let textView: NSTextView
  public private(set) var document: RenderedDocument?
  /// Opens an external link; replaced by checks so nothing leaves the app.
  public var openExternal: (URL) -> Void = { NSWorkspace.shared.open($0) }
  /// Called after navigation to a heading, with its outline index.
  public var onNavigate: ((Int) -> Void)?

  public override init() {
    scrollView = NSTextView.scrollableTextView()
    textView = scrollView.documentView as! NSTextView
    super.init()
    precondition(textView.textLayoutManager != nil, "TextKit 2 text view expected")
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

  /// Replace the presented text and return to the top.
  public func show(_ document: RenderedDocument?) {
    self.document = document
    textView.textStorage?.setAttributedString(document?.text ?? NSAttributedString())
    textView.setSelectedRange(NSRange(location: 0, length: 0))
    textView.scroll(.zero)
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
    guard let layout = textView.textLayoutManager, let content = layout.textContentManager,
      let start = content.location(content.documentRange.location, offsetBy: range.location),
      let end = content.location(start, offsetBy: range.length),
      let textRange = NSTextRange(location: start, end: end)
    else { return nil }
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

  private func scroll(to range: NSRange) {
    guard let rect = frame(of: range) else {
      textView.scrollRangeToVisible(range)
      return
    }
    let top = max(rect.minY - textView.textContainerInset.height, 0)
    textView.scroll(NSPoint(x: 0, y: top))
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
