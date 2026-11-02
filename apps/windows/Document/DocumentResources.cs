using System.IO;
using System.Runtime.InteropServices;
using System.Text;

namespace Markdown.Windows.Document;

/// <summary>
/// Why an image resource beside the document was not loaded.
/// User-facing messages name no path.
/// </summary>
public enum ResourceError
{
    OutsideFolder,
    NotFound,
    PermissionDenied,
    NotRegularFile,
    UnsupportedType,
    TooLarge,
    Limit,
    Undecodable,
    Remote,
    Blocked,
    NotLoaded,
}

public static class ResourceErrorExtensions
{
    public static string Message(this ResourceError error) => error switch
    {
        ResourceError.Remote => "Remote images are blocked",
        ResourceError.Blocked => "This image was blocked",
        ResourceError.NotLoaded => "Image not shown",
        ResourceError.OutsideFolder => "Images outside the document’s folder are not shown",
        ResourceError.NotFound => "Image not found",
        ResourceError.PermissionDenied => "You don’t have permission to read this image",
        ResourceError.NotRegularFile => "This image file is not a regular file",
        ResourceError.UnsupportedType => "This image format is not supported",
        ResourceError.TooLarge => "This image is too large to show",
        ResourceError.Limit => "Too many images to show",
        ResourceError.Undecodable => "This image couldn’t be read",
        _ => "This image couldn’t be read",
    };
}

/// <summary>
/// Per-document image bounds. Decoding is bounded before any pixels exist:
/// files are size-capped and declared dimensions are verified from headers.
/// </summary>
public sealed class ImageBudget
{
    public int MaxImages { get; set; } = 128;
    public int MaxFileBytes { get; set; } = 16 * 1024 * 1024; // 16 MiB
    public long MaxSourcePixels { get; set; } = 32L * 1024 * 1024; // 32 MP
    public int MaxPixelSide { get; set; } = 2048;
    public long MaxDecodedBytes { get; set; } = 128L * 1024 * 1024; // 128 MiB
}

