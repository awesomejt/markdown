using System.IO;
using System.Runtime.InteropServices;

namespace Markdown.Windows.Document;

public enum FolderError
{
    NotFound,
    PermissionDenied,
    AccessRevoked,
    VolumeUnavailable,
    NotDirectory,
    NonLocalFolder,
    OutsideFolder,
    OpenFailed,
}

public static class FolderErrorExtensions
{
    public static bool IsRecoverable(this FolderError error) => error switch
    {
        FolderError.NotFound or
        FolderError.PermissionDenied or
        FolderError.AccessRevoked or
        FolderError.VolumeUnavailable => true,
        _ => false,
    };

    public static string Message(this FolderError error) => error switch
    {
        FolderError.NotFound =>
            "The selected folder could not be found. It may have been moved, renamed, or deleted.",
        FolderError.PermissionDenied =>
            "Permission to read the selected folder was denied.",
        FolderError.AccessRevoked =>
            "Access to the selected folder was revoked or closed.",
        FolderError.VolumeUnavailable =>
            "The storage volume for the selected folder is unavailable.",
        FolderError.NotDirectory =>
            "The selected item is not a folder.",
        FolderError.NonLocalFolder =>
            "Only local folders can be opened as workspaces.",
        FolderError.OutsideFolder =>
            "The target item is outside the approved folder workspace.",
        FolderError.OpenFailed =>
            "The selected folder could not be opened.",
        _ =>
            "The selected folder could not be opened.",
    };

    public static string Recovery(this FolderError error) => error switch
    {
        FolderError.NotFound =>
            "Choose Locate… to select the moved folder, or Try Again if it was restored.",
        FolderError.PermissionDenied =>
            "Restore read permissions and choose Try Again, or choose Grant Access… to re-select the folder.",
        FolderError.AccessRevoked =>
            "Choose Grant Access… to re-authorize the folder, or restore permissions and choose Try Again.",
        FolderError.VolumeUnavailable =>
            "Reconnect the drive or storage volume and choose Try Again, or choose Locate… to pick another folder.",
        FolderError.NotDirectory =>
            "Choose File > Open Folder (Ctrl+Shift+O) and select a local directory.",
        FolderError.NonLocalFolder =>
            "Copy the folder to a local drive, then choose File > Open Folder (Ctrl+Shift+O).",
        FolderError.OutsideFolder =>
            "Open the item directly via File > Open (Ctrl+O), or open its containing folder.",
        _ =>
            "Choose File > Open Folder (Ctrl+Shift+O) to select another folder.",
    };

    public static string ToDisplayString(this FolderError error) =>
        $"{error.Message()}\n\n{error.Recovery()}";
}

/// <summary>
/// Shared revocable capability grant for an approved workspace folder.
/// Revoking this grant invalidates the folder and all child document capabilities bound to it.
/// </summary>
public sealed class FolderGrant
{
    private int _revoked;

    public bool IsRevoked => Volatile.Read(ref _revoked) != 0;

    public void Revoke() => Interlocked.Exchange(ref _revoked, 1);
}

/// <summary>
/// Platform-owned folder access capability. Holds the user-approved local folder path
/// internally without exposing it through <see cref="ToString"/>.
/// </summary>
public sealed unsafe class FolderCapability
{
    private readonly string _rawPath;
    private readonly string _fullPath;
    private string? _canonicalRoot;
    private int _wasValidated;

    internal string RawPath => _rawPath;
    internal string FullPath => _fullPath;
    internal string CanonicalRoot => Volatile.Read(ref _canonicalRoot) ?? NormalizeDirectoryRoot(_fullPath);
    public FolderGrant Grant { get; }
    public string DisplayName { get; }
    public bool IsRevoked => Grant.IsRevoked;

    public FolderCapability(string path, FolderGrant? grant = null)
    {
        ArgumentNullException.ThrowIfNull(path);
        _rawPath = path;
        _fullPath = DocumentCapability.NormalizeLocalPath(path);
        Grant = grant ?? new FolderGrant();
        DisplayName = DocumentCapability.SanitizeDisplayName(DocumentCapability.ExtractFileName(_rawPath, _fullPath));
    }

    public static FolderCapability FromPickerPath(string path) => new(path);

