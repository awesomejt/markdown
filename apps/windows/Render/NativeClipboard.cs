using System.ComponentModel;
using System.IO;
using System.Runtime.InteropServices;

namespace Markdown.Windows.Render;

/// <summary>
/// Win32 clipboard provider: CF_UNICODETEXT for plain rendered text and a registered
/// "text/markdown" format holding the exact UTF-8 source bytes.
/// </summary>
public static unsafe partial class NativeClipboard
{
    public const string MarkdownFormatName = "text/markdown";
    public const uint CfUnicodeText = 13;
    private const uint GmemMoveable = 2;

    [LibraryImport("user32.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static partial bool OpenClipboard(nint hWndNewOwner);

    [LibraryImport("user32.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static partial bool CloseClipboard();

    [LibraryImport("user32.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static partial bool EmptyClipboard();

    [LibraryImport("user32.dll", SetLastError = true)]
    private static partial nint SetClipboardData(uint uFormat, nint hMem);

    [LibraryImport("user32.dll", SetLastError = true)]
    private static partial nint GetClipboardData(uint uFormat);

    [LibraryImport("user32.dll", EntryPoint = "RegisterClipboardFormatW", StringMarshalling = StringMarshalling.Utf16, SetLastError = true)]
    private static partial uint RegisterClipboardFormat(string lpszFormat);

    [LibraryImport("kernel32.dll", SetLastError = true)]
    private static partial nint GlobalAlloc(uint uFlags, nuint dwBytes);

    [LibraryImport("kernel32.dll", SetLastError = true)]
    private static partial nint GlobalLock(nint hMem);

    [LibraryImport("kernel32.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static partial bool GlobalUnlock(nint hMem);

    [LibraryImport("kernel32.dll", SetLastError = true)]
    private static partial nuint GlobalSize(nint hMem);

    [LibraryImport("kernel32.dll", SetLastError = true)]
    private static partial nint GlobalFree(nint hMem);

    private static uint s_markdownFormatId;

    public static uint MarkdownFormatId
    {
        get
        {
            if (s_markdownFormatId == 0)
            {
                s_markdownFormatId = RegisterClipboardFormat(MarkdownFormatName);
            }
            return s_markdownFormatId;
        }
    }

    private static bool Open(nint owner)
    {
        for (var attempt = 0; attempt < 50; attempt++)
        {
            if (OpenClipboard(owner))
            {
                return true;
            }
            Thread.Sleep(10);
        }
        return false;
    }

    private static nint AllocGlobal(ReadOnlySpan<byte> bytes, int extraTrailingZeroBytes)
    {
        var totalBytes = checked((nuint)(bytes.Length + extraTrailingZeroBytes));
        var memory = GlobalAlloc(GmemMoveable, totalBytes);
        if (memory == 0)
        {
            throw new OutOfMemoryException("GlobalAlloc for clipboard failed.");
        }

        var target = (byte*)GlobalLock(memory);
        if (target is null)
        {
            _ = GlobalFree(memory);
            throw new Win32Exception(Marshal.GetLastPInvokeError(), "GlobalLock failed.");
        }

        try
        {
            bytes.CopyTo(new Span<byte>(target, bytes.Length));
            if (extraTrailingZeroBytes > 0)
            {
                new Span<byte>(target + bytes.Length, extraTrailingZeroBytes).Clear();
            }
        }
        finally
        {
            _ = GlobalUnlock(memory);
        }

        return memory;
    }

    /// <summary>
    /// Writes rendered plain text as CF_UNICODETEXT and raw markdown bytes as text/markdown to the system clipboard.
    /// </summary>
    public static void Set(nint owner, string plainText, ReadOnlySpan<byte> markdownBytes)
    {
        ArgumentNullException.ThrowIfNull(plainText);

        if (!Open(owner))
        {
            throw new IOException("Unable to open clipboard.");
        }

        try
        {
            if (!EmptyClipboard())
            {
                throw new Win32Exception(Marshal.GetLastPInvokeError(), "EmptyClipboard failed.");
            }

            // UTF-16 characters plus 2-byte null terminator for CF_UNICODETEXT
            var textBytes = MemoryMarshal.AsBytes(plainText.AsSpan());
            var hText = AllocGlobal(textBytes, extraTrailingZeroBytes: 2);
            if (SetClipboardData(CfUnicodeText, hText) == 0)
            {
                _ = GlobalFree(hText);
                throw new Win32Exception(Marshal.GetLastPInvokeError(), "SetClipboardData(CF_UNICODETEXT) failed.");
            }

            // Raw UTF-8 markdown source bytes for registered text/markdown format
            var formatId = MarkdownFormatId;
            if (formatId != 0 && markdownBytes.Length > 0)
            {
                var hMarkdown = AllocGlobal(markdownBytes, extraTrailingZeroBytes: 0);
                if (SetClipboardData(formatId, hMarkdown) == 0)
                {
                    _ = GlobalFree(hMarkdown);
                    throw new Win32Exception(Marshal.GetLastPInvokeError(), "SetClipboardData(text/markdown) failed.");
                }
            }
        }
        finally
        {
            _ = CloseClipboard();
        }
    }

    /// <summary>
    /// Reads the raw bytes of the specified clipboard format (e.g. for testing).
    /// </summary>
    public static byte[]? ReadFormat(uint formatId)
    {
        if (!Open(0))
        {
            return null;
        }

        try
        {
            var hMem = GetClipboardData(formatId);
            if (hMem == 0)
            {
                return null;
            }

            var size = (int)GlobalSize(hMem);
            if (size <= 0)
            {
                return [];
            }

            var ptr = (byte*)GlobalLock(hMem);
            if (ptr is null)
            {
                return null;
            }

            try
            {
                var result = new byte[size];
                new ReadOnlySpan<byte>(ptr, size).CopyTo(result);
                return result;
            }
            finally
            {
                _ = GlobalUnlock(hMem);
            }
        }
        finally
        {
            _ = CloseClipboard();
        }
    }

    /// <summary>
    /// Reads CF_UNICODETEXT from the clipboard as string.
    /// </summary>
    public static string? ReadUnicodeText()
    {
        var bytes = ReadFormat(CfUnicodeText);
        if (bytes is null || bytes.Length < 2)
        {
            return null;
        }

        var chars = MemoryMarshal.Cast<byte, char>(bytes);
        var nullIndex = chars.IndexOf('\0');
        return nullIndex >= 0 ? new string(chars[..nullIndex]) : new string(chars);
    }

    /// <summary>
    /// Reads text/markdown from the clipboard as UTF-8 byte array.
    /// </summary>
    public static byte[]? ReadMarkdownBytes()
    {
        return ReadFormat(MarkdownFormatId);
    }
}
