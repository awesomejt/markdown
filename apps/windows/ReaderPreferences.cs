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
    private readonly Dictionary<string, object?> _fallback = new(StringComparer.Ordinal);

    public object? Get(string key)
    {
        try
        {
            var localSettings = global::Windows.Storage.ApplicationData.Current.LocalSettings;
            if (localSettings.Values.TryGetValue(key, out var val))
            {
                return val;
            }
        }
        catch
        {
            // Unpackaged or LocalSettings unavailable
        }

        return _fallback.TryGetValue(key, out var fallbackVal) ? fallbackVal : null;
    }

    public void Set(string key, object? value)
    {
        _fallback[key] = value;
        try
        {
            var localSettings = global::Windows.Storage.ApplicationData.Current.LocalSettings;
            localSettings.Values[key] = value;
        }
        catch
        {
            // Unpackaged or LocalSettings unavailable
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
            if (raw is double d)
            {
                return TextScale.Clamped(d);
            }
            if (raw is float f)
            {
                return TextScale.Clamped(f);
            }
            if (raw is string s && double.TryParse(s, System.Globalization.CultureInfo.InvariantCulture, out var parsed))
            {
                return TextScale.Clamped(parsed);
            }
            return TextScale.Standard;
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
