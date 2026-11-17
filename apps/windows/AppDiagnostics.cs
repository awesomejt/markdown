using Markdown.Windows.Core;

namespace Markdown.Windows;

internal static class AppDiagnostics
{
    public static string Text
    {
        get
        {
            CoreRuntime.Validate();
            var version = typeof(AppDiagnostics).Assembly.GetName().Version;
            return $"Markdown {version?.ToString(3)}\nSemantic model v2 · ABI v1 · anchor v1\n" +
                $"Core {CorePackageIdentity.Configuration}\nSHA-256 {CorePackageIdentity.Fingerprint}";
        }
    }
}
