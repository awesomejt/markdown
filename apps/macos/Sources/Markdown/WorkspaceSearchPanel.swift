import AppKit
import MarkdownCore
import MarkdownDocument

/// Where to take the reader once a document opens: a heading (a link's
/// fragment) or a workspace search hit (MAC-002C).
enum NavigationTarget: Sendable, Equatable {
  case heading(String)
  case match(SearchMatch)
}

/// A content hit and the query that found it, so an edited document can be
/// searched again near the hit.
struct SearchMatch: Sendable, Equatable {
  let target: CoreContentTarget
  let query: WorkspaceQuery
}

/// A group of search results: the matching file names, or one document's
/// content matches. Classes, since NSOutlineView keeps items by identity.
final class SearchGroup {
  enum Kind {
    case names
    case document(WorkspaceItem, truncated: Bool)
  }

  let kind: Kind
  let rows: [SearchRow]

  init(_ kind: Kind, rows: [SearchRow]) {
    self.kind = kind
    self.rows = rows
  }
}

final class SearchRow {
  enum Kind {
    case name(WorkspaceNameMatch)
    case hit(WorkspaceItem, CoreContentHit)
  }

  let kind: Kind
  init(_ kind: Kind) { self.kind = kind }

  var item: WorkspaceItem {
    switch kind {
    case .name(let match): match.item
    case .hit(let item, _): item
    }
  }
}

