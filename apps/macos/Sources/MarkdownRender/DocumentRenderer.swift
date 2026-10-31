import AppKit
import CMarkdown
import MarkdownCore

/// One outline heading and its location in the rendered text.
public struct RenderedHeading: Sendable, Equatable {
  public let level: Int
  public let title: String
  public let anchor: String
  /// UTF-16 range of the heading's text, excluding its paragraph terminator.
  public let range: NSRange
}

/// Where a rendered link leads. Only these two kinds carry a `.link` attribute.
public enum LinkDestination: Sendable, Equatable {
  /// A heading anchor in this document.
  case fragment(String)
  /// An http, https or mailto URI the core classified as external.
  case external(URL)

  /// The `.link` attribute value: a fragment-only URL or the external URI.
  public var url: URL? {
    switch self {
    case .fragment(let anchor):
      var components = URLComponents()
      components.fragment = anchor
      return components.url
    case .external(let url): return url
    }
  }

  public init?(link url: URL) {
    // Decode the fragment: GitHub-style anchors keep non-ASCII letters.
    if url.scheme == nil, url.host == nil, url.path.isEmpty,
      let fragment = url.fragment(percentEncoded: false)
    {
      self = .fragment(fragment)
    } else if let scheme = url.scheme?.lowercased(), Self.externalSchemes.contains(scheme) {
      self = .external(url)
    } else {
      return nil
    }
  }

  static let externalSchemes: Set<String> = ["http", "https", "mailto"]
}

/// Immutable rendered text and heading map, built off the main thread from an
/// owned snapshot. The unchecked conformance covers `text`: an immutable copy
/// that is never mutated after `render` returns.
public struct RenderedDocument: @unchecked Sendable {
  public let generation: UInt64
  public let text: NSAttributedString
  public let headings: [RenderedHeading]
  /// Every image in document order, loaded or shown as its alt text.
  public let images: [RenderedImage]
  /// The UTF-16 range of each table, in document order.
  public let tables: [NSRange]
  /// False for a lead: the first blocks of a long document, shown while the
  /// rest renders. Its text is a prefix of the complete rendering's, except
  /// that only the complete text drops the final paragraph terminator.
  public let isComplete: Bool
  /// The text size factor the document was rendered at.
  public let scale: CGFloat
  /// Where each node's display text was rendered, by node index, so core find
  /// segments map to rendered ranges.
  let nodeText: [NodeText]
  /// Ascending UTF-16 locations of the line breaks inserted into very long
  /// unbroken runs (`RenderStyle.hardParagraphLimit`). They are not part of
  /// any node's display text.
  let insertedBreaks: [Int]

  /// The first heading with this anchor, as GitHub resolves duplicate slugs.
  public func heading(anchor: String) -> RenderedHeading? {
    headings.first { $0.anchor == anchor }
  }
}

/// Converts the owned model-v2 snapshot into one TextKit 2 attributed string
/// (MAC-001D, MAC-001E). Headings, prose, lists, task items, quotes, links,
/// code and tables are styled. Local images are read through `images` under
/// the core's resource policy and `budget`; without a loader, and for remote
/// or denied targets, an image shows its alt text. Alerts and footnotes get
/// plain readable fallbacks. Raw HTML and unsupported syntax stay inert text.
public enum DocumentRenderer {
  /// `scale` multiplies every font size and text metric (the text size
  /// setting); images keep their pixel size. With `lead`, a document whose
  /// rendering passes `RenderStyle.leadLength` UTF-16 units before its last
  /// block first passes a lead, so the window can show its start sooner.
  public static func render(
    _ document: CoreDocument, images: ImageLoader? = nil, budget: ImageBudget = ImageBudget(),
    scale: CGFloat = 1, lead: (@Sendable (RenderedDocument) -> Void)? = nil
  ) throws -> RenderedDocument {
    var builder = Builder(
      document: document, images: ImageCache(loader: images, budget: budget), scale: scale)
    var lead = lead
    for (index, root) in document.roots.enumerated() {
      try builder.block(Int(root), context: .root)
      // Each root block is final once rendered, so the text so far is a prefix.
      if let send = lead, builder.length >= RenderStyle.leadLength,
        index < document.roots.count - 1
      {
        lead = nil
        send(try builder.finish(complete: false))
      }
    }
    builder.trimTrailingNewline()
    return try builder.finish(complete: true)
  }
}

