using System.IO;
using System.Runtime.InteropServices;
using System.Text.Json;

namespace Markdown.Windows.Document;

public enum RecentKind
{
    File,
    Folder,
}

public enum RecentAvailability
{
    Unknown,
    Available,
    Missing,
    PermissionDenied,
    AccessRevoked,
    VolumeUnavailable,
    NotLocal,
    InvalidType,
}

public static class RecentAvailabilityExtensions
{
    public static string Label(this RecentAvailability availability) => availability switch
    {
        RecentAvailability.Available => "Available",
        RecentAvailability.Missing => "Missing / Moved",
        RecentAvailability.PermissionDenied => "Access Denied",
        RecentAvailability.AccessRevoked => "Access Revoked",
        RecentAvailability.VolumeUnavailable => "Volume Unavailable",
        RecentAvailability.NotLocal => "Non-local",
        RecentAvailability.InvalidType => "Wrong Type",
        _ => "Checking…",
    };

    public static bool IsRecoverable(this RecentAvailability availability) => availability switch
    {
        RecentAvailability.Missing or
        RecentAvailability.PermissionDenied or
        RecentAvailability.AccessRevoked or
        RecentAvailability.VolumeUnavailable => true,
        _ => false,
    };
}

public sealed record RecentEntry(
    RecentKind Kind,
    string Path,
    string DisplayName,
    DateTimeOffset LastOpenedUtc)
{
    public override string ToString() => $"RecentEntry({Kind}:{DisplayName})";
}

/// <summary>
/// Persistent JSON store for recent files and folder workspaces (WIN-002A).
/// Capped at 10 files and 10 folders, deduplicated case-insensitively by normalized path,
/// and stored in %LOCALAPPDATA%\MarkdownReader\recents.json (or MARKDOWN_RECENTS_PATH).
/// </summary>
public sealed class RecentsStore
{
    public const int CurrentVersion = 1;
    public const int MaxEntriesPerKind = 10;

    private readonly object _lock = new();
    private readonly string _filePath;
    private readonly List<RecentEntry> _files = [];
    private readonly List<RecentEntry> _folders = [];
    private bool _hasFutureVersion;

    public string FilePath => _filePath;
    public event Action? Changed;