    public static bool TryFromUserInput(
        string? rawInput,
        out FolderCapability? capability,
        out FolderError error)
    {
        capability = null;
        if (string.IsNullOrWhiteSpace(rawInput))
        {
            error = FolderError.NotFound;
            return false;
        }

        var trimmed = rawInput.Trim();
        if (DocumentCapability.HasUriScheme(trimmed, out var scheme))
        {
            if (!scheme.Equals("file", StringComparison.OrdinalIgnoreCase))
            {
                error = FolderError.NonLocalFolder;
                return false;
            }

            if (!Uri.TryCreate(trimmed, UriKind.Absolute, out var fileUri) ||
                !fileUri.Scheme.Equals("file", StringComparison.OrdinalIgnoreCase))
            {
                error = FolderError.NonLocalFolder;
                return false;
            }

            if (fileUri.IsUnc ||
                (!string.IsNullOrEmpty(fileUri.Host) &&
                 !fileUri.Host.Equals("localhost", StringComparison.OrdinalIgnoreCase)))
            {
                error = FolderError.NonLocalFolder;
                return false;
            }

            var localPath = fileUri.LocalPath;
            if (DocumentCapability.IsUncNetworkPath(localPath))
            {
                error = FolderError.NonLocalFolder;
                return false;
            }

            capability = new FolderCapability(localPath);
            error = default;
            return true;
        }

        if (DocumentCapability.IsUncNetworkPath(trimmed))
        {
            error = FolderError.NonLocalFolder;
            return false;
        }

        capability = new FolderCapability(trimmed);
        error = default;
        return true;
    }

    public void Revoke() => Grant.Revoke();

    public bool SameFolderAs(FolderCapability? other) =>
        other is not null &&
        string.Equals(
            NormalizeDirectoryRoot(_fullPath),
            NormalizeDirectoryRoot(other._fullPath),
            StringComparison.OrdinalIgnoreCase);