/// A node's display text in the rendered string: a UTF-16 start and length.
/// An atomic node (a break, an image attachment) has no display text of its
/// own, so any part of it covers the whole range.
struct NodeText: Sendable {
  var start: Int32 = -1
  var length: Int32 = 0
  var atomic = false
}

/// Fonts and colours. Dynamic system colours follow the window's appearance
/// at draw time, so one rendering serves light and dark mode.
enum RenderStyle {
  static let bodySize: CGFloat = 14
  static let indent: CGFloat = 24
  static let markerWidth: CGFloat = 20
  /// Space between a table cell's text and its rules.
  static let cellPadding: CGFloat = 8
  static let rowPadding: CGFloat = 4
  /// Wider cells overflow their column rather than widen the whole table.
  static let maxColumnWidth: CGFloat = 360
  /// TextKit 2 lays out and draws a whole paragraph to show any of it, so a
  /// pathological paragraph stalls the main thread. Past this many UTF-16
  /// units, the next space or soft break becomes a line break (both are one
  /// UTF-8 byte and one UTF-16 unit, so find mapping is unchanged).
  static let paragraphLimit = 16_384
  /// A run with no space this long gets a line break inserted at a grapheme
  /// boundary; find mapping skips these breaks.
  static let hardParagraphLimit = 32_768
  /// The lead of a long document: enough text to fill a large window at the
  /// smallest text size.
  static let leadLength = 32_768

  static func headingSize(_ level: Int) -> CGFloat {
    [28, 22, 18, 16, 14, 13][min(max(level, 1), 6) - 1]
  }
}

private struct Context {
  var indent: CGFloat = 0
  var quoted = false
  var tight = false
  /// The list marker the next paragraph of a fresh item starts with.
  var marker: String?
  var markerColor = NSColor.labelColor

  static let root = Context()
}

private struct Font: Hashable {
  var bold = false
  var italic = false
  var code = false
  var size = RenderStyle.bodySize

  func font(scale: CGFloat) -> NSFont {
    if code {
      return .monospacedSystemFont(ofSize: (size - 1) * scale, weight: bold ? .bold : .regular)
    }
    let size = size * scale
    var traits: NSFontDescriptor.SymbolicTraits = []
    if bold { traits.insert(.bold) }
    if italic { traits.insert(.italic) }
    let base = NSFont.systemFont(ofSize: size)
    let descriptor = base.fontDescriptor.withSymbolicTraits(traits)
    return NSFont(descriptor: descriptor, size: size) ?? base
  }
}

/// Shared run attributes: resolving a font descriptor per text run dominated
/// rendering, and a document uses only a handful of font/colour pairs.
private final class AttributeCache {
  private struct Key: Hashable {
    let font: Font
    let color: NSColor
  }
  private var values: [Key: [NSAttributedString.Key: Any]] = [:]
  private let scale: CGFloat

  init(scale: CGFloat) { self.scale = scale }

  func attributes(_ font: Font, _ color: NSColor) -> [NSAttributedString.Key: Any] {
    let key = Key(font: font, color: color)
    if let cached = values[key] { return cached }
    let value: [NSAttributedString.Key: Any] = [
      .font: font.font(scale: scale), .foregroundColor: color,
    ]
    values[key] = value
    return value
  }
}

private struct Builder {
  let document: CoreDocument
  let output = NSMutableAttributedString()
  let scale: CGFloat
  private let cache: AttributeCache
  private let imageCache: ImageCache
  var headingRanges: [Int: NSRange] = [:]
  var images: [RenderedImage] = []
  var tables: [NSRange] = []
  var nodeText: [NodeText]
  var insertedBreaks: [Int] = []
  /// Where the current TextKit paragraph starts in `output`.
  private var paragraphStart = 0
  private var visited = 0

