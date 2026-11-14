using System.IO;
using System.Runtime.InteropServices;
using System.Text;
using Markdown.Windows.Core;

namespace Markdown.Windows.Document;

public enum DocumentError
{
    NotFound,
    PermissionDenied,
    AccessRevoked,
    VolumeUnavailable,
    NotRegularFile,
    NonLocalFile,
    TooLarge,
    InvalidText,
    TooComplex,
    Busy,
    ReadFailed,
    ParseFailed,
    Cancelled,
}

public static class DocumentErrorExtensions
{
    public static string Message(this DocumentError error) => error switch
    {
        DocumentError.NotFound =>
            "The selected file could not be found.",
        DocumentError.PermissionDenied =>
            "Permission to read the selected file was denied.",
        DocumentError.AccessRevoked =>
            "Access to the selected file was revoked or withdrawn.",
        DocumentError.VolumeUnavailable =>
            "The storage volume for the selected file is unavailable.",
        DocumentError.NotRegularFile =>
            "The selected item is not a regular file.",
        DocumentError.NonLocalFile =>
            "Only local files can be opened.",
        DocumentError.TooLarge =>
            "The selected file exceeds the 5 MiB size limit.",
        DocumentError.InvalidText =>
            "The selected file is not valid UTF-8 text.",
        DocumentError.TooComplex =>
            "The selected document exceeds complexity limits.",
        DocumentError.Busy =>
            "The reader is busy processing another document.",
        DocumentError.ReadFailed =>
            "The selected file could not be read.",
        DocumentError.ParseFailed =>
            "The selected document could not be parsed.",
        DocumentError.Cancelled =>
            "The document load was cancelled.",
        _ =>
            "The selected document could not be opened.",
    };

    public static string Recovery(this DocumentError error) => error switch
    {
        DocumentError.NotFound =>
            "Restore the file and choose File > Reload (Ctrl+R), or choose File > Open (Ctrl+O) to pick another file.",
        DocumentError.PermissionDenied =>
            "Grant read access to the file and choose File > Reload (Ctrl+R), or choose File > Open (Ctrl+O) to re-select it.",
        DocumentError.AccessRevoked =>
            "Restore read permissions and choose File > Reload (Ctrl+R), or choose File > Open (Ctrl+O) to grant access again.",
        DocumentError.VolumeUnavailable =>
            "Reconnect the drive or storage volume and choose File > Reload (Ctrl+R), or open a different local file.",
        DocumentError.NotRegularFile =>
            "Choose File > Open (Ctrl+O) and select a regular Markdown or text file.",
        DocumentError.NonLocalFile =>
            "Download or copy the file to a local drive, then choose File > Open (Ctrl+O).",
        DocumentError.TooLarge =>
            "Open a Markdown or text file that is 5 MiB or smaller.",
        DocumentError.InvalidText =>
            "Re-save the file as UTF-8 text and choose File > Reload (Ctrl+R).",
        DocumentError.TooComplex =>
            "Simplify the document structure or split it into smaller files, then choose File > Reload (Ctrl+R).",
        DocumentError.Busy =>
            "Wait for active loads to finish and choose File > Reload (Ctrl+R).",
        DocumentError.ReadFailed =>
            "Check that the file is not locked and the storage device is healthy, then choose File > Reload (Ctrl+R).",
        DocumentError.ParseFailed =>
            "Choose File > Reload (Ctrl+R) to try again, or open a different file.",
        DocumentError.Cancelled =>
            "Choose File > Reload (Ctrl+R) to load the document again.",
        _ =>
            "Choose File > Open (Ctrl+O) to select another file.",
    };

    public static string ToDisplayString(this DocumentError error) =>
        $"{error.Message()}\n\n{error.Recovery()}";

    public static CoreUnavailability? UnavailableReason(this DocumentError error) => error switch
    {
        DocumentError.NotFound => CoreUnavailability.Deleted,
        DocumentError.PermissionDenied => CoreUnavailability.PermissionDenied,
        DocumentError.AccessRevoked => CoreUnavailability.AccessRevoked,
        DocumentError.VolumeUnavailable => CoreUnavailability.Io,
        DocumentError.ReadFailed => CoreUnavailability.Io,
        _ => null,
    };

