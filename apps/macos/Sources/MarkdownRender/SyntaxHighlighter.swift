import AppKit

/// Bounded syntax highlighting for fenced code blocks (MAC-002F, R1.2).
///
/// A small lexical tokenizer, the same design and language set as the Linux
/// reader's (LNX-002F) plus Swift: keywords, type-like names, strings,
/// comments and numbers. It runs while the document renders, off the main
/// thread and under the render's cancellation, and never parses: a token
/// the tokenizer misreads only changes a colour. Each block is limited to
/// `maxBlockUnits` UTF-16 units and `maxTokens` tokens, and a document to
/// `maxDocumentUnits` highlighted units in all, so long or adversarial code
/// stays plain monospace text rather than slowing the render.
public enum SyntaxHighlighter {
  /// A longer block is left plain.
  public static let maxBlockUnits = 16 * 1024
  /// Tokens past this many in one block are left plain.
  public static let maxTokens = 2_048
  /// A block that would take the document's highlighted code past this many units stays plain.
  public static let maxDocumentUnits = 512 * 1024

  public enum TokenKind: Sendable, Equatable, CaseIterable {
    case keyword, type, string, comment, number
  }

  public struct Token: Sendable, Equatable {
    public let range: NSRange
    public let kind: TokenKind
  }

  enum Language: Sendable {
    case rust, python, script, cFamily, go, swift, shell, json, config, sql, web
  }

  /// The language a fence's info string names, from its first word, or nil
  /// for an unknown or missing one.
  static func language(_ info: String) -> Language? {
    let tag =
      info.split(whereSeparator: { $0.isWhitespace || $0 == "," || $0 == "{" }).first?
      .lowercased() ?? ""
    switch tag {
    case "rust", "rs": return .rust
    case "python", "py", "python3": return .python
    case "javascript", "js", "typescript", "ts", "jsx", "tsx", "mjs", "cjs": return .script
    case "c", "cpp", "c++", "cc", "cxx", "h", "hpp", "objc", "objective-c": return .cFamily
    case "go", "golang": return .go
    case "swift": return .swift
    case "bash", "sh", "zsh", "shell", "console": return .shell
    case "json", "jsonc": return .json
    case "yaml", "yml", "toml": return .config
    case "sql": return .sql
    case "html", "xml", "svg", "css": return .web
    default: return nil
    }
  }

  /// Whether `info` names a language this highlighter colours.
  public static func supports(_ info: String) -> Bool { language(info) != nil }

  /// The tokens of `code` for the fence info string `info`, as UTF-16 ranges
  /// in ascending, non-overlapping order; empty for an unknown language or a
  /// block over `maxBlockUnits`.
  public static func tokens(_ code: String, info: String) -> [Token] {
    guard let language = language(info) else { return [] }
    let units = Array(code.utf16)
    guard !units.isEmpty, units.count <= maxBlockUnits else { return [] }
    var lexer = Lexer(units: units, language: language)
    return lexer.run()
  }

  /// The text colour for each token kind, for light and dark appearances.
  static func color(_ kind: TokenKind) -> NSColor { colors[kind]! }

  private static let colors: [TokenKind: NSColor] = [
    .keyword: dynamic(light: 0x9B2393, dark: 0xFF7AB2),
    .type: dynamic(light: 0x0B4F79, dark: 0x6BDFFF),
    .string: dynamic(light: 0xC41A16, dark: 0xFF8170),
    .comment: dynamic(light: 0x5D6C79, dark: 0x9AA6B1),
    .number: dynamic(light: 0x1C00CF, dark: 0xD9C97C),
  ]

  private static func dynamic(light: UInt32, dark: UInt32) -> NSColor {
    func rgb(_ value: UInt32) -> NSColor {
      NSColor(
        srgbRed: CGFloat((value >> 16) & 0xFF) / 255, green: CGFloat((value >> 8) & 0xFF) / 255,
        blue: CGFloat(value & 0xFF) / 255, alpha: 1)
    }
    let lightColor = rgb(light)
    let darkColor = rgb(dark)
    return NSColor(name: nil) { appearance in
      appearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua ? darkColor : lightColor
    }
  }
}

private struct Lexer {
  let units: [UTF16.CodeUnit]
  let language: SyntaxHighlighter.Language
  var tokens: [SyntaxHighlighter.Token] = []
  var index = 0

  init(units: [UTF16.CodeUnit], language: SyntaxHighlighter.Language) {
    self.units = units
    self.language = language
  }

  private func at(_ offset: Int) -> UTF16.CodeUnit? {
    let position = index + offset
    return position < units.count ? units[position] : nil
  }