  init(document: CoreDocument, images: ImageCache, scale: CGFloat) {
    self.document = document
    self.scale = scale
    cache = AttributeCache(scale: scale)
    imageCache = images
    nodeText = Array(repeating: NodeText(), count: document.nodes.count)
  }

  var length: Int { output.length }

  /// A text metric at the rendering scale.
  func pt(_ value: CGFloat) -> CGFloat { value * scale }

  /// Record that node `index` rendered its display text from `start` to here.
  mutating func mark(_ index: Int, from start: Int, atomic: Bool = false) {
    guard nodeText.indices.contains(index), length > start else { return }
    nodeText[index] = NodeText(
      start: Int32(clamping: start), length: Int32(clamping: length - start), atomic: atomic)
  }

  private func node(_ index: Int) throws -> md_node {
    guard document.nodes.indices.contains(index) else { throw CoreError.invalidSnapshot }
    return document.nodes[index]
  }

  private func children(_ node: md_node) throws -> [Int] {
    let start = Int(node.first_child)
    let end = start + Int(node.child_count)
    guard node.child_count == 0 || (start >= 0 && end <= document.children.count) else {
      throw CoreError.invalidSnapshot
    }
    return node.child_count == 0 ? [] : document.children[start..<end].map(Int.init)
  }

  private func source(_ node: md_node) -> String {
    let start = Int(node.span_start)
    let end = Int(node.span_end)
    guard start <= end, end <= document.source.count else { return "" }
    return String(decoding: document.source[start..<end], as: UTF8.self)
  }

  /// Check cancellation regularly; a 5 MiB document has hundreds of thousands of nodes.
  private mutating func tick() throws {
    visited += 1
    if visited & 1023 == 0 { try Task.checkCancellation() }
  }

  // MARK: Blocks

  mutating func block(_ index: Int, context: Context) throws {
    try tick()
    let node = try node(index)
    switch Int(node.kind) {
    case MD_NODE_PARAGRAPH:
      try paragraph(context) { try $0.inlines(of: node, font: Font()) }
    case MD_NODE_HEADING:
      let level = Int(node.value)
      var start = length
      try paragraph(context, spacingBefore: pt(level <= 2 ? 10 : 6)) { builder in
        // After any list or footnote marker, so the range is the heading text.
        start = builder.length
        try builder.inlines(of: node, font: Font(bold: true, size: RenderStyle.headingSize(level)))
      }
      headingRanges[index] = NSRange(location: start, length: max(length - 1 - start, 0))
    case MD_NODE_BLOCK_QUOTE, MD_NODE_ALERT:
      var inner = context
      inner.indent += pt(RenderStyle.indent)
      inner.quoted = true
      if Int(node.kind) == MD_NODE_ALERT {
        try paragraph(inner) { builder in
          builder.append(Self.alertLabel(node.value), font: Font(bold: true))
        }
      }
      for child in try children(node) {
        try block(child, context: inner)
        inner.marker = nil
      }
    case MD_NODE_LIST:
      try list(node, context: context)
    case MD_NODE_LIST_ITEM:
      // Items are rendered by `list`; a stray item renders its blocks.
      for child in try children(node) { try block(child, context: context) }
    case MD_NODE_CODE_BLOCK:
      try codeBlock(node, index: index, context: context)
    case MD_NODE_THEMATIC_BREAK:
      try paragraph(context, alignment: .center) { builder in
        builder.append("* * *", font: Font(), color: .tertiaryLabelColor)
      }
    case MD_NODE_TABLE:
      try table(node, context: context)
    case MD_NODE_FOOTNOTE_DEFINITION:
      let label = try document.string(node.text)
      var first = true
      for child in try children(node) {
        var inner = context
        if first { inner.marker = "[\(label)]" }
        inner.indent += pt(RenderStyle.indent)
        first = false
        try block(child, context: inner)
      }
    default:
      // Unsupported blocks (raw HTML, disabled extensions) stay inert source text.
      // Find segments index the source slice, so only line endings are trimmed.
      var text = source(node)
      while text.last?.isNewline == true { text.removeLast() }
      try paragraph(context) { builder in
        let start = builder.length
        builder.append(text, font: Font(code: true), color: .secondaryLabelColor)
        builder.mark(index, from: start)
      }
    }
  }