/// <summary>
/// Safe image format and dimension inspector.
/// Validates magic bytes, extracts dimensions, and detects SVG / XML without allocating full image surfaces.
/// </summary>
public static class ImageFormatInspector
{
    public static bool Inspect(
        ReadOnlySpan<byte> data,
        long maxSourcePixels,
        int maxPixelSide,
        out int width,
        out int height,
        out ResourceError? error)
    {
        width = 0;
        height = 0;
        error = null;

        if (data.Length < 4)
        {
            error = ResourceError.Undecodable;
            return false;
        }

        // SVG / XML detection: reject as unsupported vector format
        if (IsSvgOrXml(data))
        {
            error = ResourceError.UnsupportedType;
            return false;
        }

        // 1. PNG: 89 50 4E 47 0D 0A 1A 0A
        if (data.Length >= 24 &&
            data[0] == 0x89 && data[1] == 0x50 && data[2] == 0x4E && data[3] == 0x47 &&
            data[4] == 0x0D && data[5] == 0x0A && data[6] == 0x1A && data[7] == 0x0A)
        {
            // First chunk must be IHDR at offset 12
            if (data[12] == (byte)'I' && data[13] == (byte)'H' && data[14] == (byte)'D' && data[15] == (byte)'R')
            {
                var w = (data[16] << 24) | (data[17] << 16) | (data[18] << 8) | data[19];
                var h = (data[20] << 24) | (data[21] << 16) | (data[22] << 8) | data[23];
                return ValidateDimensions(w, h, maxSourcePixels, maxPixelSide, out width, out height, out error);
            }
            error = ResourceError.Undecodable;
            return false;
        }

        // 2. JPEG: FF D8 FF
        if (data.Length >= 4 && data[0] == 0xFF && data[1] == 0xD8 && data[2] == 0xFF)
        {
            var offset = 2;
            while (offset + 4 < data.Length)
            {
                if (data[offset] != 0xFF)
                {
                    break;
                }
                while (offset < data.Length && data[offset] == 0xFF)
                {
                    offset++;
                }
                if (offset >= data.Length) break;
                var marker = data[offset++];

                // SOF markers: C0, C1, C2, C3, C5, C6, C7, C9, CA, CB, CD, CE, CF
                if (marker is (>= 0xC0 and <= 0xC3) or (>= 0xC5 and <= 0xC7) or (>= 0xC9 and <= 0xCB) or (>= 0xCD and <= 0xCF))
                {
                    if (offset + 7 < data.Length)
                    {
                        var h = (data[offset + 3] << 8) | data[offset + 4];
                        var w = (data[offset + 5] << 8) | data[offset + 6];
                        return ValidateDimensions(w, h, maxSourcePixels, maxPixelSide, out width, out height, out error);
                    }
                    break;
                }

                // EOI or SOS: stop
                if (marker is 0xD9 or 0xDA)
                {
                    break;
                }

                if (offset + 1 >= data.Length) break;
                var len = (data[offset] << 8) | data[offset + 1];
                if (len < 2) break;
                offset += len;
            }
            error = ResourceError.Undecodable;
            return false;
        }

        // 3. GIF: GIF87a or GIF89a
        if (data.Length >= 10 && data[0] == (byte)'G' && data[1] == (byte)'I' && data[2] == (byte)'F' &&
            data[3] == (byte)'8' && (data[4] == (byte)'7' || data[4] == (byte)'8' || data[4] == (byte)'9') && data[5] == (byte)'a')
        {
            var w = data[6] | (data[7] << 8);
            var h = data[8] | (data[9] << 8);
            return ValidateDimensions(w, h, maxSourcePixels, maxPixelSide, out width, out height, out error);
        }

        // 4. BMP: 42 4D ('BM')
        if (data.Length >= 26 && data[0] == 0x42 && data[1] == 0x4D)
        {
            var w = BitConverter.ToInt32(data.Slice(18, 4));
            var rawH = (long)BitConverter.ToInt32(data.Slice(22, 4));
            var h = Math.Abs(rawH);
            return ValidateDimensions(w, h, maxSourcePixels, maxPixelSide, out width, out height, out error);
        }

        // 5. WebP: RIFF....WEBP
        if (data.Length >= 30 &&
            data[0] == (byte)'R' && data[1] == (byte)'I' && data[2] == (byte)'F' && data[3] == (byte)'F' &&
            data[8] == (byte)'W' && data[9] == (byte)'E' && data[10] == (byte)'B' && data[11] == (byte)'P')
        {
            if (data[12] == (byte)'V' && data[13] == (byte)'P' && data[14] == (byte)'8' && data[15] == (byte)' ')
            {
                // Simple lossy WebP
                if (data.Length >= 30)
                {
                    var w = (data[26] | (data[27] << 8)) & 0x3FFF;
                    var h = (data[28] | (data[29] << 8)) & 0x3FFF;
                    return ValidateDimensions(w, h, maxSourcePixels, maxPixelSide, out width, out height, out error);
                }
            }
            else if (data[12] == (byte)'V' && data[13] == (byte)'P' && data[14] == (byte)'8' && data[15] == (byte)'L')
            {
                // Lossless WebP
                if (data.Length >= 25 && data[20] == 0x2F)
                {
                    var b1 = data[21];
                    var b2 = data[22];
                    var b3 = data[23];
                    var b4 = data[24];
                    var w = 1 + (b1 | ((b2 & 0x3F) << 8));
                    var h = 1 + (((b2 >> 6) | (b3 << 2) | ((b4 & 0xF) << 10)));
                    return ValidateDimensions(w, h, maxSourcePixels, maxPixelSide, out width, out height, out error);
                }
            }
            else if (data[12] == (byte)'V' && data[13] == (byte)'P' && data[14] == (byte)'8' && data[15] == (byte)'X')
            {
                // Extended WebP
                if (data.Length >= 30)
                {
                    var w = 1 + (data[24] | (data[25] << 8) | (data[26] << 16));
                    var h = 1 + (data[27] | (data[28] << 8) | (data[29] << 16));
                    return ValidateDimensions(w, h, maxSourcePixels, maxPixelSide, out width, out height, out error);
                }
            }
        }

        // Unrecognized format
        error = ResourceError.Undecodable;
        return false;
    }

