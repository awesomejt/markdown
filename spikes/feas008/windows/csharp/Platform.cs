// Windows adapter pieces shared by the C# hosts: the D-012 clock, the DWM
// presentation wait, and the document-folder image policy.
using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Text;
using Microsoft.Win32.SafeHandles;

namespace Feas008;

public static partial class Clock
{
    /// QueryPerformanceCounter value the launcher read just before spawning
    /// this process (FEAS008_LAUNCH_QPC), or the process creation time.
    public static readonly long Launch = LaunchTicks();
    public static long Now => Stopwatch.GetTimestamp();
    public static double Ms(long from, long to) => (to - from) * 1000.0 / Stopwatch.Frequency;
    public static double SinceLaunch(long to) => Ms(Launch, to);

    static long LaunchTicks()
    {
        if (long.TryParse(Environment.GetEnvironmentVariable("FEAS008_LAUNCH_QPC"), out var qpc)) return qpc;
        var age = DateTime.Now - Process.GetCurrentProcess().StartTime;
        return Now - (long)(age.TotalSeconds * Stopwatch.Frequency);
    }

    public static bool FromLauncher => Environment.GetEnvironmentVariable("FEAS008_LAUNCH_QPC") is not null;

    [LibraryImport("dwmapi.dll")] private static partial int DwmFlush();

    /// Blocks a worker until DWM presents the frame after the caller's commit.
    /// Returns the QPC time after that present, or null when DWM refuses.
    public static Task<long?> AfterNextPresent() => Task.Run<long?>(() => DwmFlush() == 0 ? Now : null);

    public static long PeakWorkingSetKiB() => Process.GetCurrentProcess().PeakWorkingSet64 / 1024;
}

public static partial class ImagePolicy
{
    public const long MaxBytes = 20L * 1024 * 1024;
    public const int MaxSide = 16_384;

    [LibraryImport("kernel32.dll", EntryPoint = "CreateFileW", StringMarshalling = StringMarshalling.Utf16, SetLastError = true)]
    private static partial SafeFileHandle CreateFile(string name, uint access, uint share, nint security, uint disposition, uint flags, nint template);
    [LibraryImport("kernel32.dll", EntryPoint = "GetFinalPathNameByHandleW", SetLastError = true)]
    private static unsafe partial uint GetFinalPathNameByHandle(SafeFileHandle file, char* path, uint length, uint flags);

    const uint GenericRead = 0x80000000, ShareRead = 1, ShareWrite = 2, OpenExisting = 3, BackupSemantics = 0x02000000;

    static unsafe string FinalPath(SafeFileHandle handle)
    {
        var buffer = stackalloc char[32768];
        var length = GetFinalPathNameByHandle(handle, buffer, 32768, 0);
        if (length == 0 || length >= 32768) throw new IOException($"final path unavailable ({Marshal.GetLastPInvokeError()})");
        return new string(buffer, 0, (int)length);
    }