/// The sidebar's folder search (MAC-002C): a search field, Match Case and
/// Whole Words, and the results as an outline: matching file names first,
/// then each document's matches as one-line snippets with the line number.
/// The search runs off the main thread over the files the sidebar lists;
/// typing a new query, changing an option or a new listing supersedes it,
/// and Stop (or Escape in the field) ends it between documents. Results
/// arrive as documents are searched.
///
/// Keyboard: Down arrow in the field moves to the results; the arrow keys
/// move through them, and Return, Enter or a click opens a document at the
/// match. VoiceOver reads each row's document, line and snippet, and the
/// summary is announced when the search ends.
@MainActor
final class WorkspaceSearchController: NSViewController, NSOutlineViewDataSource,
  NSOutlineViewDelegate, NSSearchFieldDelegate
{
  /// Open a document, at a content match when there is one.
  var onOpen: ((WorkspaceItem, SearchMatch?) -> Void)?
  /// The search finished, failed or stopped; the smoke waits for it.
  var onFinish: (() -> Void)?
  /// Controls are built in `loadView` (MAC-003D): init stays cheap until the pane shows.
  private(set) lazy var field = NSSearchField()
  private(set) lazy var matchCase = NSButton(
    checkboxWithTitle: "Match Case", target: nil, action: nil)
  private(set) lazy var wholeWords = NSButton(
    checkboxWithTitle: "Whole Words", target: nil, action: nil)
  private(set) lazy var results = WorkspaceOutlineView()
  private lazy var status = NSTextField(wrappingLabelWithString: "")
  private lazy var spinner = NSProgressIndicator()
  private(set) lazy var stopButton = NSButton(title: "Stop", target: nil, action: nil)
  /// The documents searched: the files sidebar's listing.
  private(set) var listing: WorkspaceListing?
  private(set) var groups: [SearchGroup] = []
  private(set) var summary: WorkspaceSearchSummary?
  private(set) var failure: String?
  /// The query the shown results are for.
  private var resultsQuery = WorkspaceQuery("")
  private var request: UInt64 = 0
  private var cancellation: WorkspaceSearchCancellation?
  /// Selecting a row from code must not open it.
  private var selecting = false
  /// Content search limits; the smoke and tests lower them.
  var limits = CoreContentOptions()

  /// Searches run on their own threads: reads block, and core calls can't
  /// be interrupted. The core permit keeps one parse at a time.
  nonisolated static let queue = DispatchQueue(
    label: "io.github.awesomejt.markdown.workspace-search", qos: .userInitiated,
    attributes: .concurrent)

  var isSearching: Bool { cancellation != nil }
  var statusText: String { status.stringValue }

  var query: WorkspaceQuery {
    WorkspaceQuery(
      field.stringValue, caseSensitive: matchCase.state == .on, wholeWord: wholeWords.state == .on)
  }

  override func loadView() {
    field.placeholderString = "Search Folder"
    field.sendsWholeSearchString = false
    field.sendsSearchStringImmediately = false
    field.target = self
    field.action = #selector(searchChanged(_:))
    field.delegate = self
    field.controlSize = .small
    field.setAccessibilityIdentifier("workspace-search-field")
    field.setAccessibilityLabel("Search folder")
    for (box, tip) in [
      (matchCase, "Match upper and lower case exactly"),
      (wholeWords, "Match whole words only"),
    ] {
      box.controlSize = .small
      box.font = .systemFont(ofSize: NSFont.smallSystemFontSize)
      box.target = self
      box.action = #selector(searchChanged(_:))
      box.toolTip = tip
    }
    let options = NSStackView(views: [matchCase, wholeWords])
    options.orientation = .horizontal
    options.spacing = 10

    let column = NSTableColumn(identifier: NSUserInterfaceItemIdentifier("result"))
    results.addTableColumn(column)
    results.outlineTableColumn = column
    results.headerView = nil
    results.style = .sourceList
    results.allowsEmptySelection = true
    results.autosaveExpandedItems = false
    results.usesAutomaticRowHeights = true
    results.dataSource = self
    results.delegate = self
    results.target = self
    results.action = #selector(rowClicked(_:))
    results.onActivate = { [weak self] in self?.activateSelection() }
    results.setAccessibilityIdentifier("workspace-search-results")
    results.setAccessibilityLabel("Search results")
    let scroll = NSScrollView()
    scroll.documentView = results
    scroll.hasVerticalScroller = true
    scroll.autohidesScrollers = true
    scroll.drawsBackground = false

    spinner.style = .spinning
    spinner.controlSize = .small
    spinner.isDisplayedWhenStopped = false
    status.font = .systemFont(ofSize: NSFont.smallSystemFontSize)
    status.textColor = .secondaryLabelColor
    status.setAccessibilityIdentifier("workspace-search-status")
    status.setContentHuggingPriority(.defaultLow, for: .horizontal)
    status.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
    stopButton.controlSize = .small
    stopButton.bezelStyle = .push
    stopButton.target = self
    stopButton.action = #selector(stop(_:))
    stopButton.isHidden = true
    stopButton.toolTip = "Stop searching"
    let footer = NSStackView(views: [spinner, status, stopButton])
    footer.orientation = .horizontal
    footer.alignment = .top
    footer.spacing = 6

    let view = NSView()
    for child in [field, options, scroll, footer] as [NSView] {
      child.translatesAutoresizingMaskIntoConstraints = false
      view.addSubview(child)
    }
    NSLayoutConstraint.activate([
      field.topAnchor.constraint(equalTo: view.topAnchor),
      field.leadingAnchor.constraint(equalTo: view.leadingAnchor, constant: 8),
      field.trailingAnchor.constraint(equalTo: view.trailingAnchor, constant: -8),
      options.topAnchor.constraint(equalTo: field.bottomAnchor, constant: 4),
      options.leadingAnchor.constraint(equalTo: field.leadingAnchor),
      options.trailingAnchor.constraint(lessThanOrEqualTo: field.trailingAnchor),
      scroll.topAnchor.constraint(equalTo: options.bottomAnchor, constant: 4),
      scroll.leadingAnchor.constraint(equalTo: view.leadingAnchor),
      scroll.trailingAnchor.constraint(equalTo: view.trailingAnchor),
      footer.topAnchor.constraint(equalTo: scroll.bottomAnchor, constant: 4),
      footer.leadingAnchor.constraint(equalTo: view.leadingAnchor, constant: 10),
      footer.trailingAnchor.constraint(equalTo: view.trailingAnchor, constant: -10),
      footer.bottomAnchor.constraint(equalTo: view.bottomAnchor, constant: -6),
    ])
    self.view = view
    show(nil)
  }

  /// Put the cursor in the search field, selecting its text.
  func focus() {
    _ = view
    view.window?.makeFirstResponder(field)
    field.currentEditor()?.selectAll(nil)
  }

  /// Search a new listing (a new folder, Refresh Files or a changed
  /// listing option), or nothing.
  /// Without a listing, `unavailable` says why there is nothing to search:
  /// the folder is being listed or can't be, or there is no folder.
  func setListing(_ listing: WorkspaceListing?, unavailable: String? = nil) {
    self.listing = listing
    if listing == nil {
      cancel()
      groups = []
      summary = nil
      failure = unavailable
      if isViewLoaded { results.reloadData() }
      show(nil)
    } else if !query.isEmpty, summary?.cancelled != true {
      // A new listing (a folder change, MAC-002E) runs the query again,
      // unless the reader stopped it: Stop holds until they search again.
      search()
    }
  }

  @objc private func searchChanged(_ sender: Any?) { search() }

  @objc func stop(_ sender: Any?) {
    guard isSearching else { return }
    cancellation?.cancel()
  }

  private func cancel() {
    request &+= 1
    cancellation?.cancel()
    cancellation = nil
  }

  /// Start the search for the field's query, superseding any running one.
  func search() {
    _ = view
    guard let listing else { return }
    cancel()
    groups = []
    summary = nil
    failure = nil
    results.reloadData()
    let query = query
    guard !query.isEmpty else {
      show(nil)
      onFinish?()
      return
    }
    let id = request
    resultsQuery = query
    let cancellation = WorkspaceSearchCancellation()
    self.cancellation = cancellation
    let limits = limits
    show(WorkspaceSearchSummary())
    Self.queue.async { [weak self] in
      let deliver: @Sendable (@escaping @MainActor () -> Void) -> Void = { work in
        // The main queue keeps the updates in order.
        DispatchQueue.main.async { MainActor.assumeIsolated { work() } }
      }
      let result = Result {
        try WorkspaceSearch.run(
          query, in: listing, limits: limits, cancellation: cancellation
        ) { update in
          deliver { self?.apply(update, id: id) }
        }
      }
      deliver { self?.finish(result, id: id) }
    }
  }

  private func apply(_ update: WorkspaceSearchUpdate, id: UInt64) {
    guard id == request else { return }
    switch update {
    case .names(let matches, let summary):
      self.summary = summary
      if !matches.isEmpty {
        append(SearchGroup(.names, rows: matches.map { SearchRow(.name($0)) }))
      }
    case .file(let file, let summary):
      self.summary = summary
      append(
        SearchGroup(
          .document(file.item, truncated: file.truncated),
          rows: file.hits.map { SearchRow(.hit(file.item, $0)) }))
    case .progress(let summary):
      self.summary = summary
    }
    show(summary)
  }

  private func append(_ group: SearchGroup) {
    groups.append(group)
    results.insertItems(at: IndexSet(integer: groups.count - 1), inParent: nil)
    results.expandItem(group)
  }

  private func finish(_ result: Result<WorkspaceSearchSummary, any Error>, id: UInt64) {
    guard id == request else { return }
    cancellation = nil
    switch result {
    case .success(let summary): self.summary = summary
    case .failure(let error as FolderError): failure = error.message
    case .failure: failure = "The folder can’t be searched."
    }
    show(summary)
    if let window = view.window {
      NSAccessibility.post(
        element: window, notification: .announcementRequested,
        userInfo: [
          .announcement: status.stringValue,
          .priority: NSAccessibilityPriorityLevel.medium.rawValue,
        ])
    }
    onFinish?()
  }

  /// The status line: progress while searching, then the counts and every
  /// reason the results may be incomplete.
  private func show(_ summary: WorkspaceSearchSummary?) {
    spinner.isHidden = !isSearching
    if isSearching { spinner.startAnimation(nil) } else { spinner.stopAnimation(nil) }
    stopButton.isHidden = !isSearching
    status.stringValue = Self.describe(
      summary, failure: failure, searching: isSearching, listed: listing != nil,
      empty: query.isEmpty)
  }

  static func describe(
    _ summary: WorkspaceSearchSummary?, failure: String?, searching: Bool, listed: Bool,
    empty: Bool
  ) -> String {
    if let failure { return failure }
    guard listed else { return "Open a folder to search it." }
    guard let summary, !empty else { return "" }
    func count(_ value: Int, _ one: String, _ many: String) -> String {
      value == 1 ? "1 \(one)" : "\(value.formatted()) \(many)"
    }
    if searching {
      return "Searching… \(count(summary.filesSearched, "document", "documents")) searched"
    }
    var parts: [String] = []
    if summary.matches == 0 && summary.names == 0 {
      parts.append(summary.cancelled ? "Stopped. No results so far." : "No results.")
    } else {
      var found: [String] = []
      if summary.names > 0 { found.append(count(summary.names, "file name", "file names")) }
      if summary.matches > 0 {
        found.append(
          "\(count(summary.matches, "match", "matches")) in \(count(summary.documents, "document", "documents"))"
        )
      }
      parts.append(found.joined(separator: ", ") + ".")
      if summary.cancelled { parts.append("Stopped before the end.") }
    }
    switch summary.stopped {
    case .files?: parts.append("Stopped at the document limit; results are incomplete.")
    case .bytes?: parts.append("Stopped at the reading limit; results are incomplete.")
    case .matches?: parts.append("Stopped at the match limit; results are incomplete.")
    case nil: break
    }
    if summary.namesTruncated { parts.append("Not every matching file name is shown.") }
    if summary.filesTruncated > 0 {
      parts.append(
        "\(count(summary.filesTruncated, "document has", "documents have")) more matches than shown."
      )
    }
    if summary.skippedTooLarge > 0 {
      parts.append(
        "\(count(summary.skippedTooLarge, "document is", "documents are")) too large to search.")
    }
    if summary.skippedTooComplex > 0 {
      parts.append(
        "\(count(summary.skippedTooComplex, "document is", "documents are")) too complex to search."
      )
    }
    if summary.skippedInvalidUTF8 > 0 {
      parts.append(
        "\(count(summary.skippedInvalidUTF8, "document isn’t", "documents aren’t")) UTF-8 text.")
    }
    if summary.unreadable > 0 {
      parts.append(
        "\(count(summary.unreadable, "document", "documents")) couldn’t be read.")
    }
    if summary.listingTruncated { parts.append("Only part of the folder is listed.") }
    return parts.joined(separator: " ")
  }

  // MARK: Opening results

  var selectedRow: SearchRow? { results.item(atRow: results.selectedRow) as? SearchRow }

  /// Select a result row without opening it.
  func select(_ row: SearchRow) {
    let index = results.row(forItem: row)
    guard index >= 0 else { return }
    selecting = true
    results.selectRowIndexes(IndexSet(integer: index), byExtendingSelection: false)
    results.scrollRowToVisible(index)
    selecting = false
  }

  /// Open the selected result, or expand or collapse the selected group.
  func activateSelection() {
    let item = results.item(atRow: results.selectedRow)
    if let row = item as? SearchRow {
      open(row)
    } else if let group = item as? SearchGroup {
      if results.isItemExpanded(group) {
        results.collapseItem(group)
      } else {
        results.expandItem(group)
      }
    }
  }

  private func open(_ row: SearchRow) {
    switch row.kind {
    case .name(let match): onOpen?(match.item, nil)
    case .hit(let item, let hit):
      onOpen?(item, SearchMatch(target: hit.target, query: resultsQuery))
    }
  }

  @objc private func rowClicked(_ sender: Any?) {
    guard !selecting, let row = results.item(atRow: results.clickedRow) as? SearchRow else {
      return
    }
    open(row)
  }

  /// Down arrow in the field moves to the first result; Escape stops a
  /// running search before the field clears.
  func control(_ control: NSControl, textView: NSTextView, doCommandBy selector: Selector) -> Bool {
    switch selector {
    case #selector(NSResponder.moveDown(_:)) where results.numberOfRows > 0:
      view.window?.makeFirstResponder(results)
      if results.selectedRow < 0 {
        let first = (0..<results.numberOfRows).first { results.item(atRow: $0) is SearchRow }
        if let first {
          selecting = true
          results.selectRowIndexes(IndexSet(integer: first), byExtendingSelection: false)
          selecting = false
        }
      }
      return true
    case #selector(NSResponder.cancelOperation(_:)) where isSearching:
      stop(nil)
      return true
    default:
      return false
    }
  }

  // MARK: Data source

  func outlineView(_ outlineView: NSOutlineView, numberOfChildrenOfItem item: Any?) -> Int {
    (item as? SearchGroup)?.rows.count ?? (item == nil ? groups.count : 0)
  }

  func outlineView(_ outlineView: NSOutlineView, child index: Int, ofItem item: Any?) -> Any {
    (item as? SearchGroup)?.rows[index] ?? groups[index]
  }

  func outlineView(_ outlineView: NSOutlineView, isItemExpandable item: Any) -> Bool {
    item is SearchGroup
  }

  func outlineView(_ outlineView: NSOutlineView, viewFor tableColumn: NSTableColumn?, item: Any)
    -> NSView?
  {
    let identifier = NSUserInterfaceItemIdentifier("search-cell")
    let cell =
      outlineView.makeView(withIdentifier: identifier, owner: nil) as? SearchCell
      ?? SearchCell(identifier: identifier)
    if let group = item as? SearchGroup {
      cell.configure(group)
    } else if let row = item as? SearchRow {
      cell.configure(row)
    }
    return cell
  }
}

