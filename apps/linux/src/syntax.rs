//! Bounded, lazy syntax highlighting for fenced code blocks (LNX-002F / R1.2).
//!
//! Runs on demand only for realized/visible code fragments, with strict byte and
//! token budgets so large or adversarial code blocks fall back to plain monospace.

/// Maximum UTF-8 byte length of a code fragment eligible for highlighting.
pub const MAX_HIGHLIGHT_BYTES: usize = 16 * 1024;
/// Maximum number of tokens emitted per fragment.
pub const MAX_HIGHLIGHT_TOKENS: usize = 2048;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TokenKind {
    Keyword,
    Type,
    String,
    Comment,
    Number,
}

impl TokenKind {
    /// Pango-safe foreground color readable on both light and dark GTK themes.
    pub fn pango_open(self) -> &'static str {
        match self {
            TokenKind::Keyword => "<span foreground=\"#9141ac\"><b>",
            TokenKind::Type => "<span foreground=\"#26a269\">",
            TokenKind::String => "<span foreground=\"#1a5fb4\">",
            TokenKind::Comment => "<span foreground=\"#77767b\"><i>",
            TokenKind::Number => "<span foreground=\"#c64600\">",
        }
    }

    pub fn pango_close(self) -> &'static str {
        match self {
            TokenKind::Keyword => "</b></span>",
            TokenKind::Comment => "</i></span>",
            TokenKind::Type | TokenKind::String | TokenKind::Number => "</span>",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Language {
    Rust,
    Python,
    JsTs,
    CFamily,
    Go,
    Shell,
    Json,
    YamlToml,
    Sql,
    Web,
}

fn normalize_language(info: &str) -> Option<Language> {
    let tag = info
        .split(|c: char| c.is_ascii_whitespace() || c == ',' || c == '{')
        .next()?
        .trim()
        .to_ascii_lowercase();
    match tag.as_str() {
        "rust" | "rs" => Some(Language::Rust),
        "python" | "py" | "python3" => Some(Language::Python),
        "javascript" | "js" | "typescript" | "ts" | "jsx" | "tsx" | "mjs" | "cjs" => {
            Some(Language::JsTs)
        }
        "c" | "cpp" | "c++" | "cc" | "cxx" | "h" | "hpp" => Some(Language::CFamily),
        "go" | "golang" => Some(Language::Go),
        "bash" | "sh" | "zsh" | "shell" | "console" => Some(Language::Shell),
        "json" | "jsonc" => Some(Language::Json),
        "yaml" | "yml" | "toml" => Some(Language::YamlToml),
        "sql" => Some(Language::Sql),
        "html" | "xml" | "svg" | "css" => Some(Language::Web),
        _ => None,
    }
}

fn is_keyword(lang: Language, word: &str) -> bool {
    match lang {
        Language::Rust => matches!(
            word,
            "as" | "async"
                | "await"
                | "break"
                | "const"
                | "continue"
                | "crate"
                | "dyn"
                | "else"
                | "enum"
                | "extern"
                | "false"
                | "fn"
                | "for"
                | "if"
                | "impl"
                | "in"
                | "let"
                | "loop"
                | "match"
                | "mod"
                | "move"
                | "mut"
                | "pub"
                | "ref"
                | "return"
                | "self"
                | "Self"
                | "static"
                | "struct"
                | "super"
                | "trait"
                | "true"
                | "type"
                | "unsafe"
                | "use"
                | "where"
                | "while"
        ),
        Language::Python => matches!(
            word,
            "False"
                | "None"
                | "True"
                | "and"
                | "as"
                | "assert"
                | "async"
                | "await"
                | "break"
                | "class"
                | "continue"
                | "def"
                | "del"
                | "elif"
                | "else"
                | "except"
                | "finally"
                | "for"
                | "from"
                | "global"
                | "if"
                | "import"
                | "in"
                | "is"
                | "lambda"
                | "nonlocal"
                | "not"
                | "or"
                | "pass"
                | "raise"
                | "return"
                | "try"
                | "while"
                | "with"
                | "yield"
        ),
        Language::JsTs => matches!(
            word,
            "async"
                | "await"
                | "break"
                | "case"
                | "catch"
                | "class"
                | "const"
                | "continue"
                | "default"
                | "delete"
                | "do"
                | "else"
                | "enum"
                | "export"
                | "extends"
                | "false"
                | "finally"
                | "for"
                | "function"
                | "if"
                | "implements"
                | "import"
                | "in"
                | "instanceof"
                | "interface"
                | "let"
                | "new"
                | "null"
                | "of"
                | "return"
                | "static"
                | "super"
                | "switch"
                | "this"
                | "throw"
                | "true"
                | "try"
                | "type"
                | "typeof"
                | "undefined"
                | "var"
                | "void"
                | "while"
                | "yield"
        ),
        Language::CFamily => matches!(
            word,
            "auto"
                | "bool"
                | "break"
                | "case"
                | "char"
                | "class"
                | "const"
                | "constexpr"
                | "continue"
                | "default"
                | "delete"
                | "do"
                | "double"
                | "else"
                | "enum"
                | "extern"
                | "false"
                | "float"
                | "for"
                | "if"
                | "inline"
                | "int"
                | "long"
                | "namespace"
                | "new"
                | "nullptr"
                | "private"
                | "protected"
                | "public"
                | "return"
                | "short"
                | "signed"
                | "sizeof"
                | "static"
                | "struct"
                | "switch"
                | "template"
                | "this"
                | "true"
                | "typedef"
                | "typename"
                | "union"
                | "unsigned"
                | "using"
                | "virtual"
                | "void"
                | "volatile"
                | "while"
        ),
        Language::Go => matches!(
            word,
            "break"
                | "case"
                | "chan"
                | "const"
                | "continue"
                | "default"
                | "defer"
                | "else"
                | "fallthrough"
                | "false"
                | "for"
                | "func"
                | "go"
                | "goto"
                | "if"
                | "import"
                | "interface"
                | "map"
                | "nil"
                | "package"
                | "range"
                | "return"
                | "select"
                | "struct"
                | "switch"
                | "true"
                | "type"
                | "var"
        ),
        Language::Shell => matches!(
            word,
            "case"
                | "do"
                | "done"
                | "elif"
                | "else"
                | "esac"
                | "export"
                | "fi"
                | "for"
                | "function"
                | "if"
                | "in"
                | "local"
                | "readonly"
                | "return"
                | "then"
                | "until"
                | "while"
        ),
        Language::Json | Language::YamlToml => matches!(word, "true" | "false" | "null"),
        Language::Sql => matches!(
            word.to_ascii_uppercase().as_str(),
            "ALL"
                | "AND"
                | "AS"
                | "ASC"
                | "BY"
                | "CREATE"
                | "DELETE"
                | "DESC"
                | "DISTINCT"
                | "DROP"
                | "FROM"
                | "GROUP"
                | "HAVING"
                | "IN"
                | "INNER"
                | "INSERT"
                | "INTO"
                | "IS"
                | "JOIN"
                | "LEFT"
                | "LIMIT"
                | "NOT"
                | "NULL"
                | "ON"
                | "OR"
                | "ORDER"
                | "OUTER"
                | "PRIMARY"
                | "RIGHT"
                | "SELECT"
                | "SET"
                | "TABLE"
                | "UNION"
                | "UPDATE"
                | "VALUES"
                | "WHERE"
        ),
        Language::Web => matches!(
            word,
            "import" | "media" | "important" | "none" | "auto" | "inherit"
        ),
    }
}

fn is_type_name(lang: Language, word: &str) -> bool {
    match lang {
        Language::Rust => {
            matches!(
                word,
                "u8" | "u16"
                    | "u32"
                    | "u64"
                    | "usize"
                    | "i8"
                    | "i16"
                    | "i32"
                    | "i64"
                    | "isize"
                    | "f32"
                    | "f64"
                    | "bool"
                    | "str"
                    | "String"
                    | "Vec"
                    | "Option"
                    | "Result"
                    | "Box"
                    | "Rc"
                    | "Arc"
            ) || word.chars().next().is_some_and(|c| c.is_ascii_uppercase())
        }
        Language::Python | Language::JsTs | Language::Go | Language::CFamily => {
            word.chars().next().is_some_and(|c| c.is_ascii_uppercase())
                && word.chars().any(|c| c.is_ascii_lowercase())
        }
        _ => false,
    }
}

/// Tokenize `text` for `language_info` into non-overlapping character-index spans
/// `(char_start, char_end, TokenKind)`. Returns an empty vector if the language is
/// unsupported or `text` exceeds `MAX_HIGHLIGHT_BYTES`.
pub fn highlight_char_spans(language_info: &str, text: &str) -> Vec<(usize, usize, TokenKind)> {
    let Some(lang) = normalize_language(language_info) else {
        return Vec::new();
    };
    if text.len() > MAX_HIGHLIGHT_BYTES || text.is_empty() {
        return Vec::new();
    }

    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let len = chars.len();
    let mut i = 0usize;
    let mut tokens = Vec::new();

    let push_tok =
        |tokens: &mut Vec<(usize, usize, TokenKind)>, start: usize, end: usize, kind: TokenKind| {
            if start < end && tokens.len() < MAX_HIGHLIGHT_TOKENS {
                tokens.push((start, end, kind));
            }
        };

    while i < len && tokens.len() < MAX_HIGHLIGHT_TOKENS {
        let ch = chars[i].1;
        let next = chars.get(i + 1).map(|&(_, c)| c);

        // Line comments: //, #, --
        let is_line_comment = match lang {
            Language::Python | Language::Shell | Language::YamlToml => ch == '#',
            Language::Sql => ch == '-' && next == Some('-'),
            Language::Rust | Language::JsTs | Language::CFamily | Language::Go => {
                ch == '/' && next == Some('/')
            }
            Language::Json | Language::Web => false,
        };
        if is_line_comment {
            let start = i;
            while i < len && chars[i].1 != '\n' {
                i += 1;
            }
            push_tok(&mut tokens, start, i, TokenKind::Comment);
            continue;
        }

        // Block comments: /* ... */ or <!-- ... -->
        if matches!(
            lang,
            Language::Rust | Language::JsTs | Language::CFamily | Language::Go | Language::Web
        ) && ch == '/'
            && next == Some('*')
        {
            let start = i;
            i += 2;
            while i + 1 < len && !(chars[i].1 == '*' && chars[i + 1].1 == '/') {
                i += 1;
            }
            i = (i + 2).min(len);
            push_tok(&mut tokens, start, i, TokenKind::Comment);
            continue;
        }
        if lang == Language::Web
            && ch == '<'
            && chars.get(i + 1).map(|&(_, c)| c) == Some('!')
            && chars.get(i + 2).map(|&(_, c)| c) == Some('-')
            && chars.get(i + 3).map(|&(_, c)| c) == Some('-')
        {
            let start = i;
            i += 4;
            while i + 2 < len
                && !(chars[i].1 == '-' && chars[i + 1].1 == '-' && chars[i + 2].1 == '>')
            {
                i += 1;
            }
            i = (i + 3).min(len);
            push_tok(&mut tokens, start, i, TokenKind::Comment);
            continue;
        }

        // String literals: "...", '...', `...`
        if ch == '"'
            || (ch == '\'' && lang != Language::Rust)
            || (ch == '`' && lang == Language::JsTs)
        {
            let quote = ch;
            let start = i;
            i += 1;
            while i < len {
                let c = chars[i].1;
                if c == '\\' && i + 1 < len {
                    i += 2;
                    continue;
                }
                i += 1;
                if c == quote || (c == '\n' && quote != '`') {
                    break;
                }
            }
            push_tok(&mut tokens, start, i, TokenKind::String);
            continue;
        }

        // Numbers
        if ch.is_ascii_digit()
            && (i == 0 || (!chars[i - 1].1.is_ascii_alphanumeric() && chars[i - 1].1 != '_'))
        {
            let start = i;
            i += 1;
            while i < len
                && (chars[i].1.is_ascii_alphanumeric() || chars[i].1 == '_' || chars[i].1 == '.')
            {
                i += 1;
            }
            push_tok(&mut tokens, start, i, TokenKind::Number);
            continue;
        }

        // Identifiers / keywords / types
        if ch.is_ascii_alphabetic() || ch == '_' {
            let start = i;
            let byte_start = chars[i].0;
            i += 1;
            while i < len && (chars[i].1.is_ascii_alphanumeric() || chars[i].1 == '_') {
                i += 1;
            }
            let byte_end = if i < len { chars[i].0 } else { text.len() };
            let word = &text[byte_start..byte_end];
            if is_keyword(lang, word) {
                push_tok(&mut tokens, start, i, TokenKind::Keyword);
            } else if is_type_name(lang, word) {
                push_tok(&mut tokens, start, i, TokenKind::Type);
            }
            continue;
        }

        i += 1;
    }

    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlights_rust_keywords_strings_comments_and_numbers() {
        let code = "let x: u32 = 42; // hi\nlet s = \"ok\";";
        let spans = highlight_char_spans("rust", code);
        assert!(
            spans
                .iter()
                .any(|&(s, e, k)| k == TokenKind::Keyword && &code[s..e] == "let")
        );
        assert!(
            spans
                .iter()
                .any(|&(s, e, k)| k == TokenKind::Type && &code[s..e] == "u32")
        );
        assert!(
            spans
                .iter()
                .any(|&(s, e, k)| k == TokenKind::Number && &code[s..e] == "42")
        );
        assert!(
            spans
                .iter()
                .any(|&(s, e, k)| k == TokenKind::Comment && &code[s..e] == "// hi")
        );
        assert!(
            spans
                .iter()
                .any(|&(s, e, k)| k == TokenKind::String && &code[s..e] == "\"ok\"")
        );
    }

    #[test]
    fn enforces_byte_and_token_budgets() {
        let huge = "let a = 1;\n".repeat(2000);
        assert!(huge.len() > MAX_HIGHLIGHT_BYTES);
        assert!(highlight_char_spans("rust", &huge).is_empty());
    }

    #[test]
    fn ignores_unknown_language() {
        assert!(highlight_char_spans("brainfuck", "+++").is_empty());
    }
}
