using System.IO;
using System.Text.Json;
using System.Text.Json.Nodes;
using Markdown.Windows.Core;
using Markdown.Windows.Document;
using Markdown.Windows.Render;

namespace Markdown.Windows;

/// <summary>
/// Window state saved across application relaunches (WIN-001H / WIN-002E):
/// Document file path, reading position as core anchors, text scale, outline visibility,
/// and optional workspace folder, sidebar visibility, expanded directory paths, and filter toggles.
/// Stored as JSON in %LOCALAPPDATA%\MarkdownReader\restoration.json
/// (or custom path from MARKDOWN_RESTORATION_PATH).
/// </summary>
public sealed record WindowRestorationState(
    string FilePath,
    string TopAnchorBase64,
    string? SelectionStartAnchorBase64,
    string? SelectionEndAnchorBase64,
    double TextScale,
    bool OutlineShown,
    string? WorkspacePath = null,
    bool WorkspaceSidebarVisible = true,
    IReadOnlyList<string>? ExpandedRelativePaths = null,
    bool ShowAllFiles = false,
    bool ShowHidden = false)
{
    public const int CurrentVersion = 2;

    public SavedPosition? ToSavedPosition()
    {
        try
        {
            if (string.IsNullOrEmpty(TopAnchorBase64))
            {
                return null;
            }
            var top = Convert.FromBase64String(TopAnchorBase64);
            var start = !string.IsNullOrEmpty(SelectionStartAnchorBase64)
                ? Convert.FromBase64String(SelectionStartAnchorBase64)
                : top;
            var end = !string.IsNullOrEmpty(SelectionEndAnchorBase64)
                ? Convert.FromBase64String(SelectionEndAnchorBase64)
                : null;
            return new SavedPosition(top, start, end);
        }
        catch
        {
            return null;
        }
    }

    public static WindowRestorationState Create(
        string filePath,
        SavedPosition? position,
        double textScale,
        bool outlineShown,
        string? workspacePath = null,
        bool workspaceSidebarVisible = true,
        IReadOnlyList<string>? expandedRelativePaths = null,
        bool showAllFiles = false,
        bool showHidden = false)
    {
        return new WindowRestorationState(
            filePath,
            position is not null ? Convert.ToBase64String(position.Top) : string.Empty,
            position is not null ? Convert.ToBase64String(position.SelectionStart) : null,
            position?.SelectionEnd is not null ? Convert.ToBase64String(position.SelectionEnd) : null,
            textScale,
            outlineShown,
            workspacePath,
            workspaceSidebarVisible,
            expandedRelativePaths,
            showAllFiles,
            showHidden);
    }
}

public static class RestorationStore
{
    private static readonly object s_lock = new();

