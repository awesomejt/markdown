import AppKit
import MarkdownCore
import MarkdownDocument
import MarkdownRender

/// One document window: an outline sidebar beside the TextKit 2 reading
/// surface and its find bar, with a centred status label for the empty,
/// loading and error states, and a notice below a stale document.
///
/// A new revision of the shown document (an external change or Reload)
/// keeps the reader's place: the top line and selection are anchored in the
/// old snapshot and resolved in the new one (MAC-001H). A restorable window
/// saves the same anchors with a bookmark to its document, so a relaunch
/// reopens the document where the reader left it.
@MainActor
final class ReaderWindowController: NSWindowController, NSWindowDelegate, NSMenuItemValidation {
  var onClose: ((ReaderWindowController) -> Void)?
  let loader = DocumentLoader()
  let surface = ReaderSurface()
  let outline = OutlineController()
  let findBar = FindBar()
  let notice = NoticeBar()
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
  /// The reading surface starts below the safe area, or below a visible find bar,
  /// and ends at the bottom, or above a visible notice.
  private var surfaceTop: NSLayoutConstraint!
  private var surfaceBelowFindBar: NSLayoutConstraint!
  private var surfaceBottom: NSLayoutConstraint!
  private var surfaceAboveNotice: NSLayoutConstraint!
  private let split = NSSplitViewController()
  private let status = NSTextField(labelWithString: "No document open")
  private let detail = NSTextField(wrappingLabelWithString: "")
  private let statusStack: NSStackView
  private var renderTask: Task<Void, Never>?
  private var renderRequest: UInt64 = 0
  /// Where the running render puts the reader.
  private var renderPlacement: Placement = .top
  /// The user chose Reload: render the result even if it is unchanged.
  private var reloadRequested = false
  /// A relaunch's saved position, resolved when the document first loads.
  private var pendingRestore: SavedPosition?
  /// The anchored position most recently saved for state restoration.
  private(set) var savedPosition: SavedPosition?
  private var positionTask: Task<Void, Never>?
  /// The title of the presented document, once its text is shown.
  private(set) var documentTitle: String?
  private var shownCapability: DocumentCapability?
  /// Development measurements observe when a load finishes, when its first
  /// text is installed and when the whole document is shown (MAC-001G).
  var onMilestone: ((Milestone) -> Void)?
  enum Milestone { case loaded, installed, completed }

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
    notice.translatesAutoresizingMaskIntoConstraints = false
    notice.isHidden = true
    content.addSubview(surface.scrollView)
    content.addSubview(findBar)
    content.addSubview(notice)
    content.addSubview(statusStack)
    NSLayoutConstraint.activate([
      findBar.topAnchor.constraint(equalTo: content.safeAreaLayoutGuide.topAnchor),
      findBar.leadingAnchor.constraint(equalTo: content.leadingAnchor),
      findBar.trailingAnchor.constraint(equalTo: content.trailingAnchor),
      notice.bottomAnchor.constraint(equalTo: content.bottomAnchor),
      notice.leadingAnchor.constraint(equalTo: content.leadingAnchor),
      notice.trailingAnchor.constraint(equalTo: content.trailingAnchor),
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
    surfaceBottom = surface.scrollView.bottomAnchor.constraint(equalTo: content.bottomAnchor)
    surfaceAboveNotice = surface.scrollView.bottomAnchor.constraint(equalTo: notice.topAnchor)
    surfaceBottom.isActive = true
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
    notice.onReload = { [weak self] in self?.reloadDocument(nil) }
    // Restorable windows save the reading position once the reader pauses.
    surface.scrollView.contentView.postsBoundsChangedNotifications = true
    NotificationCenter.default.addObserver(
      self, selector: #selector(readingPositionChanged(_:)),
      name: NSView.boundsDidChangeNotification, object: surface.scrollView.contentView)
    NotificationCenter.default.addObserver(
      self, selector: #selector(readingPositionChanged(_:)),
      name: NSTextView.didChangeSelectionNotification, object: surface.textView)
  }

  required init?(coder: NSCoder) {
    fatalError("Reader windows are constructed programmatically")
  }

  var statusText: String { status.stringValue }
  var detailText: String { detail.stringValue }
  /// Whether a load or the off-main rendering of its result is still running.
  var isBusy: Bool { loader.isLoading || loader.isRefreshing || renderTask != nil }
  var isSidebarCollapsed: Bool { split.splitViewItems[0].isCollapsed }

  /// Where a rendering puts the reader.
  private enum Placement {
    /// A newly opened document: the top, with a lead first for a long one.
    case top
    /// The shown text at a new text size, restyled in place.
    case restyle
    /// The shown text (a lead's) at a new text size: this position.
    case keep(ReadingPosition)
    /// A new revision of the shown document: the reader's place, anchored
    /// in the old snapshot and resolved in the new one.
    case transfer
    /// A restored window's saved position.
    case restore(SavedPosition)
  }

  /// Whether `loaded` is the revision on screen (or being completed from its lead).
  private func isShowing(_ loaded: LoadedDocument) -> Bool {
    loaded.capability == shownCapability
      && surface.document?.generation == loaded.document.generation
  }

  private func present(_ state: DocumentLoader.State) {
    switch state {
    case .loaded(let loaded) where isShowing(loaded) && !reloadRequested:
      // An unchanged refresh, or the file of a stale document came back.
      setNotice(nil)
      return
    case .stale(let loaded, let error) where isShowing(loaded):
      setNotice(error)
      return
    default: break
    }
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
      reloadRequested = false
      setNotice(nil)
      onMilestone?(.loaded)
      render(loaded, placement: placement(for: loaded))
    case .stale(let loaded, let error):
      // A newer revision loaded but wasn't shown before the file went away.
      setNotice(error)
      render(loaded, placement: placement(for: loaded))
    case .failed(let capability, let error):
      reloadRequested = false
      pendingRestore = nil
      showStatus(error.message, error.recovery, title: capability.displayName, url: nil)
    }
  }

