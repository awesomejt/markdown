using System.Text;
using Markdown.Windows.Core;

namespace Markdown.Windows.Render;

// Bounded, owned presentation values prepared off the UI thread from one owned
// model-v2 CoreDocument. No WinUI types, file access, parser internals or
// native handles: the reader only ever binds these values to recycled rows.

public enum RenderErrorKind
{
    Cancelled,
    Budget,
    InvalidModel,
}

public sealed class RenderException(RenderErrorKind kind, string message) : Exception(message)
{
    public RenderErrorKind Kind { get; } = kind;
}

[Flags]
public enum RunStyle : byte
{
    None = 0,
    Strong = 1,
    Emphasis = 2,
    Strikethrough = 4,
    Code = 8,
    Marker = 16,
}

public enum FragmentKind : byte
{
    Paragraph,
    Heading,
    Code,
    ThematicBreak,
    Fallback,
}

public enum TargetKind : byte
{
    /// <summary>A same-document heading anchor resolved to a plan fragment.</summary>
    Fragment,
    /// <summary>A core-canonical external http, https or mailto URI.</summary>
    External,
    /// <summary>Relative, capability-requiring, denied, unknown-scheme or missing-anchor targets.</summary>
    Inert,
}

public readonly record struct LinkTarget(TargetKind Kind, int Fragment, string? Uri)
{
    public static LinkTarget Inert { get; } = new(TargetKind.Inert, -1, null);
}

public sealed record RenderRun(string Text, RunStyle Style, int Link)
{
    public const int NoLink = -1;
}

public sealed class RenderFragment
{
    internal RenderFragment(FragmentKind kind, int headingLevel, bool headingStart, int indent, int quote)
    {
        Kind = kind;
        HeadingLevel = headingLevel;
        HeadingStart = headingStart;
        Indent = indent;
        Quote = quote;
    }

    public FragmentKind Kind { get; }

    /// <summary>Heading level 1-6 for heading fragments and their styled continuations; 0 otherwise.</summary>
    public int HeadingLevel { get; }

    /// <summary>Only the first fragment of a heading is a semantic heading; continuations keep the style.</summary>
    public bool HeadingStart { get; }

    /// <summary>List nesting depth used for visual indentation.</summary>
    public int Indent { get; }

    /// <summary>Block quote / alert nesting depth.</summary>
    public int Quote { get; }

    public List<RenderRun> Runs { get; } = [];

    public int Utf8Length { get; internal set; }

    public string Text => string.Concat(Runs.Select(static r => r.Text));

    public bool HasLink(int link) => link >= 0 && Runs.Exists(r => r.Link == link);
}

public sealed record OutlineItem(string Title, string Anchor, int Level, int Fragment);

public sealed class RenderPlan
{
    /// <summary>Accounted owned payload ceiling for one plan (an allocation policy, not exact RSS).</summary>
    public const long DefaultBudget = 128L * 1024 * 1024;

    /// <summary>Maximum UTF-8 bytes per displayed fragment (matching D-017 / Linux 512-byte bound).</summary>
    public const int MaxFragmentBytes = 512;

    /// <summary>Maximum UTF-8 bytes per fragment when a run has no whitespace break opportunity.</summary>
    public const int MaxUnbreakableBytes = 512;

    internal const int MaxDepth = 512;

    private static readonly string[] s_externalSchemes = ["https:", "http:", "mailto:"];

    public List<RenderFragment> Fragments { get; } = [];
    public List<OutlineItem> Outline { get; } = [];
    public List<LinkTarget> Targets { get; } = [];
    public long AccountedBytes { get; private set; }
    public ulong Generation { get; private init; }

    public static RenderPlan Build(CoreDocument document, Func<bool> cancelled, long budget = DefaultBudget)
    {
        ArgumentNullException.ThrowIfNull(document);
        ArgumentNullException.ThrowIfNull(cancelled);
        var builder = new Builder(document, cancelled, budget, new RenderPlan { Generation = document.Generation });
        return builder.Run();
    }

    /// <summary>
    /// Resolves a link activated from a bound row. The link must belong to that fragment, so a
    /// stale or forged (fragment, link) pair from a recycled container never resolves.
    /// </summary>
    public LinkTarget? TargetFor(int fragment, int link)
    {
        if ((uint)fragment >= (uint)Fragments.Count || (uint)link >= (uint)Targets.Count)
        {
            return null;
        }
        return Fragments[fragment].HasLink(link) ? Targets[link] : null;
    }

    private sealed class Builder(CoreDocument doc, Func<bool> cancelled, long budget, RenderPlan plan)
    {
        private readonly int[] _first = CreateFirst(doc);
        private long _visits;

        private static int[] CreateFirst(CoreDocument doc)
        {
            var first = new int[doc.NodeCount];
            Array.Fill(first, -1);
            return first;
        }

