import CMarkdown
import Foundation

/// Immutable owned values, never an md_snapshot_view or a borrowed pointer.
/// The unchecked conformance covers imported C records: all stored records consist
/// solely of fixed-width integers/md_str offsets (frozen ABI v1). Arrays and Data
/// are copied before the snapshot is released; no mutable/reference C storage escapes.
public struct CoreDocument: @unchecked Sendable {
  public let generation: UInt64
  public let source: Data
  public let strings: Data
  public let nodes: [md_node]
  public let roots: [UInt32]
  public let children: [UInt32]
  public let alignments: [UInt32]
  public let links: [md_intent]
  public let resources: [md_intent]
  public let outline: [md_outline_entry]
  public let diagnostics: [md_diagnostic]
  public let properties: [md_property]
  public let stringList: [md_str]
  public let capabilities: UInt32
  public let flags: UInt32
  public let frontMatterRange: Range<UInt32>?
  public let documentID: String
  public let revisionID: String
  public let sourceSHA256: String
  public let title: String?
  public let copiedBytes: UInt64

  init(copying view: md_snapshot_view) throws {
    guard view.model_version == UInt32(MD_MODEL_V2) else { throw CoreError.incompatibleABI }
    // Price the whole copy before allocating any array; all additions are checked.
    var bytes: UInt64 = 0
    func price<T>(_ pointer: UnsafePointer<T>?, _ count: UInt64) throws {
      let (cost, overflow) = count.multipliedReportingOverflow(by: UInt64(MemoryLayout<T>.stride))
      let (total, sumOverflow) = bytes.addingReportingOverflow(cost)
      guard !overflow, !sumOverflow, total <= CoreRuntime.maxSnapshotBytes,
        count == 0 || pointer != nil
      else { throw CoreError.snapshotBudget }
      bytes = total
    }
    try price(view.source, view.source_length)
    try price(view.strings, view.strings_length)
    try price(view.nodes, UInt64(view.node_count))
    try price(view.roots, UInt64(view.root_count))
    try price(view.children, UInt64(view.child_count))
    try price(view.alignments, UInt64(view.alignment_count))
    try price(view.links, UInt64(view.link_count))
    try price(view.resources, UInt64(view.resource_count))
    try price(view.outline, UInt64(view.outline_count))
    try price(view.diagnostics, UInt64(view.diagnostic_count))
    try price(view.properties, UInt64(view.property_count))
    try price(view.string_list, UInt64(view.string_list_count))
    func copy<T>(_ pointer: UnsafePointer<T>?, _ count: UInt32) -> [T] {
      count == 0 ? [] : Array(UnsafeBufferPointer(start: pointer, count: Int(count)))
    }
    func data(_ pointer: UnsafePointer<UInt8>?, _ count: UInt64) -> Data {
      count == 0 ? Data() : Data(bytes: pointer!, count: Int(count))
    }
    let table = data(view.strings, view.strings_length)
    func text(_ range: md_str) throws -> String {
      try Self.decode(range, from: table)
    }
    guard view.front_matter_start <= view.front_matter_end,
      UInt64(view.front_matter_end) <= view.source_length
    else { throw CoreError.invalidSnapshot }
    generation = view.generation
    source = data(view.source, view.source_length)
    strings = table
    nodes = copy(view.nodes, view.node_count)
    roots = copy(view.roots, view.root_count)
    children = copy(view.children, view.child_count)
    alignments = copy(view.alignments, view.alignment_count)
    links = copy(view.links, view.link_count)
    resources = copy(view.resources, view.resource_count)
    outline = copy(view.outline, view.outline_count)
    diagnostics = copy(view.diagnostics, view.diagnostic_count)
    properties = copy(view.properties, view.property_count)
    stringList = copy(view.string_list, view.string_list_count)
    capabilities = view.capabilities
    flags = view.flags
    frontMatterRange =
      view.flags & UInt32(MD_VIEW_HAS_FRONT_MATTER) == 0
      ? nil
      : view.front_matter_start..<view.front_matter_end
    documentID = try text(view.document_id)
    revisionID = try text(view.revision_id)
    sourceSHA256 = try text(view.source_sha256)
    title = view.flags & UInt32(MD_VIEW_HAS_TITLE) == 0 ? nil : try text(view.title)
    copiedBytes = bytes
  }

  public func string(_ range: md_str) throws -> String { try Self.decode(range, from: strings) }

  private static func decode(_ range: md_str, from table: Data) throws -> String {
    let start = Int(range.offset)
    let end = Int(range.offset) + Int(range.length)
    guard start <= end, end <= table.count,
      let text = String(data: table[start..<end], encoding: .utf8)
    else { throw CoreError.invalidSnapshot }
    return text
  }
}
