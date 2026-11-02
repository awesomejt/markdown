using System.Text;
using Markdown.Windows.Core;
using Markdown.Windows.Document;

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
    TaskChecked = 32,
    TaskUnchecked = 64,
}

public enum FragmentKind : byte
{
    Paragraph,
    Heading,
    Code,
    ThematicBreak,
    Fallback,
    TableRow,
    Image,
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

public sealed class RenderedImage(
    string target,
    string alt,
    ResourceError? failure,
    byte[]? data,
    int width,
    int height)
{
    public string Target { get; } = target;
    public string Alt { get; } = alt;
    public ResourceError? Failure { get; } = failure;
    public byte[]? Data { get; } = data;
    public int Width { get; } = width;
    public int Height { get; } = height;
    public string FailureMessage => Failure.HasValue ? Failure.Value.Message() : string.Empty;
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

    public bool IsHeader { get; internal set; }

    public List<RenderedImage> Images { get; } = [];

    public RenderedImage? Image => Images.Count > 0 ? Images[0] : null;

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
    public List<RenderedImage> Images { get; } = [];
    public long AccountedBytes { get; private set; }
    public ulong Generation { get; private init; }

    public static RenderPlan Build(
        CoreDocument document,
        Func<bool> cancelled,
        long budget = DefaultBudget,
        Func<string, int, (byte[]? Data, ResourceError? Error)>? imageLoader = null,
        ImageBudget? imageBudget = null)
    {
        ArgumentNullException.ThrowIfNull(document);
        ArgumentNullException.ThrowIfNull(cancelled);
        var builder = new Builder(
            document,
            cancelled,
            budget,
            imageLoader,
            imageBudget ?? new ImageBudget(),
            new RenderPlan { Generation = document.Generation });
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

    private sealed class Builder(
        CoreDocument doc,
        Func<bool> cancelled,
        long budget,
        Func<string, int, (byte[]? Data, ResourceError? Error)>? imageLoader,
        ImageBudget imageBudget,
        RenderPlan plan)
    {
        private static readonly CoreFfi.md_intent s_emptyIntent = default;
        private readonly int[] _first = CreateFirst(doc);
        private readonly Dictionary<string, RenderedImage> _imageCache = new(StringComparer.Ordinal);
        private readonly HashSet<string> _distinctImages = new(StringComparer.Ordinal);
        private readonly List<RenderedImage> _currentBlockImages = [];
        private long _decodedBytes;
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
                case CoreFfi.MD_NODE_IMAGE:
                    HandleImage(id, in n, depth, style, runs);
                    break;
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
                    foreach (var child in ChildrenOf(in n))
                    {
                        Inline(child, depth + 1, style, link, runs);
                    }
                    break;
            }
        }

        private void HandleImage(uint id, in CoreFfi.md_node n, int depth, RunStyle style, List<RenderRun> runs)
        {
            var resourceIndex = (int)n.aux;
            var target = resourceIndex < doc.ResourceCount ? doc.ResourceDestination(resourceIndex) : string.Empty;
            ref readonly var intent = ref (resourceIndex < doc.ResourceCount ? ref doc.Resources[resourceIndex] : ref s_emptyIntent);

            string alt;
            if (n.child_count > 0)
            {
                var altRuns = new List<RenderRun>();
                foreach (var child in ChildrenOf(in n))
                {
                    Inline(child, depth + 1, RunStyle.None, RenderRun.NoLink, altRuns);
                }
                alt = string.Concat(altRuns.Select(r => r.Text));
            }
            else
            {
                alt = doc.NodeText((int)id);
            }

            ResourceError? failure = null;
            byte[]? rawData = null;
            var w = 0;
            var h = 0;

            if (intent.target_kind == CoreFfi.MD_TARGET_EXTERNAL ||
                target.StartsWith("http:", StringComparison.OrdinalIgnoreCase) ||
                target.StartsWith("https:", StringComparison.OrdinalIgnoreCase))
            {
                failure = ResourceError.Remote;
            }
            else if (intent.target_kind == CoreFfi.MD_TARGET_DENIED)
            {
                failure = ResourceError.Blocked;
            }
            else if (string.IsNullOrWhiteSpace(target) ||
                     target.StartsWith('/') ||
                     target.StartsWith('\\') ||
                     target.Contains('\0') ||
                     target.Contains(':') ||
                     target.StartsWith("..", StringComparison.Ordinal) ||
                     target.Contains("/..") ||
                     target.Contains("\\.."))
            {
                failure = ResourceError.OutsideFolder;
            }
            else
            {
                // Relative local image
                if (_imageCache.TryGetValue(target, out var cached))
                {
                    failure = cached.Failure;
                    rawData = cached.Data;
                    w = cached.Width;
                    h = cached.Height;
                }
                else if (_distinctImages.Count >= imageBudget.MaxImages)
                {
                    failure = ResourceError.Limit;
                }
                else if (_decodedBytes >= imageBudget.MaxDecodedBytes)
                {
                    failure = ResourceError.Limit;
                }
                else if (imageLoader is null)
                {
                    failure = ResourceError.NotLoaded;
                }
                else
                {
                    _distinctImages.Add(target);
                    var (loadedData, loadErr) = imageLoader(target, imageBudget.MaxFileBytes);
                    if (loadErr is not null)
                    {
                        failure = loadErr;
                    }
                    else if (loadedData is null || loadedData.Length == 0)
                    {
                        failure = ResourceError.NotFound;
                    }
                    else if (!ImageFormatInspector.Inspect(loadedData, imageBudget.MaxSourcePixels, out var inspectedW, out var inspectedH, out var inspectErr))
                    {
                        failure = inspectErr ?? ResourceError.Undecodable;
                    }
                    else
                    {
                        var mem = (long)inspectedW * inspectedH * 4;
                        if (_decodedBytes + mem > imageBudget.MaxDecodedBytes)
                        {
                            failure = ResourceError.Limit;
                        }
                        else
                        {
                            _decodedBytes += mem;
                            w = inspectedW;
                            h = inspectedH;
                            rawData = loadedData;
                        }
                    }
                }
            }

            var renderedImage = new RenderedImage(target, alt, failure, rawData, w, h);
            if (!_imageCache.ContainsKey(target) && failure != ResourceError.Remote && failure != ResourceError.Blocked && failure != ResourceError.OutsideFolder)
            {
                _imageCache[target] = renderedImage;
            }

            plan.Images.Add(renderedImage);
            _currentBlockImages.Add(renderedImage);
            _first[id] = plan.Fragments.Count;

            AddRun(runs, string.IsNullOrEmpty(alt) ? "[image]" : $"[{alt}]", style, RenderRun.NoLink);
        }

        private void Table(uint id, in CoreFfi.md_node n, int depth, int indent, int quote)
        {
            _first[id] = plan.Fragments.Count;
            var colCount = (int)n.aux_count;
            var rows = ChildrenOf(in n);

            for (var r = 0; r < rows.Length; r++)
            {
                var rowId = rows[r];
                ref readonly var rowNode = ref Node(rowId, depth + 1);
                _first[rowId] = plan.Fragments.Count;
                var isHeader = (rowNode.flags & CoreFfi.MD_NODE_HEADER) != 0 || r == 0;
                var cells = ChildrenOf(in rowNode);
                var rowRuns = new List<RenderRun>();
                var totalCols = Math.Max(colCount, cells.Length);

                for (var c = 0; c < totalCols; c++)
                {
                    if (c > 0)
                    {
                        AddRun(rowRuns, "\t", RunStyle.None, RenderRun.NoLink);
                    }
                    if (c < cells.Length)
                    {
                        var cellId = cells[c];
                        ref readonly var cellNode = ref Node(cellId, depth + 2);
                        var inlines = ChildrenOf(in cellNode);
                        foreach (var inline in inlines)
                        {
                            Inline(inline, depth + 3, isHeader ? RunStyle.Strong : RunStyle.None, RenderRun.NoLink, rowRuns);
                        }
                    }
                }

                Emit(rowRuns, FragmentKind.TableRow, 0, indent, quote, isHeader: isHeader);
            }
        }

        private void Block(uint id, int depth, string? marker, int indent, int quote, RunStyle markerStyle = RunStyle.None)
        {
            ref readonly var n = ref Node(id, depth);
            var kind = n.kind;
            if ((kind == CoreFfi.MD_NODE_LIST || (kind == CoreFfi.MD_NODE_LIST_ITEM && n.child_count == 0)) &&
                marker is not null)
            {
                // A list item that starts with a nested list, or has no content, keeps its marker visible.
                var runs = new List<RenderRun>();
                AddRun(runs, marker, RunStyle.Marker | markerStyle, RenderRun.NoLink);
                Emit(runs, FragmentKind.Paragraph, 0, indent, quote);
                marker = null;
            }

            if (kind == CoreFfi.MD_NODE_TABLE)
            {
                Table(id, in n, depth, indent, quote);
                return;
            }

            if (kind == CoreFfi.MD_NODE_LIST)
            {
                var ordered = (n.flags & CoreFfi.MD_NODE_ORDERED) != 0;
                var start = n.number;
                var children = ChildrenOf(in n);
                for (var i = 0; i < children.Length; i++)
                {
                    var childId = children[i];
                    ref readonly var childNode = ref Node(childId, depth + 1);
                    string itemMarker;
                    var taskStyle = RunStyle.None;
                    if ((childNode.flags & CoreFfi.MD_NODE_HAS_CHECKED) != 0)
                    {
                        var isChecked = (childNode.flags & CoreFfi.MD_NODE_CHECKED) != 0;
                        itemMarker = isChecked ? "☑ " : "☐ ";
                        taskStyle = isChecked ? RunStyle.TaskChecked : RunStyle.TaskUnchecked;
                    }
                    else
                    {
                        itemMarker = ordered
                            ? $"{unchecked(start + (ulong)i)}. "
                            : "• ";
                    }
                    Block(childId, depth + 1, itemMarker, indent + 1, quote, taskStyle);
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
                    AddRun(runs, marker, RunStyle.Marker | markerStyle, RenderRun.NoLink);
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
                var blockImgs = _currentBlockImages.ToArray();
                _currentBlockImages.Clear();
                Emit(runs, fragmentKind, level, indent, quote, images: blockImgs);
                return;
            }

            var childQuote = kind is CoreFfi.MD_NODE_BLOCK_QUOTE or CoreFfi.MD_NODE_ALERT ? quote + 1 : quote;
            var blockChildren = ChildrenOf(in n);
            for (var i = 0; i < blockChildren.Length; i++)
            {
                Block(blockChildren[i], depth + 1, i == 0 ? marker : null, indent, childQuote, markerStyle);
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
        private void Emit(
            List<RenderRun> runs,
            FragmentKind kind,
            int headingLevel,
            int indent,
            int quote,
            bool isHeader = false,
            RenderedImage[]? images = null)
        {
            var initial = plan.Fragments.Count;
            var fragment = new RenderFragment(kind, headingLevel, headingLevel > 0, indent, quote)
            {
                IsHeader = isHeader,
            };
            if (images is not null && images.Length > 0)
            {
                fragment.Images.AddRange(images);
            }
            var used = 0;

            void Push()
            {
                Charge(64);
                fragment.Utf8Length = used;
                plan.Fragments.Add(fragment);
                fragment = new RenderFragment(kind, headingLevel, headingStart: false, indent, quote)
                {
                    IsHeader = isHeader,
                };
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