    public static DocumentError FromParseFailure(CoreParseFailure failure) => failure switch
    {
        CoreParseFailure.InvalidUtf8 => DocumentError.InvalidText,
        CoreParseFailure.SourceLimit => DocumentError.TooLarge,
        CoreParseFailure.EventLimit or
        CoreParseFailure.NodeLimit or
        CoreParseFailure.DepthLimit or
        CoreParseFailure.TextLimit => DocumentError.TooComplex,
        _ => DocumentError.ParseFailed,
    };

    public static DocumentError FromCoreException(CoreException ex) => ex.Kind switch
    {
        CoreErrorKind.Busy => DocumentError.Busy,
        CoreErrorKind.SnapshotTooLarge => DocumentError.TooComplex,
        CoreErrorKind.Cancelled or
        CoreErrorKind.Superseded or
        CoreErrorKind.SessionClosed => DocumentError.Cancelled,
        CoreErrorKind.ParseFailed => FromParseFailure((CoreParseFailure)ex.FailureKind),
        _ => DocumentError.ParseFailed,
    };
}

/// <summary>
/// Platform-owned file access capability. Holds the user-approved local path internally
/// without exposing it through <see cref="ToString"/> or core session identifiers.
/// </summary>
public sealed class DocumentCapability
{
    public const int MaxDisplayScalars = 160;

    private readonly string _rawPath;
    private readonly string _fullPath;
    private int _revoked;

    internal string RawPath => _rawPath;
    internal string FullPath => _fullPath;
    public string DisplayName { get; }

    public DocumentCapability(string path)
    {
        ArgumentNullException.ThrowIfNull(path);
        _rawPath = path;
        _fullPath = NormalizeLocalPath(path);
        DisplayName = SanitizeDisplayName(ExtractFileName(_rawPath, _fullPath));
    }

    public static DocumentCapability FromPickerPath(string path) => new(path);

    public static bool TryFromUserInput(
        string? rawInput,
        out DocumentCapability? capability,
        out DocumentError error)
    {
        capability = null;
        if (string.IsNullOrWhiteSpace(rawInput))
        {
            error = DocumentError.NotFound;
            return false;
        }

        var trimmed = rawInput.Trim();

        // Check for URI schemes (2+ ASCII scheme chars followed by ':' so single-letter Windows drives C:\ pass through).
        if (HasUriScheme(trimmed, out var scheme))
        {
            if (!scheme.Equals("file", StringComparison.OrdinalIgnoreCase))
            {
                error = DocumentError.NonLocalFile;
                return false;
            }

            if (!Uri.TryCreate(trimmed, UriKind.Absolute, out var fileUri) ||
                !fileUri.Scheme.Equals("file", StringComparison.OrdinalIgnoreCase))
            {
                error = DocumentError.NonLocalFile;
                return false;
            }

            if (fileUri.IsUnc ||
                (!string.IsNullOrEmpty(fileUri.Host) &&
                 !fileUri.Host.Equals("localhost", StringComparison.OrdinalIgnoreCase)))
            {
                error = DocumentError.NonLocalFile;
                return false;
            }

            var localPath = fileUri.LocalPath;
            if (IsUncNetworkPath(localPath))
            {
                error = DocumentError.NonLocalFile;
                return false;
            }

            capability = new DocumentCapability(localPath);
            error = default;
            return true;
        }

        if (IsUncNetworkPath(trimmed))
        {
            error = DocumentError.NonLocalFile;
            return false;
        }

        capability = new DocumentCapability(trimmed);
        error = default;
        return true;
    }

    public void Revoke() => Interlocked.Exchange(ref _revoked, 1);

    public bool IsRevoked => Volatile.Read(ref _revoked) != 0;

    public bool SameFileAs(DocumentCapability? other) =>
        other is not null &&
        string.Equals(_fullPath, other._fullPath, StringComparison.OrdinalIgnoreCase);

    public override string ToString() => "DocumentCapability(<private>)";

