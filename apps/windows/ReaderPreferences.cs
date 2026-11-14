using System.Text.Json;
using Markdown.Windows.Render;
using Microsoft.UI.Xaml;

namespace Markdown.Windows;

public enum AppTheme
{
    System = 0,
    Light = 1,
    Dark = 2,
}

public interface IPreferenceStore
{
    object? Get(string key);
    void Set(string key, object? value);
}

public sealed class MemoryPreferenceStore : IPreferenceStore
{
    private readonly Dictionary<string, object?> _values = new(StringComparer.Ordinal);

    public object? Get(string key) => _values.TryGetValue(key, out var val) ? val : null;
    public void Set(string key, object? value) => _values[key] = value;
}

public sealed class LocalSettingsPreferenceStore : IPreferenceStore
{
    private readonly string _filePath;
    private readonly object _lock = new();
    private readonly Dictionary<string, object?> _cache = new(StringComparer.Ordinal);

    public string FilePath => _filePath;

    public LocalSettingsPreferenceStore(string? customPath = null)
    {
        if (customPath is not null)
        {
            _filePath = customPath;
        }
        else
        {
            var localAppData = Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData);
            _filePath = Path.Combine(localAppData, "MarkdownReader", "preferences.json");
        }
        Load();
    }

    private void Load()
    {
        lock (_lock)
        {
            try
            {
                if (File.Exists(_filePath))
                {
                    using var stream = File.OpenRead(_filePath);
                    using var doc = JsonDocument.Parse(stream);
                    if (doc.RootElement.ValueKind == JsonValueKind.Object)
                    {
                        foreach (var prop in doc.RootElement.EnumerateObject())
                        {
                            switch (prop.Value.ValueKind)
                            {
                                case JsonValueKind.Number when prop.Value.TryGetInt32(out var i):
                                    _cache[prop.Name] = i;
                                    break;
                                case JsonValueKind.Number when prop.Value.TryGetDouble(out var d):
                                    _cache[prop.Name] = d;
                                    break;
                                case JsonValueKind.String:
                                    _cache[prop.Name] = prop.Value.GetString();
                                    break;
                                case JsonValueKind.True:
                                    _cache[prop.Name] = true;
                                    break;
                                case JsonValueKind.False:
                                    _cache[prop.Name] = false;
                                    break;
                            }
                        }
                    }
                }
            }
            catch
            {
                // Unreadable or corrupt preferences; keep cache empty
            }
        }
    }

    public object? Get(string key)
    {
        lock (_lock)
        {
            return _cache.TryGetValue(key, out var val) ? val : null;
        }
    }

    public void Set(string key, object? value)
    {
        lock (_lock)
        {
            _cache[key] = value;
            try
            {
                var dir = Path.GetDirectoryName(_filePath);
                if (!string.IsNullOrEmpty(dir))
                {
                    Directory.CreateDirectory(dir);
                }
                var tmpPath = _filePath + ".tmp." + Guid.NewGuid().ToString("N");
                using (var stream = File.Create(tmpPath))
                using (var writer = new Utf8JsonWriter(stream, new JsonWriterOptions { Indented = true }))
                {
                    writer.WriteStartObject();
                    foreach (var (k, v) in _cache)
                    {
                        switch (v)
                        {
                            case int i:
                                writer.WriteNumber(k, i);
                                break;
                            case double d:
                                writer.WriteNumber(k, d);
                                break;
                            case float f:
                                writer.WriteNumber(k, f);
                                break;
                            case string s:
                                writer.WriteString(k, s);
                                break;
                            case bool b:
                                writer.WriteBoolean(k, b);
                                break;
                            case null:
                                writer.WriteNull(k);
                                break;
                        }
                    }
                    writer.WriteEndObject();
                }
                File.Move(tmpPath, _filePath, overwrite: true);
            }
            catch
            {
                // In-memory cache continues to serve even if persistence fails
            }
        }
    }
}

public sealed class ReaderPreferences
{
    public const string ThemeKey = "theme";
    public const string TextScaleKey = "textScale";

    public IPreferenceStore Store { get; }
    public event Action? Changed;

    public ReaderPreferences(IPreferenceStore store)
    {
        Store = store;
    }

    public AppTheme Theme
    {
        get
        {
            var raw = Store.Get(ThemeKey);
            if (raw is int intVal && Enum.IsDefined(typeof(AppTheme), intVal))
            {
                return (AppTheme)intVal;
            }
            if (raw is string strVal && Enum.TryParse<AppTheme>(strVal, ignoreCase: true, out var parsed))
            {
                return parsed;
            }
            return AppTheme.System;
        }
        set
        {
            if (value == Theme) return;
            Store.Set(ThemeKey, (int)value);
            Changed?.Invoke();
        }
    }

    public double TextScaleValue
    {
        get
        {
            var raw = Store.Get(TextScaleKey);
            if (raw is double dVal)
            {
                return TextScale.Clamped(dVal);
            }
            if (raw is float fVal)
            {
                return TextScale.Clamped(fVal);
            }
            if (raw is int iVal)
            {
                return TextScale.Clamped(iVal);
            }
            if (raw is string sVal && double.TryParse(sVal, System.Globalization.CultureInfo.InvariantCulture, out var parsed))
            {
                return TextScale.Clamped(parsed);
            }
            return 1.0;
        }
        set
        {
            var clamped = TextScale.Clamped(value);
            if (Math.Abs(clamped - TextScaleValue) < 0.001) return;
            Store.Set(TextScaleKey, clamped);
            Changed?.Invoke();
        }
    }

    public ElementTheme ToElementTheme() => Theme switch
    {
        AppTheme.Light => ElementTheme.Light,
        AppTheme.Dark => ElementTheme.Dark,
        _ => ElementTheme.Default,
    };
}
