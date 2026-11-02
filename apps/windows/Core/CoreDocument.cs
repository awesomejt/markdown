using System.Runtime.CompilerServices;
using System.Text;
using System.Text.Unicode;

namespace Markdown.Windows.Core;

public sealed unsafe class CoreDocument
{
    public uint ModelVersion { get; }
    public ulong Generation { get; }
    public uint Capabilities { get; }
    public uint Flags { get; }
    public uint FrontMatterStart { get; }
    public uint FrontMatterEnd { get; }
    public string DocumentId { get; }
    public string RevisionId { get; }
    public string SourceSha256 { get; }
    public string? Title { get; }

    internal byte[] Source { get; }
    internal byte[] Strings { get; }
    internal CoreFfi.md_node[] Nodes { get; }
    internal uint[] Roots { get; }
    internal uint[] Children { get; }
    internal uint[] Alignments { get; }
    internal CoreFfi.md_intent[] Links { get; }
    internal CoreFfi.md_intent[] Resources { get; }
    internal CoreFfi.md_outline_entry[] Outline { get; }
    internal CoreFfi.md_diagnostic[] Diagnostics { get; }
    internal CoreFfi.md_property[] Properties { get; }
    internal CoreFfi.md_str[] StringList { get; }
    public ulong CopiedBytes { get; }

    public int NodeCount => Nodes.Length;
    public int RootCount => Roots.Length;
    public int ChildCount => Children.Length;
    public int AlignmentCount => Alignments.Length;
    public int LinkCount => Links.Length;
    public int ResourceCount => Resources.Length;
    public int OutlineCount => Outline.Length;
    public int DiagnosticCount => Diagnostics.Length;
    public int PropertyCount => Properties.Length;
    public int StringListCount => StringList.Length;
    public int SourceByteCount => Source.Length;
    public int StringsByteCount => Strings.Length;

    public bool HasFrontMatter => (Flags & CoreFfi.MD_VIEW_HAS_FRONT_MATTER) != 0;
    public bool HasTitle => (Flags & CoreFfi.MD_VIEW_HAS_TITLE) != 0;

    internal CoreDocument(
        uint modelVersion,
        ulong generation,
        uint capabilities,
        uint flags,
        uint frontMatterStart,
        uint frontMatterEnd,
        CoreFfi.md_str documentId,
        CoreFfi.md_str revisionId,
        CoreFfi.md_str sourceSha256,
        CoreFfi.md_str title,
        byte[] source,
        byte[] strings,
        CoreFfi.md_node[] nodes,
        uint[] roots,
        uint[] children,
        uint[] alignments,
        CoreFfi.md_intent[] links,
        CoreFfi.md_intent[] resources,
        CoreFfi.md_outline_entry[] outline,
        CoreFfi.md_diagnostic[] diagnostics,
        CoreFfi.md_property[] properties,
        CoreFfi.md_str[] stringList,
        ulong copiedBytes)
    {
        ModelVersion = modelVersion;
        Generation = generation;
        Capabilities = capabilities;
        Flags = flags;
        FrontMatterStart = frontMatterStart;
        FrontMatterEnd = frontMatterEnd;
        Source = source;
        Strings = strings;
        Nodes = nodes;
        Roots = roots;
        Children = children;
        Alignments = alignments;
        Links = links;
        Resources = resources;
        Outline = outline;
        Diagnostics = diagnostics;
        Properties = properties;
        StringList = stringList;
        CopiedBytes = copiedBytes;

        CheckIndices(documentId, revisionId, sourceSha256, title);

        DocumentId = DecodeStr(documentId);
        RevisionId = DecodeStr(revisionId);
        SourceSha256 = DecodeStr(sourceSha256);
        Title = HasTitle ? DecodeStr(title) : null;
    }

