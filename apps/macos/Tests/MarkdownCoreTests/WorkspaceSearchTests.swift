import Foundation
import Testing

@testable import MarkdownCore

/// The workspace search C ABI wrappers (MAC-002C): name ranks and ranges,
/// content hits with snippets and targets, progress, skips and stops.
struct WorkspaceSearchTests {
  private func search(
    _ query: String, _ files: [(String, String)],
    _ options: CoreContentOptions = CoreContentOptions()
  ) throws -> (CoreContentSearch, [CoreContentOutcome]) {
    let search = try CoreContentSearch(query: query, options: options)
    var outcomes: [CoreContentOutcome] = []
    for (path, text) in files {
      let data = Data(text.utf8)
      // As the adapter does: search only what admission lets through.
      switch try search.admit(path: path, size: UInt64(data.count)) {
      case .read: outcomes.append(try search.search(path: path, content: data))
      case .skipped(let skip): outcomes.append(.skipped(skip))
      case .stopped(let stop):
        outcomes.append(.stopped(stop))
        return (search, outcomes)
      }
    }
    return (search, outcomes)
  }

  @Test func namesAreRankedWithUTF16Ranges() throws {
    let paths = ["docs/install-guide.md", "café/Install.md", "notes.md", "a/reinstall.md", "x.txt"]
    let result = try CoreWorkspaceSearch.findNames(paths, query: "install")
    // Prefix ranks first, then name, then path; ties by path bytes.
    #expect(result.hits.map(\.entry) == [1, 0, 3])
    #expect(result.hits.map(\.rank) == [.prefix, .prefix, .name])
    // "café/" is five UTF-16 units but six UTF-8 bytes.
    #expect(result.hits[0].range == NSRange(location: 5, length: 7))
    #expect((paths[3] as NSString).substring(with: result.hits[2].range) == "install")
    #expect(result.totalHits == 3 && !result.truncated)

    #expect(try CoreWorkspaceSearch.findNames(paths, query: "   ").hits.isEmpty)
    let limited = try CoreWorkspaceSearch.findNames(
      paths, query: "install", options: CoreNameOptions(maxResults: 1))
    #expect(limited.hits.count == 1 && limited.truncated && limited.totalHits == 3)
    let cased = try CoreWorkspaceSearch.findNames(
      paths, query: "Install", options: CoreNameOptions(caseSensitive: true))
    #expect(cased.hits.map(\.entry) == [1])
    let all = try CoreWorkspaceSearch.findNames(
      paths, query: "x", options: CoreNameOptions(allFiles: true))
    #expect(all.hits.map(\.entry) == [4])
    let hidden = try CoreWorkspaceSearch.findNames([".git/x.md", "x.md"], query: "x")
    #expect(hidden.hits.map(\.entry) == [1])
    #expect(throws: CoreError.self) {
      try CoreWorkspaceSearch.findNames(paths, query: String(repeating: "a", count: 4097))
    }
  }

  @Test func contentHitsCarrySnippetsAndTargets() throws {
    let text = "# Title\n\nSome *café* text\nwith Café again.\n\n```\ncafé in code\n```\n"
    let (search, outcomes) = try search("café", [("notes/a.md", text)])
    guard case .searched(let file) = try #require(outcomes.first) else {
      Issue.record("expected a searched file")
      return
    }
    #expect(file.hits.count == 3 && !file.truncated)
    let first = file.hits[0]
    #expect(first.line == 3)
    #expect(first.snippet == "Some café text with Café again.")
    #expect((first.snippet as NSString).substring(with: first.match) == "café")
    #expect(file.hits[1].line == 4)
    #expect((file.hits[1].snippet as NSString).substring(with: file.hits[1].match) == "Café")
    #expect(file.hits[2].line == 7)
    // The span covers the match in the source bytes; the anchor is present.
    let source = Data(text.utf8)
    let span = first.target.span
    #expect(
      String(decoding: source[Int(span.lowerBound)..<Int(span.upperBound)], as: UTF8.self)
        == "café")
    #expect(first.target.sourceLength == UInt64(source.count))
    #expect(first.target.sourceSHA256.count == 64 && first.target.anchor != nil)
    #expect(search.progress.filesRead == 1 && search.progress.matches == 3)
    #expect(search.progress.filesMatched == 1 && search.progress.stopped == nil)

    // Whole words and case.
    let (_, cased) = try self.search(
      "Café", [("a.md", text)], CoreContentOptions(caseSensitive: true))
    if case .searched(let file) = cased.first { #expect(file.hits.count == 1) }
    let (_, words) = try self.search(
      "caf", [("a.md", text)], CoreContentOptions(wholeWord: true))
    if case .searched(let file) = words.first { #expect(file.hits.isEmpty) }
  }

  @Test func skipsAndStopsAreReported() throws {
    let big = String(repeating: "word ", count: 300)
    let (skips, outcomes) = try search(
      "word",
      [
        ("notes.txt", "word"), (".hidden/a.md", "word"), ("big.md", big),
        ("ok.md", "word"),
      ], CoreContentOptions(maxFileBytes: 1024))
    #expect(
      outcomes.prefix(3) == [.skipped(.notMarkdown), .skipped(.hidden), .skipped(.tooLarge)])
    #expect(skips.progress.skippedTooLarge == 1 && skips.progress.filesRead == 1)

    // Invalid UTF-8 is read (and charged) but not searched.
    let invalid = try CoreContentSearch(query: "x")
    #expect(
      try invalid.search(path: "bad.md", content: Data([0x78, 0xFF])) == .skipped(.invalidUTF8))
    #expect(invalid.progress.filesRead == 1 && invalid.progress.skippedInvalidUTF8 == 1)

    // The file budget stops the search for good.
    let (files, stopped) = try search(
      "word", [("a.md", "word"), ("b.md", "word"), ("c.md", "word")],
      CoreContentOptions(maxFiles: 2))
    #expect(stopped.last == .stopped(.files) && files.progress.stopped == .files)
    #expect(try files.admit(path: "d.md", size: 4) == .stopped(.files))

    // Per-file truncation and the total match budget.
    let (truncated, limited) = try search(
      "word", [("a.md", big), ("b.md", "word")],
      CoreContentOptions(maxMatchesPerFile: 5, maxTotalMatches: 7))
    if case .searched(let file) = limited.first {
      #expect(file.hits.count == 5 && file.truncated)
    }
    #expect(truncated.progress.filesTruncated >= 1)
    #expect(truncated.progress.matches <= 7)

    #expect(throws: CoreError.self) {
      try CoreContentSearch(query: "   ").admit(path: "a.md", size: 1)
    }
    #expect(throws: CoreError.self) {
      try CoreContentSearch(query: String(repeating: "a", count: 4097))
    }
  }
}
