import AppKit
import Foundation
import MarkdownCore
import Testing

@testable import MarkdownRender

/// R1 conformance pieces added in MAC-002F: bounded syntax highlighting,
/// front-matter properties kept apart from the text, and build diagnostics.
@Suite(.serialized)
@MainActor
struct ConformanceTests {
  private static func render(_ markdown: String) async throws -> RenderedDocument {
    let session = try CoreSession(documentID: "conformance")
    let result = try await session.parse(Data(markdown.utf8))
    try await session.close()
    return try DocumentRenderer.render(try #require(result.document))
  }

  private static func tokens(_ code: String, _ info: String) -> [(
    String, SyntaxHighlighter.TokenKind
  )] {
    let string = code as NSString
    return SyntaxHighlighter.tokens(code, info: info).map {
      (string.substring(with: $0.range), $0.kind)
    }
  }

  private static func color(of text: String, in rendered: RenderedDocument) -> NSColor? {
    let range = (rendered.text.string as NSString).range(of: text)
    guard range.location != NSNotFound else { return nil }
    return rendered.text.attribute(.foregroundColor, at: range.location, effectiveRange: nil)
      as? NSColor
  }

  @Test func highlighterTokenizesEachKind() {
    let rust = Self.tokens("let x: u32 = 42; // hi\nlet s = \"ok\";", "rust")
    #expect(rust.contains { $0 == ("let", .keyword) })
    #expect(rust.contains { $0 == ("u32", .type) })
    #expect(rust.contains { $0 == ("42", .number) })
    #expect(rust.contains { $0 == ("// hi", .comment) })
    #expect(rust.contains { $0 == ("\"ok\"", .string) })
    let swift = Self.tokens("guard let value else { return } /* done */", "swift")
    #expect(swift.map(\.0) == ["guard", "let", "else", "return", "/* done */"])
    let python = Self.tokens("def f(): return 'a'  # note", "Python3 {.numberLines}")
    #expect(python.map(\.1) == [.keyword, .keyword, .string, .comment])
    let sql = Self.tokens("SELECT a FROM t -- all", "sql")
    #expect(sql.map(\.1) == [.keyword, .keyword, .comment])
    let html = Self.tokens("<!-- c --><p>", "html")
    #expect(html.map(\.0) == ["<!-- c -->"])
  }

  @Test func highlighterAvoidsCommonMisreads() {
    // A `#` inside a word or parameter expansion is not a shell comment.
    #expect(Self.tokens("echo ${#arr[@]} a#b # real", "bash").map(\.0) == ["# real"])
    // An apostrophe isn't a string, nor is a Rust lifetime.
    #expect(Self.tokens("note: don't break", "yaml").isEmpty)
    #expect(Self.tokens("fn f<'a>(x: &'a str)", "rust").map(\.0) == ["fn", "str"])
    // A Rust char literal holding a quote opens no string.
    #expect(
      Self.tokens("let q = '\"'; let b = b'\\''; // end", "rust").map(\.0)
        == ["let", "'\"'", "let", "'\\''", "// end"])
    // A digit inside a name isn't a number.
    #expect(Self.tokens("x1 = 2", "python").map(\.0) == ["2"])
    // Ranges are UTF-16, past non-ASCII text.
    let tokens = Self.tokens("let é = \"ü\" // ✓", "swift")
    #expect(tokens.map(\.0) == ["let", "\"ü\"", "// ✓"])
  }

  @Test func highlighterIsBounded() {
    #expect(Self.tokens("+++", "brainfuck").isEmpty)
    #expect(Self.tokens("let a = 1", "").isEmpty)
    let long = String(repeating: "let a = 1;\n", count: 2_000)
    #expect((long as NSString).length > SyntaxHighlighter.maxBlockUnits)
    #expect(SyntaxHighlighter.tokens(long, info: "rust").isEmpty)
    let many = String(repeating: "1 ", count: 4_000)
    #expect(SyntaxHighlighter.tokens(many, info: "rust").count == SyntaxHighlighter.maxTokens)
    // An unterminated string or comment ends at the block's end.
    #expect(Self.tokens("/* open", "c").map(\.0) == ["/* open"])
    #expect(Self.tokens("\"open", "json").map(\.0) == ["\"open"])
  }