  private static func isDigit(_ unit: UTF16.CodeUnit) -> Bool { (0x30...0x39).contains(unit) }
  private static func isLetter(_ unit: UTF16.CodeUnit) -> Bool {
    (0x41...0x5A).contains(unit) || (0x61...0x7A).contains(unit)
  }
  private static func isWord(_ unit: UTF16.CodeUnit) -> Bool {
    isLetter(unit) || isDigit(unit) || unit == 0x5F
  }
  private static let newline: UTF16.CodeUnit = 0x0A

  private mutating func push(_ start: Int, _ kind: SyntaxHighlighter.TokenKind) {
    guard index > start, tokens.count < SyntaxHighlighter.maxTokens else { return }
    tokens.append(.init(range: NSRange(location: start, length: index - start), kind: kind))
  }

  private mutating func skipLine() {
    while index < units.count, units[index] != Self.newline { index += 1 }
  }

  /// Advance past `close`, or to the end when it never comes.
  private mutating func skip(past close: [UTF16.CodeUnit]) {
    while index < units.count {
      if units[index] == close[0], index + close.count <= units.count,
        Array(units[index..<index + close.count]) == close
      {
        index += close.count
        return
      }
      index += 1
    }
  }

  private var startsLineComment: Bool {
    let unit = units[index]
    switch language {
    case .python, .config: return unit == 0x23  // #
    case .shell:
      // `#` starts a comment only at a word boundary: `${#a}` and `a#b` don't.
      return unit == 0x23
        && (index == 0 || [0x0A, 0x20, 0x09].contains(units[index - 1]))
    case .sql: return unit == 0x2D && at(1) == 0x2D  // --
    case .rust, .script, .cFamily, .go, .swift: return unit == 0x2F && at(1) == 0x2F  // //
    case .json, .web: return false
    }
  }

  private var startsBlockComment: Bool {
    switch language {
    case .rust, .script, .cFamily, .go, .swift, .web: return units[index] == 0x2F && at(1) == 0x2A
    default: return false
    }
  }

  /// A single quote opens a string unless it follows a word character (an
  /// apostrophe) or no closing quote follows on its line; Rust and Swift
  /// use it for lifetimes and never for strings.
  private var opensSingleQuote: Bool {
    guard units[index] == 0x27, language != .rust, language != .swift else { return false }
    if index > 0, Self.isWord(units[index - 1]) || units[index - 1] >= 0x80 { return false }
    var position = index + 1
    while position < units.count, units[position] != Self.newline {
      if units[position] == 0x27 { return true }
      position += 1
    }
    return false
  }

  mutating func run() -> [SyntaxHighlighter.Token] {
    while index < units.count, tokens.count < SyntaxHighlighter.maxTokens {
      let start = index
      let unit = units[index]
      if startsLineComment {
        skipLine()
        push(start, .comment)
      } else if startsBlockComment {
        index += 2
        skip(past: [0x2A, 0x2F])
        push(start, .comment)
      } else if language == .web, unit == 0x3C, at(1) == 0x21, at(2) == 0x2D, at(3) == 0x2D {
        index += 4
        skip(past: [0x2D, 0x2D, 0x3E])
        push(start, .comment)
      } else if unit == 0x22 || opensSingleQuote || (unit == 0x60 && language == .script) {
        string(quote: unit)
        push(start, .string)
      } else if Self.isDigit(unit), index == 0 || !Self.isWord(units[index - 1]) {
        index += 1
        while index < units.count, Self.isWord(units[index]) || units[index] == 0x2E {
          index += 1
        }
        push(start, .number)
      } else if Self.isLetter(unit) || unit == 0x5F {
        index += 1
        while index < units.count, Self.isWord(units[index]) { index += 1 }
        // Words are ASCII, so their UTF-16 units are their characters.
        let word = String(decoding: units[start..<index], as: UTF16.self)
        if Keywords.isKeyword(word, language) {
          push(start, .keyword)
        } else if Keywords.isType(word, language) {
          push(start, .type)
        }
      } else {
        index += 1
      }
    }
    return tokens
  }

  /// Advance past a string opened at `index`; escapes skip a unit, and
  /// only a template literal continues past the end of its line.
  private mutating func string(quote: UTF16.CodeUnit) {
    index += 1
    while index < units.count {
      let unit = units[index]
      if unit == 0x5C, index + 1 < units.count {
        index += 2
        continue
      }
      index += 1
      if unit == quote || (unit == Self.newline && quote != 0x60) { return }
    }
  }
}

private enum Keywords {
  static func isKeyword(_ word: String, _ language: SyntaxHighlighter.Language) -> Bool {
    switch language {
    case .rust: rust.contains(word)
    case .python: python.contains(word)
    case .script: script.contains(word)
    case .cFamily: cFamily.contains(word)
    case .go: go.contains(word)
    case .swift: swift.contains(word)
    case .shell: shell.contains(word)
    case .json: json.contains(word)
    case .config: config.contains(word)
    case .sql: sql.contains(word.lowercased())
    case .web: false
    }
  }

