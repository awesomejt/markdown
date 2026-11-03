using System.IO;
using System.Text.Json;
using System.Text.Json.Nodes;
using Markdown.Windows.Core;
using Markdown.Windows.Render;

namespace Markdown.Windows;

/// <summary>
/// Window state saved across application relaunches (WIN-001H):
/// Document file path, reading position as core anchors, text scale, and outline visibility.
/// Stored as JSON in %LOCALAPPDATA%\MarkdownReader\restoration.json
/// (or custom path from MARKDOWN_RESTORATION_PATH).
/// </summary>
public sealed record WindowRestorationState(
    string FilePath,
    string TopAnchorBase64,
    string? SelectionStartAnchorBase64,
    string? SelectionEndAnchorBase64,
    double TextScale,
    bool OutlineShown)
{
    public const int CurrentVersion = 1;

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
        bool outlineShown)
    {
        return new WindowRestorationState(
            filePath,
            position is not null ? Convert.ToBase64String(position.Top) : string.Empty,
            position is not null ? Convert.ToBase64String(position.SelectionStart) : null,
            position?.SelectionEnd is not null ? Convert.ToBase64String(position.SelectionEnd) : null,
            textScale,
            outlineShown);
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
                    arr.Add((JsonNode)new JsonObject
                    {
                        ["file_path"] = win.FilePath,
                        ["top_anchor"] = win.TopAnchorBase64,
                        ["sel_start"] = win.SelectionStartAnchorBase64,
                        ["sel_end"] = win.SelectionEndAnchorBase64,
                        ["text_scale"] = win.TextScale,
                        ["outline_shown"] = win.OutlineShown,
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
                if (doc.RootElement.TryGetProperty("version", out var v) && v.GetInt32() == WindowRestorationState.CurrentVersion &&
                    doc.RootElement.TryGetProperty("windows", out var arr) && arr.ValueKind == JsonValueKind.Array)
                {
                    foreach (var item in arr.EnumerateArray())
                    {
                        var filePath = item.GetProperty("file_path").GetString() ?? string.Empty;
                        var topAnchor = item.TryGetProperty("top_anchor", out var t) ? t.GetString() ?? string.Empty : string.Empty;
                        var selStart = item.TryGetProperty("sel_start", out var ss) ? ss.GetString() : null;
                        var selEnd = item.TryGetProperty("sel_end", out var se) ? se.GetString() : null;
                        var scale = item.TryGetProperty("text_scale", out var ts) && ts.TryGetDouble(out var s) ? s : 1.0;
                        var outline = !item.TryGetProperty("outline_shown", out var os) || os.GetBoolean();

                        list.Add(new WindowRestorationState(filePath, topAnchor, selStart, selEnd, scale, outline));
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