        public RenderPlan Run()
        {
            Charge((long)doc.NodeCount * sizeof(int));
            foreach (var root in doc.Roots)
            {
                Block(root, 0, null, 0, 0);
            }

            var anchors = new Dictionary<string, int>(StringComparer.Ordinal);
            for (var i = 0; i < doc.OutlineCount; i++)
            {
                Check();
                ref readonly var entry = ref doc.Outline[i];
                if (entry.node >= (uint)_first.Length || _first[entry.node] < 0)
                {
                    throw Invalid("outline heading has no fragment");
                }
                var title = Owned(doc.OutlineTitle(i));
                var anchor = Owned(doc.OutlineAnchor(i));
                Charge(48 + 2L * anchor.Length);
                anchors.TryAdd(anchor, _first[entry.node]);
                Charge(48);
                plan.Outline.Add(new OutlineItem(title, anchor, (int)entry.level, _first[entry.node]));
            }

            for (var i = 0; i < doc.LinkCount; i++)
            {
                Check();
                ref readonly var intent = ref doc.Links[i];
                var target = doc.LinkTarget(i);
                LinkTarget resolved;
                if (intent.target_kind == CoreFfi.MD_TARGET_FRAGMENT)
                {
                    resolved = anchors.TryGetValue(target, out var fragment)
                        ? new LinkTarget(TargetKind.Fragment, fragment, null)
                        : LinkTarget.Inert;
                }
                else if (intent.target_kind == CoreFfi.MD_TARGET_EXTERNAL && IsAllowedExternal(target))
                {
                    Charge(2L * target.Length);
                    resolved = new LinkTarget(TargetKind.External, -1, target);
                }
                else
                {
                    resolved = LinkTarget.Inert;
                }
                Charge(32);
                plan.Targets.Add(resolved);
            }

            Check();
            return plan;
        }

        private static bool IsAllowedExternal(string target)
        {
            // The core canonicalizes schemes; still require an exact lowercase allowed prefix and
            // an absolute URI that parses with the same scheme before anything can launch it.
            foreach (var scheme in s_externalSchemes)
            {
                if (target.StartsWith(scheme, StringComparison.Ordinal))
                {
                    return Uri.TryCreate(target, UriKind.Absolute, out var uri) &&
                        string.Equals(uri.Scheme + ":", scheme, StringComparison.Ordinal);
                }
            }
            return false;
        }

        private void Check()
        {
            if (cancelled())
            {
                throw new RenderException(RenderErrorKind.Cancelled, "render plan cancelled");
            }
        }

        private void Charge(long bytes)
        {
            Check();
            var next = plan.AccountedBytes + bytes;
            if (bytes < 0 || next > budget)
            {
                throw new RenderException(RenderErrorKind.Budget, "render plan exceeds its payload budget");
            }
            plan.AccountedBytes = next;
        }

        private static RenderException Invalid(string message) =>
            new(RenderErrorKind.InvalidModel, message);

        private string Owned(string text)
        {
            Charge(24 + 2L * text.Length);
            return text;
        }

        private ref readonly CoreFfi.md_node Node(uint id, int depth)
        {
            Check();
            _visits++;
            if (depth > MaxDepth || _visits > 2L * Math.Max(1, doc.NodeCount) || id >= (uint)doc.NodeCount)
            {
                throw Invalid("model traversal exceeded its node or depth bound");
            }
            return ref doc.Nodes[id];
        }

        private uint[] ChildrenOf(in CoreFfi.md_node node)
        {
            if ((ulong)node.first_child + node.child_count > (ulong)doc.ChildCount)
            {
                throw Invalid("child slice out of bounds");
            }
            Charge(4L * node.child_count);
            return doc.Children.AsSpan((int)node.first_child, (int)node.child_count).ToArray();
        }

        private string Fallback(in CoreFfi.md_node node)
        {
            if (node.span_start > node.span_end || node.span_end > (uint)doc.SourceByteCount)
            {
                throw Invalid("fallback span out of bounds");
            }
            var text = Encoding.UTF8.GetString(doc.Source, (int)node.span_start, (int)(node.span_end - node.span_start));
            return Owned(text);
        }

        private static bool IsInvalidDisplayChar(char c) =>
            c is (>= '\u0000' and <= '\u0008') or '\u000B' or '\u000C' or (>= '\u000E' and <= '\u001F')
                or '\uFFFE' or '\uFFFF';