  private func placement(for loaded: LoadedDocument) -> Placement {
    if let saved = pendingRestore {
      pendingRestore = nil
      return .restore(saved)
    }
    return loaded.capability == shownCapability && surface.document != nil ? .transfer : .top
  }

  /// Build the attributed text off the main actor; only the newest request
  /// installs its result, so a slow render never replaces newer content. A
  /// newly opened long document shows its lead first and is completed in
  /// place; any other placement waits for the whole text, since its
  /// position may lie past the lead.
  private func render(_ loaded: LoadedDocument, placement: Placement) {
    let id = renderRequest
    let document = loaded.document
    let capability = loaded.capability
    let scale = preferences.textScale
    renderPlacement = placement
    var lead: (@Sendable (RenderedDocument) -> Void)?
    var saved: SavedPosition?
    switch placement {
    case .top:
      lead = { @Sendable [weak self] partial in
        Task { @MainActor in self?.installLead(partial, id: id, loaded) }
      }
    case .restore(let position): saved = position
    case .restyle, .keep, .transfer: break
    }
    let work = Task.detached(priority: .userInitiated) { [lead, saved] in
      // Resolving the folder touches the filesystem, so it stays off the main thread.
      let images = Self.imageLoader(DocumentResources(document: capability))
      let rendered = try DocumentRenderer.render(
        document, images: images, scale: scale, lead: lead)
      return (rendered, saved.flatMap { rendered.restore($0) })
    }
    renderTask = Task { [weak self] in
      // Cancelling the window's task (a newer state, or close) stops the render.
      let result = await withTaskCancellationHandler {
        await work.result
      } onCancel: {
        work.cancel()
      }
      guard let self, id == self.renderRequest else { return }
      let title = document.title ?? capability.displayName
      switch (result, placement) {
      case (.success((let rendered, _)), .restyle):
        self.surface.restyle(rendered)
      case (.success((let rendered, _)), .keep(let position)):
        self.install(rendered, title: title, loaded, at: position)
      case (.success((let rendered, let restored)), .restore):
        self.install(rendered, title: title, loaded, at: restored)
      case (.success((let rendered, _)), .transfer):
        // Anchor the reader's place as it is now; the core calls run off
        // the main thread, and a newer state still wins.
        let shown = self.surface.document
        let position = self.surface.readingPosition
        let moved = await Task.detached {
          shown.flatMap { try? rendered.restore($0.save(position)) }
        }.value
        guard id == self.renderRequest else { return }
        self.install(rendered, title: title, loaded, at: moved)
      case (.success((let rendered, _)), .top)
      where self.surface.document?.isComplete == false
        && self.surface.document?.generation == rendered.generation:
        self.completeLead(rendered)
      case (.success((let rendered, _)), .top):
        self.install(rendered, title: title, loaded)
      case (.failure, _):
        let error = DocumentError.parseFailed
        self.showStatus(error.message, error.recovery, title: capability.displayName, url: nil)
      }
      self.renderTask = nil
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

  /// Show the start of a long document while the rest renders; a lead that
  /// arrives after its render finished, or was replaced, is dropped.
  private func installLead(_ lead: RenderedDocument, id: UInt64, _ loaded: LoadedDocument) {
    guard id == renderRequest, renderTask != nil else { return }
    install(lead, title: loaded.document.title ?? loaded.capability.displayName, loaded)
  }

  /// Append the rest of the document to its lead, keeping the reader's place.
  private func completeLead(_ rendered: RenderedDocument) {
    let row = outline.table.selectedRow
    surface.complete(rendered)
    outline.show(rendered.headings)
    if row >= 0 { outline.select(row) }
    // A search started while the lead was shown runs now.
    if !findBar.isHidden { search(then: pendingStep) }
    onMilestone?(.completed)
  }

  /// Show a rendering at the top, or at `position` when there is one.
  private func install(
    _ rendered: RenderedDocument, title: String, _ loaded: LoadedDocument,
    at position: ReadingPosition? = nil
  ) {
    documentTitle = title
    shownCapability = loaded.capability
    if let position { surface.show(rendered, at: position) } else { surface.show(rendered) }
    outline.show(rendered.headings)
    surface.scrollView.isHidden = false
    statusStack.isHidden = true
    window?.title = loaded.capability.displayName
    window?.representedURL = loaded.capability.url
    // A reloaded document gets a new snapshot: search it again.
    findMatches = nil
    findKey = nil
    if !findBar.isHidden { search(then: nil) }
    onMilestone?(.installed)
    if rendered.isComplete { onMilestone?(.completed) }
    invalidateReadingPosition()
  }

  /// Re-render the presented document at the current text size, keeping the
  /// reading position, selection and find highlights. A lead that is still
  /// being completed keeps the reader's place in it too.
  func applyTextScale() {
    guard let loaded = loader.document else { return }
    let placement: Placement
    if isShowing(loaded), let shown = surface.document {
      if renderTask == nil {
        placement = .restyle
      } else {
        placement = shown.isComplete ? renderPlacement : .keep(surface.readingPosition)
      }
    } else if renderTask != nil {
      // A transfer or restoration in progress keeps its placement.
      placement = renderPlacement
    } else {
      return
    }
    renderRequest &+= 1
    renderTask?.cancel()
    renderTask = nil
    render(loaded, placement: placement)
  }

  private func showStatus(_ primary: String, _ secondary: String?, title: String, url: URL?) {
    closeFindBar()
    setNotice(nil)
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
    reloadRequested = true
    loader.reload()
  }

  /// Show or hide the notice below a stale document.
  private func setNotice(_ error: DocumentError?) {
    let visible = error != nil
    if let error {
      notice.show(error.message, detail: "Showing the version last read. " + error.recovery)
    }
    guard visible == notice.isHidden else { return }
    notice.isHidden = !visible
    surfaceBottom.isActive = !visible
    surfaceAboveNotice.isActive = visible
    if visible, let window {
      NSAccessibility.post(
        element: window, notification: .announcementRequested,
        userInfo: [
          .announcement: notice.message.stringValue,
          .priority: NSAccessibilityPriorityLevel.high.rawValue,
        ])
    }
  }

  var noticeText: String? { notice.isHidden ? nil : notice.message.stringValue }

  // MARK: Restoration

  /// Save the reading position for state restoration once the reader pauses.
  @objc private func readingPositionChanged(_ notification: Notification) {
    guard window?.isRestorable == true, surface.document != nil else { return }
    positionTask?.cancel()
    positionTask = Task { [weak self] in
      try? await Task.sleep(for: .milliseconds(500))
      guard !Task.isCancelled else { return }
      self?.invalidateReadingPosition()
    }
  }

  /// Save the current position and ask AppKit to encode the window's state again.
  private func invalidateReadingPosition() {
    guard window?.isRestorable == true, surface.document != nil else { return }
    positionTask?.cancel()
    positionTask = Task { [weak self] in
      await self?.saveReadingPosition()
      guard let self, !Task.isCancelled else { return }
      self.positionTask = nil
      self.invalidateRestorableState()
    }
  }

  /// Anchor the current position off the main thread for state restoration.
  func saveReadingPosition() async {
    guard let rendered = surface.document else { return }
    let position = surface.readingPosition
    let saved = await Task.detached { try? rendered.save(position) }.value
    guard !Task.isCancelled, let saved, surface.document?.generation == rendered.generation
    else { return }
    savedPosition = saved
  }

  /// Make the window restorable: AppKit saves its frame, and this
  /// controller its document and reading position.
  func enableRestoration() {
    guard let window else { return }
    window.identifier = ReaderWindowRestoration.identifier
    window.restorationClass = ReaderWindowRestoration.self
    window.isRestorable = true
  }

  /// Open the document a relaunch restores, at its saved position.
  func restore(_ capability: DocumentCapability, at position: SavedPosition?, sidebar: Bool) {
    pendingRestore = position
    savedPosition = position
    split.splitViewItems[0].isCollapsed = !sidebar
    loader.open(capability)
  }

  func window(_ window: NSWindow, willEncodeRestorableState state: NSCoder) {
    // Only a document that loaded is restored; a failed one isn't.
    guard let capability = loader.document?.capability ?? pendingCapability,
      let bookmark = capability.bookmark()
    else { return }
    RestorationState(
      bookmark: bookmark, position: savedPosition, sidebar: !isSidebarCollapsed
    ).encode(to: state)
  }

  /// A restored document that hasn't loaded yet keeps its saved state.
  private var pendingCapability: DocumentCapability? {
    if pendingRestore != nil || loader.isLoading { return loader.capability }
    return nil
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
    // Find maps matches over the whole rendering, so a lead waits for it.
    guard let rendered = surface.document, rendered.isComplete, let key = currentFindKey else {
      return
    }
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

  /// Look for a change the watcher may have missed, as on a network volume.
  func windowDidBecomeKey(_ notification: Notification) {
    if loader.document != nil { loader.checkForChanges() }
  }

  func windowWillClose(_ notification: Notification) {
    NotificationCenter.default.removeObserver(self)
    positionTask?.cancel()
    positionTask = nil
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
