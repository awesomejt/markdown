import AppKit

/// The notice below a stale document (MAC-001H): the document was deleted,
/// moved or became unreadable after it loaded, so the window keeps showing
/// the version last read. Reload tries again; the notice also goes away by
/// itself when the file comes back.
@MainActor
final class NoticeBar: NSView {
  let icon = NSImageView()
  let message = NSTextField(labelWithString: "")
  let detail = NSTextField(wrappingLabelWithString: "")
  let reload = NSButton(title: "Reload", target: nil, action: nil)
  var onReload: (() -> Void)?

  init() {
    super.init(frame: .zero)
    icon.image = NSImage(
      systemSymbolName: "exclamationmark.triangle.fill", accessibilityDescription: "Warning")
    icon.contentTintColor = .systemOrange
    icon.setContentHuggingPriority(.required, for: .horizontal)
    message.font = .boldSystemFont(ofSize: NSFont.smallSystemFontSize)
    message.setAccessibilityIdentifier("notice-message")
    detail.font = .systemFont(ofSize: NSFont.smallSystemFontSize)
    detail.textColor = .secondaryLabelColor
    detail.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
    reload.target = self
    reload.action = #selector(reload(_:))
    reload.bezelStyle = .push
    reload.controlSize = .small
    let text = NSStackView(views: [message, detail])
    text.orientation = .vertical
    text.alignment = .leading
    text.spacing = 2
    let row = NSStackView(views: [icon, text, reload])
    row.orientation = .horizontal
    row.spacing = 8
    row.edgeInsets = NSEdgeInsets(top: 6, left: 12, bottom: 6, right: 12)
    row.translatesAutoresizingMaskIntoConstraints = false
    let separator = NSBox()
    separator.boxType = .separator
    separator.translatesAutoresizingMaskIntoConstraints = false
    addSubview(separator)
    addSubview(row)
    NSLayoutConstraint.activate([
      separator.topAnchor.constraint(equalTo: topAnchor),
      separator.leadingAnchor.constraint(equalTo: leadingAnchor),
      separator.trailingAnchor.constraint(equalTo: trailingAnchor),
      row.topAnchor.constraint(equalTo: separator.bottomAnchor),
      row.leadingAnchor.constraint(equalTo: leadingAnchor),
      row.trailingAnchor.constraint(equalTo: trailingAnchor),
      row.bottomAnchor.constraint(equalTo: bottomAnchor),
    ])
    setAccessibilityElement(true)
    setAccessibilityRole(.group)
    setAccessibilityLabel("Document notice")
    setAccessibilityIdentifier("notice-bar")
  }

  required init?(coder: NSCoder) { fatalError("The notice bar is constructed programmatically") }

  /// Paint the window background, like the find bar.
  override func draw(_ dirtyRect: NSRect) {
    NSColor.windowBackgroundColor.setFill()
    dirtyRect.fill()
  }

  func show(_ text: String, detail secondary: String) {
    message.stringValue = text
    detail.stringValue = secondary
  }

  @objc private func reload(_ sender: Any?) { onReload?() }
}
