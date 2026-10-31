import AppKit
import MarkdownCore

/// Core find matches located in the rendered text (MAC-001F). The core finds
/// in the document's visible semantic text and reports each match as
/// segments of node display text; those map exactly to the rendered string,
/// so highlighting never searches the attributed text itself.
public struct FindMatches: Sendable, Equatable {
  public let generation: UInt64
  /// One UTF-16 range per match, in reading order.
  public let ranges: [NSRange]
  /// The core stopped at its match limit; more matches exist.
  public let truncated: Bool

  public init(generation: UInt64, ranges: [NSRange], truncated: Bool) {
    self.generation = generation
    self.ranges = ranges
    self.truncated = truncated
  }

  public var count: Int { ranges.count }

  /// The first match starting at or after `location`, wrapping to the first.
  public func index(atOrAfter location: Int) -> Int? {
    guard !ranges.isEmpty else { return nil }
    var low = 0
    var high = ranges.count
    while low < high {
      let middle = (low + high) / 2
      if ranges[middle].location < location { low = middle + 1 } else { high = middle }
    }
    return low == ranges.count ? 0 : low
  }

  /// The last match starting before `location`, wrapping to the last.
  public func index(before location: Int) -> Int? {
    guard let next = index(atOrAfter: location) else { return nil }
    return ranges[next].location < location ? ranges.count - 1 : index(after: next, backwards: true)
  }

  /// The match after (or before) `index`, wrapping around the document.
  public func index(after index: Int, backwards: Bool = false) -> Int? {
    guard !ranges.isEmpty else { return nil }
    return (index + (backwards ? ranges.count - 1 : 1)) % ranges.count
  }

  /// "3 of 12", "1 of 10,000+" or "Not found".
  public func status(current: Int?) -> String {
    guard !ranges.isEmpty else { return "Not found" }
    let total = ranges.count.formatted() + (truncated ? "+" : "")
    guard let current else { return ranges.count == 1 ? "1 match" : "\(total) matches" }
    return "\((current + 1).formatted()) of \(total)"
  }
}

extension RenderedDocument {
  /// Map a core find result for this document's snapshot to rendered ranges.
  /// A result for another generation maps to no matches.
  public func matches(for result: CoreFindResult) -> FindMatches {
    guard result.generation == generation else {
      return FindMatches(generation: generation, ranges: [], truncated: false)
    }
    let string = text.string as NSString
    // Segments arrive in reading order, so offsets within one node only grow:
    // resume from the previous position instead of rescanning a long node.
    var cursor = (node: -1, byte: 0, unit: 0)
    func utf16Offset(_ node: Int, _ entry: NodeText, byte target: Int) -> Int {
      if cursor.node != node || target < cursor.byte { cursor = (node, 0, 0) }
      var (byte, unit) = (cursor.byte, cursor.unit)
      let base = Int(entry.start)
      let length = Int(entry.length)
      while byte < target, unit < length {
        let code = string.character(at: base + unit)
        if UTF16.isLeadSurrogate(code), unit + 1 < length {
          byte += 4
          unit += 2
        } else {
          byte += code < 0x80 ? 1 : code < 0x800 ? 2 : 3
          unit += 1
        }
      }
      cursor = (node, byte, unit)
      return unit
    }
    var ranges: [NSRange] = []
    ranges.reserveCapacity(result.matches.count)
    for match in result.matches {
      var lower = Int.max
      var upper = Int.min
      for segment in result.segments[match.segments] {
        let node = Int(segment.node)
        guard nodeText.indices.contains(node) else { continue }
        let entry = nodeText[node]
        guard entry.start >= 0 else { continue }
        let base = Int(entry.start)
        var start = base
        var end = base + Int(entry.length)
        if !entry.atomic {
          guard !segment.text.isEmpty else { continue }
          start = base + utf16Offset(node, entry, byte: Int(segment.text.lowerBound))
          end = base + utf16Offset(node, entry, byte: Int(segment.text.upperBound))
        }
        guard end > start else { continue }
        lower = min(lower, start)
        upper = max(upper, end)
      }
      if lower < upper { ranges.append(NSRange(lower..<upper)) }
    }
    return FindMatches(generation: generation, ranges: ranges, truncated: result.truncated)
  }
}

/// The text size steps of View → Make Text Bigger/Smaller and Settings.
public enum TextScale {
  public static let steps: [CGFloat] = [0.75, 0.85, 1, 1.15, 1.3, 1.5, 1.75, 2, 2.5]
  public static let standard: CGFloat = 1

  /// The nearest step, so a stored or hand-edited value stays in range.
  public static func clamped(_ value: CGFloat) -> CGFloat {
    guard value.isFinite else { return standard }
    return steps.min { abs($0 - value) < abs($1 - value) } ?? standard
  }

  public static func larger(_ value: CGFloat) -> CGFloat? {
    steps.first { $0 > clamped(value) }
  }

  public static func smaller(_ value: CGFloat) -> CGFloat? {
    steps.last { $0 < clamped(value) }
  }
}