  private mutating func list(_ node: md_node, context: Context) throws {
    let ordered = node.flags & UInt32(MD_NODE_ORDERED) != 0
    var number = node.flags & UInt32(MD_NODE_HAS_START) != 0 ? node.number : 1
    for itemIndex in try children(node) {
      let item = try self.node(itemIndex)
      var inner = context
      inner.indent += pt(RenderStyle.indent)
      inner.tight = node.flags & UInt32(MD_NODE_TIGHT) != 0
      inner.marker = ordered ? "\(number)." : "•"
      inner.markerColor = .labelColor
      if item.flags & UInt32(MD_NODE_HAS_CHECKED) != 0 {
        let checked = item.flags & UInt32(MD_NODE_CHECKED) != 0
        inner.marker = checked ? "☑" : "☐"
        inner.markerColor = checked ? .controlAccentColor : .secondaryLabelColor
      }
      number &+= 1
      let blocks = try children(item)
      if blocks.isEmpty {
        try paragraph(inner) { _ in }
        continue
      }
      for child in blocks {
        try block(child, context: inner)
        inner.marker = nil
      }
    }
    // A tight list still ends with a block gap, unless it is nested in one.
    if !(context.tight && context.indent > 0), length > 0 {
      let last = output.mutableString.paragraphRange(for: NSRange(location: length - 1, length: 0))
      if let style = output.attribute(.paragraphStyle, at: last.location, effectiveRange: nil)
        as? NSParagraphStyle, let spaced = style.mutableCopy() as? NSMutableParagraphStyle
      {
        spaced.paragraphSpacing = pt(10)
        output.addAttribute(.paragraphStyle, value: spaced, range: last)
      }
    }
  }

  private mutating func codeBlock(_ node: md_node, index: Int, context: Context) throws {
    var code = try document.string(node.text)
    if code.hasSuffix("\n") { code.removeLast() }
    try paragraph(context, code: true) { builder in
      let start = builder.length
      builder.append(code, font: Font(code: true))
      builder.mark(index, from: start)
    }
  }

  /// One paragraph per row. Cells are separated by tabs, so a copied table
  /// pastes as tab-separated text; tab stops follow each column's measured
  /// width and alignment, and a leading tab aligns a centred or right first
  /// column.
  private mutating func table(_ node: md_node, context: Context) throws {
    let alignments = (0..<Int(node.aux_count)).map { column in
      let index = Int(node.aux) + column
      return document.alignments.indices.contains(index)
        ? Int(document.alignments[index]) : MD_ALIGN_NONE
    }
    let leading = alignments.first.map { $0 == MD_ALIGN_CENTER || $0 == MD_ALIGN_RIGHT } ?? false
    var rows: [(range: NSRange, header: Bool)] = []
    var widths = [CGFloat](repeating: 0, count: alignments.count)
    for rowIndex in try children(node) {
      try tick()
      let row = try self.node(rowIndex)
      let header = row.flags & UInt32(MD_NODE_HEADER) != 0
      let start = length
      for (column, cellIndex) in try children(row).enumerated() {
        if column > 0 || leading { append("\t", font: Font()) }
        let cellStart = length
        try inlines(of: try self.node(cellIndex), font: Font(bold: header))
        let width = output.attributedSubstring(from: NSRange(cellStart..<length)).boundingRect(
          with: CGSize(width: CGFloat.greatestFiniteMagnitude, height: .greatestFiniteMagnitude),
          options: [.usesLineFragmentOrigin]
        ).width.rounded(.up)
        if column >= widths.count { widths.append(0) }
        widths[column] = max(widths[column], min(width, pt(RenderStyle.maxColumnWidth)))
      }
      append("\n", font: Font())
      rows.append((NSRange(start..<length), header))
    }
    let pad = pt(RenderStyle.cellPadding)
    var edges = [context.indent]
    var tabs: [NSTextTab] = []
    for (column, width) in widths.enumerated() {
      let left = edges[column]
      let alignment = column < alignments.count ? alignments[column] : MD_ALIGN_NONE
      switch alignment {
      case MD_ALIGN_CENTER:
        tabs.append(NSTextTab(textAlignment: .center, location: left + pad + width / 2))
      case MD_ALIGN_RIGHT:
        tabs.append(NSTextTab(textAlignment: .right, location: left + pad + width))
      default: tabs.append(NSTextTab(textAlignment: .left, location: left + pad))
      }
      edges.append(left + width + 2 * pad)
    }
    if let first = rows.first, let last = rows.last {
      // Without the last row's terminator, like a heading's range.
      tables.append(NSRange(first.range.location..<(last.range.upperBound - 1)))
    }
    var body = 0
    for (index, row) in rows.enumerated() {
      let style = NSMutableParagraphStyle()
      style.tabStops = tabs
      style.firstLineHeadIndent = leading ? context.indent : context.indent + pad
      style.headIndent = context.indent + pad
      style.paragraphSpacingBefore = pt(RenderStyle.rowPadding)
      // The last row keeps the block gap below the table.
      style.paragraphSpacing = pt(index == rows.count - 1 ? 10 : RenderStyle.rowPadding)
      output.addAttribute(.paragraphStyle, value: style, range: row.range)
      if !row.header { body += 1 }
      let grid = TableRowStyle(
        edges: edges, header: row.header, first: index == 0, shaded: !row.header && body % 2 == 0,
        padding: pt(RenderStyle.rowPadding))
      output.addAttribute(.tableRow, value: grid, range: row.range)
      if context.quoted {
        output.addAttribute(.foregroundColor, value: NSColor.secondaryLabelColor, range: row.range)
      }
    }
  }