    internal static CoreDocument CopyFromView(in CoreFfi.md_snapshot_view view, ulong maxCopyBytes)
    {
        if (view.model_version != CoreFfi.MD_MODEL_V2)
        {
            throw CoreException.InvalidSnapshot(
                $"expected model_version {CoreFfi.MD_MODEL_V2}, got {view.model_version}");
        }

        var budget = SnapshotBudget(in view, maxCopyBytes);

        var source = CopySlice(view.source, (uint)view.source_length, 1, "source");
        var strings = CopySlice(view.strings, (uint)view.strings_length, 1, "strings");
        var nodes = CopySlice(view.nodes, view.node_count, 8, "nodes");
        var roots = CopySlice(view.roots, view.root_count, 4, "roots");
        var children = CopySlice(view.children, view.child_count, 4, "children");
        var alignments = CopySlice(view.alignments, view.alignment_count, 4, "alignments");
        var links = CopySlice(view.links, view.link_count, 4, "links");
        var resources = CopySlice(view.resources, view.resource_count, 4, "resources");
        var outline = CopySlice(view.outline, view.outline_count, 4, "outline");
        var diagnostics = CopySlice(view.diagnostics, view.diagnostic_count, 4, "diagnostics");
        var properties = CopySlice(view.properties, view.property_count, 8, "properties");
        var stringList = CopySlice(view.string_list, view.string_list_count, 4, "string_list");

        return new CoreDocument(
            modelVersion: view.model_version,
            generation: view.generation,
            capabilities: view.capabilities,
            flags: view.flags,
            frontMatterStart: view.front_matter_start,
            frontMatterEnd: view.front_matter_end,
            documentId: view.document_id,
            revisionId: view.revision_id,
            sourceSha256: view.source_sha256,
            title: view.title,
            source: source,
            strings: strings,
            nodes: nodes,
            roots: roots,
            children: children,
            alignments: alignments,
            links: links,
            resources: resources,
            outline: outline,
            diagnostics: diagnostics,
            properties: properties,
            stringList: stringList,
            copiedBytes: budget);
    }

    internal static ulong SnapshotBudget(in CoreFfi.md_snapshot_view view, ulong maxBytes)
    {
        ulong total = 0;

        void Price(ulong count, ulong itemSize)
        {
            ulong bytes;
            try
            {
                bytes = checked(count * itemSize);
                total = checked(total + bytes);
            }
            catch (OverflowException)
            {
                throw CoreException.SnapshotTooLarge(ulong.MaxValue, maxBytes);
            }
            if (count > (ulong)Array.MaxLength)
            {
                throw CoreException.SnapshotTooLarge(total, maxBytes);
            }
        }

        Price(view.source_length, 1);
        Price(view.strings_length, 1);
        Price(view.node_count, (ulong)sizeof(CoreFfi.md_node));
        Price(view.root_count, sizeof(uint));
        Price(view.child_count, sizeof(uint));
        Price(view.alignment_count, sizeof(uint));
        Price(view.link_count, (ulong)sizeof(CoreFfi.md_intent));
        Price(view.resource_count, (ulong)sizeof(CoreFfi.md_intent));
        Price(view.outline_count, (ulong)sizeof(CoreFfi.md_outline_entry));
        Price(view.diagnostic_count, (ulong)sizeof(CoreFfi.md_diagnostic));
        Price(view.property_count, (ulong)sizeof(CoreFfi.md_property));
        Price(view.string_list_count, (ulong)sizeof(CoreFfi.md_str));

        if (total > maxBytes)
        {
            throw CoreException.SnapshotTooLarge(total, maxBytes);
        }
        return total;
    }

    internal static T[] CopySlice<T>(T* ptr, uint count, nuint alignment, string name)
        where T : unmanaged
    {
        if (count == 0)
        {
            return [];
        }
        if (ptr is null)
        {
            throw CoreException.InvalidSnapshot($"null pointer for non-empty table '{name}'");
        }
        if (((nuint)ptr % alignment) != 0)
        {
            throw CoreException.InvalidSnapshot($"misaligned pointer for table '{name}'");
        }

        var result = new T[count];
        fixed (T* dst = result)
        {
            Buffer.MemoryCopy(
                ptr,
                dst,
                (long)count * sizeof(T),
                (long)count * sizeof(T));
        }
        return result;
    }

