using Markdown.Windows.Core;
using Markdown.Windows.Document;
using Microsoft.UI.Text;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Controls.Primitives;
using Microsoft.UI.Xaml.Documents;
using Microsoft.UI.Xaml.Markup;
using Windows.System;

namespace Markdown.Windows.Render;

public sealed record WorkspaceSearchRow(
    WorkspaceSearchNameHit? NameHit,
    WorkspaceSearchContentHit? ContentHit)
{
    public bool IsNameHit => NameHit is not null;
}

internal sealed partial class WorkspaceSearchRowItem : ListViewItem
{
    private readonly Grid _grid = new()
    {
        RowSpacing = 2,
    };
    private readonly Grid _headerGrid = new();
    private readonly TextBlock _title = new()
    {
        FontSize = 12.5,
        FontWeight = FontWeights.SemiBold,
        TextTrimming = TextTrimming.CharacterEllipsis,
        TextWrapping = TextWrapping.NoWrap,
        VerticalAlignment = VerticalAlignment.Center,
    };
    private readonly TextBlock _warning = new()
    {
        Text = "⚠",
        FontSize = 11,
        Margin = new Thickness(6, 0, 0, 0),
        VerticalAlignment = VerticalAlignment.Center,
        Visibility = Visibility.Collapsed,
    };
    private readonly TextBlock _detail = new()
    {
        FontSize = 12,
        Opacity = 0.85,
        TextTrimming = TextTrimming.CharacterEllipsis,
        TextWrapping = TextWrapping.NoWrap,
    };

    public WorkspaceSearchRow? BoundRow { get; private set; }

    public WorkspaceSearchRowItem()
    {
        HorizontalContentAlignment = HorizontalAlignment.Stretch;
        MinHeight = 36;
        Padding = new Thickness(8, 4, 8, 4);

        _headerGrid.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        _headerGrid.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        Grid.SetColumn(_title, 0);
        Grid.SetColumn(_warning, 1);
        _headerGrid.Children.Add(_title);
        _headerGrid.Children.Add(_warning);

        _grid.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        _grid.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        Grid.SetRow(_headerGrid, 0);
        Grid.SetRow(_detail, 1);
        _grid.Children.Add(_headerGrid);
        _grid.Children.Add(_detail);
    }

    public void Attach()
    {
        if (RowItem.TryAs<Grid>(ContentTemplateRoot) is not { } host || ReferenceEquals(_grid.Parent, host))
        {
            return;
        }

        if (_grid.Parent is Panel oldParent)
        {
            oldParent.Children.Remove(_grid);
        }

        host.Children.Clear();
        host.Children.Add(_grid);
    }

    public void Bind(WorkspaceSearchRow row)
    {
        BoundRow = row;
        _detail.Inlines.Clear();

        if (row.NameHit is { } nameHit)
        {
            _title.Text = nameHit.Name;
            _detail.Inlines.Add(new Run { Text = $"File · {nameHit.RelativePath}" });
            SetWarning(nameHit.HasInvisible);
            var warningNote = nameHit.HasInvisible ? ", warning invisible characters" : string.Empty;
            AutomationProperties.SetName(this, $"File {nameHit.RelativePath}{warningNote}");
            return;
        }

        if (row.ContentHit is { } contentHit)
        {
            _title.Text = $"{contentHit.Name} : line {contentHit.Line}";
            AppendHighlightedSnippet(_detail, contentHit);
            SetWarning(contentHit.HasInvisible);
            var warningNote = contentHit.HasInvisible ? ", warning invisible characters" : string.Empty;
            AutomationProperties.SetName(
                this,
                $"{contentHit.Name}, line {contentHit.Line}, {contentHit.Snippet}{warningNote}");
        }
    }

    private void SetWarning(bool hasInvisible)
    {
        _warning.Visibility = hasInvisible ? Visibility.Visible : Visibility.Collapsed;
        ToolTipService.SetToolTip(
            _warning,
            hasInvisible ? "Contains invisible Unicode characters" : null);
    }

