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

  /// The first heading with this anchor, as GitHub resolves duplicate slugs.
  public func heading(anchor: String) -> RenderedHeading? {
    headings.first { $0.anchor == anchor }
  }
}

/// Converts the owned model-v2 snapshot into one TextKit 2 attributed string
/// (MAC-001D). Headings, prose, lists, quotes, links and code are styled;
/// tables, images, task markers, alerts and footnotes get plain readable
/// fallbacks until MAC-001E. Raw HTML and unsupported syntax stay inert text.
public enum DocumentRenderer {
  public static func render(_ document: CoreDocument) throws -> RenderedDocument {
    var builder = Builder(document: document)
    for root in document.roots { try builder.block(Int(root), context: .root) }
    builder.trimTrailingNewline()
    var headings: [RenderedHeading] = []
    for entry in document.outline {
      guard let range = builder.headingRanges[Int(entry.node)] else { continue }
      headings.append(
        RenderedHeading(
          level: Int(entry.level), title: try document.string(entry.title),
          anchor: try document.string(entry.anchor), range: range))
    }
    return RenderedDocument(
      generation: document.generation, text: builder.output.copy() as! NSAttributedString,
      headings: headings)
  }
}

/// Fonts and colours. Dynamic system colours follow the window's appearance
/// at draw time, so one rendering serves light and dark mode.
enum RenderStyle {
  static let bodySize: CGFloat = 14
  static let indent: CGFloat = 24
  static let markerWidth: CGFloat = 20

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

  static let root = Context()
}

private struct Font: Hashable {
  var bold = false
  var italic = false
  var code = false
  var size = RenderStyle.bodySize

  var font: NSFont {
    if code { return .monospacedSystemFont(ofSize: size - 1, weight: bold ? .bold : .regular) }
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

  func attributes(_ font: Font, _ color: NSColor) -> [NSAttributedString.Key: Any] {
    let key = Key(font: font, color: color)
    if let cached = values[key] { return cached }
    let value: [NSAttributedString.Key: Any] = [.font: font.font, .foregroundColor: color]
    values[key] = value
    return value
  }
}

private struct Builder {
  let document: CoreDocument
  let output = NSMutableAttributedString()
  private let cache = AttributeCache()
  var headingRanges: [Int: NSRange] = [:]
  private var visited = 0

  init(document: CoreDocument) { self.document = document }

  var length: Int { output.length }

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
      try paragraph(context, spacingBefore: level <= 2 ? 10 : 6) { builder in
        // After any list or footnote marker, so the range is the heading text.
        start = builder.length
        try builder.inlines(of: node, font: Font(bold: true, size: RenderStyle.headingSize(level)))
      }
      headingRanges[index] = NSRange(location: start, length: max(length - 1 - start, 0))
    case MD_NODE_BLOCK_QUOTE, MD_NODE_ALERT:
      var inner = context
      inner.indent += RenderStyle.indent
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
      try codeBlock(node, context: context)
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
        inner.indent += RenderStyle.indent
        first = false
        try block(child, context: inner)
      }
    default:
      // Unsupported blocks (raw HTML, disabled extensions) stay inert source text.
      try paragraph(context) { builder in
        builder.append(
          builder.source(node).trimmingCharacters(in: .newlines), font: Font(code: true),
          color: .secondaryLabelColor)
      }
    }
  }

  private mutating func list(_ node: md_node, context: Context) throws {
    let ordered = node.flags & UInt32(MD_NODE_ORDERED) != 0
    var number = node.flags & UInt32(MD_NODE_HAS_START) != 0 ? node.number : 1
    for itemIndex in try children(node) {
      let item = try self.node(itemIndex)
      var inner = context
      inner.indent += RenderStyle.indent
      inner.tight = node.flags & UInt32(MD_NODE_TIGHT) != 0
      var marker = ordered ? "\(number)." : "•"
      if item.flags & UInt32(MD_NODE_HAS_CHECKED) != 0 {
        marker = item.flags & UInt32(MD_NODE_CHECKED) != 0 ? "☑" : "☐"
      }
      number &+= 1
      inner.marker = marker
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
        spaced.paragraphSpacing = 10
        output.addAttribute(.paragraphStyle, value: spaced, range: last)
      }
    }
  }

  private mutating func codeBlock(_ node: md_node, context: Context) throws {
    var code = try document.string(node.text)
    if code.hasSuffix("\n") { code.removeLast() }
    try paragraph(context, code: true) { builder in
      builder.append(code, font: Font(code: true))
    }
  }

  private mutating func table(_ node: md_node, context: Context) throws {
    for rowIndex in try children(node) {
      let row = try self.node(rowIndex)
      let header = row.flags & UInt32(MD_NODE_HEADER) != 0
      try paragraph(context) { builder in
        for (column, cellIndex) in try builder.children(row).enumerated() {
          if column > 0 { builder.append("\t", font: Font()) }
          try builder.inlines(of: try builder.node(cellIndex), font: Font(bold: header))
        }
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
      append(marker + "\t", font: Font())
      head = context.indent - RenderStyle.markerWidth
    }
    try body(&self)
    append("\n", font: Font())
    let range = NSRange(start..<length)
    let style = NSMutableParagraphStyle()
    style.alignment = alignment
    style.firstLineHeadIndent = max(head, 0)
    style.headIndent = context.indent
    style.tabStops = [NSTextTab(textAlignment: .natural, location: context.indent)]
    style.defaultTabInterval = 120
    style.lineSpacing = code ? 1 : 3
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
    spaced.paragraphSpacing = context.tight && context.indent > 0 ? 2 : 10
    output.addAttribute(.paragraphStyle, value: spaced, range: last)
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
      append(try document.string(node.text), font: font)
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
      output.addAttribute(
        .backgroundColor, value: NSColor.quaternarySystemFill, range: NSRange(start..<length))
    case MD_NODE_SOFT_BREAK:
      append(" ", font: font)
    case MD_NODE_HARD_BREAK:
      // A line separator keeps the paragraph's style and spacing together.
      append("\u{2028}", font: font)
    case MD_NODE_LINK:
      try link(node, font: font)
    case MD_NODE_IMAGE:
      var inner = font
      inner.italic = true
      let alt = try document.string(node.text)
      let start = length
      append(alt.isEmpty ? "[image]" : "[\(alt)]", font: inner, color: .secondaryLabelColor)
      output.addAttribute(.toolTip, value: "Image not shown", range: NSRange(start..<length))
    case MD_NODE_FOOTNOTE_REFERENCE:
      let start = length
      append("[\(try document.string(node.text))]", font: Font(size: font.size * 0.75))
      output.addAttribute(.baselineOffset, value: font.size * 0.3, range: NSRange(start..<length))
    default:
      // Raw inline HTML and unsupported syntax: inert source text.
      append(source(node), font: font, color: .secondaryLabelColor)
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

  /// `output` is a reference, so appending needs no mutation of the builder.
  private func append(_ text: String, font: Font, color: NSColor = .labelColor) {
    guard !text.isEmpty else { return }
    output.append(NSAttributedString(string: text, attributes: cache.attributes(font, color)))
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