    private void CheckIndices(
        CoreFfi.md_str documentId,
        CoreFfi.md_str revisionId,
        CoreFfi.md_str sourceSha256,
        CoreFfi.md_str title)
    {
        if (!Utf8.IsValid(Source))
        {
            throw CoreException.InvalidSnapshot("source buffer is not valid UTF-8");
        }
        if (!Utf8.IsValid(Strings))
        {
            throw CoreException.InvalidSnapshot("strings buffer is not valid UTF-8");
        }

        CheckStr(documentId, "document_id");
        CheckStr(revisionId, "revision_id");
        CheckStr(sourceSha256, "source_sha256");
        if (sourceSha256.length != 64)
        {
            throw CoreException.InvalidSnapshot(
                $"source_sha256 length expected 64, got {sourceSha256.length}");
        }
        if (HasTitle)
        {
            CheckStr(title, "title");
        }

        if (HasFrontMatter)
        {
            CheckSourceSpan(FrontMatterStart, FrontMatterEnd, "front_matter");
        }

        var nodeCount = (uint)Nodes.Length;
        var childCount = (uint)Children.Length;
        var linkCount = (uint)Links.Length;
        var resourceCount = (uint)Resources.Length;
        var alignmentCount = (uint)Alignments.Length;
        var stringListCount = (uint)StringList.Length;

        foreach (var root in Roots)
        {
            if (root >= nodeCount)
            {
                throw CoreException.InvalidSnapshot($"root node index {root} out of bounds ({nodeCount})");
            }
            if (Nodes[root].parent != CoreFfi.MD_NONE)
            {
                throw CoreException.InvalidSnapshot($"root node {root} has non-none parent {Nodes[root].parent}");
            }
        }

        for (uint i = 0; i < nodeCount; i++)
        {
            ref readonly var node = ref Nodes[i];
            if (node.kind < CoreFfi.MD_NODE_PARAGRAPH || node.kind > CoreFfi.MD_NODE_UNSUPPORTED_INLINE)
            {
                throw CoreException.InvalidSnapshot($"node {i} has unknown kind {node.kind}");
            }
            if (node.parent != CoreFfi.MD_NONE && node.parent >= nodeCount)
            {
                throw CoreException.InvalidSnapshot($"node {i} parent {node.parent} out of bounds ({nodeCount})");
            }
            CheckSourceSpan(node.span_start, node.span_end, "node.span");

            if ((ulong)node.first_child + node.child_count > childCount)
            {
                throw CoreException.InvalidSnapshot($"node {i} child slice out of bounds");
            }
            for (uint c = 0; c < node.child_count; c++)
            {
                var childIdx = Children[node.first_child + c];
                if (childIdx >= nodeCount)
                {
                    throw CoreException.InvalidSnapshot($"child index {childIdx} out of bounds ({nodeCount})");
                }
                if (Nodes[childIdx].parent != i)
                {
                    throw CoreException.InvalidSnapshot(
                        $"child node {childIdx} parent {Nodes[childIdx].parent} != {i}");
                }
            }

            CheckStr(node.id, "node.id");
            CheckStr(node.text, "node.text");
            CheckStr(node.extra, "node.extra");

            if (node.kind == CoreFfi.MD_NODE_LINK && node.aux >= linkCount)
            {
                throw CoreException.InvalidSnapshot($"link node {i} aux {node.aux} out of bounds ({linkCount})");
            }
            if (node.kind == CoreFfi.MD_NODE_IMAGE && node.aux >= resourceCount)
            {
                throw CoreException.InvalidSnapshot(
                    $"image node {i} aux {node.aux} out of bounds ({resourceCount})");
            }
            if (node.kind == CoreFfi.MD_NODE_TABLE &&
                (ulong)node.aux + node.aux_count > alignmentCount)
            {
                throw CoreException.InvalidSnapshot($"table node {i} alignment slice out of bounds");
            }
        }

        foreach (ref readonly var link in Links.AsSpan())
        {
            CheckIntent(in link, "link");
        }

        foreach (ref readonly var resource in Resources.AsSpan())
        {
            CheckIntent(in resource, "resource");
        }

        foreach (ref readonly var entry in Outline.AsSpan())
        {
            if (entry.node >= nodeCount || Nodes[entry.node].kind != CoreFfi.MD_NODE_HEADING)
            {
                throw CoreException.InvalidSnapshot($"outline entry node {entry.node} is not a heading");
            }
            CheckStr(entry.title, "outline.title");
            CheckStr(entry.anchor, "outline.anchor");
        }

        foreach (ref readonly var diag in Diagnostics.AsSpan())
        {
            CheckStr(diag.code, "diagnostic.code");
            CheckStr(diag.message, "diagnostic.message");
            CheckSourceSpan(diag.span_start, diag.span_end, "diagnostic.span");
            if (diag.node != CoreFfi.MD_NONE && diag.node >= nodeCount)
            {
                throw CoreException.InvalidSnapshot($"diagnostic node {diag.node} out of bounds");
            }
        }

        foreach (var item in StringList)
        {
            CheckStr(item, "string_list");
        }

        foreach (ref readonly var prop in Properties.AsSpan())
        {
            CheckStr(prop.key, "property.key");
            CheckStr(prop.@string, "property.string");
            if ((ulong)prop.first_string + prop.string_count > stringListCount)
            {
                throw CoreException.InvalidSnapshot("property string_list slice out of bounds");
            }
        }
    }