  /// Built-in type names, and capitalized names in languages that
  /// capitalize their types.
  static func isType(_ word: String, _ language: SyntaxHighlighter.Language) -> Bool {
    let first = word.utf16.first ?? 0
    let capitalized = (0x41...0x5A).contains(first)
    let mixed = word.utf16.contains { (0x61...0x7A).contains($0) }
    switch language {
    case .rust: return rustTypes.contains(word) || capitalized
    case .swift: return capitalized && mixed
    case .cFamily: return cTypes.contains(word) || (capitalized && mixed)
    case .go: return goTypes.contains(word) || (capitalized && mixed)
    case .python, .script: return capitalized && mixed
    default: return false
    }
  }

  static let rust: Set<String> = [
    "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum",
    "extern", "false", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move",
    "mut", "pub", "ref", "return", "self", "Self", "static", "struct", "super", "trait", "true",
    "type", "unsafe", "use", "where", "while",
  ]
  static let rustTypes: Set<String> = [
    "bool", "char", "str", "u8", "u16", "u32", "u64", "u128", "usize", "i8", "i16", "i32", "i64",
    "i128", "isize", "f32", "f64",
  ]
  static let python: Set<String> = [
    "False", "None", "True", "and", "as", "assert", "async", "await", "break", "class",
    "continue", "def", "del", "elif", "else", "except", "finally", "for", "from", "global", "if",
    "import", "in", "is", "lambda", "nonlocal", "not", "or", "pass", "raise", "return", "try",
    "while", "with", "yield",
  ]
  static let script: Set<String> = [
    "async", "await", "break", "case", "catch", "class", "const", "continue", "default",
    "delete", "do", "else", "enum", "export", "extends", "false", "finally", "for", "function",
    "if", "implements", "import", "in", "instanceof", "interface", "let", "new", "null", "of",
    "return", "static", "super", "switch", "this", "throw", "true", "try", "type", "typeof",
    "undefined", "var", "void", "while", "yield",
  ]
  static let cFamily: Set<String> = [
    "auto", "break", "case", "catch", "class", "const", "constexpr", "continue", "default",
    "delete", "do", "else", "enum", "explicit", "extern", "false", "for", "goto", "if", "inline",
    "namespace", "new", "nullptr", "operator", "private", "protected", "public", "return",
    "sizeof", "static", "struct", "switch", "template", "this", "throw", "true", "try",
    "typedef", "typename", "union", "using", "virtual", "volatile", "while",
  ]
  static let cTypes: Set<String> = [
    "bool", "char", "double", "float", "int", "long", "short", "signed", "unsigned", "void",
    "size_t", "int8_t", "int16_t", "int32_t", "int64_t", "uint8_t", "uint16_t", "uint32_t",
    "uint64_t",
  ]
  static let go: Set<String> = [
    "break", "case", "chan", "const", "continue", "default", "defer", "else", "fallthrough",
    "false", "for", "func", "go", "goto", "if", "import", "interface", "map", "nil", "package",
    "range", "return", "select", "struct", "switch", "true", "type", "var",
  ]
  static let goTypes: Set<String> = [
    "bool", "byte", "error", "float32", "float64", "int", "int8", "int16", "int32", "int64",
    "rune", "string", "uint", "uint8", "uint16", "uint32", "uint64", "uintptr", "any",
  ]
  static let swift: Set<String> = [
    "actor", "any", "as", "associatedtype", "async", "await", "break", "case", "catch", "class",
    "continue", "default", "defer", "deinit", "do", "else", "enum", "extension", "fallthrough",
    "false", "fileprivate", "final", "for", "func", "guard", "if", "import", "in", "init",
    "inout", "internal", "is", "let", "mutating", "nil", "nonisolated", "open", "override",
    "private", "protocol", "public", "repeat", "rethrows", "return", "self", "Self", "some",
    "static", "struct", "subscript", "super", "switch", "throw", "throws", "true", "try",
    "typealias", "var", "where", "while",
  ]
  static let shell: Set<String> = [
    "case", "do", "done", "elif", "else", "esac", "export", "fi", "for", "function", "if", "in",
    "local", "return", "then", "until", "while",
  ]
  static let json: Set<String> = ["true", "false", "null"]
  static let config: Set<String> = ["true", "false", "null", "yes", "no", "on", "off"]
  static let sql: Set<String> = [
    "and", "as", "asc", "by", "create", "delete", "desc", "distinct", "drop", "from", "group",
    "having", "in", "index", "inner", "insert", "into", "is", "join", "left", "limit", "not",
    "null", "on", "or", "order", "outer", "primary", "key", "right", "select", "set", "table",
    "union", "update", "values", "where", "with",
  ]
}