        private void AddRun(List<RenderRun> runs, string text, RunStyle style, int link)
        {
            Charge(48 + 2L * text.Length);
            // NUL and other C0 controls would truncate or corrupt native text; keep a visible
            // replacement in presentation, accessibility names and selected text.
            if (text.AsSpan().IndexOfAnyInRange('\u0000', '\u001F') >= 0 || text.Contains('\uFFFE') || text.Contains('\uFFFF'))
            {
                var sb = new StringBuilder(text.Length);
                foreach (var c in text)
                {
                    sb.Append(IsInvalidDisplayChar(c) ? '\uFFFD' : c);
                }
                text = sb.ToString();
                Charge(2L * text.Length);
            }
            if (text.Length > 0)
            {
                runs.Add(new RenderRun(text, style, link));
            }
        }

        private void Inline(uint id, int depth, RunStyle style, int link, List<RenderRun> runs)
        {
            ref readonly var n = ref Node(id, depth);
            style |= n.kind switch
            {
                CoreFfi.MD_NODE_STRONG => RunStyle.Strong,
                CoreFfi.MD_NODE_EMPHASIS => RunStyle.Emphasis,
                CoreFfi.MD_NODE_STRIKETHROUGH => RunStyle.Strikethrough,
                CoreFfi.MD_NODE_CODE_SPAN => RunStyle.Code,
                _ => RunStyle.None,
            };
            if (n.kind == CoreFfi.MD_NODE_LINK)
            {
                if (n.aux >= (uint)doc.LinkCount)
                {
                    throw Invalid("link index out of bounds");
                }
                link = (int)n.aux;
            }

            switch (n.kind)
            {
                case CoreFfi.MD_NODE_SOFT_BREAK:
                    AddRun(runs, " ", style, link);
                    break;
                case CoreFfi.MD_NODE_HARD_BREAK:
                    AddRun(runs, "\n", style, link);
                    break;
                case CoreFfi.MD_NODE_TEXT:
                case CoreFfi.MD_NODE_CODE_SPAN:
                case CoreFfi.MD_NODE_FOOTNOTE_REFERENCE:
                    AddRun(runs, Owned(doc.NodeText((int)id)), style, link);
                    break;
                case CoreFfi.MD_NODE_UNSUPPORTED_INLINE:
                    AddRun(runs, Fallback(in n), style, link);
                    break;
                default:
                    var image = n.kind == CoreFfi.MD_NODE_IMAGE;
                    var childCount = n.child_count;
                    foreach (var child in ChildrenOf(in n))
                    {
                        Inline(child, depth + 1, style, link, runs);
                    }
                    if (image && childCount == 0)
                    {
                        // Images are never fetched here; their readable alternative text stands in.
                        AddRun(runs, Owned(doc.NodeText((int)id)), style, RenderRun.NoLink);
                    }
                    break;
            }
        }

        private void Block(uint id, int depth, string? marker, int indent, int quote)
        {
            ref readonly var n = ref Node(id, depth);
            var kind = n.kind;
            if ((kind == CoreFfi.MD_NODE_LIST || (kind == CoreFfi.MD_NODE_LIST_ITEM && n.child_count == 0)) &&
                marker is not null)
            {
                // A list item that starts with a nested list, or has no content, keeps its marker visible.
                var runs = new List<RenderRun>();
                AddRun(runs, marker, RunStyle.Marker, RenderRun.NoLink);
                Emit(runs, FragmentKind.Paragraph, 0, indent, quote);
                marker = null;
            }

            if (kind == CoreFfi.MD_NODE_LIST)
            {
                var ordered = (n.flags & CoreFfi.MD_NODE_ORDERED) != 0;
                var start = n.number;
                var children = ChildrenOf(in n);
                for (var i = 0; i < children.Length; i++)
                {
                    var itemMarker = ordered
                        ? $"{unchecked(start + (ulong)i)}. "
                        : "• ";
                    Block(children[i], depth + 1, itemMarker, indent + 1, quote);
                }
                return;
            }

            if (kind is CoreFfi.MD_NODE_PARAGRAPH or CoreFfi.MD_NODE_HEADING or CoreFfi.MD_NODE_CODE_BLOCK
                or CoreFfi.MD_NODE_UNSUPPORTED_BLOCK or CoreFfi.MD_NODE_THEMATIC_BREAK or CoreFfi.MD_NODE_TABLE_CELL)
            {
                _first[id] = plan.Fragments.Count;
                var runs = new List<RenderRun>();
                if (marker is not null)
                {
                    AddRun(runs, marker, RunStyle.Marker, RenderRun.NoLink);
                }

                FragmentKind fragmentKind;
                switch (kind)
                {
                    case CoreFfi.MD_NODE_CODE_BLOCK:
                        fragmentKind = FragmentKind.Code;
                        var code = Owned(doc.NodeText((int)id));
                        if (code.EndsWith('\n'))
                        {
                            // The final line terminator would render as an empty trailing line.
                            code = code[..^1];
                        }
                        AddRun(runs, code, RunStyle.Code, RenderRun.NoLink);
                        break;
                    case CoreFfi.MD_NODE_UNSUPPORTED_BLOCK:
                        // Raw HTML and unsupported blocks are shown as literal, inert source text.
                        fragmentKind = FragmentKind.Fallback;
                        AddRun(runs, Fallback(in n).TrimEnd('\r', '\n'), RunStyle.Code, RenderRun.NoLink);
                        break;
                    case CoreFfi.MD_NODE_THEMATIC_BREAK:
                        fragmentKind = FragmentKind.ThematicBreak;
                        AddRun(runs, "—", RunStyle.Marker, RenderRun.NoLink);
                        break;
                    default:
                        fragmentKind = kind == CoreFfi.MD_NODE_HEADING ? FragmentKind.Heading : FragmentKind.Paragraph;
                        foreach (var child in ChildrenOf(in n))
                        {
                            Inline(child, depth + 1, RunStyle.None, RenderRun.NoLink, runs);
                        }
                        break;
                }

                var level = kind == CoreFfi.MD_NODE_HEADING ? (int)Math.Clamp(n.value, 1u, 6u) : 0;
                Emit(runs, fragmentKind, level, indent, quote);
                return;
            }

            var childQuote = kind is CoreFfi.MD_NODE_BLOCK_QUOTE or CoreFfi.MD_NODE_ALERT ? quote + 1 : quote;
            var blockChildren = ChildrenOf(in n);
            for (var i = 0; i < blockChildren.Length; i++)
            {
                Block(blockChildren[i], depth + 1, i == 0 ? marker : null, indent, childQuote);
            }
        }