    public bool ValidateAccess(out FolderError error)
    {
        if (IsRevoked)
        {
            error = FolderError.AccessRevoked;
            return false;
        }

        if (DocumentReader.IsDeviceOrSpecialPath(_rawPath) || DocumentReader.IsDeviceOrSpecialPath(_fullPath))
        {
            error = FolderError.NotDirectory;
            return false;
        }

        nint handle;
        fixed (char* pathPtr = _fullPath)
        {
            handle = DocumentReader.CreateFileW(
                pathPtr,
                DocumentReader.FILE_LIST_DIRECTORY,
                DocumentReader.FILE_SHARE_READ | DocumentReader.FILE_SHARE_WRITE | DocumentReader.FILE_SHARE_DELETE,
                0,
                DocumentReader.OPEN_EXISTING,
                DocumentReader.FILE_FLAG_BACKUP_SEMANTICS,
                0);
        }

        if (handle == DocumentReader.INVALID_HANDLE_VALUE || handle == 0)
        {
            var win32 = Marshal.GetLastWin32Error();
            error = MapWin32FolderError(win32, Volatile.Read(ref _wasValidated) != 0);
            return false;
        }

        try
        {
            if (!DocumentReader.GetFileInformationByHandle(handle, out var info))
            {
                error = MapWin32FolderError(Marshal.GetLastWin32Error(), Volatile.Read(ref _wasValidated) != 0);
                return false;
            }

            if ((info.dwFileAttributes & DocumentReader.FILE_ATTRIBUTE_DEVICE) != 0 ||
                (info.dwFileAttributes & DocumentReader.FILE_ATTRIBUTE_DIRECTORY) == 0)
            {
                error = FolderError.NotDirectory;
                return false;
            }

            if (DocumentReader.IsRemoteHandle(handle))
            {
                error = FolderError.NonLocalFolder;
                return false;
            }

            if (DocumentReader.TryGetFinalPathName(handle, out var finalPath))
            {
                var stripped = StripExtendedPrefix(finalPath);
                if (stripped.StartsWith(@"\\?\UNC\", StringComparison.OrdinalIgnoreCase))
                {
                    error = FolderError.NonLocalFolder;
                    return false;
                }
                Interlocked.Exchange(ref _canonicalRoot, NormalizeDirectoryRoot(stripped));
            }
            else
            {
                Interlocked.Exchange(ref _canonicalRoot, NormalizeDirectoryRoot(_fullPath));
            }

            Interlocked.Exchange(ref _wasValidated, 1);
            error = default;
            return true;
        }
        finally
        {
            _ = DocumentReader.CloseHandle(handle);
        }
    }

    public bool ResolveDocument(
        string relativeOrChildPath,
        out DocumentCapability? document,
        out FolderError error)
    {
        document = null;
        if (!ValidateAccess(out error))
        {
            return false;
        }

        if (string.IsNullOrWhiteSpace(relativeOrChildPath))
        {
            error = FolderError.OutsideFolder;
            return false;
        }

        if (DocumentCapability.HasUriScheme(relativeOrChildPath, out _) ||
            DocumentCapability.IsUncNetworkPath(relativeOrChildPath))
        {
            error = FolderError.OutsideFolder;
            return false;
        }

        string candidateFull;
        try
        {
            var root = CanonicalRoot;
            candidateFull = Path.IsPathRooted(relativeOrChildPath)
                ? Path.GetFullPath(relativeOrChildPath)
                : Path.GetFullPath(Path.Combine(root, relativeOrChildPath));
        }
        catch
        {
            error = FolderError.OutsideFolder;
            return false;
        }

        var canonicalRoot = CanonicalRoot;
        var logicalRoot = NormalizeDirectoryRoot(_fullPath);
        if (!IsPathInsideRoot(candidateFull, canonicalRoot) &&
            !IsPathInsideRoot(candidateFull, logicalRoot))
        {
            error = FolderError.OutsideFolder;
            return false;
        }

        // If the target exists on disk, verify its final canonical path (resolving symlinks/junctions)
        // also stays strictly inside the canonical folder root.
        if (TryGetFinalPathForExistingItem(candidateFull, out var finalTargetPath))
        {
            if (finalTargetPath.StartsWith(@"\\?\UNC\", StringComparison.OrdinalIgnoreCase))
            {
                error = FolderError.OutsideFolder;
                return false;
            }
            var strippedTarget = StripExtendedPrefix(finalTargetPath);
            if (!IsPathInsideRoot(strippedTarget, canonicalRoot))
            {
                error = FolderError.OutsideFolder;
                return false;
            }
            candidateFull = strippedTarget;
        }

        document = DocumentCapability.FromFolder(candidateFull, Grant);
        error = default;
        return true;
    }

    internal static bool IsPathInsideRoot(string candidatePath, string canonicalRootWithTrailingSep)
    {
        var normalizedRoot = NormalizeDirectoryRoot(canonicalRootWithTrailingSep);
        var rootWithoutSlash = normalizedRoot.TrimEnd('\\', '/');
        if (string.Equals(candidatePath.TrimEnd('\\', '/'), rootWithoutSlash, StringComparison.OrdinalIgnoreCase))
        {
            return false;
        }

        return candidatePath.StartsWith(normalizedRoot, StringComparison.OrdinalIgnoreCase);
    }

    internal static string NormalizeDirectoryRoot(string path)
    {
        var full = DocumentCapability.NormalizeLocalPath(path).Replace('/', '\\');
        return full.EndsWith('\\') ? full : full + "\\";
    }

    private static string StripExtendedPrefix(string path)
    {
        if (path.StartsWith(@"\\?\UNC\", StringComparison.OrdinalIgnoreCase))
        {
            return path;
        }
        if (path.StartsWith(@"\\?\", StringComparison.Ordinal))
        {
            return path[4..];
        }
        return path;
    }

    private static bool TryGetFinalPathForExistingItem(string path, out string finalPath)
    {
        finalPath = string.Empty;
        nint handle;
        fixed (char* pathPtr = path)
        {
            handle = DocumentReader.CreateFileW(
                pathPtr,
                DocumentReader.FILE_READ_ATTRIBUTES,
                DocumentReader.FILE_SHARE_READ | DocumentReader.FILE_SHARE_WRITE | DocumentReader.FILE_SHARE_DELETE,
                0,
                DocumentReader.OPEN_EXISTING,
                DocumentReader.FILE_FLAG_BACKUP_SEMANTICS,
                0);
        }

        if (handle == DocumentReader.INVALID_HANDLE_VALUE || handle == 0)
        {
            return false;
        }

        try
        {
            return DocumentReader.TryGetFinalPathName(handle, out finalPath);
        }
        finally
        {
            _ = DocumentReader.CloseHandle(handle);
        }
    }

    private static FolderError MapWin32FolderError(int win32Error, bool wasValidated)
    {
        var docErr = DocumentReader.MapWin32Error(win32Error);
        return docErr switch
        {
            DocumentError.NotFound => FolderError.NotFound,
            DocumentError.PermissionDenied => wasValidated ? FolderError.AccessRevoked : FolderError.PermissionDenied,
            DocumentError.VolumeUnavailable => FolderError.VolumeUnavailable,
            _ => FolderError.OpenFailed,
        };
    }

    public override string ToString() => "FolderCapability(<private>)";
}
