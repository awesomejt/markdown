using Markdown.Windows.Render;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;

namespace Markdown.Windows;

public sealed partial class SettingsWindow : Window
{
    private readonly ReaderPreferences _preferences;
    private readonly ComboBox _themeCombo = new()
    {
        Width = 160,
    };
    private readonly Slider _textSizeSlider = new()
    {
        Minimum = 0,
        Maximum = TextScale.Steps.Length - 1,
        StepFrequency = 1,
        SnapsTo = Microsoft.UI.Xaml.Controls.Primitives.SliderSnapsTo.StepValues,
        TickFrequency = 1,
        TickPlacement = Microsoft.UI.Xaml.Controls.Primitives.TickPlacement.BottomRight,
        Width = 200,
    };
    private readonly TextBlock _textSizeLabel = new()
    {
        Width = 56,
        VerticalAlignment = VerticalAlignment.Center,
        FontFamily = new Microsoft.UI.Xaml.Media.FontFamily("Cascadia Mono, Consolas, Courier New"),
    };

    public ComboBox ThemeCombo => _themeCombo;
    public Slider TextSizeSlider => _textSizeSlider;
    public TextBlock TextSizeLabel => _textSizeLabel;

    public SettingsWindow(ReaderPreferences preferences)
    {
        _preferences = preferences;
        Title = "Settings";
        AppWindow.Resize(new global::Windows.Graphics.SizeInt32(440, 240));

        AutomationProperties.SetName(_themeCombo, "Appearance");
        AutomationProperties.SetName(_textSizeSlider, "Text size");
        AutomationProperties.SetName(_textSizeLabel, "Text size value");

        _themeCombo.Items.Add("System");
        _themeCombo.Items.Add("Light");
        _themeCombo.Items.Add("Dark");

        _themeCombo.SelectionChanged += OnThemeSelectionChanged;
        _textSizeSlider.ValueChanged += OnTextSizeValueChanged;

        var panel = new StackPanel
        {
            Padding = new Thickness(24),
            Spacing = 16,
        };

        var themeRow = new StackPanel
        {
            Orientation = Orientation.Horizontal,
            Spacing = 16,
        };
        themeRow.Children.Add(new TextBlock
        {
            Text = "Appearance:",
            Width = 90,
            VerticalAlignment = VerticalAlignment.Center,
        });
        themeRow.Children.Add(_themeCombo);

        var sizeRow = new StackPanel
        {
            Orientation = Orientation.Horizontal,
            Spacing = 16,
        };
        sizeRow.Children.Add(new TextBlock
        {
            Text = "Text size:",
            Width = 90,
            VerticalAlignment = VerticalAlignment.Center,
        });
        sizeRow.Children.Add(_textSizeSlider);
        sizeRow.Children.Add(_textSizeLabel);

        panel.Children.Add(themeRow);
        panel.Children.Add(sizeRow);

        Content = panel;
        Refresh();
    }

    public void Refresh()
    {
        _themeCombo.SelectedIndex = (int)_preferences.Theme;
        var scale = _preferences.TextScaleValue;
        var stepIndex = 0;
        for (var i = 0; i < TextScale.Steps.Length; i++)
        {
            if (Math.Abs(TextScale.Steps[i] - scale) < 0.001)
            {
                stepIndex = i;
                break;
            }
        }
        _textSizeSlider.Value = stepIndex;
        var percent = $"{Math.Round(scale * 100)}%";
        _textSizeLabel.Text = percent;
        AutomationProperties.SetName(_textSizeSlider, $"Text size {percent}");
    }

    private void OnThemeSelectionChanged(object sender, SelectionChangedEventArgs e)
    {
        if (_themeCombo.SelectedIndex >= 0 && _themeCombo.SelectedIndex <= 2)
        {
            _preferences.Theme = (AppTheme)_themeCombo.SelectedIndex;
        }
    }

    private void OnTextSizeValueChanged(object sender, Microsoft.UI.Xaml.Controls.Primitives.RangeBaseValueChangedEventArgs e)
    {
        var stepIndex = (int)Math.Round(_textSizeSlider.Value);
        if (stepIndex >= 0 && stepIndex < TextScale.Steps.Length)
        {
            _preferences.TextScaleValue = TextScale.Steps[stepIndex];
            Refresh();
        }
    }
}