    /// Resolve `relative` against the document-folder capability, follow every
    /// link through the opened handle, and refuse anything outside the folder,
    /// too large, not a known raster format or claiming oversize dimensions.
    public static byte[] Read(string folder, string relative)
    {
        using var directory = CreateFile(folder, 0, ShareRead | ShareWrite, 0, OpenExisting, BackupSemantics, 0);
        if (directory.IsInvalid) throw new IOException("document folder unavailable");
        var root = FinalPath(directory).TrimEnd('\\') + "\\";
        var candidate = Path.Combine(folder, relative.Replace('/', '\\'));
        using var file = CreateFile(candidate, GenericRead, ShareRead, 0, OpenExisting, 0, 0);
        if (file.IsInvalid) throw new IOException($"not found ({Marshal.GetLastPInvokeError()})");
        var final = FinalPath(file);
        if (!final.StartsWith(root, StringComparison.OrdinalIgnoreCase)) throw new IOException("resolves outside the document folder");
        var length = RandomAccess.GetLength(file);
        if (length > MaxBytes) throw new IOException("not a regular file within the size limit");
        var bytes = new byte[length];
        RandomAccess.Read(file, bytes, 0);
        var png = bytes.AsSpan().StartsWith((byte[])[0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
        var known = png || bytes.AsSpan().StartsWith((byte[])[0xff, 0xd8, 0xff]) || bytes.AsSpan().StartsWith("GIF8"u8)
            || (bytes.Length > 12 && bytes.AsSpan(0, 4).SequenceEqual("RIFF"u8) && bytes.AsSpan(8, 4).SequenceEqual("WEBP"u8));
        if (!known) throw new IOException("content is not an allowed raster format");
        if (png && bytes.Length >= 24)
        {
            var w = System.Buffers.Binary.BinaryPrimitives.ReadUInt32BigEndian(bytes.AsSpan(16));
            var h = System.Buffers.Binary.BinaryPrimitives.ReadUInt32BigEndian(bytes.AsSpan(20));
            if (w > MaxSide || h > MaxSide) throw new IOException($"{w}x{h} exceeds the dimension limit");
        }
        return bytes;
    }
}

/// Line-oriented JSON events on stdout for the measurement and UIA scripts.
public static class Events
{
    static readonly object Gate = new();

    /// Redirected stdout defaults to the console code page; events are UTF-8.
    public static void UseUtf8() =>
        Console.SetOut(new StreamWriter(Console.OpenStandardOutput(), new UTF8Encoding(false)) { AutoFlush = true });

    public static void Emit(string json)
    {
        lock (Gate) { Console.Out.Write("EVENT " + json + "\n"); Console.Out.Flush(); }
    }
    public static string Quote(string value)
    {
        var builder = new StringBuilder("\"");
        foreach (var c in value)
            builder.Append(c switch
            {
                '"' => "\\\"", '\\' => "\\\\", '\n' => "\\n", '\r' => "\\r", '\t' => "\\t",
                < ' ' => $"\\u{(int)c:x4}", _ => c.ToString(),
            });
        return builder.Append('"').ToString();
    }
}

/// Win32 clipboard writes: CF_UNICODETEXT for rendered text and a registered
/// "text/markdown" format holding the exact UTF-8 source bytes.
public static unsafe partial class NativeClipboard
{
    public const string MarkdownFormat = "text/markdown";
    const uint UnicodeText = 13, Moveable = 2;
    [LibraryImport("user32.dll", SetLastError = true)][return: MarshalAs(UnmanagedType.Bool)] private static partial bool OpenClipboard(nint owner);
    [LibraryImport("user32.dll")][return: MarshalAs(UnmanagedType.Bool)] private static partial bool CloseClipboard();
    [LibraryImport("user32.dll")][return: MarshalAs(UnmanagedType.Bool)] private static partial bool EmptyClipboard();
    [LibraryImport("user32.dll")] private static partial nint SetClipboardData(uint format, nint memory);
    [LibraryImport("user32.dll")] private static partial nint GetClipboardData(uint format);
    [LibraryImport("user32.dll", EntryPoint = "RegisterClipboardFormatW", StringMarshalling = StringMarshalling.Utf16)] private static partial uint RegisterClipboardFormat(string name);
    [LibraryImport("kernel32.dll")] private static partial nint GlobalAlloc(uint flags, nuint bytes);
    [LibraryImport("kernel32.dll")] private static partial nint GlobalLock(nint memory);
    [LibraryImport("kernel32.dll")][return: MarshalAs(UnmanagedType.Bool)] private static partial bool GlobalUnlock(nint memory);
    [LibraryImport("kernel32.dll")] private static partial nuint GlobalSize(nint memory);
    [LibraryImport("kernel32.dll")] private static partial nint GlobalFree(nint memory);

    static bool Open(nint owner)
    {
        for (var attempt = 0; attempt < 50; attempt++)
        {
            if (OpenClipboard(owner)) return true;
            Thread.Sleep(10);
        }
        return false;
    }

    static nint Global(ReadOnlySpan<byte> bytes, int terminator)
    {
        var memory = GlobalAlloc(Moveable, (nuint)(bytes.Length + terminator));
        if (memory == 0) throw new OutOfMemoryException("GlobalAlloc failed");
        var target = (byte*)GlobalLock(memory);
        bytes.CopyTo(new Span<byte>(target, bytes.Length));
        new Span<byte>(target + bytes.Length, terminator).Clear();
        GlobalUnlock(memory);
        return memory;
    }

    public static void Set(nint owner, string plain, ReadOnlySpan<byte> markdown)
    {
        if (!Open(owner)) throw new IOException("clipboard unavailable");
        try
        {
            EmptyClipboard();
            var text = Global(MemoryMarshal.AsBytes(plain.AsSpan()), 2);
            if (SetClipboardData(UnicodeText, text) == 0) { GlobalFree(text); throw new IOException("SetClipboardData text failed"); }
            var source = Global(markdown, 0);
            if (SetClipboardData(RegisterClipboardFormat(MarkdownFormat), source) == 0) { GlobalFree(source); throw new IOException("SetClipboardData markdown failed"); }
        }
        finally { CloseClipboard(); }
    }

    /// Bytes of a clipboard format, for the in-process read-back check.
    public static byte[]? Read(string format)
    {
        if (!Open(0)) return null;
        try
        {
            var memory = GetClipboardData(RegisterClipboardFormat(format));
            if (memory == 0) return null;
            var size = (int)GlobalSize(memory);
            var bytes = new ReadOnlySpan<byte>((void*)GlobalLock(memory), size).ToArray();
            GlobalUnlock(memory);
            return bytes;
        }
        finally { CloseClipboard(); }
    }
}
