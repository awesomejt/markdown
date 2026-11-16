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
    void SetMany(IReadOnlyDictionary<string, object?> values)
    {
        foreach (var (key, value) in values)
        {
            Set(key, value);
        }
    }
}

public sealed class MemoryPreferenceStore : IPreferenceStore
{
    private readonly Dictionary<string, object?> _values = new(StringComparer.Ordinal);

    public object? Get(string key) => _values.TryGetValue(key, out var val) ? val : null;
    public void Set(string key, object? value) => _values[key] = value;

    public void SetMany(IReadOnlyDictionary<string, object?> values)
    {
        foreach (var (key, value) in values)
        {
            _values[key] = value;
        }
    }
}

public sealed class LocalSettingsPreferenceStore : IPreferenceStore
{
    public const int CurrentSchemaVersion = 2;

    private readonly string _filePath;
    private readonly object _lock = new();
    private readonly Dictionary<string, object?> _cache = new(StringComparer.Ordinal);
    private readonly Dictionary<string, string> _extraRawJson = new(StringComparer.Ordinal);

    public string FilePath => _filePath;
    public int SchemaVersion { get; private set; } = CurrentSchemaVersion;

    public static string DefaultStorePath
    {
        get
        {
            var env = Environment.GetEnvironmentVariable("MARKDOWN_PREFERENCES_PATH");
            if (!string.IsNullOrWhiteSpace(env))
            {
                return env.Trim();
            }
            var localAppData = Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData);
            return Path.Combine(localAppData, "MarkdownReader", "preferences.json");
        }
    }

    public LocalSettingsPreferenceStore(string? customPath = null)
    {
        _filePath = customPath ?? DefaultStorePath;
        Load();
    }

    private void Load()
    {
        lock (_lock)
        {
            try
            {
                if (!File.Exists(_filePath))
                {
                    SchemaVersion = CurrentSchemaVersion;
                    return;
                }

                var loadedVersion = 1;
                using (var stream = File.OpenRead(_filePath))
                using (var doc = JsonDocument.Parse(stream))
                {
                    if (doc.RootElement.ValueKind != JsonValueKind.Object)
                    {
                        SchemaVersion = CurrentSchemaVersion;
                        return;
                    }

                    if (doc.RootElement.TryGetProperty(ReaderPreferences.VersionKey, out var verProp) &&
                        verProp.ValueKind == JsonValueKind.Number &&
                        verProp.TryGetInt32(out var verInt) &&
                        verInt >= 1)
                    {
                        loadedVersion = verInt;
                    }

                    foreach (var prop in doc.RootElement.EnumerateObject())
                    {
                        switch (prop.Value.ValueKind)
                        {
                            case JsonValueKind.Number when prop.Name is ReaderPreferences.VersionKey or ReaderPreferences.ThemeKey or ReaderPreferences.TextScaleKey:
                                if (prop.Value.TryGetInt32(out var i))
                                {
                                    _cache[prop.Name] = i;
                                }
                                else if (prop.Value.TryGetDouble(out var d) && double.IsFinite(d))
                                {
                                    _cache[prop.Name] = d;
                                }
                                break;
                            case JsonValueKind.Number:
                                _extraRawJson[prop.Name] = prop.Value.GetRawText();
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
                            case JsonValueKind.Null:
                                _cache[prop.Name] = null;
                                break;
                            case JsonValueKind.Object:
                            case JsonValueKind.Array:
                                _extraRawJson[prop.Name] = prop.Value.GetRawText();
                                break;
                        }
                    }
                }

                if (loadedVersion < CurrentSchemaVersion)
                {
                    MigrateFromV1Locked();
                    SchemaVersion = CurrentSchemaVersion;
                    PersistLocked();
                }
                else
                {
                    SchemaVersion = loadedVersion;
                    _cache[ReaderPreferences.VersionKey] = loadedVersion;
                }
            }
            catch
            {
                // Unreadable or corrupt preferences; keep cache empty
                _cache.Clear();
                _extraRawJson.Clear();
                SchemaVersion = CurrentSchemaVersion;
            }
        }
    }

    private void MigrateFromV1Locked()
    {
        if (_cache.TryGetValue(ReaderPreferences.ThemeKey, out var rawTheme))
        {
            if (ReaderPreferences.TryParseTheme(rawTheme, out var parsedTheme))
            {
                _cache[ReaderPreferences.ThemeKey] = (int)parsedTheme;
            }
            else
            {
                _cache.Remove(ReaderPreferences.ThemeKey);
            }
        }

        if (_cache.TryGetValue(ReaderPreferences.TextScaleKey, out var rawScale))
        {
            if (ReaderPreferences.TryParseTextScale(rawScale, out var parsedScale))
            {
                _cache[ReaderPreferences.TextScaleKey] = parsedScale;
            }
            else
            {
                _cache.Remove(ReaderPreferences.TextScaleKey);
            }
        }

        if (!_cache.TryGetValue(ReaderPreferences.ShowOutlineByDefaultKey, out var rawOutline) || rawOutline is not bool)
        {
            _cache[ReaderPreferences.ShowOutlineByDefaultKey] = true;
        }

        if (!_cache.TryGetValue(ReaderPreferences.RestoreSessionOnLaunchKey, out var rawRestore) || rawRestore is not bool)
        {
            _cache[ReaderPreferences.RestoreSessionOnLaunchKey] = true;
        }

        _cache[ReaderPreferences.VersionKey] = CurrentSchemaVersion;
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
            _extraRawJson.Remove(key);
            _cache[ReaderPreferences.VersionKey] = Math.Max(SchemaVersion, CurrentSchemaVersion);
            PersistLocked();
        }
    }

    public void SetMany(IReadOnlyDictionary<string, object?> values)
    {
        lock (_lock)
        {
            foreach (var (key, value) in values)
            {
                _cache[key] = value;
                _extraRawJson.Remove(key);
            }
            _cache[ReaderPreferences.VersionKey] = Math.Max(SchemaVersion, CurrentSchemaVersion);
            PersistLocked();
        }
    }

    private void PersistLocked()
    {
        var tmpPath = _filePath + ".tmp." + Guid.NewGuid().ToString("N");
        try
        {
            var dir = Path.GetDirectoryName(_filePath);
            if (!string.IsNullOrEmpty(dir))
            {
                Directory.CreateDirectory(dir);
            }
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
                        case double d when double.IsFinite(d):
                            writer.WriteNumber(k, d);
                            break;
                        case float f when float.IsFinite(f):
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
                foreach (var (k, rawJson) in _extraRawJson)
                {
                    if (_cache.ContainsKey(k))
                    {
                        continue;
                    }
                    writer.WritePropertyName(k);
                    writer.WriteRawValue(rawJson);
                }
                writer.WriteEndObject();
            }
            File.Move(tmpPath, _filePath, overwrite: true);
        }
        catch
        {
            // In-memory cache continues to serve even if persistence fails
            try
            {
                if (File.Exists(tmpPath))
                {
                    File.Delete(tmpPath);
                }
            }
            catch
            {
            }
        }
    }
}