    public static string DefaultStorePath
    {
        get
        {
            var env = Environment.GetEnvironmentVariable("MARKDOWN_RECENTS_PATH");
            if (!string.IsNullOrWhiteSpace(env))
            {
                return env.Trim();
            }
            var localAppData = Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData);
            return Path.Combine(localAppData, "MarkdownReader", "recents.json");
        }
    }

    public RecentsStore(string? customPath = null)
    {
        _filePath = customPath ?? DefaultStorePath;
        Load();
    }

    public IReadOnlyList<RecentEntry> Files
    {
        get
        {
            lock (_lock)
            {
                return _files.ToArray();
            }
        }
    }

    public IReadOnlyList<RecentEntry> Folders
    {
        get
        {
            lock (_lock)
            {
                return _folders.ToArray();
            }
        }
    }

    public void RecordFile(DocumentCapability capability, DateTimeOffset? timestamp = null)
    {
        ArgumentNullException.ThrowIfNull(capability);
        if (capability.IsRevoked || string.IsNullOrWhiteSpace(capability.FullPath))
        {
            return;
        }
        var entry = new RecentEntry(
            RecentKind.File,
            capability.FullPath,
            capability.DisplayName,
            timestamp ?? DateTimeOffset.UtcNow);
        Upsert(RecentKind.File, entry);
    }

    public void RecordFolder(FolderCapability capability, DateTimeOffset? timestamp = null)
    {
        ArgumentNullException.ThrowIfNull(capability);
        if (capability.IsRevoked || string.IsNullOrWhiteSpace(capability.FullPath))
        {
            return;
        }
        var entry = new RecentEntry(
            RecentKind.Folder,
            FolderCapability.NormalizeDirectoryRoot(capability.FullPath).TrimEnd('\\'),
            capability.DisplayName,
            timestamp ?? DateTimeOffset.UtcNow);
        Upsert(RecentKind.Folder, entry);
    }

    public bool Relocate(RecentKind kind, string oldPath, string newPath, DateTimeOffset? timestamp = null)
    {
        if (string.IsNullOrWhiteSpace(oldPath) || string.IsNullOrWhiteSpace(newPath))
        {
            return false;
        }

        string normalizedNew;
        string displayName;
        if (kind == RecentKind.File)
        {
            if (!DocumentCapability.TryFromUserInput(newPath, out var docCap, out _) || docCap is null)
            {
                return false;
            }
            normalizedNew = docCap.FullPath;
            displayName = docCap.DisplayName;
        }
        else
        {
            if (!FolderCapability.TryFromUserInput(newPath, out var folderCap, out _) || folderCap is null)
            {
                return false;
            }
            normalizedNew = FolderCapability.NormalizeDirectoryRoot(folderCap.FullPath).TrimEnd('\\');
            displayName = folderCap.DisplayName;
        }

        var normalizedOld = NormalizeKey(kind, oldPath);
        lock (_lock)
        {
            var list = kind == RecentKind.File ? _files : _folders;
            list.RemoveAll(e => string.Equals(NormalizeKey(kind, e.Path), normalizedOld, StringComparison.OrdinalIgnoreCase) ||
                                string.Equals(NormalizeKey(kind, e.Path), normalizedNew, StringComparison.OrdinalIgnoreCase));
            list.Insert(0, new RecentEntry(kind, normalizedNew, displayName, timestamp ?? DateTimeOffset.UtcNow));
            if (list.Count > MaxEntriesPerKind)
            {
                list.RemoveRange(MaxEntriesPerKind, list.Count - MaxEntriesPerKind);
            }
            SaveLocked();
        }
        Changed?.Invoke();
        return true;
    }

    public bool Remove(RecentKind kind, string path)
    {
        if (string.IsNullOrWhiteSpace(path))
        {
            return false;
        }
        var key = NormalizeKey(kind, path);
        bool removed;
        lock (_lock)
        {
            var list = kind == RecentKind.File ? _files : _folders;
            removed = list.RemoveAll(e => string.Equals(NormalizeKey(kind, e.Path), key, StringComparison.OrdinalIgnoreCase)) > 0;
            if (removed)
            {
                SaveLocked();
            }
        }
        if (removed)
        {
            Changed?.Invoke();
        }
        return removed;
    }

    public void Clear(RecentKind? kind = null)
    {
        lock (_lock)
        {
            if (kind is null or RecentKind.File)
            {
                _files.Clear();
            }
            if (kind is null or RecentKind.Folder)
            {
                _folders.Clear();
            }
            SaveLocked();
        }
        Changed?.Invoke();
    }

    public static unsafe RecentAvailability CheckAvailability(RecentEntry entry)
    {
        ArgumentNullException.ThrowIfNull(entry);
        if (entry.Kind == RecentKind.Folder)
        {
            if (!FolderCapability.TryFromUserInput(entry.Path, out var folderCap, out var err) || folderCap is null)
            {
                return err == FolderError.NonLocalFolder ? RecentAvailability.NotLocal : RecentAvailability.InvalidType;
            }
            if (folderCap.ValidateAccess(out var validateErr))
            {
                return RecentAvailability.Available;
            }
            return validateErr switch
            {
                FolderError.NotFound => RecentAvailability.Missing,
                FolderError.PermissionDenied => RecentAvailability.PermissionDenied,
                FolderError.AccessRevoked => RecentAvailability.AccessRevoked,
                FolderError.VolumeUnavailable => RecentAvailability.VolumeUnavailable,
                FolderError.NonLocalFolder => RecentAvailability.NotLocal,
                FolderError.NotDirectory => RecentAvailability.InvalidType,
                _ => RecentAvailability.Missing,
            };
        }

        if (!DocumentCapability.TryFromUserInput(entry.Path, out var docCap, out var docErr) || docCap is null)
        {
            return docErr == DocumentError.NonLocalFile ? RecentAvailability.NotLocal : RecentAvailability.InvalidType;
        }
        if (DocumentReader.IsDeviceOrSpecialPath(docCap.RawPath) || DocumentReader.IsDeviceOrSpecialPath(docCap.FullPath))
        {
            return RecentAvailability.InvalidType;
        }

        nint handle;
        fixed (char* pathPtr = docCap.FullPath)
        {
            handle = DocumentReader.CreateFileW(
                pathPtr,
                DocumentReader.GENERIC_READ,
                DocumentReader.FILE_SHARE_READ | DocumentReader.FILE_SHARE_WRITE | DocumentReader.FILE_SHARE_DELETE,
                0,
                DocumentReader.OPEN_EXISTING,
                DocumentReader.FILE_FLAG_BACKUP_SEMANTICS,
                0);
        }

        if (handle == DocumentReader.INVALID_HANDLE_VALUE || handle == 0)
        {
            var mapped = DocumentReader.MapWin32Error(Marshal.GetLastWin32Error());
            return mapped switch
            {
                DocumentError.NotFound => RecentAvailability.Missing,
                DocumentError.PermissionDenied => RecentAvailability.PermissionDenied,
                DocumentError.VolumeUnavailable => RecentAvailability.VolumeUnavailable,
                _ => RecentAvailability.Missing,
            };
        }

        try
        {
            if (DocumentReader.GetFileType(handle) != DocumentReader.FILE_TYPE_DISK)
            {
                return RecentAvailability.InvalidType;
            }
            if (!DocumentReader.GetFileInformationByHandle(handle, out var info))
            {
                return RecentAvailability.Missing;
            }
            if ((info.dwFileAttributes & (DocumentReader.FILE_ATTRIBUTE_DIRECTORY | DocumentReader.FILE_ATTRIBUTE_DEVICE)) != 0)
            {
                return RecentAvailability.InvalidType;
            }
            if (DocumentReader.IsRemoteHandle(handle))
            {
                return RecentAvailability.NotLocal;
            }
            return RecentAvailability.Available;
        }
        finally
        {
            _ = DocumentReader.CloseHandle(handle);
        }
    }

    private void Upsert(RecentKind kind, RecentEntry entry)
    {
        var key = NormalizeKey(kind, entry.Path);
        lock (_lock)
        {
            var list = kind == RecentKind.File ? _files : _folders;
            list.RemoveAll(e => string.Equals(NormalizeKey(kind, e.Path), key, StringComparison.OrdinalIgnoreCase));
            list.Insert(0, entry);
            if (list.Count > MaxEntriesPerKind)
            {
                list.RemoveRange(MaxEntriesPerKind, list.Count - MaxEntriesPerKind);
            }
            SaveLocked();
        }
        Changed?.Invoke();
    }

    private static string NormalizeKey(RecentKind kind, string path)
    {
        var full = DocumentCapability.NormalizeLocalPath(path);
        return kind == RecentKind.Folder ? full.TrimEnd('\\', '/') : full;
    }

    private void Load()
    {
        lock (_lock)
        {
            _files.Clear();
            _folders.Clear();
            _hasFutureVersion = false;

            try
            {
                if (!File.Exists(_filePath))
                {
                    return;
                }

                using var stream = File.OpenRead(_filePath);
                using var doc = JsonDocument.Parse(stream);
                if (doc.RootElement.ValueKind != JsonValueKind.Object)
                {
                    return;
                }

                if (doc.RootElement.TryGetProperty("version", out var vProp) && vProp.TryGetInt32(out var version))
                {
                    if (version > CurrentVersion)
                    {
                        _hasFutureVersion = true;
                        return;
                    }
                    if (version < 1)
                    {
                        return;
                    }
                }
                else
                {
                    return;
                }

                if (doc.RootElement.TryGetProperty("files", out var filesProp) && filesProp.ValueKind == JsonValueKind.Array)
                {
                    DecodeList(filesProp, RecentKind.File, _files);
                }
                if (doc.RootElement.TryGetProperty("folders", out var foldersProp) && foldersProp.ValueKind == JsonValueKind.Array)
                {
                    DecodeList(foldersProp, RecentKind.Folder, _folders);
                }
            }
            catch
            {
                // Corrupt or unreadable recents file; start empty in memory
            }
        }
    }

    private static void DecodeList(JsonElement array, RecentKind kind, List<RecentEntry> destination)
    {
        var seen = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
        foreach (var item in array.EnumerateArray())
        {
            if (destination.Count >= MaxEntriesPerKind)
            {
                break;
            }
            if (item.ValueKind != JsonValueKind.Object)
            {
                continue;
            }
            if (!item.TryGetProperty("path", out var pathProp) || pathProp.ValueKind != JsonValueKind.String)
            {
                continue;
            }

            var rawPath = pathProp.GetString();
            if (string.IsNullOrWhiteSpace(rawPath) || !Path.IsPathRooted(rawPath))
            {
                continue;
            }

            string normalizedPath;
            string defaultDisplay;
            if (kind == RecentKind.File)
            {
                if (!DocumentCapability.TryFromUserInput(rawPath, out var cap, out _) || cap is null)
                {
                    continue;
                }
                normalizedPath = cap.FullPath;
                defaultDisplay = cap.DisplayName;
            }
            else
            {
                if (!FolderCapability.TryFromUserInput(rawPath, out var cap, out _) || cap is null)
                {
                    continue;
                }
                normalizedPath = FolderCapability.NormalizeDirectoryRoot(cap.FullPath).TrimEnd('\\');
                defaultDisplay = cap.DisplayName;
            }

            var key = NormalizeKey(kind, normalizedPath);
            if (!seen.Add(key))
            {
                continue;
            }

            var displayName = defaultDisplay;
            if (item.TryGetProperty("display_name", out var nameProp) && nameProp.ValueKind == JsonValueKind.String)
            {
                var candidate = nameProp.GetString();
                if (!string.IsNullOrWhiteSpace(candidate))
                {
                    displayName = DocumentCapability.SanitizeDisplayName(candidate);
                }
            }

            var timestamp = DateTimeOffset.UtcNow;
            if (item.TryGetProperty("last_opened_utc", out var tsProp) && tsProp.ValueKind == JsonValueKind.String &&
                DateTimeOffset.TryParse(tsProp.GetString(), out var parsedTs))
            {
                timestamp = parsedTs;
            }

            destination.Add(new RecentEntry(kind, normalizedPath, displayName, timestamp));
        }
    }

    private void SaveLocked()
    {
        if (_hasFutureVersion)
        {
            return;
        }

        try
        {
            var dir = Path.GetDirectoryName(_filePath);
            if (!string.IsNullOrEmpty(dir) && !Directory.Exists(dir))
            {
                Directory.CreateDirectory(dir);
            }

            var tmpPath = _filePath + ".tmp." + Guid.NewGuid().ToString("N");
            using (var stream = File.Create(tmpPath))
            using (var writer = new Utf8JsonWriter(stream, new JsonWriterOptions { Indented = true }))
            {
                writer.WriteStartObject();
                writer.WriteNumber("version", CurrentVersion);
                WriteEntries(writer, "files", _files);
                WriteEntries(writer, "folders", _folders);
                writer.WriteEndObject();
            }
            File.Move(tmpPath, _filePath, overwrite: true);
        }
        catch
        {
            // Best-effort persistence; in-memory state remains intact
        }
    }

    private static void WriteEntries(Utf8JsonWriter writer, string propertyName, List<RecentEntry> entries)
    {
        writer.WriteStartArray(propertyName);
        foreach (var entry in entries)
        {
            writer.WriteStartObject();
            writer.WriteString("path", entry.Path);
            writer.WriteString("display_name", entry.DisplayName);
            writer.WriteString("last_opened_utc", entry.LastOpenedUtc.ToString("O"));
            writer.WriteEndObject();
        }
        writer.WriteEndArray();
    }
}