  /// Append one paragraph-level block: run `body`, end it with a newline and
  /// apply the indentation, spacing and quote styling for `context`.
  private mutating func paragraph(
    _ context: Context, spacingBefore: CGFloat = 0, alignment: NSTextAlignment = .natural,
    code: Bool = false, body: (inout Builder) throws -> Void
  ) throws {
    let start = length
    var head = context.indent
    if let marker = context.marker {
      append(marker + "\t", font: Font(), color: context.markerColor)
      head = context.indent - pt(RenderStyle.markerWidth)
    }
    try body(&self)
    append("\n", font: Font())
    let range = NSRange(start..<length)
    let style = NSMutableParagraphStyle()
    style.alignment = alignment
    style.firstLineHeadIndent = max(head, 0)
    style.headIndent = context.indent
    style.tabStops = [NSTextTab(textAlignment: .natural, location: context.indent)]
    style.defaultTabInterval = pt(120)
    style.lineSpacing = pt(code ? 1 : 3)
    style.paragraphSpacingBefore = spacingBefore
    output.addAttribute(.paragraphStyle, value: style, range: range)
    if code {
      output.addAttribute(.backgroundColor, value: NSColor.quaternarySystemFill, range: range)
    }
    if context.quoted {
      output.addAttribute(.foregroundColor, value: NSColor.secondaryLabelColor, range: range)
    }
    // Only the last line of a block gets the gap below it, so code and
    // hard-broken lines stay together.
    let last = output.mutableString.paragraphRange(
      for: NSRange(location: length - 1, length: 0))
    let spaced = style.mutableCopy() as! NSMutableParagraphStyle
    spaced.paragraphSpacing = pt(context.tight && context.indent > 0 ? 2 : 10)
    output.addAttribute(.paragraphStyle, value: spaced, range: last)
  }

  /// The document rendered so far, with the outline's headings it contains.
  func finish(complete: Bool) throws -> RenderedDocument {
    var headings: [RenderedHeading] = []
    for entry in document.outline {
      guard let range = headingRanges[Int(entry.node)] else { continue }
      headings.append(
        RenderedHeading(
          level: Int(entry.level), title: try document.string(entry.title),
          anchor: try document.string(entry.anchor), range: range))
    }
    return RenderedDocument(
      generation: document.generation, text: output.copy() as! NSAttributedString,
      headings: headings, images: images, tables: tables, isComplete: complete, scale: scale,
      nodeText: nodeText, insertedBreaks: insertedBreaks)
  }

