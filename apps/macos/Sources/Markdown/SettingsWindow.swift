import AppKit
import MarkdownRender

/// Markdown → Settings… (Command-comma): appearance and text size. Changes
/// apply at once to every window and persist; the View menu commands change
/// the same settings.
@MainActor
final class SettingsWindowController: NSWindowController, NSWindowDelegate {
  let preferences: ReaderPreferences
  let appearance = NSPopUpButton()
  let textSize = NSSlider()
  let textSizeLabel = NSTextField(labelWithString: "")

  init(preferences: ReaderPreferences) {
    self.preferences = preferences
    let window = NSWindow(
      contentRect: NSRect(x: 0, y: 0, width: 420, height: 140),
      styleMask: [.titled, .closable], backing: .buffered, defer: false)
    window.title = "Settings"
    window.isReleasedWhenClosed = false
    window.tabbingMode = .disallowed
    super.init(window: window)

    for option in ReaderPreferences.Appearance.allCases {
      appearance.addItem(withTitle: option.title)
      appearance.lastItem?.representedObject = option.rawValue
    }
    appearance.target = self
    appearance.action = #selector(appearanceChanged(_:))
    appearance.setAccessibilityIdentifier("settings-appearance")

    textSize.minValue = 0
    textSize.maxValue = Double(TextScale.steps.count - 1)
    textSize.numberOfTickMarks = TextScale.steps.count
    textSize.allowsTickMarkValuesOnly = true
    textSize.target = self
    textSize.action = #selector(textSizeChanged(_:))
    textSize.setAccessibilityLabel("Text size")
    textSize.setAccessibilityIdentifier("settings-text-size")
    textSize.widthAnchor.constraint(greaterThanOrEqualToConstant: 200).isActive = true
    textSizeLabel.font = .monospacedDigitSystemFont(ofSize: NSFont.systemFontSize, weight: .regular)
    textSizeLabel.widthAnchor.constraint(equalToConstant: 48).isActive = true

    let sizeRow = NSStackView(views: [textSize, textSizeLabel])
    sizeRow.spacing = 8
    let grid = NSGridView(views: [
      [NSTextField(labelWithString: "Appearance:"), appearance],
      [NSTextField(labelWithString: "Text size:"), sizeRow],
    ])
    grid.column(at: 0).xPlacement = .trailing
    grid.rowAlignment = .firstBaseline
    grid.rowSpacing = 12
    grid.columnSpacing = 8
    grid.translatesAutoresizingMaskIntoConstraints = false
    let content = NSView()
    content.addSubview(grid)
    NSLayoutConstraint.activate([
      grid.topAnchor.constraint(equalTo: content.topAnchor, constant: 20),
      grid.bottomAnchor.constraint(equalTo: content.bottomAnchor, constant: -20),
      grid.leadingAnchor.constraint(equalTo: content.leadingAnchor, constant: 20),
      grid.trailingAnchor.constraint(equalTo: content.trailingAnchor, constant: -20),
    ])
    window.contentView = content
    window.center()
    refresh()
  }

  required init?(coder: NSCoder) { fatalError("Settings are constructed programmatically") }

  /// Show the stored settings, whichever control or command changed them.
  func refresh() {
    appearance.selectItem(
      at: ReaderPreferences.Appearance.allCases.firstIndex(of: preferences.appearance) ?? 0)
    let scale = preferences.textScale
    let step = TextScale.steps.firstIndex(of: scale) ?? 0
    textSize.integerValue = step
    let percent = "\(Int((scale * 100).rounded()))%"
    textSizeLabel.stringValue = percent
    textSize.setAccessibilityValueDescription(percent)
  }

  @objc private func appearanceChanged(_ sender: NSPopUpButton) {
    guard let value = sender.selectedItem?.representedObject as? String,
      let option = ReaderPreferences.Appearance(rawValue: value)
    else { return }
    preferences.appearance = option
  }

  @objc private func textSizeChanged(_ sender: NSSlider) {
    let step = min(max(sender.integerValue, 0), TextScale.steps.count - 1)
    preferences.textScale = TextScale.steps[step]
    refresh()
  }
}
