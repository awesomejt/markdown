using System.Diagnostics;

namespace Markdown.Windows.Render;

public enum SyntaxKind { Keyword, String, Comment, Number }
public readonly record struct SyntaxToken(int Start, int Length, SyntaxKind Kind);

/// <summary>Small, linear lexical hints for realized code rows. Never parses or executes code.</summary>
public static class Syntax
{
    public const int MaxChars = 512;
    public const int MaxTokens = 128;
    public const int MaxMilliseconds = 2;
    private static readonly HashSet<string> s_languages = new(StringComparer.Ordinal)
    {
        "rust", "rs", "python", "py", "javascript", "js", "typescript", "ts", "json",
        "c", "cpp", "c++", "csharp", "cs", "go", "bash", "sh", "shell", "sql",
    };
    private static readonly HashSet<string> s_keywords = new(StringComparer.OrdinalIgnoreCase)
    {
        "fn", "let", "mut", "pub", "use", "impl", "struct", "enum", "match", "self",
        "def", "class", "import", "from", "as", "in", "is", "not", "and", "or", "elif",
        "function", "const", "var", "new", "this", "export", "default", "interface",
        "if", "else", "for", "while", "return", "break", "continue", "true", "false", "null", "none",
        "public", "private", "static", "void", "int", "string", "bool", "async", "await",
        "package", "func", "select", "where", "insert", "into", "update", "delete", "create", "table",
    };

    public static IReadOnlyList<SyntaxToken> Tokens(string text, string language)
    {
        if (text.Length > MaxChars || !s_languages.Contains(language)) return Array.Empty<SyntaxToken>();
        var startTime = Stopwatch.GetTimestamp();
        var result = new List<SyntaxToken>();
        var hashComments = language is "python" or "py" or "bash" or "sh" or "shell";
        var json = language == "json";
        for (var i = 0; i < text.Length && result.Count < MaxTokens;)
        {
            if (Stopwatch.GetElapsedTime(startTime).TotalMilliseconds > MaxMilliseconds)
                return Array.Empty<SyntaxToken>();
            var start = i;
            var c = text[i++];
            SyntaxKind? kind = null;
            if ((!json && c == '/' && i < text.Length && text[i] == '/') ||
                (hashComments && c == '#') ||
                (language == "sql" && c == '-' && i < text.Length && text[i] == '-'))
            {
                while (i < text.Length && text[i] != '\n') i++;
                kind = SyntaxKind.Comment;
            }
            else if (c is '\'' or '"' or '`')
            {
                while (i < text.Length)
                {
                    if (text[i++] == c) break;
                    if (text[i - 1] == '\\' && i < text.Length) i++;
                }
                kind = SyntaxKind.String;
            }
            else if (char.IsDigit(c))
            {
                while (i < text.Length && (char.IsLetterOrDigit(text[i]) || text[i] is '.' or '_')) i++;
                kind = SyntaxKind.Number;
            }
            else if (char.IsLetter(c) || c == '_')
            {
                while (i < text.Length && (char.IsLetterOrDigit(text[i]) || text[i] == '_')) i++;
                if (s_keywords.Contains(text[start..i])) kind = SyntaxKind.Keyword;
            }
            if (kind.HasValue) result.Add(new SyntaxToken(start, i - start, kind.Value));
        }
        return result;
    }
}
