import AppKit
import MarkdownCore
import MarkdownDocument
import MarkdownRender

/// One document window: an outline sidebar beside the TextKit 2 reading
/// surface, with a centred status label for the empty, loading and error states.
@MainActor
final class ReaderWindowController: NSWindowController, NSWindowDelegate, NSMenuItemValidation {
  var onClose: ((ReaderWindowController) -> Void)?
  let loader = DocumentLoader()
  let surface = ReaderSurface()
  let outline = OutlineController()
  private let split = NSSplitViewController()
  private let status = NSTextField(labelWithString: "No document open")
  private let detail = NSTextField(wrappingLabelWithString: "")
  private let statusStack: NSStackView
  private var renderTask: Task<Void, Never>?
  private var renderRequest: UInt64 = 0
  /// The title of the presented document, once its text is shown.
  private(set) var documentTitle: String?
  private var shownCapability: DocumentCapability?

  init() {
    let window = NSWindow(
      contentRect: NSRect(x: 0, y: 0, width: 900, height: 640),
      styleMask: [.titled, .closable, .miniaturizable, .resizable, .fullSizeContentView],
      backing: .buffered, defer: false
    )
    window.title = "Markdown"
    window.minSize = NSSize(width: 480, height: 320)
    window.isReleasedWhenClosed = false
    window.tabbingMode = .disallowed

    status.font = .systemFont(ofSize: 22, weight: .semibold)
    status.textColor = .secondaryLabelColor
    status.alignment = .center
    status.setAccessibilityIdentifier("document-status-label")
    detail.font = .systemFont(ofSize: 13)
    detail.textColor = .secondaryLabelColor
    detail.alignment = .center
    detail.isHidden = true
    detail.setAccessibilityIdentifier("document-detail-label")
    statusStack = NSStackView(views: [status, detail])
    statusStack.orientation = .vertical
    statusStack.spacing = 8
    statusStack.translatesAutoresizingMaskIntoConstraints = false

    let content = NSView()
    surface.scrollView.translatesAutoresizingMaskIntoConstraints = false
    surface.scrollView.isHidden = true
    content.addSubview(surface.scrollView)
    content.addSubview(statusStack)
    NSLayoutConstraint.activate([
      surface.scrollView.topAnchor.constraint(equalTo: content.safeAreaLayoutGuide.topAnchor),
      surface.scrollView.bottomAnchor.constraint(equalTo: content.bottomAnchor),
      surface.scrollView.leadingAnchor.constraint(equalTo: content.leadingAnchor),
      surface.scrollView.trailingAnchor.constraint(equalTo: content.trailingAnchor),
      statusStack.centerXAnchor.constraint(equalTo: content.centerXAnchor),
      statusStack.centerYAnchor.constraint(equalTo: content.centerYAnchor),
      statusStack.widthAnchor.constraint(lessThanOrEqualTo: content.widthAnchor, constant: -48),
      detail.widthAnchor.constraint(lessThanOrEqualToConstant: 480),
    ])
    let documentController = NSViewController()
    documentController.view = content
    let sidebar = NSSplitViewItem(sidebarWithViewController: outline)
    sidebar.minimumThickness = 160
    sidebar.maximumThickness = 360
    split.addSplitViewItem(sidebar)
    split.addSplitViewItem(NSSplitViewItem(viewController: documentController))
    split.splitView.autosaveName = nil
    window.contentViewController = split
    window.setContentSize(NSSize(width: 900, height: 640))
    window.center()
    super.init(window: window)
    window.delegate = self
    outline.onSelect = { [weak self] index in self?.surface.reveal(heading: index) }
    surface.onNavigate = { [weak self] index in self?.outline.select(index) }
    loader.onChange = { [weak self] state in self?.present(state) }
  }

  required init?(coder: NSCoder) {
    fatalError("Reader windows are constructed programmatically")
  }

  var statusText: String { status.stringValue }
  var detailText: String { detail.stringValue }
  /// Whether a load or the off-main rendering of its result is still running.
  var isBusy: Bool { loader.isLoading || renderTask != nil }
  var isSidebarCollapsed: Bool { split.splitViewItems[0].isCollapsed }

  private func present(_ state: DocumentLoader.State) {
    renderRequest &+= 1
    renderTask?.cancel()
    renderTask = nil
    switch state {
    case .empty:
      showStatus("No document open", nil, title: "Markdown", url: nil)
    case .loading(let capability):
      // Keep the previous text visible only while reloading the same document.
      if capability != shownCapability {
        showStatus("Loading…", nil, title: capability.displayName, url: capability.url)
      }
    case .loaded(let loaded):
      render(loaded)
    case .failed(let capability, let error):
      showStatus(error.message, error.recovery, title: capability.displayName, url: nil)
    }
  }

  /// Build the attributed text off the main actor; only the newest request
  /// installs its result, so a slow render never replaces newer content.
  private func render(_ loaded: LoadedDocument) {
    let id = renderRequest
    let document = loaded.document
    let capability = loaded.capability
    let work = Task.detached(priority: .userInitiated) {
      // Resolving the folder touches the filesystem, so it stays off the main thread.
      let images = Self.imageLoader(DocumentResources(document: capability))
      return try DocumentRenderer.render(document, images: images)
    }
    renderTask = Task { [weak self] in
      // Cancelling the window's task (a newer state, or close) stops the render.
      let result = await withTaskCancellationHandler {
        await work.result
      } onCancel: {
        work.cancel()
      }
      guard let self, id == self.renderRequest else { return }
      self.renderTask = nil
      switch result {
      case .success(let rendered):
        self.install(rendered, title: document.title ?? loaded.capability.displayName, loaded)
      case .failure:
        let error = DocumentError.parseFailed
        self.showStatus(
          error.message, error.recovery, title: loaded.capability.displayName, url: nil)
      }
    }
  }