    private static bool IsSvgOrXml(ReadOnlySpan<byte> data)
    {
        // Strip BOM if present
        if (data.Length >= 3 && data[0] == 0xEF && data[1] == 0xBB && data[2] == 0xBF)
        {
            data = data[3..];
        }

        // Skip leading whitespace
        var i = 0;
        while (i < data.Length && (data[i] is (byte)' ' or (byte)'\t' or (byte)'\r' or (byte)'\n'))
        {
            i++;
        }

        if (i >= data.Length) return false;
        var slice = data[i..];

        if (slice.StartsWith("<?xml"u8) ||
            slice.StartsWith("<svg"u8) ||
            slice.StartsWith("<!DOCTYPE svg"u8) ||
            slice.StartsWith("<!doctype svg"u8))
        {
            return true;
        }

        return false;
    }

    private static bool ValidateDimensions(
        long w,
        long h,
        long maxSourcePixels,
        int maxPixelSide,
        out int width,
        out int height,
        out ResourceError? error)
    {
        if (w <= 0 || h <= 0)
        {
            width = 0;
            height = 0;
            error = ResourceError.Undecodable;
            return false;
        }

        var pixels = w * h;
        if (pixels > maxSourcePixels || w > maxPixelSide || h > maxPixelSide)
        {
            width = (int)Math.Min(w, int.MaxValue);
            height = (int)Math.Min(h, int.MaxValue);
            error = ResourceError.TooLarge;
            return false;
        }

        width = (int)w;
        height = (int)h;
        error = null;
        return true;
    }
}

/// <summary>
/// Read-only access to images in the folder of one granted document (WIN-001E).
/// Prevents folder escapes, directory traversal, non-regular files, and SVG vectors.
/// </summary>
public sealed class DocumentResources
{
    private static readonly HashSet<string> s_allowedExtensions = new(StringComparer.OrdinalIgnoreCase)
    {
        ".png", ".jpg", ".jpeg", ".gif", ".bmp", ".webp"
    };

    public string? RootFolder { get; }

    public DocumentResources(DocumentCapability capability)
    {
        ArgumentNullException.ThrowIfNull(capability);
        RootFolder = ResolveCanonicalFolder(capability.FullPath);
    }

    public DocumentResources(string folderPath)
    {
        ArgumentNullException.ThrowIfNull(folderPath);
        RootFolder = ResolveCanonicalFolder(folderPath);
    }