  @Test func codeBlocksAreHighlightedWithinBudgets() async throws {
    let rendered = try await Self.render(
      """
      ```swift
      let answer = 42
      ```

      ```
      let plain = 1
      ```

      > ```swift
      > let quoted = 2
      > ```
      """)
    #expect(Self.color(of: "let answer", in: rendered) == SyntaxHighlighter.color(.keyword))
    #expect(Self.color(of: "42", in: rendered) == SyntaxHighlighter.color(.number))
    #expect(Self.color(of: "answer", in: rendered) == .labelColor)
    // No info string, or inside a quote: plain.
    #expect(Self.color(of: "let plain", in: rendered) == .labelColor)
    #expect(Self.color(of: "let quoted", in: rendered) == .secondaryLabelColor)

    // A block that would pass the per-document budget stays plain.
    let block = "```rust\n" + String(repeating: "let a = 1;\n", count: 1_400) + "```\n\n"
    let count = SyntaxHighlighter.maxDocumentUnits / (block.utf16.count - 16) + 2
    let large = try await Self.render(String(repeating: block, count: count))
    let text = large.text.string as NSString
    let first = text.range(of: "let a")
    let last = text.range(of: "let a", options: .backwards)
    func color(at location: Int) -> NSColor? {
      large.text.attribute(.foregroundColor, at: location, effectiveRange: nil) as? NSColor
    }
    #expect(color(at: first.location) == SyntaxHighlighter.color(.keyword))
    #expect(color(at: last.location) == .labelColor)
  }

  @Test func frontMatterBecomesProperties() async throws {
    let rendered = try await Self.render(
      """
      ---
      title: Field notes
      draft: false
      count: 3
      tags: [a, b]
      empty: null
      ---
      # Body
      """)
    let properties = try #require(rendered.properties)
    #expect(properties.unreadSource == nil)
    // In the core's order: sorted by key.
    #expect(properties.entries.map(\.key) == ["count", "draft", "empty", "tags", "title"])
    #expect(properties.entries.map(\.value) == ["3", "false", "null", "a, b", "Field notes"])
    // The front matter is not in the text.
    #expect(!rendered.text.string.contains("draft"))
    #expect(rendered.text.string.hasPrefix("Body"))
  }

  @Test func unreadFrontMatterStaysReadable() async throws {
    // A duplicate key discards the whole map; its source is shown instead.
    let rendered = try await Self.render("---\nkey: one\nkey: two\n---\nBody\n")
    let properties = try #require(rendered.properties)
    #expect(properties.entries.isEmpty)
    #expect(properties.unreadSource == "---\nkey: one\nkey: two\n---")
    #expect(rendered.text.string == "Body")
    let none = try await Self.render("# No front matter\n")
    #expect(none.properties == nil)
    // Empty or comment-only front matter is a valid, empty mapping: no row.
    for empty in ["---\n---\nBody\n", "---\n# draft\n---\nBody\n"] {
      let rendered = try await Self.render(empty)
      #expect(rendered.properties == nil)
      #expect(rendered.text.string == "Body")
    }
  }

  @Test func diagnosticsNameTheContract() throws {
    let diagnostics = try CoreRuntime.diagnostics()
    #expect(diagnostics.contract == "model-v2 (ABI v1, anchor v1)")
    #expect(["debug", "release"].contains(diagnostics.packageConfiguration))
    #expect(diagnostics.packageFingerprint.count == 64)
    #expect(
      diagnostics.package
        == "\(diagnostics.packageConfiguration) \(diagnostics.packageFingerprint.prefix(12))")
  }
}
