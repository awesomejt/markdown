import AppKit
import MarkdownCore

/// A reading position in rendered text: the UTF-16 location of the line at
/// the top of the view, and the selection.
public struct ReadingPosition: Sendable, Equatable {
  public var top: Int
  public var selection: NSRange

  public init(top: Int, selection: NSRange) {
    self.top = top
    self.selection = selection
  }
}

/// A reading position as core block anchors (specs/session), which survive
/// edits, reloads and relaunches where rendered offsets do not. An empty
/// selection keeps only its insertion point. Anchors are private
/// document-derived state: never log or upload them.
public struct SavedPosition: Sendable, Equatable {
  public let top: Data
  public let selectionStart: Data
  /// Nil for an insertion point.
  public let selectionEnd: Data?

  public init(top: Data, selectionStart: Data, selectionEnd: Data?) {
    self.top = top
    self.selectionStart = selectionStart
    self.selectionEnd = selectionEnd
  }
}

/// One node's display text in the rendered string and the source span it
/// came from, for mapping between rendered locations and source offsets.
struct PositionMark: Sendable {
  let unit: Int32
  let length: Int32
  let source: UInt32
  let sourceEnd: UInt32
  let atomic: Bool
}

extension RenderedDocument {
  /// Anchor a rendered position in this document's snapshot. Runs core calls,
  /// so keep it off the main thread for long documents.
  public func save(_ position: ReadingPosition) throws -> SavedPosition {
    let end = position.selection.length > 0 ? position.selection.upperBound : nil
    return SavedPosition(
      top: try snapshot.anchor(atSource: UInt64(sourceOffset(at: position.top))),
      selectionStart: try snapshot.anchor(
        atSource: UInt64(sourceOffset(at: position.selection.location))),
      selectionEnd: try end.map { try snapshot.anchor(atSource: UInt64(sourceOffset(at: $0))) })
  }

  /// Resolve a saved position in this document's snapshot. Nil when the top
  /// anchor is lost or unreadable; a lost selection becomes an insertion
  /// point at the top line.
  public func restore(_ saved: SavedPosition) -> ReadingPosition? {
    func locate(_ anchor: Data) -> Int? {
      guard let resolution = try? snapshot.resolve(anchor) else { return nil }
      return location(atSource: Int(clamping: resolution.sourceOffset))
    }
    guard let top = locate(saved.top) else { return nil }
    guard let start = locate(saved.selectionStart) else {
      return ReadingPosition(top: top, selection: NSRange(location: top, length: 0))
    }
    let end = saved.selectionEnd.flatMap(locate).map { max($0, start) } ?? start
    return ReadingPosition(top: top, selection: NSRange(start..<end))
  }

  /// The source byte offset shown at a rendered location. Inside a node's
  /// display text it advances by the UTF-8 length of the text before the
  /// location, clamped to the node's span; elsewhere (a list marker, a
  /// table rule, a block gap) it snaps to the next node's start.
  func sourceOffset(at location: Int) -> Int {
    guard !positions.isEmpty else { return 0 }
    var low = 0
    var high = positions.count
    while low < high {
      let middle = (low + high) / 2
      if Int(positions[middle].unit) <= location { low = middle + 1 } else { high = middle }
    }
    // `low` is the first mark after the location.
    if low > 0 {
      let mark = positions[low - 1]
      let start = Int(mark.unit)
      if location < start + Int(mark.length) {
        guard !mark.atomic, location > start else { return Int(mark.source) }
        let prefix = (text.string as NSString).substring(with: NSRange(start..<location))
        return min(Int(mark.source) + prefix.utf8.count, Int(mark.sourceEnd))
      }
    }
    return Int(low < positions.count ? positions[low].source : positions[low - 1].sourceEnd)
  }

  /// The rendered location of a source byte offset: inside the node whose
  /// span holds it (the inverse of `sourceOffset(at:)`), else the start of
  /// the first node after it, else the end of the text.
  func location(atSource offset: Int) -> Int {
    var after: PositionMark?
    for mark in positions {
      let source = Int(mark.source)
      if source <= offset, offset < Int(mark.sourceEnd) {
        guard !mark.atomic else { return Int(mark.unit) }
        let string = text.string as NSString
        let start = Int(mark.unit)
        let end = start + Int(mark.length)
        var bytes = source
        var unit = start
        while unit < end, bytes < offset {
          let code = string.character(at: unit)
          if UTF16.isLeadSurrogate(code), unit + 1 < end {
            bytes += 4
            unit += 2
          } else {
            bytes += code < 0x80 ? 1 : code < 0x800 ? 2 : 3
            unit += 1
          }
        }
        return unit
      }
      if source >= offset, after.map({ source < Int($0.source) }) ?? true { after = mark }
    }
    return after.map { Int($0.unit) } ?? text.length
  }
}
