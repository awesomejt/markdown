import AppKit

extension NSAttributedString.Key {
  /// A `TableRowStyle` on every character of one table row's paragraph.
  static let tableRow = NSAttributedString.Key("io.github.awesomejt.markdown.tableRow")
}

/// The grid geometry of one table row. TextKit 2 lays `NSTextTable` blocks
/// out as stacked paragraphs, so a table is one paragraph per row: cells sit at
/// measured tab stops, and `TableRowFragment` draws the shading and rules.
final class TableRowStyle: NSObject {
  /// Column boundaries in text-container coordinates, before line padding.
  let edges: [CGFloat]
  let header: Bool
  let first: Bool
  /// Alternate body rows are shaded.
  let shaded: Bool
  /// Space added above and below the row's text, inside its cell.
  let padding: CGFloat

  init(edges: [CGFloat], header: Bool, first: Bool, shaded: Bool, padding: CGFloat) {
    self.edges = edges
    self.header = header
    self.first = first
    self.shaded = shaded
    self.padding = padding
  }
}

/// Draws one table row's cell backgrounds and rules behind its text.
final class TableRowFragment: NSTextLayoutFragment {
  let style: TableRowStyle

  init(style: TableRowStyle, textElement: NSTextElement, range: NSTextRange?) {
    self.style = style
    super.init(textElement: textElement, range: range)
  }

  required init?(coder: NSCoder) { fatalError("Table fragments are constructed in code") }

  /// The row's cell area in fragment coordinates.
  var gridRect: CGRect {
    let lines = textLineFragments.reduce(CGRect.null) { $0.union($1.typographicBounds) }
    guard !lines.isNull, let first = style.edges.first, let last = style.edges.last else {
      return .zero
    }
    let padding = textLayoutManager?.textContainer?.lineFragmentPadding ?? 0
    let left = first + padding - layoutFragmentFrame.minX
    return CGRect(
      x: left, y: lines.minY - style.padding, width: last - first,
      height: lines.height + 2 * style.padding)
  }

  override var renderingSurfaceBounds: CGRect {
    super.renderingSurfaceBounds.union(gridRect.insetBy(dx: -1, dy: -1))
  }

  override func draw(at point: CGPoint, in context: CGContext) {
    let grid = gridRect.offsetBy(dx: point.x, dy: point.y)
    if grid.width > 0 {
      context.saveGState()
      if style.header || style.shaded {
        let fill: NSColor = style.header ? .tertiarySystemFill : .quinarySystemFill
        context.setFillColor(fill.cgColor)
        context.fill(grid)
      }
      let line = 1 / max(context.userSpaceToDeviceSpaceTransform.a, 1)
      context.setStrokeColor(NSColor.separatorColor.cgColor)
      context.setLineWidth(line)
      let shift = grid.minX - style.edges[0]
      for edge in style.edges {
        let x = edge + shift
        context.move(to: CGPoint(x: x, y: grid.minY))
        context.addLine(to: CGPoint(x: x, y: grid.maxY))
      }
      if style.first {
        context.move(to: CGPoint(x: grid.minX, y: grid.minY))
        context.addLine(to: CGPoint(x: grid.maxX, y: grid.minY))
      }
      context.move(to: CGPoint(x: grid.minX, y: grid.maxY))
      context.addLine(to: CGPoint(x: grid.maxX, y: grid.maxY))
      context.strokePath()
      context.restoreGState()
    }
    super.draw(at: point, in: context)
  }
}
