import AppKit
import MarkdownRender

/// Markdown → Settings… (Command-comma): appearance, text size, the sidebar
/// in new windows and reopening windows at launch (MAC-001F, MAC-002D).
/// Changes apply at once to every window and persist; the View menu
/// commands change the same settings, and Restore Defaults clears them.
@MainActor
final class SettingsWindowController: NSWindowController, NSWindowDelegate {
  let preferences: ReaderPreferences
  let appearance = NSPopUpButton()
  let textSize = NSSlider()
  let textSizeLabel = NSTextField(labelWithString: "")
  let showsSidebar = NSButton(
    checkboxWithTitle: "Show the sidebar in new windows", target: nil, action: nil)
  let reopensWindows = NSButton(
    checkboxWithTitle: "Reopen windows from last time", target: nil, action: nil)
  let restoreDefaults = NSButton(title: "Restore Defaults", target: nil, action: nil)

  init(preferences: ReaderPreferences) {
    self.preferences = preferences
    let window = NSWindow(
      contentRect: NSRect(x: 0, y: 0, width: 460, height: 260),
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

    for (button, action, identifier) in [
      (showsSidebar, #selector(showsSidebarChanged(_:)), "settings-shows-sidebar"),
      (reopensWindows, #selector(reopensWindowsChanged(_:)), "settings-reopens-windows"),
      (restoreDefaults, #selector(restoreDefaultsChosen(_:)), "settings-restore-defaults"),
    ] {
      button.target = self
      button.action = action
      button.setAccessibilityIdentifier(identifier)
    }
    let reopenNote = NSTextField(
      wrappingLabelWithString:
        "Windows also stay closed when “Close windows when quitting an application” is on in "
        + "System Settings ▸ Desktop & Dock.")
    reopenNote.font = .systemFont(ofSize: NSFont.smallSystemFontSize)
    reopenNote.textColor = .secondaryLabelColor
    reopenNote.preferredMaxLayoutWidth = 300

    let sizeRow = NSStackView(views: [textSize, textSizeLabel])
    sizeRow.spacing = 8
    let reopenColumn = NSStackView(views: [reopensWindows, reopenNote])
    reopenColumn.orientation = .vertical
    reopenColumn.alignment = .leading
    reopenColumn.spacing = 4
    let grid = NSGridView(views: [
      [NSTextField(labelWithString: "Appearance:"), appearance],
      [NSTextField(labelWithString: "Text size:"), sizeRow],
      [NSTextField(labelWithString: "Sidebar:"), showsSidebar],
      [NSTextField(labelWithString: "At launch:"), reopenColumn],
      [NSGridCell.emptyContentView, restoreDefaults],
    ])
    grid.cell(for: restoreDefaults)?.xPlacement = .trailing
    grid.row(at: 4).topPadding = 8
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
    showsSidebar.state = preferences.showsSidebar ? .on : .off
    reopensWindows.state = preferences.reopensWindows ? .on : .off
    restoreDefaults.isEnabled = !preferences.isDefault
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

  @objc private func showsSidebarChanged(_ sender: NSButton) {
    preferences.showsSidebar = sender.state == .on
  }

  @objc private func reopensWindowsChanged(_ sender: NSButton) {
    preferences.reopensWindows = sender.state == .on
  }

  @objc private func restoreDefaultsChosen(_ sender: Any?) {
    preferences.restoreDefaults()
    refresh()
  }
}