    private void CheckIntent(in CoreFfi.md_intent intent, string label)
    {
        CheckStr(intent.id, $"{label}.id");
        CheckStr(intent.destination, $"{label}.destination");
        CheckStr(intent.target, $"{label}.target");
        CheckStr(intent.fragment, $"{label}.fragment");
        CheckSourceSpan(intent.span_start, intent.span_end, $"{label}.span");
        CheckSourceSpan(intent.definition_start, intent.definition_end, $"{label}.definition");
    }

    private void CheckStr(CoreFfi.md_str str, string label)
    {
        var end = (ulong)str.offset + str.length;
        if (end > (ulong)Strings.Length)
        {
            throw CoreException.InvalidSnapshot($"{label} range [{str.offset}..{end}) out of strings bounds ({Strings.Length})");
        }
        if (!IsCharBoundary(Strings, str.offset) || !IsCharBoundary(Strings, (uint)end))
        {
            throw CoreException.InvalidSnapshot($"{label} range [{str.offset}..{end}) is not on UTF-8 char boundaries");
        }
    }

    private void CheckSourceSpan(uint start, uint end, string label)
    {
        if (start > end || end > (uint)Source.Length)
        {
            throw CoreException.InvalidSnapshot($"{label} span [{start}..{end}) out of source bounds ({Source.Length})");
        }
        if (!IsCharBoundary(Source, start) || !IsCharBoundary(Source, end))
        {
            throw CoreException.InvalidSnapshot($"{label} span [{start}..{end}) is not on UTF-8 char boundaries");
        }
    }

    [MethodImpl(MethodImplOptions.AggressiveInlining)]
    private static bool IsCharBoundary(byte[] utf8, uint index)
    {
        if (index == (uint)utf8.Length)
        {
            return true;
        }
        if (index > (uint)utf8.Length)
        {
            return false;
        }
        // In valid UTF-8, any byte that is not a continuation byte (10xxxxxx, i.e. 0x80..0xBF)
        // is a character boundary.
        return (utf8[index] & 0xC0) != 0x80;
    }

    internal string DecodeStr(CoreFfi.md_str range)
    {
        CheckStr(range, "md_str");
        return range.length == 0 ? string.Empty : Encoding.UTF8.GetString(Strings, (int)range.offset, (int)range.length);
    }

    public string NodeText(int nodeIndex) => DecodeStr(Nodes[nodeIndex].text);

    public string NodeExtra(int nodeIndex) => DecodeStr(Nodes[nodeIndex].extra);

    public string OutlineTitle(int outlineIndex) => DecodeStr(Outline[outlineIndex].title);

    public string OutlineAnchor(int outlineIndex) => DecodeStr(Outline[outlineIndex].anchor);

    public string PropertyKey(int propertyIndex) => DecodeStr(Properties[propertyIndex].key);

    public string PropertyString(int propertyIndex) => DecodeStr(Properties[propertyIndex].@string);

    public string LinkDestination(int linkIndex) => DecodeStr(Links[linkIndex].destination);

    public string LinkTarget(int linkIndex) => DecodeStr(Links[linkIndex].target);

    public string LinkFragment(int linkIndex) => DecodeStr(Links[linkIndex].fragment);

    public string ResourceDestination(int resourceIndex) => DecodeStr(Resources[resourceIndex].destination);

    public string ResourceTarget(int resourceIndex) => DecodeStr(Resources[resourceIndex].target);
}
