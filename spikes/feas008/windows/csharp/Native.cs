// FEAS-008 C# view of feas008.h. The model stays in Rust memory; the host
// reads fixed-layout values through pointers and decodes fragment text only
// for rows that are realized.
using System.Runtime.InteropServices;
using System.Text;

namespace Feas008;

[StructLayout(LayoutKind.Sequential)] public unsafe struct F8Str { public byte* Ptr; public ulong Len; }
[StructLayout(LayoutKind.Sequential)] public struct F8Item { public uint Block, Start, End; }
[StructLayout(LayoutKind.Sequential)] public struct F8Run { public uint Start, End, Link, Style; }
[StructLayout(LayoutKind.Sequential)]
public struct F8Block
{
    public uint Kind, Level, Indent, Quote;
    public F8Str Text, SepBefore, Slug, Image;
    public uint ImageKind, RunCount;
    public ulong RunOffset, SourceStart, SourceEnd;
}
[StructLayout(LayoutKind.Sequential)] public struct F8Outline { public uint Block, Level; public F8Str Title; }
[StructLayout(LayoutKind.Sequential)] public struct F8Link { public uint Intent, TargetBlock; public F8Str Dest, Detail; }
[StructLayout(LayoutKind.Sequential)] public struct F8Match { public uint Block, Start, End; }
[StructLayout(LayoutKind.Sequential)]
public unsafe struct F8Doc
{
    public uint Version, StructBytes;
    public ulong Handle;
    public F8Block* Blocks; public ulong BlockCount;
    public F8Run* Runs; public ulong RunCount;
    public F8Item* Items; public ulong ItemCount;
    public uint* FirstItem;
    public F8Outline* Outline; public ulong OutlineCount;
    public F8Link* Links; public ulong LinkCount;
    public F8Str Source;
    public ulong ParseNs;
}

public static unsafe partial class Ffi
{
    const string Lib = "feas008_ffi";
    [LibraryImport(Lib)] public static partial uint f8_open_v1(byte* utf8, ulong len, ulong fragmentBytes, F8Doc* output);
    [LibraryImport(Lib)] public static partial uint f8_free_v1(ulong handle);
    [LibraryImport(Lib)] public static partial uint f8_text_between_v1(ulong handle, uint aBlock, uint aOffset, uint bBlock, uint bOffset, F8Str* output);
    [LibraryImport(Lib)] public static partial uint f8_source_between_v1(ulong handle, uint aBlock, uint aOffset, uint bBlock, uint bOffset, F8Str* output);
    [LibraryImport(Lib)] public static partial ulong f8_plain_offset_v1(ulong handle, uint block, uint offset);
    [LibraryImport(Lib)] public static partial uint f8_item_for_v1(ulong handle, uint block, uint offset);
    [LibraryImport(Lib)] public static partial uint f8_find_v1(ulong handle, byte* query, ulong len, F8Match** output, ulong* count);
    [LibraryImport(Lib)] public static partial uint f8_html_v1(byte* utf8, ulong len, F8Str* output);
    [LibraryImport(Lib)] public static partial void f8_str_free_v1(F8Str value);
    [LibraryImport(Lib)] public static partial void f8_matches_free_v1(F8Match* matches, ulong count);
}

public readonly record struct Pos(uint Block, uint Offset) : IComparable<Pos>
{
    public int CompareTo(Pos other) => Block != other.Block ? Block.CompareTo(other.Block) : Offset.CompareTo(other.Offset);
    public static bool operator <(Pos a, Pos b) => a.CompareTo(b) < 0;
    public static bool operator >(Pos a, Pos b) => a.CompareTo(b) > 0;
    public static bool operator <=(Pos a, Pos b) => a.CompareTo(b) <= 0;
    public static bool operator >=(Pos a, Pos b) => a.CompareTo(b) >= 0;
}

public enum Kind : uint { Heading, Paragraph, ListItem, Code, TableRow, Rule, Html, Math, Image }
public enum Intent : uint { External, Fragment, MissingFragment, LocalDocument, Blocked }

public static class Style
{
    public const uint Bold = 1, Italic = 2, Code = 4, Strike = 8, Marker = 16;
}