public sealed class ReaderPreferences
{
    public const int CurrentSchemaVersion = LocalSettingsPreferenceStore.CurrentSchemaVersion;
    public const string VersionKey = "version";
    public const string ThemeKey = "theme";
    public const string TextScaleKey = "textScale";
    public const string ShowOutlineByDefaultKey = "showOutlineByDefault";
    public const string RestoreSessionOnLaunchKey = "restoreSessionOnLaunch";

    public IPreferenceStore Store { get; }
    public event Action? Changed;

    public ReaderPreferences(IPreferenceStore store)
    {
        Store = store;
    }

    public AppTheme Theme
    {
        get => TryParseTheme(Store.Get(ThemeKey), out var theme) ? theme : AppTheme.System;
        set
        {
            if (value == Theme) return;
            Store.Set(ThemeKey, (int)value);
            Changed?.Invoke();
        }
    }

    public double TextScaleValue
    {
        get => TryParseTextScale(Store.Get(TextScaleKey), out var scale) ? scale : TextScale.Standard;
        set
        {
            var clamped = TextScale.Clamped(value);
            if (Math.Abs(clamped - TextScaleValue) < 0.001) return;
            Store.Set(TextScaleKey, clamped);
            Changed?.Invoke();
        }
    }

    public bool ShowOutlineByDefault
    {
        get => Store.Get(ShowOutlineByDefaultKey) is bool b ? b : true;
        set
        {
            if (value == ShowOutlineByDefault) return;
            Store.Set(ShowOutlineByDefaultKey, value);
            Changed?.Invoke();
        }
    }

    public bool RestoreSessionOnLaunch
    {
        get => Store.Get(RestoreSessionOnLaunchKey) is bool b ? b : true;
        set
        {
            if (value == RestoreSessionOnLaunch) return;
            Store.Set(RestoreSessionOnLaunchKey, value);
            Changed?.Invoke();
        }
    }

    public void ResetToDefaults()
    {
        var changed = Theme != AppTheme.System ||
                      Math.Abs(TextScaleValue - TextScale.Standard) >= 0.001 ||
                      !ShowOutlineByDefault ||
                      !RestoreSessionOnLaunch;

        Store.SetMany(new Dictionary<string, object?>(StringComparer.Ordinal)
        {
            [ThemeKey] = (int)AppTheme.System,
            [TextScaleKey] = TextScale.Standard,
            [ShowOutlineByDefaultKey] = true,
            [RestoreSessionOnLaunchKey] = true,
        });

        if (changed)
        {
            Changed?.Invoke();
        }
    }

    public ElementTheme ToElementTheme() => Theme switch
    {
        AppTheme.Light => ElementTheme.Light,
        AppTheme.Dark => ElementTheme.Dark,
        _ => ElementTheme.Default,
    };

    internal static bool TryParseTheme(object? raw, out AppTheme theme)
    {
        if (raw is int intVal && Enum.IsDefined(typeof(AppTheme), intVal))
        {
            theme = (AppTheme)intVal;
            return true;
        }
        if (raw is string strVal &&
            !int.TryParse(strVal, out _) &&
            Enum.TryParse<AppTheme>(strVal, ignoreCase: true, out var parsed) &&
            Enum.IsDefined(typeof(AppTheme), parsed))
        {
            theme = parsed;
            return true;
        }
        theme = AppTheme.System;
        return false;
    }

    internal static bool TryParseTextScale(object? raw, out double scale)
    {
        if (raw is double dVal && double.IsFinite(dVal))
        {
            scale = TextScale.Clamped(dVal);
            return true;
        }
        if (raw is float fVal && float.IsFinite(fVal))
        {
            scale = TextScale.Clamped(fVal);
            return true;
        }
        if (raw is int iVal)
        {
            scale = TextScale.Clamped(iVal);
            return true;
        }
        if (raw is string sVal &&
            double.TryParse(sVal, System.Globalization.CultureInfo.InvariantCulture, out var parsed) &&
            double.IsFinite(parsed))
        {
            scale = TextScale.Clamped(parsed);
            return true;
        }
        scale = TextScale.Standard;
        return false;
    }
}
