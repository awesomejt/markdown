import AppKit
import MarkdownCore
import MarkdownDocument
import MarkdownRender

/// One document window: an outline sidebar beside the TextKit 2 reading
/// surface and its find bar, with a centred status label for the empty,
/// loading and error states.
@MainActor
final class ReaderWindowController: NSWindowController, NSWindowDelegate, NSMenuItemValidation {
  var onClose: ((ReaderWindowController) -> Void)?
  let loader = DocumentLoader()
  let surface = ReaderSurface()
  let outline = OutlineController()
  let findBar = FindBar()
  let preferences: ReaderPreferences
  /// The matches for the find bar's current query, options and document.
  private(set) var findMatches: FindMatches?
  private var findKey: FindKey?
  private var findTask: Task<Void, Never>?
  private var findRequest: UInt64 = 0
  /// The document selection when the current match was shown; navigation
  /// continues from that match until the reader moves the selection.
  private var selectionAtMatch: NSRange?
  /// The key of the running search, and the step to take when it finishes.
  private var findTaskKey: FindKey?
  private var pendingStep: Bool?
  /// The reading surface starts below the safe area, or below a visible find bar.
  private var surfaceTop: NSLayoutConstraint!
  private var surfaceBelowFindBar: NSLayoutConstraint!
  private let split = NSSplitViewController()
  private let status = NSTextField(labelWithString: "No document open")
  private let detail = NSTextField(wrappingLabelWithString: "")
  private let statusStack: NSStackView
  private var renderTask: Task<Void, Never>?
  private var renderRequest: UInt64 = 0
  /// The title of the presented document, once its text is shown.
  private(set) var documentTitle: String?
  private var shownCapability: DocumentCapability?