        private static int Utf8Bytes(string s, int index, out int chars)
        {
            var c = s[index];
            if (char.IsHighSurrogate(c) && index + 1 < s.Length && char.IsLowSurrogate(s[index + 1]))
            {
                chars = 2;
                return 4;
            }
            chars = 1;
            return c < 0x80 ? 1 : c < 0x800 ? 2 : 3;
        }

        /// <summary>
        /// Splits runs into bounded fragments: at most <see cref="MaxFragmentBytes"/> UTF-8 bytes,
        /// preferring the last whitespace break, and at most <see cref="MaxUnbreakableBytes"/> when a
        /// fragment would otherwise have no break opportunity. Never splits a surrogate pair.
        /// </summary>
        private void Emit(List<RenderRun> runs, FragmentKind kind, int headingLevel, int indent, int quote)
        {
            var initial = plan.Fragments.Count;
            var fragment = new RenderFragment(kind, headingLevel, headingLevel > 0, indent, quote);
            var used = 0;

            void Push()
            {
                Charge(64);
                fragment.Utf8Length = used;
                plan.Fragments.Add(fragment);
                fragment = new RenderFragment(kind, headingLevel, headingStart: false, indent, quote);
                used = 0;
            }

            foreach (var run in runs)
            {
                var text = run.Text;
                var pos = 0;
                while (pos < text.Length)
                {
                    Check();
                    var available = MaxFragmentBytes - used;
                    var bytes = 0;
                    var end = pos;
                    var lastBreak = -1;
                    var bytesAtBreak = 0;
                    while (end < text.Length)
                    {
                        var size = Utf8Bytes(text, end, out var chars);
                        if (bytes + size > available)
                        {
                            break;
                        }
                        bytes += size;
                        end += chars;
                        if (char.IsWhiteSpace(text[end - 1]))
                        {
                            lastBreak = end;
                            bytesAtBreak = bytes;
                        }
                    }

                    var forced = end < text.Length;
                    if (forced)
                    {
                        if (lastBreak > pos)
                        {
                            end = lastBreak;
                            bytes = bytesAtBreak;
                        }
                        else if (used > 0)
                        {
                            // No break opportunity in what fits here: start a fresh fragment.
                            Push();
                            continue;
                        }
                        else
                        {
                            // An unbreakable run: cut at the smaller bound on a scalar boundary.
                            end = pos;
                            bytes = 0;
                            while (end < text.Length)
                            {
                                var size = Utf8Bytes(text, end, out var chars);
                                if (bytes + size > MaxUnbreakableBytes)
                                {
                                    break;
                                }
                                bytes += size;
                                end += chars;
                            }
                        }
                    }

                    var piece = pos == 0 && end == text.Length ? text : text[pos..end];
                    Charge(48 + 2L * piece.Length);
                    fragment.Runs.Add(new RenderRun(piece, run.Style, run.Link));
                    used += bytes;
                    pos = end;
                    if (forced || used >= MaxFragmentBytes)
                    {
                        Push();
                    }
                }
            }

            if (fragment.Runs.Count > 0 || plan.Fragments.Count == initial)
            {
                Push();
            }
        }
    }
}
