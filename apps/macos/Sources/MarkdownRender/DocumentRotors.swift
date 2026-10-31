import AppKit

/// VoiceOver rotors for the reading surface (MAC-001G): headings, headings by
/// level, tables and images. The document is one text view, so these are the
/// only way to move by structure; each item is a range of that view's text.
/// AppKit asks for rotor items on the main thread, which the preconcurrency
/// conformance checks at run time.
@MainActor
final class DocumentRotors: NSObject, @preconcurrency NSAccessibilityCustomRotorItemSearchDelegate {
  private weak var textView: NSTextView?
  private var document: RenderedDocument?
  private(set) var rotors: [NSAccessibilityCustomRotor] = []
  private var kinds: [ObjectIdentifier: Kind] = [:]

  enum Kind: Equatable {
    case heading(level: Int?)
    case table
    case image
  }

  init(textView: NSTextView) {
    self.textView = textView
    super.init()
    let types: [(NSAccessibilityCustomRotor.RotorType, Kind)] =
      [(.heading, .heading(level: nil))]
      + [
        .headingLevel1, .headingLevel2, .headingLevel3, .headingLevel4, .headingLevel5,
        .headingLevel6,
      ].enumerated().map { ($1, .heading(level: $0 + 1)) }
      + [(.table, .table), (.image, .image)]
    for (type, kind) in types {
      let rotor = NSAccessibilityCustomRotor(rotorType: type, itemSearchDelegate: self)
      kinds[ObjectIdentifier(rotor)] = kind
      rotors.append(rotor)
    }
  }

  func show(_ document: RenderedDocument?) {
    self.document = document
  }

  /// The items of one rotor, in reading order: a range and its label.
  func items(_ kind: Kind) -> [(range: NSRange, label: String)] {
    guard let document else { return [] }
    switch kind {
    case .heading(let level):
      return document.headings.filter { level == nil || $0.level == level }.map {
        ($0.range, $0.title)
      }
    case .table:
      return document.tables.enumerated().map { ($1, "Table \($0 + 1)") }
    case .image:
      return document.images.map { ($0.range, Self.label(of: $0, in: document)) }
    }
  }

  /// A loaded image's alt text, or the bracketed alt text shown in its place.
  private static func label(of image: RenderedImage, in document: RenderedDocument) -> String {
    let attachment =
      document.text.attribute(.attachment, at: image.range.location, effectiveRange: nil)
      as? NSTextAttachment
    if let description = attachment?.image?.accessibilityDescription { return description }
    return (document.text.string as NSString).substring(with: image.range)
  }

  /// The next or previous item after the current one (the first or last
  /// without one) whose label contains the filter text.
  func rotor(
    _ rotor: NSAccessibilityCustomRotor,
    resultFor searchParameters: NSAccessibilityCustomRotor.SearchParameters
  ) -> NSAccessibilityCustomRotor.ItemResult? {
    guard let textView, let kind = kinds[ObjectIdentifier(rotor)] else { return nil }
    let items = items(kind)
    let filter = searchParameters.filterString
    let forward = searchParameters.searchDirection == .next
    let current = searchParameters.currentItem?.targetRange.location
    // Items are in reading order: find where the current one falls.
    var low = 0
    var high = items.count
    while low < high {
      let middle = (low + high) / 2
      if let current, items[middle].range.location <= current {
        low = middle + 1
      } else {
        high = middle
      }
    }
    let candidates: [Int]
    if forward {
      candidates = Array(low..<items.count)
    } else if let current {
      let before = items[..<low].lastIndex { $0.range.location < current } ?? -1
      candidates = before < 0 ? [] : Array((0...before).reversed())
    } else {
      candidates = Array(items.indices.reversed())
    }
    guard
      let index = candidates.first(where: {
        filter.isEmpty || items[$0].label.localizedCaseInsensitiveContains(filter)
      })
    else { return nil }
    let result = NSAccessibilityCustomRotor.ItemResult(targetElement: textView)
    result.targetRange = items[index].range
    result.customLabel = items[index].label
    return result
  }
}