  mutating func trimTrailingNewline() {
    if output.mutableString.hasSuffix("\n") {
      output.deleteCharacters(in: NSRange(location: length - 1, length: 1))
    }
  }

  // MARK: Inlines

  private mutating func inlines(of node: md_node, font: Font) throws {
    for child in try children(node) { try inline(child, font: font) }
  }

  private mutating func inline(_ index: Int, font: Font) throws {
    try tick()
    let node = try node(index)
    switch Int(node.kind) {
    case MD_NODE_TEXT:
      let start = length
      append(try document.string(node.text), font: font)
      mark(index, from: start)
    case MD_NODE_EMPHASIS:
      var inner = font
      inner.italic = true
      try inlines(of: node, font: inner)
    case MD_NODE_STRONG:
      var inner = font
      inner.bold = true
      try inlines(of: node, font: inner)
    case MD_NODE_STRIKETHROUGH:
      let start = length
      try inlines(of: node, font: font)
      output.addAttribute(
        .strikethroughStyle, value: NSUnderlineStyle.single.rawValue, range: NSRange(start..<length)
      )
    case MD_NODE_CODE_SPAN:
      var inner = font
      inner.code = true
      let start = length
      append(try document.string(node.text), font: inner)
      mark(index, from: start)
      output.addAttribute(
        .backgroundColor, value: NSColor.quaternarySystemFill, range: NSRange(start..<length))
    case MD_NODE_SOFT_BREAK:
      append(" ", font: font)
      mark(index, from: length - 1, atomic: true)
    case MD_NODE_HARD_BREAK:
      // A line separator keeps the paragraph's style and spacing together.
      append("\u{2028}", font: font)
      mark(index, from: length - 1, atomic: true)
    case MD_NODE_LINK:
      try link(node, font: font)
    case MD_NODE_IMAGE:
      try image(node, index: index, font: font)
    case MD_NODE_FOOTNOTE_REFERENCE:
      let start = length
      append("[\(try document.string(node.text))]", font: Font(size: font.size * 0.75))
      output.addAttribute(
        .baselineOffset, value: pt(font.size * 0.3), range: NSRange(start..<length))
    default:
      // Raw inline HTML and unsupported syntax: inert source text.
      let start = length
      append(source(node), font: font, color: .secondaryLabelColor)
      mark(index, from: start)
    }
  }

  private mutating func link(_ node: md_node, font: Font) throws {
    let start = length
    try inlines(of: node, font: font)
    let range = NSRange(start..<length)
    guard range.length > 0, document.links.indices.contains(Int(node.aux)) else { return }
    let intent = document.links[Int(node.aux)]
    let target = try document.string(intent.target)
    var destination: LinkDestination?
    switch Int(intent.target_kind) {
    case MD_TARGET_FRAGMENT:
      destination = .fragment(target)
    case MD_TARGET_EXTERNAL:
      if let url = URL(string: target), LinkDestination(link: url) != nil {
        destination = .external(url)
      }
    default: break
    }
    if let destination, let url = destination.url {
      output.addAttribute(.link, value: url, range: range)
      if node.flags & UInt32(MD_NODE_HAS_TITLE) != 0 {
        output.addAttribute(.toolTip, value: try document.string(node.extra), range: range)
      }
    } else {
      // Links to other files need a folder grant (MAC-002A); denied links stay inert.
      let reason =
        Int(intent.target_kind) == MD_TARGET_REQUIRES_CAPABILITY
        ? "Links to other files are not available yet" : "This link was blocked"
      output.addAttribute(.toolTip, value: reason, range: range)
      output.addAttribute(
        .underlineStyle,
        value: NSUnderlineStyle.single.rawValue | NSUnderlineStyle.patternDot.rawValue,
        range: range)
    }
  }

