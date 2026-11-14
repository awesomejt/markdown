using System.IO;
using Microsoft.UI.Xaml.Media.Imaging;

namespace Markdown.Windows.Render;

/// <summary>
/// Per-reader UI-thread cache for decoded <see cref="BitmapImage"/> surfaces.
/// Prevents redundant surface allocations within a document presentation without
/// sharing bitmaps across independent windows or surviving document reloads.
/// </summary>
public sealed class ImageSourceCache
{
    private readonly Dictionary<string, BitmapImage> _cache = new(StringComparer.Ordinal);

    public BitmapImage? GetOrCreate(RenderedImage image)
    {
        if (image.Data is null || image.Data.Length == 0) return null;
        if (_cache.TryGetValue(image.Target, out var cached))
        {
            return cached;
        }

        try
        {
            using var ms = new MemoryStream(image.Data);
            var ras = System.IO.WindowsRuntimeStreamExtensions.AsRandomAccessStream(ms);
            var bitmap = new BitmapImage();
            bitmap.SetSource(ras);
            _cache[image.Target] = bitmap;
            return bitmap;
        }
        catch
        {
            return null;
        }
    }

    public void Clear()
    {
        _cache.Clear();
    }
}