  /// Images are read only from the document's folder (MAC-001E).
  nonisolated static func imageLoader(_ resources: DocumentResources) -> ImageLoader {
    { path, limit throws(ImageFailure) in
      do throws(ResourceError) {
        return try resources.readImage(path, limit: limit)
      } catch {
        switch error {
        case .outsideFolder: throw .outsideFolder
        case .notFound: throw .notFound
        case .permissionDenied: throw .permissionDenied
        case .notRegularFile, .unsupportedType: throw .unsupportedType
        case .tooLarge: throw .tooLarge
        case .unreadable: throw .undecodable
        }
      }
    }
  }

  private func install(_ rendered: RenderedDocument, title: String, _ loaded: LoadedDocument) {
    documentTitle = title
    shownCapability = loaded.capability
    surface.show(rendered)
    outline.show(rendered.headings)
    surface.scrollView.isHidden = false
    statusStack.isHidden = true
    window?.title = loaded.capability.displayName
    window?.representedURL = loaded.capability.url
  }

  private func showStatus(_ primary: String, _ secondary: String?, title: String, url: URL?) {
    documentTitle = nil
    shownCapability = nil
    surface.show(nil)
    outline.show([])
    surface.scrollView.isHidden = true
    statusStack.isHidden = false
    status.stringValue = primary
    detail.stringValue = secondary ?? ""
    detail.isHidden = secondary == nil
    window?.title = title
    window?.representedURL = url
  }

  @objc func reloadDocument(_ sender: Any?) {
    loader.reload()
  }

  func validateMenuItem(_ menuItem: NSMenuItem) -> Bool {
    if menuItem.action == #selector(reloadDocument(_:)) { return loader.capability != nil }
    return true
  }

  func windowWillClose(_ notification: Notification) {
    renderRequest &+= 1
    renderTask?.cancel()
    renderTask = nil
    let loader = loader
    Task { await loader.close() }
    onClose?(self)
  }
}

/// The heading outline: a source-list table, indented by level. Choosing a
/// row (click or arrow keys) navigates the reading surface to that heading.
@MainActor
final class OutlineController: NSViewController, NSTableViewDataSource, NSTableViewDelegate {
  var onSelect: ((Int) -> Void)?
  private(set) var headings: [RenderedHeading] = []
  let table = NSTableView()
  private var selecting = false

  override func loadView() {
    let column = NSTableColumn(identifier: NSUserInterfaceItemIdentifier("heading"))
    table.addTableColumn(column)
    table.headerView = nil
    table.style = .sourceList
    table.rowSizeStyle = .default
    table.allowsEmptySelection = true
    table.dataSource = self
    // Selection changes only fire for a new row; a click on the selected row
    // must still navigate back to its heading.
    table.target = self
    table.action = #selector(rowClicked(_:))
    table.delegate = self
    table.setAccessibilityIdentifier("document-outline")
    table.setAccessibilityLabel("Outline")
    let scroll = NSScrollView()
    scroll.documentView = table
    scroll.hasVerticalScroller = true
    scroll.autohidesScrollers = true
    scroll.drawsBackground = false
    view = scroll
  }

  func show(_ headings: [RenderedHeading]) {
    self.headings = headings
    _ = view
    table.reloadData()
  }

  /// Reflect navigation from the document (links, restoration) without
  /// navigating again.
  func select(_ index: Int) {
    guard headings.indices.contains(index), table.selectedRow != index else { return }
    selecting = true
    table.selectRowIndexes(IndexSet(integer: index), byExtendingSelection: false)
    table.scrollRowToVisible(index)
    selecting = false
  }

  func numberOfRows(in tableView: NSTableView) -> Int { headings.count }

  func tableView(_ tableView: NSTableView, viewFor tableColumn: NSTableColumn?, row: Int)
    -> NSView?
  {
    let identifier = NSUserInterfaceItemIdentifier("heading-cell")
    let cell =
      tableView.makeView(withIdentifier: identifier, owner: nil) as? OutlineCell
      ?? OutlineCell(identifier: identifier)
    cell.configure(headings[row])
    return cell
  }

  @objc private func rowClicked(_ sender: Any?) {
    guard headings.indices.contains(table.clickedRow) else { return }
    onSelect?(table.clickedRow)
  }

  func tableViewSelectionDidChange(_ notification: Notification) {
    guard !selecting, headings.indices.contains(table.selectedRow) else { return }
    onSelect?(table.selectedRow)
  }
}

private final class OutlineCell: NSTableCellView {
  private let label = NSTextField(labelWithString: "")
  private var leading: NSLayoutConstraint!

  init(identifier: NSUserInterfaceItemIdentifier) {
    super.init(frame: .zero)
    self.identifier = identifier
    label.lineBreakMode = .byTruncatingTail
    label.translatesAutoresizingMaskIntoConstraints = false
    addSubview(label)
    textField = label
    leading = label.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 4)
    NSLayoutConstraint.activate([
      leading, label.trailingAnchor.constraint(lessThanOrEqualTo: trailingAnchor, constant: -4),
      label.centerYAnchor.constraint(equalTo: centerYAnchor),
    ])
  }

  required init?(coder: NSCoder) { fatalError("Outline cells are constructed programmatically") }

  func configure(_ heading: RenderedHeading) {
    label.stringValue = heading.title
    label.font = heading.level == 1 ? .boldSystemFont(ofSize: 13) : .systemFont(ofSize: 13)
    label.toolTip = heading.title
    leading.constant = 4 + CGFloat(min(heading.level, 6) - 1) * 12
    setAccessibilityLabel("Heading level \(heading.level), \(heading.title)")
  }
}