    private static void AppendHighlightedSnippet(TextBlock target, WorkspaceSearchContentHit hit)
    {
        if (hit.LeadingMore)
        {
            target.Inlines.Add(new Run { Text = "…" });
        }

        var snippet = hit.Snippet;
        var start = Math.Clamp(hit.MatchStartUtf16, 0, snippet.Length);
        var len = Math.Clamp(hit.MatchLengthUtf16, 0, snippet.Length - start);

        if (start > 0)
        {
            target.Inlines.Add(new Run { Text = snippet[..start] });
        }
        if (len > 0)
        {
            target.Inlines.Add(new Run
            {
                Text = snippet.Substring(start, len),
                FontWeight = FontWeights.Bold,
            });
        }
        if (start + len < snippet.Length)
        {
            target.Inlines.Add(new Run { Text = snippet[(start + len)..] });
        }

        if (hit.TrailingMore)
        {
            target.Inlines.Add(new Run { Text = "…" });
        }
    }
}

public sealed partial class WorkspaceSearchPanel : UserControl
{
    private readonly Grid _root = new();
    private readonly TextBox _queryBox = new()
    {
        PlaceholderText = "Search in workspace…",
        FontSize = 13,
    };
    private readonly ToggleButton _matchCaseToggle = new()
    {
        Content = "Aa",
        FontSize = 11,
        Padding = new Thickness(6, 2, 6, 2),
        MinWidth = 0,
        MinHeight = 24,
    };
    private readonly ToggleButton _wholeWordToggle = new()
    {
        Content = "ab",
        FontSize = 11,
        Padding = new Thickness(6, 2, 6, 2),
        MinWidth = 0,
        MinHeight = 24,
    };
    private readonly ToggleButton _hiddenToggle = new()
    {
        Content = ".*",
        FontSize = 11,
        Padding = new Thickness(6, 2, 6, 2),
        MinWidth = 0,
        MinHeight = 24,
    };
    private readonly Button _stopButton = new()
    {
        Content = "Stop",
        FontSize = 11,
        Padding = new Thickness(8, 2, 8, 2),
        MinWidth = 0,
        MinHeight = 24,
        IsEnabled = false,
    };
    private readonly TextBlock _statusText = new()
    {
        Text = "Type to search workspace",
        FontSize = 11,
        Opacity = 0.75,
        Margin = new Thickness(8, 2, 8, 4),
        TextTrimming = TextTrimming.CharacterEllipsis,
        TextWrapping = TextWrapping.NoWrap,
    };
    private readonly ListView _resultsList = new()
    {
        SelectionMode = ListViewSelectionMode.Single,
        IsItemClickEnabled = true,
    };

    private List<WorkspaceSearchRow> _rows = [];
    private WorkspaceSearchSnapshot _snapshot = WorkspaceSearchSnapshot.Empty;
    private bool _suppressQueryEvents;

    public event Action<string, WorkspaceSearchOptions>? SearchRequested;
    public event Action? StopRequested;
    public event Action<WorkspaceSearchNameHit>? NameHitActivated;
    public event Action<WorkspaceSearchContentHit>? ContentHitActivated;

    public IReadOnlyList<WorkspaceSearchRow> Rows => _rows;
    public WorkspaceSearchSnapshot CurrentSnapshot => _snapshot;
    public ListView ResultsList => _resultsList;
    public string StatusMessage => _statusText.Text;
    public bool IsStopEnabled => _stopButton.IsEnabled;

    public string Query
    {
        get => _queryBox.Text ?? string.Empty;
        set
        {
            if (string.Equals(_queryBox.Text, value, StringComparison.Ordinal))
            {
                return;
            }
            _suppressQueryEvents = true;
            _queryBox.Text = value ?? string.Empty;
            _suppressQueryEvents = false;
        }
    }

    public bool MatchCase
    {
        get => _matchCaseToggle.IsChecked == true;
        set => _matchCaseToggle.IsChecked = value;
    }

