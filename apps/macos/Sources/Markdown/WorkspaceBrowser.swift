import AppKit
import MarkdownDocument

/// The window's sidebar: the folder's files, the document's outline and the
/// folder search, switched by a segmented control that only appears with a
/// folder (MAC-002B, MAC-002C).
@MainActor
final class SidebarController: NSViewController {
  enum Mode: Int { case files, outline, search }

  let files: WorkspaceBrowserController
  let outline: OutlineController
  let search: WorkspaceSearchController
  let picker = NSSegmentedControl(
    labels: ["Files", "Outline", "Search"], trackingMode: .selectOne, target: nil, action: nil)
  private(set) var mode: Mode = .outline
  /// Whether the window has a folder, so the files can be shown.
  private(set) var hasFiles = false
  private var contentTop: NSLayoutConstraint!
  private var contentBelowPicker: NSLayoutConstraint!

  init(
    files: WorkspaceBrowserController, outline: OutlineController,
    search: WorkspaceSearchController
  ) {
    self.files = files
    self.outline = outline
    self.search = search
    super.init(nibName: nil, bundle: nil)
  }

  required init?(coder: NSCoder) { fatalError("The sidebar is constructed programmatically") }

  override func loadView() {
    let view = NSView()
    picker.target = self
    picker.action = #selector(pick(_:))
    picker.segmentDistribution = .fillEqually
    picker.controlSize = .small
    picker.translatesAutoresizingMaskIntoConstraints = false
    picker.setAccessibilityIdentifier("sidebar-picker")
    picker.setAccessibilityLabel("Sidebar")
    picker.setToolTip("Files in the folder (Option-Command-1)", forSegment: 0)
    picker.setToolTip("Document outline (Option-Command-2)", forSegment: 1)
    picker.setToolTip("Search the folder (Option-Command-3)", forSegment: 2)
    picker.isHidden = true
    view.addSubview(picker)
    for child in [files, outline, search] as [NSViewController] {
      addChild(child)
      child.view.translatesAutoresizingMaskIntoConstraints = false
      view.addSubview(child.view)
      NSLayoutConstraint.activate([
        child.view.leadingAnchor.constraint(equalTo: view.leadingAnchor),
        child.view.trailingAnchor.constraint(equalTo: view.trailingAnchor),
        child.view.bottomAnchor.constraint(equalTo: view.bottomAnchor),
      ])
    }
    contentTop = files.view.topAnchor.constraint(equalTo: view.safeAreaLayoutGuide.topAnchor)
    contentBelowPicker = files.view.topAnchor.constraint(equalTo: picker.bottomAnchor, constant: 6)
    NSLayoutConstraint.activate([
      picker.topAnchor.constraint(equalTo: view.safeAreaLayoutGuide.topAnchor, constant: 4),
      picker.leadingAnchor.constraint(equalTo: view.leadingAnchor, constant: 8),
      picker.trailingAnchor.constraint(equalTo: view.trailingAnchor, constant: -8),
      outline.view.topAnchor.constraint(equalTo: files.view.topAnchor),
      search.view.topAnchor.constraint(equalTo: files.view.topAnchor),
      contentTop,
    ])
    self.view = view
    apply()
  }

  /// Offer the files (a folder was granted) or only the outline.
  func setHasFiles(_ value: Bool) {
    guard value != hasFiles else { return }
    hasFiles = value
    mode = value ? .files : .outline
    apply()
  }

  /// Show the files, the outline or the search; all but the outline need a folder.
  func show(_ mode: Mode) {
    guard mode == .outline || hasFiles else { return }
    self.mode = mode
    apply()
  }

  @objc private func pick(_ sender: NSSegmentedControl) {
    show(Mode(rawValue: sender.selectedSegment) ?? .outline)
    switch mode {
    case .files: view.window?.makeFirstResponder(files.outlineView)
    case .outline: view.window?.makeFirstResponder(outline.table)
    case .search: search.focus()
    }
  }