    private static bool HasUriScheme(string input, out string scheme)
    {
        scheme = string.Empty;
        var colon = input.IndexOf(':');
        if (colon < 2)
        {
            return false;
        }

        if (!char.IsAsciiLetter(input[0]))
        {
            return false;
        }

        for (var i = 1; i < colon; i++)
        {
            var c = input[i];
            if (!char.IsAsciiLetterOrDigit(c) && c != '+' && c != '-' && c != '.')
            {
                return false;
            }
        }

        scheme = input[..colon];
        return true;
    }

    private static bool IsUncNetworkPath(string path)
    {
        if (path.StartsWith(@"\\?\UNC\", StringComparison.OrdinalIgnoreCase) ||
            path.StartsWith("//?/UNC/", StringComparison.OrdinalIgnoreCase))
        {
            return true;
        }

        if (path.StartsWith(@"\\.\", StringComparison.Ordinal) ||
            path.StartsWith(@"\\?\", StringComparison.Ordinal) ||
            path.StartsWith("//./", StringComparison.Ordinal) ||
            path.StartsWith("//?/", StringComparison.Ordinal))
        {
            return false;
        }

        return path.StartsWith(@"\\", StringComparison.Ordinal) ||
               path.StartsWith("//", StringComparison.Ordinal);
    }

    private static string NormalizeLocalPath(string path)
    {
        if (path.StartsWith(@"\\.\", StringComparison.Ordinal) ||
            path.StartsWith(@"\\?\", StringComparison.Ordinal))
        {
            return path;
        }

        try
        {
            return Path.GetFullPath(path);
        }
        catch
        {
            return path;
        }
    }

    private static string ExtractFileName(string rawPath, string fullPath)
    {
        var candidate = TrimTrailingSeparators(rawPath);
        var name = GetLastSegment(candidate);
        if (string.IsNullOrEmpty(name))
        {
            name = GetLastSegment(TrimTrailingSeparators(fullPath));
        }
        return name;
    }

    private static string TrimTrailingSeparators(string path)
    {
        var end = path.Length;
        while (end > 0 && (path[end - 1] == '\\' || path[end - 1] == '/'))
        {
            end--;
        }
        return end == path.Length ? path : path[..end];
    }

    private static string GetLastSegment(string path)
    {
        var lastBackslash = path.LastIndexOf('\\');
        var lastSlash = path.LastIndexOf('/');
        var idx = Math.Max(lastBackslash, lastSlash);
        return idx >= 0 ? path[(idx + 1)..] : path;
    }

    public static string SanitizeDisplayName(string rawName)
    {
        if (string.IsNullOrEmpty(rawName))
        {
            return "Untitled";
        }

        var builder = new StringBuilder(Math.Min(rawName.Length, MaxDisplayScalars));
        var count = 0;
        foreach (var rune in rawName.EnumerateRunes())
        {
            if (count >= MaxDisplayScalars)
            {
                break;
            }

            if (Rune.IsControl(rune) || IsBidiControl(rune))
            {
                builder.Append('\uFFFD');
            }
            else
            {
                builder.Append(rune.ToString());
            }
            count++;
        }

        return builder.Length == 0 ? "Untitled" : builder.ToString();
    }

    private static bool IsBidiControl(Rune rune) => rune.Value switch
    {
        0x061C => true,
        0x200E or 0x200F => true,
        >= 0x202A and <= 0x202E => true,
        >= 0x2066 and <= 0x2069 => true,
        _ => false,
    };
}

public sealed record ReadDocument(
    byte[] Bytes,
    byte[] RevisionToken,
    uint VolumeSerial,
    string? VolumeRoot);

public readonly record struct ReadResult(
    ReadDocument? Document,
    DocumentError? Error)
{
    public bool IsOk => Document is not null && Error is null;
    public static ReadResult Ok(ReadDocument doc) => new(doc, null);
    public static ReadResult Fail(DocumentError error) => new(null, error);
}

public static unsafe class DocumentReader
{
    public const int ChunkBytes = 64 * 1024;

    public static ReadResult Read(
        DocumentCapability capability,
        ulong maxBytes,
        Cancellation? cancellation = null,
        Action? chunkHook = null)
    {
        ArgumentNullException.ThrowIfNull(capability);

        if (cancellation is not null && cancellation.IsCancelled)
        {
            return ReadResult.Fail(DocumentError.Cancelled);
        }
        if (capability.IsRevoked)
        {
            return ReadResult.Fail(DocumentError.AccessRevoked);
        }

        if (IsDeviceOrSpecialPath(capability.RawPath) || IsDeviceOrSpecialPath(capability.FullPath))
        {
            return ReadResult.Fail(DocumentError.NotRegularFile);
        }

        var path = capability.FullPath;
        nint handle;
        fixed (char* pathPtr = path)
        {
            handle = CreateFileW(
                pathPtr,
                GENERIC_READ,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                0,
                OPEN_EXISTING,
                FILE_FLAG_BACKUP_SEMANTICS,
                0);
        }

        if (handle == INVALID_HANDLE_VALUE || handle == 0)
        {
            var err = Marshal.GetLastWin32Error();
            if (err == ERROR_ACCESS_DENIED && IsDirectoryOnDisk(path))
            {
                return ReadResult.Fail(DocumentError.NotRegularFile);
            }
            return ReadResult.Fail(MapWin32Error(err));
        }

        try
        {
            var fileType = GetFileType(handle);
            if (fileType != FILE_TYPE_DISK)
            {
                return ReadResult.Fail(DocumentError.NotRegularFile);
            }

            if (!GetFileInformationByHandle(handle, out var info))
            {
                return ReadResult.Fail(MapWin32Error(Marshal.GetLastWin32Error()));
            }

            if ((info.dwFileAttributes & (FILE_ATTRIBUTE_DIRECTORY | FILE_ATTRIBUTE_DEVICE)) != 0)
            {
                return ReadResult.Fail(DocumentError.NotRegularFile);
            }

            if (IsRemoteHandle(handle))
            {
                return ReadResult.Fail(DocumentError.NonLocalFile);
            }

            var fileSize = ((ulong)info.nFileSizeHigh << 32) | info.nFileSizeLow;
            if (fileSize > maxBytes)
            {
                return ReadResult.Fail(DocumentError.TooLarge);
            }

            var revisionToken = FormatRevisionToken(in info);






            var initialCapacity = (int)Math.Min(fileSize, Math.Min(maxBytes, 64UL * 1024UL));
            using var stream = new MemoryStream(initialCapacity);
            var chunk = new byte[ChunkBytes];
            ulong totalRead = 0;

            while (true)
            {
                if (cancellation is not null && cancellation.IsCancelled)
                {
                    return ReadResult.Fail(DocumentError.Cancelled);
                }
                if (capability.IsRevoked)
                {
                    return ReadResult.Fail(DocumentError.AccessRevoked);
                }

                chunkHook?.Invoke();

                if (cancellation is not null && cancellation.IsCancelled)
                {
                    return ReadResult.Fail(DocumentError.Cancelled);
                }
                if (capability.IsRevoked)
                {
                    return ReadResult.Fail(DocumentError.AccessRevoked);
                }

                var remainingAllowed = maxBytes - totalRead + 1;
                var toRead = (uint)Math.Min((ulong)chunk.Length, remainingAllowed);
                uint bytesRead = 0;
                bool ok;
                fixed (byte* chunkPtr = chunk)
                {
                    ok = ReadFile(handle, chunkPtr, toRead, &bytesRead, 0);
                }

                if (!ok)
                {
                    return ReadResult.Fail(MapWin32Error(Marshal.GetLastWin32Error()));
                }

                if (bytesRead == 0)
                {
                    break;
                }

                totalRead += bytesRead;
                if (totalRead > maxBytes)
                {
                    return ReadResult.Fail(DocumentError.TooLarge);
                }

                stream.Write(chunk, 0, (int)bytesRead);
            }

            if (cancellation is not null && cancellation.IsCancelled)
            {
                return ReadResult.Fail(DocumentError.Cancelled);
            }
            if (capability.IsRevoked)
            {
                return ReadResult.Fail(DocumentError.AccessRevoked);
            }

            return ReadResult.Ok(new ReadDocument(
                Bytes: stream.ToArray(),
                RevisionToken: revisionToken,
                VolumeSerial: info.dwVolumeSerialNumber,
                VolumeRoot: ExtractVolumeRoot(path)));
        }
        finally
        {
            _ = CloseHandle(handle);
        }
    }

    private static byte[] FormatRevisionToken(in BY_HANDLE_FILE_INFORMATION info)
    {
        var fileSize = ((ulong)info.nFileSizeHigh << 32) | info.nFileSizeLow;
        var fileIndex = ((ulong)info.nFileIndexHigh << 32) | info.nFileIndexLow;
        var writeTime = ((ulong)info.ftLastWriteTimeHigh << 32) | info.ftLastWriteTimeLow;
        var createTime = ((ulong)info.ftCreationTimeHigh << 32) | info.ftCreationTimeLow;
        return Encoding.UTF8.GetBytes(
            $"{info.dwVolumeSerialNumber:x8}:{fileIndex:x16}:{fileSize:x}:{writeTime:x16}:{createTime:x16}");
    }

    public static bool TryGetRevisionToken(DocumentCapability capability, out byte[]? token)
    {
        token = null;
        if (capability is null || capability.IsRevoked)
        {
            return false;
        }

        var path = capability.FullPath;
        nint handle;
        unsafe
        {
            fixed (char* pathPtr = path)
            {
                handle = CreateFileW(
                    pathPtr,
                    FILE_READ_ATTRIBUTES,
                    FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                    0,
                    OPEN_EXISTING,
                    FILE_FLAG_BACKUP_SEMANTICS,
                    0);
            }
        }

        if (handle == INVALID_HANDLE_VALUE || handle == 0)
        {
            return false;
        }

        try
        {
            if (GetFileType(handle) != FILE_TYPE_DISK)
            {
                return false;
            }

            if (!GetFileInformationByHandle(handle, out var info))
            {
                return false;
            }

            if ((info.dwFileAttributes & (FILE_ATTRIBUTE_DIRECTORY | FILE_ATTRIBUTE_DEVICE)) != 0)
            {
                return false;
            }

            token = FormatRevisionToken(in info);
            return true;
        }
        finally
        {
            _ = CloseHandle(handle);
        }
    }

    public static bool IsVolumeGone(string? volumeRoot, uint expectedSerial)
    {
        if (string.IsNullOrEmpty(volumeRoot))
        {
            return false;
        }

        uint currentSerial = 0;
        uint maxComponentLen = 0;
        uint fileSystemFlags = 0;
        bool ok;
        fixed (char* rootPtr = volumeRoot)
        {
            ok = GetVolumeInformationW(
                rootPtr,
                null,
                0,
                &currentSerial,
                &maxComponentLen,
                &fileSystemFlags,
                null,
                0);
        }

        if (!ok)
        {
            var err = Marshal.GetLastWin32Error();
            return MapWin32Error(err) == DocumentError.VolumeUnavailable ||
                   err == ERROR_PATH_NOT_FOUND ||
                   err == ERROR_FILE_NOT_FOUND;
        }

        return expectedSerial != 0 && currentSerial != expectedSerial;
    }

    public static DocumentError MapWin32Error(int error) => error switch
    {
        ERROR_FILE_NOT_FOUND or
        ERROR_PATH_NOT_FOUND or
        ERROR_INVALID_NAME or
        ERROR_BAD_PATHNAME or
        ERROR_FILENAME_EXCED_RANGE or
        ERROR_CANT_RESOLVE_FILENAME => DocumentError.NotFound,

        ERROR_ACCESS_DENIED or
        ERROR_PRIVILEGE_NOT_HELD => DocumentError.PermissionDenied,

        ERROR_INVALID_DRIVE or
        ERROR_NOT_READY or
        ERROR_SECTOR_NOT_FOUND or
        ERROR_GEN_FAILURE or
        ERROR_BAD_NETPATH or
        ERROR_NETWORK_BUSY or
        ERROR_DEV_NOT_EXIST or
        ERROR_ADAP_HDW_ERR or
        ERROR_UNEXP_NET_ERR or
        ERROR_NETNAME_DELETED or
        ERROR_BAD_NET_NAME or
        ERROR_SEM_TIMEOUT or
        ERROR_UNRECOGNIZED_VOLUME or
        ERROR_NO_MEDIA_IN_DRIVE or
        ERROR_DISK_OPERATION_FAILED or
        ERROR_DEVICE_NOT_CONNECTED or
        ERROR_FILE_CORRUPT or
        ERROR_DISK_CORRUPT => DocumentError.VolumeUnavailable,

        _ => DocumentError.ReadFailed,
    };

    public static bool IsDeviceOrSpecialPath(string path)
    {
        if (string.IsNullOrWhiteSpace(path))
        {
            return false;
        }

        var trimmed = path.Trim();
        if (trimmed.StartsWith(@"\\.\", StringComparison.OrdinalIgnoreCase) ||
            trimmed.StartsWith("//./", StringComparison.OrdinalIgnoreCase) ||
            trimmed.StartsWith(@"\\?\pipe\", StringComparison.OrdinalIgnoreCase) ||
            trimmed.StartsWith("//?/pipe/", StringComparison.OrdinalIgnoreCase))
        {
            return true;
        }

        // Check the final path segment against reserved DOS device names (CON, PRN, AUX, NUL, COM1..9, LPT1..9).
        var clean = trimmed.TrimEnd('\\', '/', ' ', '.');
        var sepIndex = Math.Max(clean.LastIndexOf('\\'), clean.LastIndexOf('/'));
        var leaf = sepIndex >= 0 ? clean[(sepIndex + 1)..] : clean;
        var dotIndex = leaf.IndexOf('.');
        var stem = (dotIndex >= 0 ? leaf[..dotIndex] : leaf).Trim();
        if (stem.EndsWith(':'))
        {
            stem = stem[..^1];
        }

        if (stem.Equals("CON", StringComparison.OrdinalIgnoreCase) ||
            stem.Equals("PRN", StringComparison.OrdinalIgnoreCase) ||
            stem.Equals("AUX", StringComparison.OrdinalIgnoreCase) ||
            stem.Equals("NUL", StringComparison.OrdinalIgnoreCase) ||
            stem.Equals("CONIN$", StringComparison.OrdinalIgnoreCase) ||
            stem.Equals("CONOUT$", StringComparison.OrdinalIgnoreCase) ||
            stem.Equals("CLOCK$", StringComparison.OrdinalIgnoreCase))
        {
            return true;
        }

        if (stem.Length == 4 &&
            (stem.StartsWith("COM", StringComparison.OrdinalIgnoreCase) ||
             stem.StartsWith("LPT", StringComparison.OrdinalIgnoreCase)))
        {
            var d = stem[3];
            if ((d >= '1' && d <= '9') || d == '\u00B9' || d == '\u00B2' || d == '\u00B3')
            {
                return true;
            }
        }

        return false;
    }

    private static bool IsRemoteHandle(nint handle)
    {
        const int capacity = 4096;
        char* buffer = stackalloc char[capacity];
        var len = GetFinalPathNameByHandleW(handle, buffer, capacity, 0);
        if (len == 0 || len >= capacity)
        {
            return false;
        }
        var finalPath = new ReadOnlySpan<char>(buffer, (int)len);
        return finalPath.StartsWith(@"\\?\UNC\".AsSpan(), StringComparison.OrdinalIgnoreCase);
    }

    private static bool IsDirectoryOnDisk(string path)
    {
        uint attrs;
        fixed (char* pathPtr = path)
        {
            attrs = GetFileAttributesW(pathPtr);
        }
        return attrs != INVALID_FILE_ATTRIBUTES && (attrs & FILE_ATTRIBUTE_DIRECTORY) != 0;
    }

    private static string? ExtractVolumeRoot(string path)
    {
        try
        {
            var root = Path.GetPathRoot(path);
            if (string.IsNullOrEmpty(root))
            {
                return null;
            }
            if (!root.EndsWith('\\') && !root.EndsWith('/'))
            {
                return root + "\\";
            }
            return root;
        }
        catch
        {
            return null;
        }
    }

    private const uint GENERIC_READ = 0x80000000;
    private const uint FILE_READ_ATTRIBUTES = 0x00000080;
    private const uint FILE_SHARE_READ = 0x00000001;
    private const uint FILE_SHARE_WRITE = 0x00000002;
    private const uint FILE_SHARE_DELETE = 0x00000004;
    private const uint OPEN_EXISTING = 3;
    private const uint FILE_FLAG_BACKUP_SEMANTICS = 0x02000000;
    private const uint FILE_TYPE_DISK = 0x0001;
    private const uint FILE_ATTRIBUTE_DIRECTORY = 0x00000010;
    private const uint FILE_ATTRIBUTE_DEVICE = 0x00000040;
    private const uint INVALID_FILE_ATTRIBUTES = 0xFFFFFFFF;
    private static readonly nint INVALID_HANDLE_VALUE = -1;

    internal const int ERROR_FILE_NOT_FOUND = 2;
    internal const int ERROR_PATH_NOT_FOUND = 3;
    internal const int ERROR_ACCESS_DENIED = 5;
    internal const int ERROR_INVALID_DRIVE = 15;
    internal const int ERROR_NOT_READY = 21;
    internal const int ERROR_SECTOR_NOT_FOUND = 27;
    internal const int ERROR_GEN_FAILURE = 31;
    internal const int ERROR_BAD_NETPATH = 53;
    internal const int ERROR_NETWORK_BUSY = 54;
    internal const int ERROR_DEV_NOT_EXIST = 55;
    internal const int ERROR_ADAP_HDW_ERR = 57;
    internal const int ERROR_UNEXP_NET_ERR = 59;
    internal const int ERROR_NETNAME_DELETED = 64;
    internal const int ERROR_BAD_NET_NAME = 67;
    internal const int ERROR_SEM_TIMEOUT = 121;
    internal const int ERROR_INVALID_NAME = 123;
    internal const int ERROR_BAD_PATHNAME = 161;
    internal const int ERROR_FILENAME_EXCED_RANGE = 206;
    internal const int ERROR_UNRECOGNIZED_VOLUME = 1005;
    internal const int ERROR_NO_MEDIA_IN_DRIVE = 1112;
    internal const int ERROR_DISK_OPERATION_FAILED = 1127;
    internal const int ERROR_DEVICE_NOT_CONNECTED = 1167;
    internal const int ERROR_PRIVILEGE_NOT_HELD = 1314;
    internal const int ERROR_FILE_CORRUPT = 1392;
    internal const int ERROR_DISK_CORRUPT = 1393;
    internal const int ERROR_CANT_RESOLVE_FILENAME = 1921;

    [StructLayout(LayoutKind.Sequential)]
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
    private static extern nint CreateFileW(
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
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool GetFileInformationByHandle(
        nint hFile,
        out BY_HANDLE_FILE_INFORMATION lpFileInformation);

    [DllImport("kernel32.dll", ExactSpelling = true, SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool ReadFile(
        nint hFile,
        byte* lpBuffer,
        uint nNumberOfBytesToRead,
        uint* lpNumberOfBytesRead,
        nint lpOverlapped);

    [DllImport("kernel32.dll", ExactSpelling = true, SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool CloseHandle(nint hObject);

    [DllImport("kernel32.dll", ExactSpelling = true, SetLastError = true)]
    private static extern uint GetFileAttributesW(char* lpFileName);

    [DllImport("kernel32.dll", ExactSpelling = true, SetLastError = true)]
    private static extern uint GetFinalPathNameByHandleW(
        nint hFile,
        char* lpszFilePath,
        uint cchFilePath,
        uint dwFlags);

    [DllImport("kernel32.dll", ExactSpelling = true, SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool GetVolumeInformationW(
        char* lpRootPathName,
        char* lpVolumeNameBuffer,
        uint nVolumeNameSize,
        uint* lpVolumeSerialNumber,
        uint* lpMaximumComponentLength,
        uint* lpFileSystemFlags,
        char* lpFileSystemNameBuffer,
        uint nFileSystemNameSize);
}