public sealed unsafe class DocModel : IDisposable
{
    public const uint None = uint.MaxValue;
    F8Doc doc;
    public DocModel(byte[] source, ulong fragmentBytes)
    {
        fixed (byte* p = source)
        fixed (F8Doc* d = &doc)
        {
            var status = Ffi.f8_open_v1(p, (ulong)source.Length, fragmentBytes, d);
            if (status != 0) throw new InvalidOperationException($"f8_open_v1 failed with status {status}");
        }
        if (doc.StructBytes != (uint)sizeof(F8Doc)) throw new InvalidOperationException("f8_doc layout mismatch");
    }
    public void Dispose() { if (doc.Handle != 0) { Ffi.f8_free_v1(doc.Handle); doc.Handle = 0; } }

    public double ParseMs => doc.ParseNs / 1e6;
    public int ItemCount => (int)doc.ItemCount;
    public int BlockCount => (int)doc.BlockCount;
    public int OutlineCount => (int)doc.OutlineCount;
    public int LinkCount => (int)doc.LinkCount;
    public long SourceBytes => (long)doc.Source.Len;
    public ref readonly F8Item Item(int index) => ref doc.Items[index];
    public ref readonly F8Block Block(uint index) => ref doc.Blocks[index];
    public ref readonly F8Outline Outline(int index) => ref doc.Outline[index];
    public ref readonly F8Link Link(uint index) => ref doc.Links[index];
    public uint FirstItem(uint block) => doc.FirstItem[block];
    public ReadOnlySpan<F8Run> Runs(in F8Block block) => new(doc.Runs + block.RunOffset, (int)block.RunCount);
    public ReadOnlySpan<byte> SourceBytesSpan => new(doc.Source.Ptr, (int)doc.Source.Len);

    public static string Text(F8Str value) => value.Len == 0 ? "" : Encoding.UTF8.GetString(value.Ptr, (int)value.Len);
    public static ReadOnlySpan<byte> Bytes(F8Str value) => new(value.Ptr, (int)value.Len);
    public ReadOnlySpan<byte> BlockBytes(uint block, uint start, uint end) => Bytes(Block(block).Text).Slice((int)start, (int)(end - start));
    public string Slice(uint block, uint start, uint end) => Encoding.UTF8.GetString(BlockBytes(block, start, end));
    public string Fragment(int item) { var i = Item(item); return Slice(i.Block, i.Start, i.End); }

    public Pos Start => new(0, 0);
    public Pos End => BlockCount == 0 ? Start : new((uint)BlockCount - 1, (uint)Block((uint)BlockCount - 1).Text.Len);

    public string TextBetween(Pos a, Pos b)
    {
        F8Str s;
        if (Ffi.f8_text_between_v1(doc.Handle, a.Block, a.Offset, b.Block, b.Offset, &s) != 0) return "";
        try { return Text(s); } finally { Ffi.f8_str_free_v1(s); }
    }
    public ReadOnlySpan<byte> SourceBetween(Pos a, Pos b)
    {
        F8Str s;
        return Ffi.f8_source_between_v1(doc.Handle, a.Block, a.Offset, b.Block, b.Offset, &s) != 0 ? default : Bytes(s);
    }
    public long PlainOffset(Pos p) => (long)Ffi.f8_plain_offset_v1(doc.Handle, p.Block, p.Offset);
    public int ItemFor(Pos p) => (int)Ffi.f8_item_for_v1(doc.Handle, p.Block, p.Offset);
    public F8Match[] Find(string query)
    {
        var bytes = Encoding.UTF8.GetBytes(query);
        F8Match* matches; ulong count;
        fixed (byte* q = bytes)
            if (Ffi.f8_find_v1(doc.Handle, q, (ulong)bytes.Length, &matches, &count) != 0) return [];
        try { return new ReadOnlySpan<F8Match>(matches, (int)count).ToArray(); }
        finally { Ffi.f8_matches_free_v1(matches, count); }
    }
    public static string Html(byte[] source)
    {
        F8Str s;
        fixed (byte* p = source)
            if (Ffi.f8_html_v1(p, (ulong)source.Length, &s) != 0) throw new InvalidOperationException("f8_html_v1 failed");
        try { return Text(s); } finally { Ffi.f8_str_free_v1(s); }
    }

    /// Clamp to a UTF-8 character boundary at or before `offset`.
    public uint Boundary(uint block, uint offset)
    {
        var text = Bytes(Block(block).Text);
        if (offset > text.Length) offset = (uint)text.Length;
        while (offset > 0 && offset < text.Length && (text[(int)offset] & 0xC0) == 0x80) offset--;
        return offset;
    }
}