  private func apply() {
    guard isViewLoaded else { return }
    picker.isHidden = !hasFiles
    contentTop.isActive = !hasFiles
    contentBelowPicker.isActive = hasFiles
    picker.selectedSegment = mode.rawValue
    files.view.isHidden = mode != .files
    outline.view.isHidden = mode != .outline
    search.view.isHidden = mode != .search
  }
}

/// The folder's Markdown documents as a native outline (MAC-002B). The
/// window lists the folder off the main thread; this view shows the
/// immutable listing. NSOutlineView only asks for the rows it shows, so a
/// large folder costs little until it is expanded.
///
/// Keyboard: the arrow keys move and expand or collapse (Right/Left), type
/// selection finds a name, and Return or Enter opens the selected document
/// or toggles the selected folder. A click on a document opens it.
@MainActor
final class WorkspaceBrowserController: NSViewController, NSOutlineViewDataSource,
  NSOutlineViewDelegate
{
  /// Open a document; a folder row is never passed.
  var onOpen: ((WorkspaceItem) -> Void)?
  private(set) var listing: WorkspaceListing?
  let outlineView = WorkspaceOutlineView()
  private let message = NSTextField(wrappingLabelWithString: "")
  private let footer = NSTextField(wrappingLabelWithString: "")
  /// Selecting a row from the document (reveal) must not open it again.
  private var revealing = false

  override func loadView() {
    let column = NSTableColumn(identifier: NSUserInterfaceItemIdentifier("name"))
    outlineView.addTableColumn(column)
    outlineView.outlineTableColumn = column
    outlineView.headerView = nil
    outlineView.style = .sourceList
    outlineView.rowSizeStyle = .default
    outlineView.allowsEmptySelection = true
    outlineView.autosaveExpandedItems = false
    outlineView.dataSource = self
    outlineView.delegate = self
    outlineView.target = self
    outlineView.action = #selector(rowClicked(_:))
    outlineView.doubleAction = #selector(rowDoubleClicked(_:))
    outlineView.onActivate = { [weak self] in self?.activateSelection() }
    outlineView.setAccessibilityIdentifier("workspace-browser")
    outlineView.setAccessibilityLabel("Files")
    let scroll = NSScrollView()
    scroll.documentView = outlineView
    scroll.hasVerticalScroller = true
    scroll.autohidesScrollers = true
    scroll.drawsBackground = false
    scroll.translatesAutoresizingMaskIntoConstraints = false

    for label in [message, footer] {
      label.font = .systemFont(ofSize: NSFont.smallSystemFontSize)
      label.textColor = .secondaryLabelColor
      label.translatesAutoresizingMaskIntoConstraints = false
      label.isHidden = true
    }
    message.alignment = .center
    message.setAccessibilityIdentifier("workspace-message")
    footer.setAccessibilityIdentifier("workspace-footer")

    let view = NSView()
    view.addSubview(scroll)
    view.addSubview(message)
    view.addSubview(footer)
    NSLayoutConstraint.activate([
      scroll.topAnchor.constraint(equalTo: view.topAnchor),
      scroll.leadingAnchor.constraint(equalTo: view.leadingAnchor),
      scroll.trailingAnchor.constraint(equalTo: view.trailingAnchor),
      footer.topAnchor.constraint(equalTo: scroll.bottomAnchor, constant: 4),
      footer.leadingAnchor.constraint(equalTo: view.leadingAnchor, constant: 10),
      footer.trailingAnchor.constraint(equalTo: view.trailingAnchor, constant: -10),
      footer.bottomAnchor.constraint(equalTo: view.bottomAnchor, constant: -6),
      message.centerYAnchor.constraint(equalTo: scroll.centerYAnchor),
      message.leadingAnchor.constraint(equalTo: view.leadingAnchor, constant: 12),
      message.trailingAnchor.constraint(equalTo: view.trailingAnchor, constant: -12),
    ])
    // The footer takes no room while hidden.
    footer.setContentCompressionResistancePriority(.required, for: .vertical)
    self.view = view
  }

  var messageText: String? { message.isHidden ? nil : message.stringValue }
  var footerText: String? { footer.isHidden ? nil : footer.stringValue }

  /// Show a listing, keeping the folders the reader had expanded and the
  /// selection where they still exist.
  func show(_ listing: WorkspaceListing) {
    _ = view
    let expanded = expandedPaths()
    let selected = selectedItem?.path
    self.listing = listing
    outlineView.reloadData()
    for path in expanded.sorted(by: { $0.count < $1.count }) {
      if let item = listing.item(at: path) { outlineView.expandItem(item) }
    }
    if let selected { reveal(selected) }
    let empty = listing.items.isEmpty
    setMessage(
      empty ? (listing.options.allFiles ? "This folder is empty." : "No Markdown documents") : nil)
    var notes: [String] = []
    if listing.isTruncated {
      notes.append(
        "Showing part of the folder: it has more than \(listing.options.maxEntries.formatted()) items or is nested too deeply."
      )
    }
    if listing.skippedInvalid > 0 {
      notes.append(
        listing.skippedInvalid == 1
          ? "1 item with an unsupported name isn’t shown."
          : "\(listing.skippedInvalid.formatted()) items with unsupported names aren’t shown.")
    }
    footer.stringValue = notes.joined(separator: " ")
    footer.isHidden = notes.isEmpty
  }

  /// No listing: the window has no folder, or it is being listed or failed.
  func clear(message text: String?) {
    _ = view
    listing = nil
    outlineView.reloadData()
    footer.isHidden = true
    setMessage(text)
  }

  private func setMessage(_ text: String?) {
    message.stringValue = text ?? ""
    message.isHidden = text == nil
  }

  private func expandedPaths() -> [String] {
    (0..<outlineView.numberOfRows).compactMap {
      guard let item = outlineView.item(atRow: $0) as? WorkspaceItem,
        outlineView.isItemExpanded(item)
      else { return nil }
      return item.path
    }
  }

  var selectedItem: WorkspaceItem? {
    outlineView.item(atRow: outlineView.selectedRow) as? WorkspaceItem
  }

  /// Select the item at `path`, expanding the folders above it, without
  /// opening it. Clears the selection when it isn't listed.
  func reveal(_ path: String?) {
    guard let listing, let path, let item = listing.item(at: path) else {
      outlineView.deselectAll(nil)
      return
    }
    for ancestor in WorkspaceListing.ancestors(of: path) {
      if let folder = listing.item(at: ancestor) { outlineView.expandItem(folder) }
    }
    let row = outlineView.row(forItem: item)
    guard row >= 0 else { return }
    revealing = true
    outlineView.selectRowIndexes(IndexSet(integer: row), byExtendingSelection: false)
    outlineView.scrollRowToVisible(row)
    revealing = false
  }

  /// Open the selected document, or expand or collapse the selected folder.
  func activateSelection() {
    guard let item = selectedItem else { return }
    activate(item)
  }

  private func activate(_ item: WorkspaceItem) {
    switch item.kind {
    case .file:
      onOpen?(item)
    case .folder:
      if outlineView.isItemExpanded(item) {
        outlineView.collapseItem(item)
      } else {
        outlineView.expandItem(item)
      }
    }
  }

  @objc private func rowClicked(_ sender: Any?) {
    guard !revealing, let item = outlineView.item(atRow: outlineView.clickedRow) as? WorkspaceItem,
      item.kind == .file
    else { return }
    onOpen?(item)
  }

  @objc private func rowDoubleClicked(_ sender: Any?) {
    guard let item = outlineView.item(atRow: outlineView.clickedRow) as? WorkspaceItem,
      item.kind == .folder
    else { return }
    activate(item)
  }

  // MARK: Data source

  func outlineView(_ outlineView: NSOutlineView, numberOfChildrenOfItem item: Any?) -> Int {
    guard let listing else { return 0 }
    return (item as? WorkspaceItem)?.children.count ?? listing.items.count
  }

  func outlineView(_ outlineView: NSOutlineView, child index: Int, ofItem item: Any?) -> Any {
    (item as? WorkspaceItem)?.children[index] ?? listing!.items[index]
  }

  func outlineView(_ outlineView: NSOutlineView, isItemExpandable item: Any) -> Bool {
    (item as? WorkspaceItem).map { $0.kind == .folder && !$0.children.isEmpty } ?? false
  }

  func outlineView(_ outlineView: NSOutlineView, viewFor tableColumn: NSTableColumn?, item: Any)
    -> NSView?
  {
    guard let item = item as? WorkspaceItem else { return nil }
    let identifier = NSUserInterfaceItemIdentifier("workspace-cell")
    let cell =
      outlineView.makeView(withIdentifier: identifier, owner: nil) as? WorkspaceCell
      ?? WorkspaceCell(identifier: identifier)
    cell.configure(item)
    return cell
  }

  func outlineView(
    _ outlineView: NSOutlineView, typeSelectStringFor tableColumn: NSTableColumn?, item: Any
  )
    -> String?
  {
    (item as? WorkspaceItem)?.name
  }
}

