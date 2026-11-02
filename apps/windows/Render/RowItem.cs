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
    private static readonly SolidColorBrush s_checkedBrush = new(global::Windows.UI.Color.FromArgb(0xFF, 0x00, 0x78, 0xD4));
    private static readonly SolidColorBrush s_codeBackground = new(global::Windows.UI.Color.FromArgb(0x18, 0x80, 0x80, 0x80));
    private static readonly SolidColorBrush s_altRowBackground = new(global::Windows.UI.Color.FromArgb(0x0C, 0x80, 0x80, 0x80));
    private static readonly double[] s_headingSizes = [28, 24, 20, 18, 16, 15];

    private static readonly SolidColorBrush s_findHighlightBrush = new(global::Windows.UI.Color.FromArgb(0x80, 0xFF, 0xEB, 0x3B));
    private static readonly SolidColorBrush s_currentFindHighlightBrush = new(global::Windows.UI.Color.FromArgb(0xC0, 0xFF, 0x98, 0x00));

    public Grid Root { get; } = new();
    public RichTextBlock TextBlock { get; } = new()
    {
        TextWrapping = TextWrapping.Wrap,
        IsTextSelectionEnabled = true,
    };
    public StackPanel ImagesPanel { get; } = new()
    {
        Spacing = 8,
        Visibility = Visibility.Collapsed,
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
        Root.Children.Add(ImagesPanel);
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
    public void SetHighlights(IReadOnlyList<(int Start, int Length, bool IsCurrent)>? highlights)
    {
        TextBlock.TextHighlighters.Clear();
        if (highlights is null || highlights.Count == 0) return;
        var normal = new TextHighlighter { Background = s_findHighlightBrush };
        var current = new TextHighlighter { Background = s_currentFindHighlightBrush };
        foreach (var (start, length, isCurrent) in highlights)
        {
            var range = new TextRange { StartIndex = start, Length = length };
            if (isCurrent) current.Ranges.Add(range);
            else normal.Ranges.Add(range);
        }
        if (normal.Ranges.Count > 0) TextBlock.TextHighlighters.Add(normal);
        if (current.Ranges.Count > 0) TextBlock.TextHighlighters.Add(current);
    }

    public void Unbind()
    {
        ItemIndex = -1;
        ShownText = string.Empty;
        TextBlock.Blocks.Clear();
        TextBlock.TextHighlighters.Clear();
        TextBlock.Visibility = Visibility.Visible;
        ImagesPanel.Children.Clear();
        ImagesPanel.Visibility = Visibility.Collapsed;
        AutomationProperties.SetName(this, string.Empty);
        AutomationProperties.SetHeadingLevel(this, AutomationHeadingLevel.None);
        AutomationProperties.SetHeadingLevel(TextBlock, AutomationHeadingLevel.None);
    }

    /// <summary>
    /// Binds the container to the given fragment from the current render plan.
    /// </summary>
    public void Bind(int index, RenderFragment fragment, Action<int, int>? onLinkActivated, ImageSourceCache? imageCache = null, double textScale = 1.0)
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
            if ((run.Style & RunStyle.TaskChecked) != 0)
            {
                inlineRun.Foreground = s_checkedBrush;
            }
            else if ((run.Style & RunStyle.TaskUnchecked) != 0)
            {
                inlineRun.Foreground = s_quoteBrush;
            }
            else if ((run.Style & RunStyle.Marker) != 0)
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

        var scale = TextScale.Clamped(textScale);
        var isHeading = fragment.Kind == FragmentKind.Heading;
        var headingLevel = fragment.HeadingLevel;
        if (isHeading && headingLevel >= 1 && headingLevel <= 6)
        {
            TextBlock.FontSize = s_headingSizes[headingLevel - 1] * scale;
            TextBlock.FontWeight = FontWeights.SemiBold;
        }
        else
        {
            TextBlock.FontSize = 15 * scale;
            TextBlock.FontWeight = FontWeights.Normal;
        }

        var isMono = fragment.Kind is FragmentKind.Code or FragmentKind.Fallback or FragmentKind.TableRow;
        TextBlock.FontFamily = isMono ? s_monoFont : FontFamily.XamlAutoFontFamily;

        if (fragment.Kind == FragmentKind.TableRow)
        {
            TextBlock.FontSize = 14 * scale;
            if (fragment.IsHeader)
            {
                Root.Background = s_codeBackground;
                Root.BorderThickness = new Thickness(0, 0, 0, 2);
                Root.BorderBrush = s_quoteBrush;
            }
            else
            {
                Root.Background = (index % 2 == 1) ? s_altRowBackground : null;
                Root.BorderThickness = new Thickness(0, 0, 0, 1);
                Root.BorderBrush = s_quoteBrush;
            }
            Root.Padding = new Thickness(16 + 24 * fragment.Indent, 4, 16, 4);
        }
        else
        {
            Root.Background = (fragment.Kind is FragmentKind.Code or FragmentKind.Fallback) ? s_codeBackground : null;
            var topMargin = isHeading ? 14 : 6;
            var bottomMargin = 6;
            Root.Padding = new Thickness(16 + 24 * fragment.Indent, topMargin, 16, bottomMargin);
            Root.BorderThickness = new Thickness(fragment.Quote > 0 ? 3 : 0, 0, 0, 0);
            Root.BorderBrush = s_quoteBrush;
        }

        // Text visibility remains visible if there are text runs (never collapsed by images)
        TextBlock.Visibility = fragment.Runs.Count > 0 ? Visibility.Visible : Visibility.Collapsed;

        // Image presentation: renders all images in the fragment without dropping secondary images
        ImagesPanel.Children.Clear();
        if (fragment.Images.Count > 0)
        {
            ImagesPanel.Visibility = Visibility.Visible;
            foreach (var img in fragment.Images)
            {
                if (img.Failure is not null || img.Data is null)
                {
                    var status = new TextBlock
                    {
                        Text = string.IsNullOrEmpty(img.Alt)
                            ? $"[{img.FailureMessage}]"
                            : $"[{img.Alt} ({img.FailureMessage})]",
                        Foreground = s_quoteBrush,
                        FontStyle = global::Windows.UI.Text.FontStyle.Italic,
                        FontSize = 14,
                        TextWrapping = TextWrapping.Wrap,
                    };
                    AutomationProperties.SetName(status, status.Text);
                    ImagesPanel.Children.Add(status);
                }
                else
                {
                    var imgControl = new Image
                    {
                        Stretch = Stretch.Uniform,
                        HorizontalAlignment = HorizontalAlignment.Left,
                        MaxWidth = Math.Min(img.Width, 800),
                        MaxHeight = Math.Min(img.Height, 600),
                        Source = imageCache?.GetOrCreate(img),
                    };
                    AutomationProperties.SetName(imgControl, string.IsNullOrEmpty(img.Alt) ? "Image" : img.Alt);
                    ImagesPanel.Children.Add(imgControl);
                }
            }
        }
        else
        {
            ImagesPanel.Visibility = Visibility.Collapsed;
        }

        ShownText = fragment.Text;

        AutomationProperties.SetName(this, ShownText);
        var autoHeadingLevel = fragment.HeadingStart && headingLevel is >= 1 and <= 6
            ? (AutomationHeadingLevel)headingLevel
            : AutomationHeadingLevel.None;
        AutomationProperties.SetHeadingLevel(this, autoHeadingLevel);
        AutomationProperties.SetHeadingLevel(TextBlock, autoHeadingLevel);
    }
}