/// A result group's or row's symbol and text. Matches are marked in bold
/// with the find highlight colour; the label says the same for VoiceOver.
private final class SearchCell: NSTableCellView {
  private let icon = NSImageView()
  private let label = NSTextField(labelWithString: "")

  init(identifier: NSUserInterfaceItemIdentifier) {
    super.init(frame: .zero)
    self.identifier = identifier
    icon.translatesAutoresizingMaskIntoConstraints = false
    label.lineBreakMode = .byTruncatingTail
    label.maximumNumberOfLines = 1
    label.translatesAutoresizingMaskIntoConstraints = false
    label.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
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
      label.topAnchor.constraint(equalTo: topAnchor, constant: 3),
      label.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -3),
    ])
  }

  required init?(coder: NSCoder) { fatalError("Search cells are constructed programmatically") }

  private static var font: NSFont { .systemFont(ofSize: NSFont.systemFontSize) }

  func configure(_ group: SearchGroup) {
    let text: String
    let spoken: String
    switch group.kind {
    case .names:
      icon.image = NSImage(systemSymbolName: "textformat", accessibilityDescription: nil)
      text = "File Names"
      spoken = "File names, \(group.rows.count) results"
    case .document(let item, let truncated):
      icon.image = NSImage(systemSymbolName: "doc.text", accessibilityDescription: nil)
      let count = group.rows.count == 1 ? "1 match" : "\(group.rows.count) matches"
      text = "\(item.path) (\(truncated ? "\(group.rows.count)+" : "\(group.rows.count)"))"
      spoken = "\(item.path), \(count)\(truncated ? ", more not shown" : "")"
    }
    icon.contentTintColor = nil
    label.attributedStringValue = NSAttributedString(
      string: text,
      attributes: [.font: NSFont.boldSystemFont(ofSize: NSFont.systemFontSize)])
    label.toolTip = text
    setAccessibilityLabel(spoken)
  }

  func configure(_ row: SearchRow) {
    switch row.kind {
    case .name(let match):
      let warning = match.item.hasInvisible
      icon.image = NSImage(
        systemSymbolName: warning ? "exclamationmark.triangle" : "doc.text",
        accessibilityDescription: nil)
      icon.contentTintColor = warning ? .systemOrange : nil
      label.attributedStringValue = Self.marked(match.path, match.range)
      label.toolTip =
        warning ? match.path + "\nThe name contains invisible characters" : match.path
      setAccessibilityLabel(
        "\(match.path), document" + (warning ? ", the name contains invisible characters" : ""))
    case .hit(let item, let hit):
      icon.image = nil
      let lead = hit.leadingMore ? "…" : ""
      let tail = hit.trailingMore ? "…" : ""
      let prefix = "\(hit.line)  " + lead
      let text = NSMutableAttributedString(
        string: prefix,
        attributes: [
          .font: NSFont.monospacedDigitSystemFont(
            ofSize: NSFont.smallSystemFontSize, weight: .regular),
          .foregroundColor: NSColor.secondaryLabelColor,
        ])
      text.append(Self.marked(hit.snippet, hit.match))
      text.append(NSAttributedString(string: tail, attributes: [.font: Self.font]))
      label.attributedStringValue = text
      label.toolTip = lead + hit.snippet + tail
      setAccessibilityLabel("Line \(hit.line) of \(item.path): \(hit.snippet)")
    }
  }

  /// `text` with `range` in bold on the find highlight colour.
  private static func marked(_ text: String, _ range: NSRange) -> NSAttributedString {
    let result = NSMutableAttributedString(
      string: text, attributes: [.font: font, .foregroundColor: NSColor.labelColor])
    if range.upperBound <= result.length, range.length > 0 {
      result.addAttributes(
        [
          .font: NSFont.boldSystemFont(ofSize: NSFont.systemFontSize),
          .backgroundColor: NSColor.findHighlightColor,
          .foregroundColor: NSColor.black,
        ], range: range)
    }
    return result
  }
}