    /// <summary>
    /// Reads one image named by the core's decoded relative path, at most <paramref name="byteLimit"/> bytes.
    /// Call off the UI thread.
    /// </summary>
    public (byte[]? Data, ResourceError? Error) ReadImage(string relativePath, int byteLimit = 16 * 1024 * 1024, ImageBudget? budget = null)
    {
        if (string.IsNullOrWhiteSpace(RootFolder) ||
            string.IsNullOrWhiteSpace(relativePath) ||
            relativePath.Contains('\0') ||
            relativePath.StartsWith('/') ||
            relativePath.StartsWith('\\') ||
            relativePath.Contains(':'))
        {
            return (null, ResourceError.OutsideFolder);
        }

        var ext = Path.GetExtension(relativePath);
        if (string.IsNullOrEmpty(ext))
        {
            return (null, ResourceError.UnsupportedType);
        }

        if (ext.Equals(".svg", StringComparison.OrdinalIgnoreCase))
        {
            return (null, ResourceError.UnsupportedType);
        }

        if (!s_allowedExtensions.Contains(ext))
        {
            return (null, ResourceError.UnsupportedType);
        }

        string candidatePath;
        try
        {
            candidatePath = Path.GetFullPath(Path.Combine(RootFolder, relativePath));
        }
        catch
        {
            return (null, ResourceError.OutsideFolder);
        }

        var rootWithSlash = RootFolder.EndsWith('\\') ? RootFolder : RootFolder + "\\";
        if (!candidatePath.StartsWith(rootWithSlash, StringComparison.OrdinalIgnoreCase) ||
            candidatePath.Equals(RootFolder, StringComparison.OrdinalIgnoreCase))
        {
            return (null, ResourceError.OutsideFolder);
        }

        // Open handle to inspect canonical path and regular file attributes
        nint handle;
        unsafe
        {
            fixed (char* pathPtr = candidatePath)
            {
                handle = CreateFileW(
                    pathPtr,
                    GENERIC_READ,
                    FILE_SHARE_READ,
                    0,
                    OPEN_EXISTING,
                    FILE_ATTRIBUTE_NORMAL,
                    0);
            }
        }

        if (handle == -1 || handle == 0)
        {
            var err = Marshal.GetLastWin32Error();
            if (Directory.Exists(candidatePath))
            {
                return (null, ResourceError.NotRegularFile);
            }
            return (null, err switch
            {
                2 or 3 => ResourceError.NotFound,       // ERROR_FILE_NOT_FOUND, ERROR_PATH_NOT_FOUND
                5 => ResourceError.PermissionDenied,    // ERROR_ACCESS_DENIED
                _ => ResourceError.NotFound,
            });
        }

        try
        {
            var fileType = GetFileType(handle);
            if (fileType != FILE_TYPE_DISK)
            {
                return (null, ResourceError.NotRegularFile);
            }

            BY_HANDLE_FILE_INFORMATION info;
            if (!GetFileInformationByHandle(handle, out info))
            {
                return (null, ResourceError.NotFound);
            }

            if ((info.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY) != 0)
            {
                return (null, ResourceError.NotRegularFile);
            }

            var fileSize = ((long)info.nFileSizeHigh << 32) | info.nFileSizeLow;
            if (fileSize < 0 || fileSize > byteLimit)
            {
                return (null, ResourceError.TooLarge);
            }

            // Verify canonical folder containment via GetFinalPathNameByHandleW
            var finalCandidate = GetFinalPath(handle);
            if (string.IsNullOrEmpty(finalCandidate))
            {
                return (null, ResourceError.OutsideFolder);
            }

            if (!finalCandidate.StartsWith(rootWithSlash, StringComparison.OrdinalIgnoreCase) ||
                finalCandidate.Equals(RootFolder, StringComparison.OrdinalIgnoreCase))
            {
                return (null, ResourceError.OutsideFolder);
            }

            // Read bounded bytes
            var buffer = new byte[fileSize];
            var bytesReadTotal = 0;
            unsafe
            {
                fixed (byte* bufPtr = buffer)
                {
                    while (bytesReadTotal < buffer.Length)
                    {
                        var toRead = (uint)Math.Min(buffer.Length - bytesReadTotal, 64 * 1024);
                        uint read;
                        if (!ReadFile(handle, bufPtr + bytesReadTotal, toRead, &read, 0) || read == 0)
                        {
                            break;
                        }
                        bytesReadTotal += (int)read;
                    }
                }
            }

            if (bytesReadTotal < buffer.Length)
            {
                Array.Resize(ref buffer, bytesReadTotal);
            }

            // Inspect format, magic bytes, dimensions, and bomb checks
            var b = budget ?? new ImageBudget();
            if (!ImageFormatInspector.Inspect(buffer, b.MaxSourcePixels, b.MaxPixelSide, out _, out _, out var inspectError))
            {
                return (null, inspectError ?? ResourceError.Undecodable);
            }

            return (buffer, null);
        }
        finally
        {
            CloseHandle(handle);
        }
    }

