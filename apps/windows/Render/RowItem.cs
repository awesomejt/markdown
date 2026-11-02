using Microsoft.UI;
using Microsoft.UI.Text;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Automation.Peers;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Documents;
using Microsoft.UI.Xaml.Media;

namespace Markdown.Windows.Render;

/// <summary>
/// A recycled row container in the virtualized document ListView.
/// Visual elements are created once and reused across item indices.
/// </summary>
public sealed partial class RowItem : ListViewItem
{
    private static readonly FontFamily s_monoFont = new("Cascadia Mono, Consolas, Courier New");
    private static readonly SolidColorBrush s_quoteBrush = new(Colors.Gray);
    private static readonly SolidColorBrush s_codeBackground = new(global::Windows.UI.Color.FromArgb(0x18, 0x80, 0x80, 0x80));
    private static readonly double[] s_headingSizes = [28, 24, 20, 18, 16, 15];

    public Grid Root { get; } = new();
    public RichTextBlock TextBlock { get; } = new()
    {
        TextWrapping = TextWrapping.Wrap,
        IsTextSelectionEnabled = true,
    };

    public int ItemIndex { get; private set; } = -1;
    public string ShownText { get; private set; } = string.Empty;

    public RowItem()
    {
        MinHeight = 0;
        Padding = new Thickness(0);
        Margin = new Thickness(0);
        CornerRadius = new CornerRadius(0);
        HorizontalContentAlignment = HorizontalAlignment.Stretch;

        Root.Children.Add(TextBlock);
    }

    public static T? TryAs<T>(object? value) where T : class
    {
        if (value is null) return null;
        if (value is T typed) return typed;
        try { return WinRT.CastExtensions.As<T>(value); }
        catch (InvalidCastException) { return null; }
    }

    /// <summary>
    /// Attaches the row visuals to the container's template root if needed.
    /// </summary>
    public void Attach()
    {
        if (TryAs<Grid>(ContentTemplateRoot) is not { } host || ReferenceEquals(Root.Parent, host))
        {
            return;
        }

        if (Root.Parent is Panel oldParent)
        {
            oldParent.Children.Remove(Root);
        }

        host.Children.Clear();
        host.Children.Add(Root);
    }

    /// <summary>
    /// Resets row state when recycled or unbound.
    /// </summary>
    public void Unbind()
    {
        ItemIndex = -1;
        ShownText = string.Empty;
        TextBlock.Blocks.Clear();
        AutomationProperties.SetName(this, string.Empty);
        AutomationProperties.SetHeadingLevel(this, AutomationHeadingLevel.None);
        AutomationProperties.SetHeadingLevel(TextBlock, AutomationHeadingLevel.None);
    }

    /// <summary>
    /// Binds the container to the given fragment from the current render plan.
    /// </summary>
    public void Bind(int index, RenderFragment fragment, Action<int, int>? onLinkActivated)
    {
        ItemIndex = index;
        TextBlock.Blocks.Clear();

        var paragraph = new Paragraph();
        var shownBuilder = new System.Text.StringBuilder();

        foreach (var run in fragment.Runs)
        {
            var text = run.Text;
            var inlineRun = new Run { Text = text };

            if ((run.Style & RunStyle.Strong) != 0)
            {
                inlineRun.FontWeight = FontWeights.Bold;
            }
            if ((run.Style & RunStyle.Emphasis) != 0)
            {
                inlineRun.FontStyle = global::Windows.UI.Text.FontStyle.Italic;
            }
            if ((run.Style & RunStyle.Strikethrough) != 0)
            {
                inlineRun.TextDecorations = global::Windows.UI.Text.TextDecorations.Strikethrough;
            }
            if ((run.Style & RunStyle.Code) != 0)
            {
                inlineRun.FontFamily = s_monoFont;
            }
            if ((run.Style & RunStyle.Marker) != 0)
            {
                inlineRun.Foreground = s_quoteBrush;
            }

            if (run.Link >= 0 && onLinkActivated is not null)
            {
                var capturedLink = run.Link;
                var hyperlink = new Hyperlink();
                hyperlink.Inlines.Add(inlineRun);
                hyperlink.Click += (_, _) => onLinkActivated(index, capturedLink);
                paragraph.Inlines.Add(hyperlink);
            }
            else
            {
                paragraph.Inlines.Add(inlineRun);
            }

            shownBuilder.Append(text);
        }

        ShownText = shownBuilder.ToString();
        TextBlock.Blocks.Add(paragraph);

        var isHeading = fragment.Kind == FragmentKind.Heading;
        var headingLevel = fragment.HeadingLevel;
        if (isHeading && headingLevel >= 1 && headingLevel <= 6)
        {
            TextBlock.FontSize = s_headingSizes[headingLevel - 1];
            TextBlock.FontWeight = FontWeights.SemiBold;
        }
        else
        {
            TextBlock.FontSize = 15;
            TextBlock.FontWeight = FontWeights.Normal;
        }

        var isMono = fragment.Kind is FragmentKind.Code or FragmentKind.Fallback;
        TextBlock.FontFamily = isMono ? s_monoFont : FontFamily.XamlAutoFontFamily;
        Root.Background = isMono ? s_codeBackground : null;

        var topMargin = isHeading ? 14 : 6;
        var bottomMargin = 6;
        Root.Padding = new Thickness(16 + 24 * fragment.Indent, topMargin, 16, bottomMargin);
        Root.BorderThickness = new Thickness(fragment.Quote > 0 ? 3 : 0, 0, 0, 0);
        Root.BorderBrush = s_quoteBrush;

        AutomationProperties.SetName(this, ShownText);
        var autoHeadingLevel = fragment.HeadingStart && headingLevel is >= 1 and <= 6
            ? (AutomationHeadingLevel)headingLevel
            : AutomationHeadingLevel.None;
        AutomationProperties.SetHeadingLevel(this, autoHeadingLevel);
        AutomationProperties.SetHeadingLevel(TextBlock, autoHeadingLevel);
    }
}
