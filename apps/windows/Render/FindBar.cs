using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Input;
using Windows.System;

namespace Markdown.Windows.Render;

public sealed partial class FindBar : UserControl
{
    private readonly TextBox _queryBox = new()
    {
        PlaceholderText = "Find in Document",
        Width = 240,
        Margin = new Thickness(0, 0, 8, 0),
    };
    private readonly Button _previousButton = new()
    {
        Content = "◀",
        Width = 32,
        Margin = new Thickness(0, 0, 4, 0),
    };
    private readonly Button _nextButton = new()
    {
        Content = "▶",
        Width = 32,
        Margin = new Thickness(0, 0, 8, 0),
    };
    private readonly CheckBox _matchCaseBox = new()
    {
        Content = "Match Case",
        Margin = new Thickness(0, 0, 8, 0),
    };
    private readonly CheckBox _wholeWordBox = new()
    {
        Content = "Whole Word",
        Margin = new Thickness(0, 0, 12, 0),
    };
    private readonly TextBlock _statusText = new()
    {
        Text = string.Empty,
        VerticalAlignment = VerticalAlignment.Center,
        Margin = new Thickness(0, 0, 12, 0),
        MinWidth = 80,
    };
    private readonly Button _closeButton = new()
    {
        Content = "✕",
        Width = 32,
    };

    public TextBox QueryBox => _queryBox;
    public Button PreviousButton => _previousButton;
    public Button NextButton => _nextButton;
    public CheckBox MatchCaseBox => _matchCaseBox;
    public CheckBox WholeWordBox => _wholeWordBox;
    public TextBlock StatusText => _statusText;
    public Button CloseButton => _closeButton;

    public string Query
    {
        get => _queryBox.Text;
        set => _queryBox.Text = value;
    }

    public bool MatchCase
    {
        get => _matchCaseBox.IsChecked == true;
        set => _matchCaseBox.IsChecked = value;
    }

    public bool WholeWord
    {
        get => _wholeWordBox.IsChecked == true;
        set => _wholeWordBox.IsChecked = value;
    }

    public event Action<string, bool, bool>? QueryChanged;
    public event Action? FindNext;
    public event Action? FindPrevious;
    public event Action? CloseRequested;

    public FindBar()
    {
        AutomationProperties.SetName(this, "Find");
        AutomationProperties.SetName(_queryBox, "Find in document");
        AutomationProperties.SetName(_previousButton, "Find previous");
        AutomationProperties.SetName(_nextButton, "Find next");
        AutomationProperties.SetName(_matchCaseBox, "Match case");
        AutomationProperties.SetName(_wholeWordBox, "Whole word");
        AutomationProperties.SetName(_statusText, "Find status");
        AutomationProperties.SetName(_closeButton, "Close find");

        ToolTipService.SetToolTip(_previousButton, "Find Previous (Shift+F3)");
        ToolTipService.SetToolTip(_nextButton, "Find Next (F3 / Enter)");
        ToolTipService.SetToolTip(_closeButton, "Close Find (Escape)");

        _queryBox.TextChanged += (_, _) => OnQueryParametersChanged();
        _queryBox.KeyDown += OnQueryBoxKeyDown;

        _matchCaseBox.Click += (_, _) => OnQueryParametersChanged();
        _wholeWordBox.Click += (_, _) => OnQueryParametersChanged();

        _previousButton.Click += (_, _) => FindPrevious?.Invoke();
        _nextButton.Click += (_, _) => FindNext?.Invoke();
        _closeButton.Click += (_, _) => CloseRequested?.Invoke();

        var panel = new StackPanel
        {
            Orientation = Orientation.Horizontal,
            Padding = new Thickness(16, 6, 16, 6),
            BorderThickness = new Thickness(0, 0, 0, 1),
            BorderBrush = new Microsoft.UI.Xaml.Media.SolidColorBrush(Microsoft.UI.Colors.Gray),
        };

        panel.Children.Add(_queryBox);
        panel.Children.Add(_previousButton);
        panel.Children.Add(_nextButton);
        panel.Children.Add(_matchCaseBox);
        panel.Children.Add(_wholeWordBox);
        panel.Children.Add(_statusText);
        panel.Children.Add(_closeButton);

        Content = panel;
    }

    public void FocusQuery()
    {
        _queryBox.Focus(FocusState.Programmatic);
        _queryBox.SelectAll();
    }

    public void SetStatus(string status)
    {
        _statusText.Text = status;
    }

    private void OnQueryParametersChanged()
    {
        QueryChanged?.Invoke(Query, MatchCase, WholeWord);
    }

    private void OnQueryBoxKeyDown(object sender, KeyRoutedEventArgs e)
    {
        if (e.Key == VirtualKey.Enter)
        {
            e.Handled = true;
            var isShiftDown = Microsoft.UI.Input.InputKeyboardSource.GetKeyStateForCurrentThread(VirtualKey.Shift)
                .HasFlag(global::Windows.UI.Core.CoreVirtualKeyStates.Down);
            if (isShiftDown)
            {
                FindPrevious?.Invoke();
            }
            else
            {
                FindNext?.Invoke();
            }
        }
        else if (e.Key == VirtualKey.Escape)
        {
            e.Handled = true;
            CloseRequested?.Invoke();
        }
    }
}
