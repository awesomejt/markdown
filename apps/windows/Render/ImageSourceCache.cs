using System.IO;
using Microsoft.UI.Xaml.Media.Imaging;

namespace Markdown.Windows.Render;

internal static class ImageSourceCache
{
    private static readonly Dictionary<string, BitmapImage> s_cache = new(StringComparer.Ordinal);

    public static BitmapImage? GetOrCreate(RenderedImage image)
    {
        if (image.Data is null || image.Data.Length == 0) return null;
        if (s_cache.TryGetValue(image.Target, out var cached))
        {
            return cached;
        }

        try
        {
            using var ms = new MemoryStream(image.Data);
            var ras = System.IO.WindowsRuntimeStreamExtensions.AsRandomAccessStream(ms);
            var bitmap = new BitmapImage();
            bitmap.SetSource(ras);
            s_cache[image.Target] = bitmap;
            return bitmap;
        }
        catch
        {
            return null;
        }
    }

    public static void Clear()
    {
        s_cache.Clear();
    }
}