    public bool WholeWord
    {
        get => _wholeWordToggle.IsChecked == true;
        set => _wholeWordToggle.IsChecked = value;
    }

    public bool IncludeHidden
    {
        get => _hiddenToggle.IsChecked == true;
        set => _hiddenToggle.IsChecked = value;
    }

    public WorkspaceSearchPanel()
    {
        AutomationProperties.SetName(_queryBox, "Search in workspace");
        AutomationProperties.SetName(_matchCaseToggle, "Match case");
        ToolTipService.SetToolTip(_matchCaseToggle, "Match case");
        AutomationProperties.SetName(_wholeWordToggle, "Whole words");
        ToolTipService.SetToolTip(_wholeWordToggle, "Match whole words");
        AutomationProperties.SetName(_hiddenToggle, "Include hidden files");
        ToolTipService.SetToolTip(_hiddenToggle, "Include hidden files");
        AutomationProperties.SetName(_stopButton, "Stop workspace search");
        ToolTipService.SetToolTip(_stopButton, "Stop workspace search");
        AutomationProperties.SetName(_resultsList, "Workspace search results");

        _queryBox.TextChanged += (_, _) =>
        {
            if (!_suppressQueryEvents)
            {
                RequestSearch();
            }
        };
        _queryBox.KeyDown += (_, args) =>
        {
            if (args.Key == VirtualKey.Enter)
            {
                args.Handled = true;
                RequestSearch();
            }
        };
        _matchCaseToggle.Checked += (_, _) => RequestSearch();
        _matchCaseToggle.Unchecked += (_, _) => RequestSearch();
        _wholeWordToggle.Checked += (_, _) => RequestSearch();
        _wholeWordToggle.Unchecked += (_, _) => RequestSearch();
        _hiddenToggle.Checked += (_, _) => RequestSearch();
        _hiddenToggle.Unchecked += (_, _) => RequestSearch();
        _stopButton.Click += (_, _) => StopRequested?.Invoke();

        _resultsList.ChoosingItemContainer += (_, args) => args.ItemContainer ??= new WorkspaceSearchRowItem();
        _resultsList.ItemTemplate = (DataTemplate)XamlReader.Load(
            "<DataTemplate xmlns='http://schemas.microsoft.com/winfx/2006/xaml/presentation'><Grid/></DataTemplate>");
        _resultsList.ContainerContentChanging += (_, args) =>
        {
            if (args.InRecycleQueue)
            {
                return;
            }
            if (args.ItemContainer is WorkspaceSearchRowItem rowItem &&
                args.ItemIndex >= 0 &&
                args.ItemIndex < _rows.Count)
            {
                rowItem.Attach();
                rowItem.Bind(_rows[args.ItemIndex]);
            }
        };
        _resultsList.ItemClick += (_, args) =>
        {
            if (args.ClickedItem is int idx && idx >= 0 && idx < _rows.Count)
            {
                ActivateRow(_rows[idx]);
            }
        };
        _resultsList.PreviewKeyDown += (_, args) =>
        {
            if (args.Key is VirtualKey.Enter or VirtualKey.Space)
            {
                var idx = _resultsList.SelectedIndex;
                if (idx >= 0 && idx < _rows.Count)
                {
                    args.Handled = true;
                    ActivateRow(_rows[idx]);
                }
            }
        };

        var controlsBar = new StackPanel
        {
            Orientation = Orientation.Horizontal,
            Spacing = 4,
            Margin = new Thickness(8, 4, 8, 2),
        };
        controlsBar.Children.Add(_matchCaseToggle);
        controlsBar.Children.Add(_wholeWordToggle);
        controlsBar.Children.Add(_hiddenToggle);
        controlsBar.Children.Add(_stopButton);

        var queryContainer = new Border
        {
            Margin = new Thickness(8, 4, 8, 0),
            Child = _queryBox,
        };

        _root.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        _root.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        _root.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        _root.RowDefinitions.Add(new RowDefinition { Height = new GridLength(1, GridUnitType.Star) });

        Grid.SetRow(queryContainer, 0);
        Grid.SetRow(controlsBar, 1);
        Grid.SetRow(_statusText, 2);
        Grid.SetRow(_resultsList, 3);

        _root.Children.Add(queryContainer);
        _root.Children.Add(controlsBar);
        _root.Children.Add(_statusText);
        _root.Children.Add(_resultsList);

        Content = _root;
    }

