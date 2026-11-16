using Markdown.Windows.Render;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;

namespace Markdown.Windows;

public sealed partial class SettingsWindow : Window
{
    private readonly ReaderPreferences _preferences;
    private bool _isRefreshing;

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
    private readonly CheckBox _showOutlineCheckBox = new()
    {
        Content = "Show document outline in new windows",
    };
    private readonly CheckBox _restoreSessionCheckBox = new()
    {
        Content = "Restore previous session on launch",
    };
    private readonly Button _resetButton = new()
    {
        Content = "Reset to Defaults",
    };

    public ComboBox ThemeCombo => _themeCombo;
    public Slider TextSizeSlider => _textSizeSlider;
    public TextBlock TextSizeLabel => _textSizeLabel;
    public CheckBox ShowOutlineCheckBox => _showOutlineCheckBox;
    public CheckBox RestoreSessionCheckBox => _restoreSessionCheckBox;
    public Button ResetDefaultsButton => _resetButton;

    public SettingsWindow(ReaderPreferences preferences)
    {
        _preferences = preferences;
        Title = "Settings";
        AppWindow.Resize(new global::Windows.Graphics.SizeInt32(460, 340));

        AutomationProperties.SetName(_themeCombo, "Appearance");
        AutomationProperties.SetName(_textSizeSlider, "Text size");
        AutomationProperties.SetName(_textSizeLabel, "Text size value");
        AutomationProperties.SetName(_showOutlineCheckBox, "Show document outline in new windows");
        AutomationProperties.SetName(_restoreSessionCheckBox, "Restore previous session on launch");
        AutomationProperties.SetName(_resetButton, "Reset to Defaults");

        _themeCombo.Items.Add("System");
        _themeCombo.Items.Add("Light");
        _themeCombo.Items.Add("Dark");

        _themeCombo.SelectionChanged += OnThemeSelectionChanged;
        _textSizeSlider.ValueChanged += OnTextSizeValueChanged;
        _showOutlineCheckBox.Checked += OnShowOutlineChanged;
        _showOutlineCheckBox.Unchecked += OnShowOutlineChanged;
        _restoreSessionCheckBox.Checked += OnRestoreSessionChanged;
        _restoreSessionCheckBox.Unchecked += OnRestoreSessionChanged;
        _resetButton.Click += OnResetDefaultsClicked;

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
        panel.Children.Add(_showOutlineCheckBox);
        panel.Children.Add(_restoreSessionCheckBox);
        panel.Children.Add(_resetButton);

        Content = panel;
        Refresh();
    }

    public void Refresh()
    {
        _isRefreshing = true;
        try
        {
            if (Content is FrameworkElement fe)
            {
                fe.RequestedTheme = _preferences.ToElementTheme();
            }
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
            _showOutlineCheckBox.IsChecked = _preferences.ShowOutlineByDefault;
            _restoreSessionCheckBox.IsChecked = _preferences.RestoreSessionOnLaunch;
        }
        finally
        {
            _isRefreshing = false;
        }
    }

    private void OnThemeSelectionChanged(object sender, SelectionChangedEventArgs e)
    {
        if (_isRefreshing) return;
        if (_themeCombo.SelectedIndex >= 0 && _themeCombo.SelectedIndex <= 2)
        {
            _preferences.Theme = (AppTheme)_themeCombo.SelectedIndex;
        }
    }

    private void OnTextSizeValueChanged(object sender, Microsoft.UI.Xaml.Controls.Primitives.RangeBaseValueChangedEventArgs e)
    {
        if (_isRefreshing) return;
        var stepIndex = (int)Math.Round(_textSizeSlider.Value);
        if (stepIndex >= 0 && stepIndex < TextScale.Steps.Length)
        {
            _preferences.TextScaleValue = TextScale.Steps[stepIndex];
            Refresh();
        }
    }

    private void OnShowOutlineChanged(object sender, RoutedEventArgs e)
    {
        if (_isRefreshing) return;
        _preferences.ShowOutlineByDefault = _showOutlineCheckBox.IsChecked == true;
    }

    private void OnRestoreSessionChanged(object sender, RoutedEventArgs e)
    {
        if (_isRefreshing) return;
        _preferences.RestoreSessionOnLaunch = _restoreSessionCheckBox.IsChecked == true;
    }

    private void OnResetDefaultsClicked(object sender, RoutedEventArgs e)
    {
        if (_isRefreshing) return;
        _preferences.ResetToDefaults();
    }
}