  init(preferences: ReaderPreferences) {
    self.preferences = preferences
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
    findBar.translatesAutoresizingMaskIntoConstraints = false
    findBar.isHidden = true
    content.addSubview(surface.scrollView)
    content.addSubview(findBar)
    content.addSubview(statusStack)
    NSLayoutConstraint.activate([
      findBar.topAnchor.constraint(equalTo: content.safeAreaLayoutGuide.topAnchor),
      findBar.leadingAnchor.constraint(equalTo: content.leadingAnchor),
      findBar.trailingAnchor.constraint(equalTo: content.trailingAnchor),
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
    surfaceTop = surface.scrollView.topAnchor.constraint(
      equalTo: content.safeAreaLayoutGuide.topAnchor)
    surfaceBelowFindBar = surface.scrollView.topAnchor.constraint(equalTo: findBar.bottomAnchor)
    surfaceTop.isActive = true
    window.contentViewController = split
    window.setContentSize(NSSize(width: 900, height: 640))
    window.center()
    super.init(window: window)
    window.delegate = self
    outline.onSelect = { [weak self] index in self?.surface.reveal(heading: index) }
    surface.onNavigate = { [weak self] index in self?.outline.select(index) }
    loader.onChange = { [weak self] state in self?.present(state) }
    findBar.onSearch = { [weak self] in self?.search(then: nil) }
    findBar.onNavigate = { [weak self] backwards in self?.step(backwards: backwards) }
    findBar.onClose = { [weak self] in self?.closeFindBar() }
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
  private func render(_ loaded: LoadedDocument, restyle: Bool = false) {
    let id = renderRequest
    let document = loaded.document
    let capability = loaded.capability
    let scale = preferences.textScale
    let work = Task.detached(priority: .userInitiated) {
      // Resolving the folder touches the filesystem, so it stays off the main thread.
      let images = Self.imageLoader(DocumentResources(document: capability))
      return try DocumentRenderer.render(document, images: images, scale: scale)
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
      case .success(let rendered) where restyle:
        self.surface.restyle(rendered)
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
    // A reloaded document gets a new snapshot: search it again.
    findMatches = nil
    findKey = nil
    if !findBar.isHidden { search(then: nil) }
  }

  /// Re-render the presented document at the current text size, keeping the
  /// reading position, selection and find highlights.
  func applyTextScale() {
    guard case .loaded(let loaded) = loader.state else { return }
    let restyle =
      renderTask == nil && loaded.capability == shownCapability
      && surface.document?.generation == loaded.document.generation
    renderRequest &+= 1
    renderTask?.cancel()
    renderTask = nil
    render(loaded, restyle: restyle)
  }

  private func showStatus(_ primary: String, _ secondary: String?, title: String, url: URL?) {
    closeFindBar()
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
    switch menuItem.action {
    case #selector(reloadDocument(_:)): loader.capability != nil
    case #selector(showFindBar(_:)), #selector(findNextMatch(_:)),
      #selector(findPreviousMatch(_:)):
      surface.document != nil && !surface.scrollView.isHidden
    case #selector(useSelectionForFind(_:)):
      surface.document != nil && surface.textView.selectedRange().length > 0
    default: true
    }
  }

  // MARK: Find

  /// The query, options and snapshot a set of matches belongs to.
  private struct FindKey: Equatable {
    let query: String
    let options: CoreFindOptions
    let generation: UInt64
  }

  var isFindBarVisible: Bool { !findBar.isHidden }

  /// Show or hide the bar; the reading surface moves below it, keeping its frame.
  private func setFindBarVisible(_ visible: Bool) {
    guard visible == findBar.isHidden else { return }
    findBar.isHidden = !visible
    surfaceTop.isActive = !visible
    surfaceBelowFindBar.isActive = visible
  }

  @objc func showFindBar(_ sender: Any?) {
    guard surface.document != nil else { return }
    setFindBarVisible(true)
    window?.makeFirstResponder(findBar.field)
    findBar.field.selectText(nil)
    if findMatches == nil, !findBar.query.isEmpty { search(then: nil) }
  }

  @objc func findNextMatch(_ sender: Any?) { step(backwards: false) }
  @objc func findPreviousMatch(_ sender: Any?) { step(backwards: true) }

  /// Search for the selected text, keeping the focus in the document.
  @objc func useSelectionForFind(_ sender: Any?) {
    let selection = surface.textView.selectedRange()
    guard surface.document != nil, selection.length > 0 else { return }
    findBar.field.stringValue = (surface.textView.string as NSString).substring(with: selection)
    setFindBarVisible(true)
    search(then: nil)
  }

  /// Hide the bar and its highlights. The current match becomes the
  /// selection, unless the reader has moved the selection since it was shown.
  func closeFindBar() {
    findRequest &+= 1
    findTask?.cancel()
    findTask = nil
    findTaskKey = nil
    if !findBar.isHidden, let current = surface.currentMatch,
      surface.matches.indices.contains(current),
      surface.textView.selectedRange() == selectionAtMatch
    {
      surface.textView.setSelectedRange(surface.matches[current])
    }
    findMatches = nil
    findKey = nil
    selectionAtMatch = nil
    surface.highlight([])
    guard !findBar.isHidden else { return }
    let refocus = window?.firstResponder === findBar.field.currentEditor()
    setFindBarVisible(false)
    findBar.status.stringValue = ""
    if refocus || window?.firstResponder == nil { window?.makeFirstResponder(surface.textView) }
  }

  /// Whether a find request is still running.
  var isFinding: Bool { findTask != nil }

  /// Go to the next or previous match from the current match or selection,
  /// wrapping around the document; search first if the query changed.
  private func step(backwards: Bool) {
    guard surface.document != nil else { return }
    if findBar.query.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
      showFindBar(nil)
      return
    }
    // Highlights are only shown with the bar that can dismiss them.
    setFindBarVisible(true)
    guard let matches = findMatches, findKey == currentFindKey else {
      // The running search already has this query: step once it finishes,
      // rather than queueing another uninterruptible core call.
      if findTask != nil, findTaskKey == currentFindKey {
        pendingStep = backwards
      } else {
        search(then: backwards)
      }
      return
    }
    guard !matches.ranges.isEmpty else {
      NSSound.beep()
      return
    }
    let selection = surface.textView.selectedRange()
    let index: Int?
    if let current = surface.currentMatch, matches.ranges.indices.contains(current),
      selection == selectionAtMatch
    {
      index = matches.index(after: current, backwards: backwards)
    } else if backwards {
      index = matches.index(before: selection.location)
    } else {
      index = matches.index(atOrAfter: selection.location)
    }
    guard let index else { return }
    showMatch(index)
    showFindStatus(matches.status(current: index), announce: false)
  }

  /// Show a match, selecting it unless the find field has the focus.
  private func showMatch(_ index: Int) {
    let editing = findBar.field.currentEditor() != nil
    surface.showMatch(index, select: !editing)
    selectionAtMatch = surface.textView.selectedRange()
  }

  private var currentFindKey: FindKey? {
    surface.document.map {
      FindKey(query: findBar.query, options: findBar.options, generation: $0.generation)
    }
  }

  /// Run the find bar's query in the core, off the main thread. Only the
  /// newest request presents; `direction` steps once the matches arrive
  /// (nil shows the first match at or after the selection).
  private func search(then direction: Bool?) {
    findRequest &+= 1
    let id = findRequest
    findTask?.cancel()
    findTask = nil
    findTaskKey = nil
    pendingStep = direction
    guard let rendered = surface.document, let key = currentFindKey else { return }
    if key.query.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
      findMatches = nil
      findKey = nil
      selectionAtMatch = nil
      surface.highlight([])
      showFindStatus("", announce: false)
      return
    }
    let loader = loader
    findTaskKey = key
    findTask = Task { [weak self] in
      let outcome: Result<FindMatches, any Error>
      do {
        let result = try await loader.find(
          key.query, options: key.options, generation: key.generation)
        // Mapping walks long text nodes, so it stays off the main thread too.
        outcome = .success(await Task.detached { rendered.matches(for: result) }.value)
      } catch {
        outcome = .failure(error)
      }
      guard let self, id == self.findRequest else { return }
      self.findTask = nil
      self.findTaskKey = nil
      let step = self.pendingStep
      self.pendingStep = nil
      switch outcome {
      case .success(let matches):
        self.presentMatches(matches, key, then: step)
      case .failure(CoreError.queryTooLong):
        self.clearMatches("Search text is too long")
      case .failure(is CancellationError), .failure(CoreError.superseded),
        .failure(CoreError.closed):
        // A newer document or a closing window replaces this search.
        break
      case .failure:
        self.clearMatches("Find is unavailable")
      }
    }
  }

  private func presentMatches(_ matches: FindMatches, _ key: FindKey, then direction: Bool?) {
    findMatches = matches
    findKey = key
    surface.highlight(matches.ranges)
    if let direction {
      step(backwards: direction)
      return
    }
    let selection = surface.textView.selectedRange()
    guard let index = matches.index(atOrAfter: selection.location) else {
      showFindStatus(matches.status(current: nil), announce: true)
      return
    }
    showMatch(index)
    showFindStatus(matches.status(current: index), announce: true)
  }

  private func clearMatches(_ message: String) {
    findMatches = nil
    findKey = nil
    selectionAtMatch = nil
    surface.highlight([])
    showFindStatus(message, announce: true)
  }

  /// Show the match status, and announce a new search's result to VoiceOver.
  private func showFindStatus(_ text: String, announce: Bool) {
    findBar.status.stringValue = text
    guard announce, !text.isEmpty, let window else { return }
    NSAccessibility.post(
      element: window, notification: .announcementRequested,
      userInfo: [
        .announcement: text, .priority: NSAccessibilityPriorityLevel.medium.rawValue,
      ])
  }

  func windowWillClose(_ notification: Notification) {
    findRequest &+= 1
    findTask?.cancel()
    findTask = nil
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
