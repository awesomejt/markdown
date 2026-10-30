import AppKit
import MarkdownCore

/// The document find bar above the reading surface: a search field, previous
/// and next buttons, the match status and the case and whole-word options.
/// Return finds the next match, Shift-Return the previous one and Escape
/// closes the bar. The window controller runs the searches.
@MainActor
final class FindBar: NSView, NSSearchFieldDelegate {
  let field = NSSearchField()
  let navigation = NSSegmentedControl()
  let status = NSTextField(labelWithString: "")
  let matchCase = NSButton(checkboxWithTitle: "Match Case", target: nil, action: nil)
  let wholeWords = NSButton(checkboxWithTitle: "Whole Words", target: nil, action: nil)
  let done = NSButton(title: "Done", target: nil, action: nil)

  /// The query or an option changed.
  var onSearch: (() -> Void)?
  /// Go to the next match, or the previous one when the argument is true.
  var onNavigate: ((Bool) -> Void)?
  var onClose: (() -> Void)?

  init() {
    super.init(frame: .zero)
    field.placeholderString = "Find in Document"
    field.setAccessibilityLabel("Find in document")
    field.setAccessibilityIdentifier("find-field")
    field.delegate = self
    field.target = self
    field.action = #selector(queryChanged(_:))
    field.sendsWholeSearchString = false
    field.translatesAutoresizingMaskIntoConstraints = false
    field.widthAnchor.constraint(greaterThanOrEqualToConstant: 160).isActive = true
    field.widthAnchor.constraint(lessThanOrEqualToConstant: 320).isActive = true

    navigation.segmentCount = 2
    navigation.trackingMode = .momentary
    navigation.segmentStyle = .separated
    for (segment, name, label) in [
      (0, "chevron.left", "Previous Match"), (1, "chevron.right", "Next Match"),
    ] {
      navigation.setImage(
        NSImage(systemSymbolName: name, accessibilityDescription: label), forSegment: segment)
      navigation.setToolTip(label, forSegment: segment)
      navigation.setWidth(28, forSegment: segment)
    }
    navigation.target = self
    navigation.action = #selector(navigate(_:))
    navigation.setAccessibilityLabel("Previous and next match")

    status.textColor = .secondaryLabelColor
    status.font = .monospacedDigitSystemFont(ofSize: NSFont.smallSystemFontSize, weight: .regular)
    status.setAccessibilityIdentifier("find-status")
    status.setContentCompressionResistancePriority(.defaultHigh, for: .horizontal)
    for option in [matchCase, wholeWords] {
      option.target = self
      option.action = #selector(optionChanged(_:))
      option.controlSize = .small
      option.font = .systemFont(ofSize: NSFont.smallSystemFontSize)
    }
    done.target = self
    done.action = #selector(close(_:))
    done.bezelStyle = .push
    done.controlSize = .small

    let spacer = NSView()
    spacer.setContentHuggingPriority(.init(1), for: .horizontal)
    let row = NSStackView(views: [field, navigation, status, spacer, matchCase, wholeWords, done])
    row.orientation = .horizontal
    row.spacing = 8
    row.edgeInsets = NSEdgeInsets(top: 6, left: 12, bottom: 6, right: 12)
    row.setHuggingPriority(.defaultHigh, for: .vertical)
    row.translatesAutoresizingMaskIntoConstraints = false
    let separator = NSBox()
    separator.boxType = .separator
    separator.translatesAutoresizingMaskIntoConstraints = false
    addSubview(row)
    addSubview(separator)
    NSLayoutConstraint.activate([
      row.topAnchor.constraint(equalTo: topAnchor),
      row.leadingAnchor.constraint(equalTo: leadingAnchor),
      row.trailingAnchor.constraint(equalTo: trailingAnchor),
      separator.topAnchor.constraint(equalTo: row.bottomAnchor),
      separator.leadingAnchor.constraint(equalTo: leadingAnchor),
      separator.trailingAnchor.constraint(equalTo: trailingAnchor),
      separator.bottomAnchor.constraint(equalTo: bottomAnchor),
    ])
    setAccessibilityElement(true)
    setAccessibilityRole(.group)
    setAccessibilityLabel("Find")
    setAccessibilityIdentifier("find-bar")
  }

  required init?(coder: NSCoder) { fatalError("The find bar is constructed programmatically") }

  /// The bar paints the window background itself, so it reads the same over
  /// any content behind it in either appearance.
  override func draw(_ dirtyRect: NSRect) {
    NSColor.windowBackgroundColor.setFill()
    dirtyRect.fill()
  }

  var query: String { field.stringValue }

  var options: CoreFindOptions {
    CoreFindOptions(caseSensitive: matchCase.state == .on, wholeWord: wholeWords.state == .on)
  }

  @objc private func queryChanged(_ sender: Any?) { onSearch?() }
  @objc private func optionChanged(_ sender: Any?) { onSearch?() }
  @objc private func close(_ sender: Any?) { onClose?() }

  @objc private func navigate(_ sender: NSSegmentedControl) {
    onNavigate?(sender.selectedSegment == 0)
  }

  func control(_ control: NSControl, textView: NSTextView, doCommandBy selector: Selector) -> Bool {
    switch selector {
    case #selector(NSResponder.insertNewline(_:)):
      onNavigate?(NSApp.currentEvent?.modifierFlags.contains(.shift) == true)
      return true
    case #selector(NSResponder.insertNewlineIgnoringFieldEditor(_:)):
      onNavigate?(true)
      return true
    case #selector(NSResponder.cancelOperation(_:)):
      onClose?()
      return true
    default:
      return false
    }
  }
}