    private static string? ResolveCanonicalFolder(string path)
    {
        try
        {
            var folder = Directory.Exists(path) ? path : Path.GetDirectoryName(path);
            if (string.IsNullOrEmpty(folder)) return null;

            nint handle;
            unsafe
            {
                fixed (char* ptr = folder)
                {
                    handle = CreateFileW(
                        ptr,
                        GENERIC_READ,
                        FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                        0,
                        OPEN_EXISTING,
                        FILE_FLAG_BACKUP_SEMANTICS,
                        0);
                }
            }

            if (handle == -1 || handle == 0)
            {
                var full = Path.GetFullPath(folder);
                return full.EndsWith('\\') ? full : full + "\\";
            }

            try
            {
                var finalPath = GetFinalPath(handle);
                if (!string.IsNullOrEmpty(finalPath))
                {
                    return finalPath.EndsWith('\\') ? finalPath : finalPath + "\\";
                }
                var full = Path.GetFullPath(folder);
                return full.EndsWith('\\') ? full : full + "\\";
            }
            finally
            {
                CloseHandle(handle);
            }
        }
        catch
        {
            return null;
        }
    }

    private static unsafe string? GetFinalPath(nint handle)
    {
        const int capacity = 4096;
        char* buffer = stackalloc char[capacity];
        var len = GetFinalPathNameByHandleW(handle, buffer, capacity, 0);
        if (len == 0 || len >= capacity)
        {
            return null;
        }
        var path = new string(buffer, 0, (int)len);
        if (path.StartsWith(@"\\?\UNC\", StringComparison.OrdinalIgnoreCase))
        {
            return @"\\" + path[8..];
        }
        if (path.StartsWith(@"\\?\", StringComparison.OrdinalIgnoreCase))
        {
            return path[4..];
        }
        return path;
    }

    private const uint GENERIC_READ = 0x80000000;
    private const uint FILE_SHARE_READ = 0x00000001;
    private const uint FILE_SHARE_WRITE = 0x00000002;
    private const uint FILE_SHARE_DELETE = 0x00000004;
    private const uint OPEN_EXISTING = 3;
    private const uint FILE_ATTRIBUTE_NORMAL = 0x00000080;
    private const uint FILE_ATTRIBUTE_DIRECTORY = 0x00000010;
    private const uint FILE_FLAG_BACKUP_SEMANTICS = 0x02000000;
    private const uint FILE_TYPE_DISK = 0x0001;

    [StructLayout(LayoutKind.Sequential, Pack = 4)]
    private struct BY_HANDLE_FILE_INFORMATION
    {
        public uint dwFileAttributes;
        public uint ftCreationTimeLow;
        public uint ftCreationTimeHigh;
        public uint ftLastAccessTimeLow;
        public uint ftLastAccessTimeHigh;
        public uint ftLastWriteTimeLow;
        public uint ftLastWriteTimeHigh;
        public uint dwVolumeSerialNumber;
        public uint nFileSizeHigh;
        public uint nFileSizeLow;
        public uint nNumberOfLinks;
        public uint nFileIndexHigh;
        public uint nFileIndexLow;
    }

    [DllImport("kernel32.dll", ExactSpelling = true, SetLastError = true)]
    private static extern unsafe nint CreateFileW(
        char* lpFileName,
        uint dwDesiredAccess,
        uint dwShareMode,
        nint lpSecurityAttributes,
        uint dwCreationDisposition,
        uint dwFlagsAndAttributes,
        nint hTemplateFile);

    [DllImport("kernel32.dll", ExactSpelling = true, SetLastError = true)]
    private static extern uint GetFileType(nint hFile);

    [DllImport("kernel32.dll", ExactSpelling = true, SetLastError = true)]
    private static extern bool GetFileInformationByHandle(nint hFile, out BY_HANDLE_FILE_INFORMATION lpFileInformation);

    [DllImport("kernel32.dll", ExactSpelling = true, SetLastError = true)]
    private static extern unsafe uint GetFinalPathNameByHandleW(
        nint hFile,
        char* lpszFilePath,
        uint cchFilePath,
        uint dwFlags);

    [DllImport("kernel32.dll", ExactSpelling = true, SetLastError = true)]
    private static extern unsafe bool ReadFile(
        nint hFile,
        byte* lpBuffer,
        uint nNumberOfBytesToRead,
        uint* lpNumberOfBytesRead,
        nint lpOverlapped);

    [DllImport("kernel32.dll", ExactSpelling = true, SetLastError = true)]
    private static extern bool CloseHandle(nint hObject);
}