    public WorkspaceSearchOptions BuildOptions() =>
        new(
            CaseSensitive: MatchCase,
            WholeWord: WholeWord,
            IncludeHidden: IncludeHidden);

    public void RequestSearch()
    {
        SearchRequested?.Invoke(Query, BuildOptions());
    }

    public void FocusQuery()
    {
        _queryBox.Focus(FocusState.Programmatic);
        _queryBox.SelectAll();
    }

    public void Clear()
    {
        Query = string.Empty;
        ApplySnapshot(WorkspaceSearchSnapshot.Empty);
    }

    public void ApplySnapshot(WorkspaceSearchSnapshot snapshot)
    {
        _snapshot = snapshot;
        _stopButton.IsEnabled = snapshot.IsRunning;

        var rows = new List<WorkspaceSearchRow>(snapshot.NameHits.Count + snapshot.ContentHits.Count);
        foreach (var hit in snapshot.NameHits)
        {
            rows.Add(new WorkspaceSearchRow(hit, null));
        }
        foreach (var hit in snapshot.ContentHits)
        {
            rows.Add(new WorkspaceSearchRow(null, hit));
        }

        _rows = rows;
        _resultsList.ItemsSource = new IndexList(rows.Count);
        _statusText.Text = FormatStatus(snapshot);
    }

    private void ActivateRow(WorkspaceSearchRow row)
    {
        if (row.NameHit is { } nameHit)
        {
            NameHitActivated?.Invoke(nameHit);
        }
        else if (row.ContentHit is { } contentHit)
        {
            ContentHitActivated?.Invoke(contentHit);
        }
    }

    internal static string FormatStatus(WorkspaceSearchSnapshot snapshot)
    {
        if (!string.IsNullOrWhiteSpace(snapshot.ErrorMessage))
        {
            return snapshot.ErrorMessage;
        }
        if (string.IsNullOrWhiteSpace(snapshot.Query))
        {
            return "Type to search workspace";
        }
        if (snapshot.IsRunning)
        {
            return $"Searching… {snapshot.FilesRead} files, {snapshot.TotalMatches} matches";
        }
        if (snapshot.IsCancelled)
        {
            return $"Stopped ({snapshot.NameHits.Count} files, {snapshot.TotalMatches} matches)";
        }
        if (snapshot.NameHits.Count == 0 && snapshot.ContentHits.Count == 0)
        {
            return snapshot.IsIncomplete
                ? "No matches (search incomplete)"
                : "No matches";
        }

        var parts = new List<string>(3);
        if (snapshot.NameHits.Count > 0)
        {
            parts.Add($"{snapshot.NameHits.Count} file{(snapshot.NameHits.Count == 1 ? "" : "s")}");
        }
        if (snapshot.TotalMatches > 0)
        {
            parts.Add($"{snapshot.TotalMatches} match{(snapshot.TotalMatches == 1 ? "" : "es")} in {snapshot.FilesMatched} file{(snapshot.FilesMatched == 1 ? "" : "s")}");
        }
        var summary = string.Join(" · ", parts);
        if (snapshot.StopReason != 0)
        {
            var reason = snapshot.StopReason switch
            {
                CoreFfi.MD_STOP_FILES => "file limit",
                CoreFfi.MD_STOP_BYTES => "byte limit",
                CoreFfi.MD_STOP_MATCHES => "match limit",
                _ => "limit",
            };
            return $"{summary} (stopped at {reason})";
        }
        if (snapshot.IsIncomplete)
        {
            return $"{summary} (incomplete)";
        }
        return summary;
    }
}