/// Return and Enter open the selection; everything else is NSOutlineView's.
final class WorkspaceOutlineView: NSOutlineView {
  var onActivate: (() -> Void)?

  override func keyDown(with event: NSEvent) {
    switch event.charactersIgnoringModifiers?.unicodeScalars.first?.value {
    case 0x0D, 0x03:  // Return, Enter
      onActivate?()
    default:
      super.keyDown(with: event)
    }
  }
}

/// A file or folder row: its symbol and name. A name with invisible
/// characters is marked, visibly and for VoiceOver, since it can look like
/// another name; an unreadable folder says so.
private final class WorkspaceCell: NSTableCellView {
  private let icon = NSImageView()
  private let label = NSTextField(labelWithString: "")

  init(identifier: NSUserInterfaceItemIdentifier) {
    super.init(frame: .zero)
    self.identifier = identifier
    icon.translatesAutoresizingMaskIntoConstraints = false
    label.lineBreakMode = .byTruncatingMiddle
    label.translatesAutoresizingMaskIntoConstraints = false
    addSubview(icon)
    addSubview(label)
    imageView = icon
    textField = label
    NSLayoutConstraint.activate([
      icon.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 2),
      icon.centerYAnchor.constraint(equalTo: centerYAnchor),
      icon.widthAnchor.constraint(equalToConstant: 16),
      icon.heightAnchor.constraint(equalToConstant: 16),
      label.leadingAnchor.constraint(equalTo: icon.trailingAnchor, constant: 5),
      label.trailingAnchor.constraint(lessThanOrEqualTo: trailingAnchor, constant: -4),
      label.centerYAnchor.constraint(equalTo: centerYAnchor),
    ])
  }

  required init?(coder: NSCoder) { fatalError("Workspace cells are constructed programmatically") }

  func configure(_ item: WorkspaceItem) {
    let warning = item.hasInvisible || item.isUnreadable
    let symbol =
      warning
      ? "exclamationmark.triangle"
      : item.kind == .folder ? "folder" : item.isMarkdown ? "doc.text" : "doc"
    icon.image = NSImage(systemSymbolName: symbol, accessibilityDescription: nil)
    icon.contentTintColor = warning ? .systemOrange : nil
    label.stringValue = item.name
    var notes: [String] = []
    if item.hasInvisible { notes.append("The name contains invisible characters") }
    if item.isUnreadable { notes.append("The folder can’t be read") }
    label.toolTip = notes.isEmpty ? item.name : ([item.name] + notes).joined(separator: "\n")
    label.textColor = item.isUnreadable ? .secondaryLabelColor : .labelColor
    let kind = item.kind == .folder ? "folder" : "document"
    setAccessibilityLabel(([item.name, kind] + notes).joined(separator: ", "))
  }
}
