import AppKit
import MarkdownRender

/// The document's front matter above its text (MAC-002F, R1.2): a disclosure
/// row, “Properties (N)”, that expands to the keys and values the core
/// published, or to the front matter's source when it published none. The
/// properties are never part of the document's text, so find, copy and
/// reading positions ignore them. Collapsed at first; a window keeps the
/// reader's choice across documents.
@MainActor
final class PropertiesBar: NSView {
  let disclosure = NSButton()
  let title = NSTextField(labelWithString: "Properties")
  private let scroll = NSScrollView()
  private let grid = NSGridView()
  private var scrollHeight: NSLayoutConstraint!
  /// The most the expanded properties take before they scroll.
  static let maxHeight: CGFloat = 180
  private(set) var properties: DocumentProperties?

  var isExpanded: Bool { disclosure.state == .on }
  /// The rows shown when expanded, as key and value, for the smoke.
  var shownRows: [(key: String, value: String)] {
    (0..<grid.numberOfRows).compactMap { index in
      let row = grid.row(at: index)
      guard row.numberOfCells == 2,
        let key = row.cell(at: 0).contentView as? NSTextField,
        let value = row.cell(at: 1).contentView as? NSTextField
      else { return nil }
      return (key.stringValue, value.stringValue)
    }
  }

  init() {
    super.init(frame: .zero)
    disclosure.bezelStyle = .disclosure
    disclosure.setButtonType(.pushOnPushOff)
    disclosure.title = ""
    disclosure.state = .off
    disclosure.target = self
    disclosure.action = #selector(toggle(_:))
    disclosure.setAccessibilityIdentifier("properties-disclosure")
    title.font = .systemFont(ofSize: NSFont.smallSystemFontSize, weight: .semibold)
    title.textColor = .secondaryLabelColor
    title.setAccessibilityIdentifier("properties-title")
    let click = NSClickGestureRecognizer(target: self, action: #selector(toggle(_:)))
    title.addGestureRecognizer(click)
    let header = NSStackView(views: [disclosure, title])
    header.orientation = .horizontal
    header.spacing = 4

    grid.rowSpacing = 4
    grid.columnSpacing = 12
    grid.translatesAutoresizingMaskIntoConstraints = false
    let document = FlippedView()
    document.translatesAutoresizingMaskIntoConstraints = false
    document.addSubview(grid)
    scroll.documentView = document
    scroll.drawsBackground = false
    scroll.hasVerticalScroller = true
    scroll.autohidesScrollers = true
    scroll.borderType = .noBorder
    scroll.isHidden = true
    scroll.setAccessibilityIdentifier("properties-list")
    scrollHeight = scroll.heightAnchor.constraint(equalToConstant: 0)

    let column = NSStackView(views: [header, scroll])
    column.orientation = .vertical
    column.alignment = .leading
    column.spacing = 4
    column.edgeInsets = NSEdgeInsets(top: 4, left: 12, bottom: 6, right: 12)
    column.translatesAutoresizingMaskIntoConstraints = false
    let separator = NSBox()
    separator.boxType = .separator
    separator.translatesAutoresizingMaskIntoConstraints = false
    addSubview(column)
    addSubview(separator)
    NSLayoutConstraint.activate([
      column.topAnchor.constraint(equalTo: topAnchor),
      column.leadingAnchor.constraint(equalTo: leadingAnchor),
      column.trailingAnchor.constraint(equalTo: trailingAnchor),
      separator.topAnchor.constraint(equalTo: column.bottomAnchor),
      separator.leadingAnchor.constraint(equalTo: leadingAnchor),
      separator.trailingAnchor.constraint(equalTo: trailingAnchor),
      separator.bottomAnchor.constraint(equalTo: bottomAnchor),
      scroll.widthAnchor.constraint(equalTo: column.widthAnchor, constant: -24),
      scrollHeight,
      grid.topAnchor.constraint(equalTo: document.topAnchor),
      grid.leadingAnchor.constraint(equalTo: document.leadingAnchor),
      grid.bottomAnchor.constraint(equalTo: document.bottomAnchor),
      document.widthAnchor.constraint(equalTo: scroll.contentView.widthAnchor),
      grid.trailingAnchor.constraint(lessThanOrEqualTo: document.trailingAnchor),
    ])
    setAccessibilityElement(true)
    setAccessibilityRole(.group)
    setAccessibilityLabel("Document properties")
    setAccessibilityIdentifier("properties-bar")
  }

  required init?(coder: NSCoder) {
    fatalError("The properties bar is constructed programmatically")
  }

  /// Paint the window background, like the find bar.
  override func draw(_ dirtyRect: NSRect) {
    NSColor.windowBackgroundColor.setFill()
    dirtyRect.fill()
  }

  /// Show `properties`, or hide the bar without any. The same properties
  /// (a refresh that left the front matter alone) keep the shown rows.
  func show(_ properties: DocumentProperties?) {
    isHidden = properties == nil
    guard properties != self.properties else { return }
    self.properties = properties
    while grid.numberOfRows > 0 { grid.removeRow(at: 0) }
    guard let properties else { return }
    if let source = properties.unreadSource {
      title.stringValue = "Properties (not read)"
      let note = Self.label(
        "These properties couldn’t be read, so they are shown as written.", secondary: true)
      let text = Self.label(source, monospaced: true)
      grid.addRow(with: [note])
      grid.addRow(with: [text])
      // A previous document's key column may have been right-aligned.
      if grid.numberOfColumns > 0 { grid.column(at: 0).xPlacement = .leading }
    } else {
      let count = properties.entries.count
      title.stringValue = "Properties (\(count))"
      for entry in properties.entries {
        let key = Self.label(entry.key, secondary: true)
        key.setContentHuggingPriority(.required, for: .horizontal)
        let value = Self.label(entry.value)
        value.setAccessibilityLabel("\(entry.key): \(entry.value)")
        grid.addRow(with: [key, value])
      }
      if grid.numberOfColumns > 0 { grid.column(at: 0).xPlacement = .trailing }
    }
    disclosure.setAccessibilityLabel(title.stringValue)
    layoutRows()
  }

  @objc private func toggle(_ sender: Any?) {
    if !(sender is NSButton) { disclosure.state = isExpanded ? .off : .on }
    layoutRows()
  }

  /// Expand or collapse, as clicking the disclosure does.
  func setExpanded(_ expanded: Bool) {
    disclosure.state = expanded ? .on : .off
    layoutRows()
  }

  private func layoutRows() {
    scroll.isHidden = !isExpanded
    grid.layoutSubtreeIfNeeded()
    scrollHeight.constant = isExpanded ? min(grid.fittingSize.height, Self.maxHeight) : 0
  }

  private static func label(
    _ text: String, secondary: Bool = false, monospaced: Bool = false
  ) -> NSTextField {
    let label = NSTextField(wrappingLabelWithString: text)
    label.isSelectable = true
    label.font =
      monospaced
      ? .monospacedSystemFont(ofSize: NSFont.smallSystemFontSize, weight: .regular)
      : .systemFont(ofSize: NSFont.smallSystemFontSize)
    label.textColor = secondary ? .secondaryLabelColor : .labelColor
    label.preferredMaxLayoutWidth = 560
    return label
  }
}

/// Lays out top-down, so short property lists start at the top of their scroll view.
private final class FlippedView: NSView {
  override var isFlipped: Bool { true }
}