    public static string DefaultStorePath
    {
        get
        {
            var env = Environment.GetEnvironmentVariable("MARKDOWN_RESTORATION_PATH");
            if (!string.IsNullOrWhiteSpace(env))
            {
                return env.Trim();
            }
            var localAppData = Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData);
            return Path.Combine(localAppData, "MarkdownReader", "restoration.json");
        }
    }

    public static void Save(IReadOnlyList<WindowRestorationState> windows, string? customPath = null)
    {
        lock (s_lock)
        {
            var path = customPath ?? DefaultStorePath;
            try
            {
                var dir = Path.GetDirectoryName(path);
                if (!string.IsNullOrEmpty(dir) && !Directory.Exists(dir))
                {
                    Directory.CreateDirectory(dir);
                }

                var root = new JsonObject
                {
                    ["version"] = WindowRestorationState.CurrentVersion,
                };
                var arr = new JsonArray();
                foreach (var win in windows)
                {
                    var expandedArr = new JsonArray();
                    if (win.ExpandedRelativePaths is not null)
                    {
                        foreach (var rel in win.ExpandedRelativePaths)
                        {
                            if (!string.IsNullOrWhiteSpace(rel))
                            {
                                expandedArr.Add((JsonNode?)JsonValue.Create(rel));
                            }
                        }
                    }

                    arr.Add((JsonNode)new JsonObject
                    {
                        ["file_path"] = win.FilePath,
                        ["top_anchor"] = win.TopAnchorBase64,
                        ["sel_start"] = win.SelectionStartAnchorBase64,
                        ["sel_end"] = win.SelectionEndAnchorBase64,
                        ["text_scale"] = win.TextScale,
                        ["outline_shown"] = win.OutlineShown,
                        ["workspace_path"] = win.WorkspacePath,
                        ["workspace_sidebar_visible"] = win.WorkspaceSidebarVisible,
                        ["expanded_paths"] = expandedArr,
                        ["show_all_files"] = win.ShowAllFiles,
                        ["show_hidden"] = win.ShowHidden,
                    });
                }
                root["windows"] = arr;

                var tempPath = path + ".tmp";
                File.WriteAllText(tempPath, root.ToJsonString(new JsonSerializerOptions { WriteIndented = true }));
                File.Move(tempPath, path, overwrite: true);
            }
            catch
            {
                // Best effort write
            }
        }
    }

    public static List<WindowRestorationState> Load(string? customPath = null)
    {
        lock (s_lock)
        {
            var path = customPath ?? DefaultStorePath;
            var list = new List<WindowRestorationState>();
            try
            {
                if (!File.Exists(path))
                {
                    return list;
                }

                using var stream = File.OpenRead(path);
                using var doc = JsonDocument.Parse(stream);
                if (doc.RootElement.TryGetProperty("version", out var v) &&
                    v.TryGetInt32(out var version) &&
                    version is 1 or WindowRestorationState.CurrentVersion &&
                    doc.RootElement.TryGetProperty("windows", out var arr) &&
                    arr.ValueKind == JsonValueKind.Array)
                {
                    foreach (var item in arr.EnumerateArray())
                    {
                        if (item.ValueKind != JsonValueKind.Object)
                        {
                            continue;
                        }

                        var rawFilePath = item.TryGetProperty("file_path", out var fp) && fp.ValueKind == JsonValueKind.String
                            ? fp.GetString() ?? string.Empty
                            : string.Empty;
                        var validFilePath = string.Empty;
                        if (!string.IsNullOrWhiteSpace(rawFilePath) &&
                            Path.IsPathRooted(rawFilePath) &&
                            DocumentCapability.TryFromUserInput(rawFilePath, out var validCap, out _) &&
                            validCap is not null)
                        {
                            validFilePath = validCap.FullPath;
                        }

                        string? validWorkspacePath = null;
                        if (version >= 2 &&
                            item.TryGetProperty("workspace_path", out var wp) &&
                            wp.ValueKind == JsonValueKind.String)
                        {
                            var rawWorkspacePath = wp.GetString();
                            if (!string.IsNullOrWhiteSpace(rawWorkspacePath) &&
                                Path.IsPathRooted(rawWorkspacePath) &&
                                FolderCapability.TryFromUserInput(rawWorkspacePath, out var validFolder, out _) &&
                                validFolder is not null)
                            {
                                validWorkspacePath = validFolder.FullPath;
                            }
                        }

                        if (string.IsNullOrEmpty(validFilePath) && validWorkspacePath is null)
                        {
                            continue;
                        }

                        var topAnchor = item.TryGetProperty("top_anchor", out var t) && t.ValueKind == JsonValueKind.String
                            ? t.GetString() ?? string.Empty
                            : string.Empty;
                        var selStart = item.TryGetProperty("sel_start", out var ss) && ss.ValueKind == JsonValueKind.String
                            ? ss.GetString()
                            : null;
                        var selEnd = item.TryGetProperty("sel_end", out var se) && se.ValueKind == JsonValueKind.String
                            ? se.GetString()
                            : null;
                        var scale = item.TryGetProperty("text_scale", out var ts) && ts.TryGetDouble(out var s) ? s : 1.0;
                        scale = Math.Clamp(scale, 0.7, 2.0);
                        var outline = !item.TryGetProperty("outline_shown", out var os) ||
                                      os.ValueKind is not (JsonValueKind.True or JsonValueKind.False) ||
                                      os.GetBoolean();

                        var sidebarVisible = !item.TryGetProperty("workspace_sidebar_visible", out var wsv) ||
                                             wsv.ValueKind is not (JsonValueKind.True or JsonValueKind.False) ||
                                             wsv.GetBoolean();
                        var showAllFiles = item.TryGetProperty("show_all_files", out var saf) &&
                                           saf.ValueKind is JsonValueKind.True or JsonValueKind.False &&
                                           saf.GetBoolean();
                        var showHidden = item.TryGetProperty("show_hidden", out var sh) &&
                                         sh.ValueKind is JsonValueKind.True or JsonValueKind.False &&
                                         sh.GetBoolean();

                        List<string>? expandedPaths = null;
                        if (version >= 2 &&
                            item.TryGetProperty("expanded_paths", out var ep) &&
                            ep.ValueKind == JsonValueKind.Array)
                        {
                            expandedPaths = new List<string>();
                            foreach (var pathElem in ep.EnumerateArray())
                            {
                                if (pathElem.ValueKind == JsonValueKind.String)
                                {
                                    var rel = pathElem.GetString();
                                    if (!string.IsNullOrWhiteSpace(rel) &&
                                        !Path.IsPathRooted(rel) &&
                                        !rel.Contains("..", StringComparison.Ordinal))
                                    {
                                        expandedPaths.Add(rel);
                                    }
                                }
                            }
                        }

                        list.Add(new WindowRestorationState(
                            validFilePath,
                            topAnchor,
                            selStart,
                            selEnd,
                            scale,
                            outline,
                            validWorkspacePath,
                            sidebarVisible,
                            expandedPaths,
                            showAllFiles,
                            showHidden));
                    }
                }
            }
            catch
            {
                // Corrupted or unreadable restoration state; return empty
            }
            return list;
        }
    }

    public static void Clear(string? customPath = null)
    {
        lock (s_lock)
        {
            var path = customPath ?? DefaultStorePath;
            try
            {
                if (File.Exists(path))
                {
                    File.Delete(path);
                }
            }
            catch
            {
            }
        }
    }
}