  /// A local image the core allows is read and decoded once per render; any
  /// other image, or one that fails, shows its alt text and the reason.
  private mutating func image(_ node: md_node, index: Int, font: Font) throws {
    let alt = try document.string(node.text)
    var result: Result<CGImage, ImageFailure> = .failure(.blocked)
    if document.resources.indices.contains(Int(node.aux)) {
      let intent = document.resources[Int(node.aux)]
      switch Int(intent.target_kind) {
      case MD_TARGET_REQUIRES_CAPABILITY:
        try Task.checkCancellation()
        result = imageCache.image(path: try document.string(intent.target))
      case MD_TARGET_DENIED where Int(intent.denial) == MD_DENIAL_REMOTE_RESOURCE:
        result = .failure(.remote)
      default: break
      }
    }
    let start = length
    switch result {
    case .success(let image):
      let attachment = NSMutableAttributedString(
        attachment: ImageAttachment(image: image, description: alt.isEmpty ? "Image" : alt))
      attachment.addAttributes(
        cache.attributes(font, .labelColor), range: NSRange(0..<attachment.length))
      output.append(attachment)
      mark(index, from: start, atomic: true)
      let tip = node.flags & UInt32(MD_NODE_HAS_TITLE) != 0 ? try document.string(node.extra) : alt
      if !tip.isEmpty { output.addAttribute(.toolTip, value: tip, range: NSRange(start..<length)) }
      images.append(RenderedImage(range: NSRange(start..<length), failure: nil, alt: alt))
    case .failure(let failure):
      var inner = font
      inner.italic = true
      append(alt.isEmpty ? "[image]" : "[\(alt)]", font: inner, color: .secondaryLabelColor)
      // The alt text follows the opening bracket.
      if !alt.isEmpty, nodeText.indices.contains(index) {
        nodeText[index] = NodeText(
          start: Int32(clamping: start + 1), length: Int32(clamping: (alt as NSString).length))
      }
      output.addAttribute(.toolTip, value: failure.message, range: NSRange(start..<length))
      images.append(RenderedImage(range: NSRange(start..<length), failure: failure, alt: alt))
    }
  }

  private mutating func append(_ text: String, font: Font, color: NSColor = .labelColor) {
    guard !text.isEmpty else { return }
    let units = text.utf16
    let start = length
    var string = text
    if start - paragraphStart + units.count > RenderStyle.paragraphLimit {
      string = bounded(units, at: start)
    } else if let last = units.lastIndex(where: Self.isParagraphSeparator) {
      paragraphStart = start + units.distance(from: units.startIndex, to: last) + 1
    }
    output.append(NSAttributedString(string: string, attributes: cache.attributes(font, color)))
  }

  private static func isParagraphSeparator(_ unit: UTF16.CodeUnit) -> Bool {
    unit == 0x0A || unit == 0x0D || unit == 0x2029
  }

  /// `text`, to be appended at `start`, with line breaks that keep every
  /// TextKit paragraph near `RenderStyle.paragraphLimit` units.
  private mutating func bounded(_ text: String.UTF16View, at start: Int) -> String {
    let source = Array(text)
    let string = NSString(characters: source, length: source.count)
    var units: [UTF16.CodeUnit] = []
    units.reserveCapacity(source.count + source.count / RenderStyle.hardParagraphLimit + 1)
    var run = start - paragraphStart
    for (index, unit) in source.enumerated() {
      if Self.isParagraphSeparator(unit) {
        run = 0
      } else if unit == 0x20 && run >= RenderStyle.paragraphLimit {
        units.append(0x0A)
        run = 0
        continue
      } else if run >= RenderStyle.hardParagraphLimit
        && string.rangeOfComposedCharacterSequence(at: index).location == index
      {
        insertedBreaks.append(start + units.count)
        units.append(0x0A)
        run = 0
      }
      units.append(unit)
      run += 1
    }
    paragraphStart = start + units.count - run
    return String(utf16CodeUnits: units, count: units.count)
  }

  private static func alertLabel(_ value: UInt32) -> String {
    switch Int(value) {
    case MD_ALERT_TIP: "Tip"
    case MD_ALERT_IMPORTANT: "Important"
    case MD_ALERT_WARNING: "Warning"
    case MD_ALERT_CAUTION: "Caution"
    default: "Note"
    }
  }
}
