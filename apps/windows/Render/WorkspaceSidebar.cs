using Markdown.Windows.Document;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Controls.Primitives;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Markup;
using Windows.System;

namespace Markdown.Windows.Render;

internal sealed partial class WorkspaceRowItem : ListViewItem
{
    private readonly Grid _grid = new();
    private readonly TextBlock _chevron = new()
    {
        Width = 16,
        VerticalAlignment = VerticalAlignment.Center,
        FontSize = 12,
        Margin = new Thickness(0, 0, 4, 0),
    };
    private readonly TextBlock _name = new()
    {
        VerticalAlignment = VerticalAlignment.Center,
        TextWrapping = TextWrapping.NoWrap,
        TextTrimming = TextTrimming.CharacterEllipsis,
        FontSize = 13,
    };
    private readonly TextBlock _warning = new()
    {
        Text = "⚠",
        VerticalAlignment = VerticalAlignment.Center,
        FontSize = 12,
        Margin = new Thickness(6, 0, 0, 0),
        Visibility = Visibility.Collapsed,
    };

    public WorkspaceVisibleRow? BoundRow { get; private set; }

    public WorkspaceRowItem()
    {
        HorizontalContentAlignment = HorizontalAlignment.Stretch;
        MinHeight = 28;
        Padding = new Thickness(8, 3, 8, 3);

        _grid.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        _grid.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        _grid.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });

        Grid.SetColumn(_chevron, 0);
        Grid.SetColumn(_name, 1);
        Grid.SetColumn(_warning, 2);

        _grid.Children.Add(_chevron);
        _grid.Children.Add(_name);
        _grid.Children.Add(_warning);
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

    public void Bind(WorkspaceVisibleRow row)
    {
        BoundRow = row;
        var entry = row.Entry;

        _grid.Margin = new Thickness(entry.Depth * 14, 0, 0, 0);
        _chevron.Text = entry.IsDirectory ? (row.IsExpanded ? "▾" : "▸") : string.Empty;
        _name.Text = entry.Name;
        _warning.Visibility = entry.HasInvisible ? Visibility.Visible : Visibility.Collapsed;
        if (entry.HasInvisible)
        {
            ToolTipService.SetToolTip(_warning, "Contains invisible Unicode characters");
        }
        else
        {
            ToolTipService.SetToolTip(_warning, null);
        }

        AutomationProperties.SetLevel(this, entry.Depth + 1);
        AutomationProperties.SetPositionInSet(this, row.PositionInSet);
        AutomationProperties.SetSizeOfSet(this, row.SizeOfSet);
        AutomationProperties.SetName(this, BuildAccessibleName(row));
    }

    internal static string BuildAccessibleName(WorkspaceVisibleRow row)
    {
        var entry = row.Entry;
        var kind = entry.IsDirectory
            ? (row.IsExpanded ? "folder, expanded" : "folder, collapsed")
            : (entry.IsMarkdown ? "Markdown document" : "file");
        var warning = entry.HasInvisible ? ", warning invisible characters" : string.Empty;
        return $"{entry.Name}, {kind}{warning}, level {entry.Depth + 1}";
    }
}

public sealed partial class WorkspaceSidebar : UserControl
{
    private readonly Grid _root = new();
    private readonly TextBlock _headerTitle = new()
    {
        Text = "Workspace",
        FontSize = 13,
        FontWeight = Microsoft.UI.Text.FontWeights.SemiBold,
        TextTrimming = TextTrimming.CharacterEllipsis,
        TextWrapping = TextWrapping.NoWrap,
        VerticalAlignment = VerticalAlignment.Center,
    };
    private readonly ToggleButton _allToggle = new()
    {
        Content = "All",
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
    private readonly Button _refreshButton = new()
    {
        Content = "↻",
        FontSize = 12,
        Padding = new Thickness(6, 2, 6, 2),
        MinWidth = 0,
        MinHeight = 24,
    };
    private readonly Button _closeButton = new()
    {
        Content = "×",
        FontSize = 12,
        Padding = new Thickness(6, 2, 6, 2),
        MinWidth = 0,
        MinHeight = 24,
    };
    private readonly TextBlock _statusText = new()
    {
        FontSize = 11,
        Opacity = 0.75,
        Margin = new Thickness(8, 2, 8, 4),
        TextTrimming = TextTrimming.CharacterEllipsis,
        TextWrapping = TextWrapping.NoWrap,
        Visibility = Visibility.Collapsed,
    };
    private readonly ListView _treeList = new()
    {
        SelectionMode = ListViewSelectionMode.Single,
        IsItemClickEnabled = true,
    };

    private readonly HashSet<ulong> _expandedDirectoryIds = [];
    private WorkspaceTree _tree = WorkspaceTree.Empty;
    private List<WorkspaceVisibleRow> _visibleRows = [];
    private bool _suppressToggleEvents;

    public event Action<WorkspaceEntry>? FileActivated;
    public event Action? RefreshRequested;
    public event Action? CloseRequested;

    public ListView TreeList => _treeList;
    public IReadOnlyList<WorkspaceVisibleRow> VisibleRows => _visibleRows;
    public IReadOnlySet<ulong> ExpandedDirectoryIds => _expandedDirectoryIds;
    public WorkspaceTree CurrentTree => _tree;

    public bool ShowAllFiles
    {
        get => _allToggle.IsChecked == true;
        set
        {
            if (_allToggle.IsChecked == value)
            {
                return;
            }
            _suppressToggleEvents = true;
            _allToggle.IsChecked = value;
            _suppressToggleEvents = false;
            RebuildVisibleRows();
        }
    }

    public bool ShowHidden
    {
        get => _hiddenToggle.IsChecked == true;
        set
        {
            if (_hiddenToggle.IsChecked == value)
            {
                return;
            }
            _suppressToggleEvents = true;
            _hiddenToggle.IsChecked = value;
            _suppressToggleEvents = false;
            RebuildVisibleRows();
        }
    }

    public WorkspaceEntry? SelectedEntry =>
        _treeList.SelectedIndex >= 0 && _treeList.SelectedIndex < _visibleRows.Count
            ? _visibleRows[_treeList.SelectedIndex].Entry
            : null;

    public WorkspaceSidebar()
    {
        Width = 260;

        AutomationProperties.SetName(_allToggle, "Show all files");
        ToolTipService.SetToolTip(_allToggle, "Show all files (off shows Markdown files only)");
        AutomationProperties.SetName(_hiddenToggle, "Show hidden files");
        ToolTipService.SetToolTip(_hiddenToggle, "Show hidden files and folders");
        AutomationProperties.SetName(_refreshButton, "Refresh workspace");
        ToolTipService.SetToolTip(_refreshButton, "Refresh workspace");
        AutomationProperties.SetName(_closeButton, "Hide workspace sidebar");
        ToolTipService.SetToolTip(_closeButton, "Hide workspace sidebar (Ctrl+B)");
        AutomationProperties.SetName(_treeList, "Workspace files");

        _allToggle.Checked += (_, _) =>
        {
            if (!_suppressToggleEvents)
            {
                RebuildVisibleRows();
            }
        };
        _allToggle.Unchecked += (_, _) =>
        {
            if (!_suppressToggleEvents)
            {
                RebuildVisibleRows();
            }
        };
        _hiddenToggle.Checked += (_, _) =>
        {
            if (!_suppressToggleEvents)
            {
                RebuildVisibleRows();
            }
        };
        _hiddenToggle.Unchecked += (_, _) =>
        {
            if (!_suppressToggleEvents)
            {
                RebuildVisibleRows();
            }
        };
        _refreshButton.Click += (_, _) => RefreshRequested?.Invoke();
        _closeButton.Click += (_, _) => CloseRequested?.Invoke();

        _treeList.ChoosingItemContainer += (_, args) => args.ItemContainer ??= new WorkspaceRowItem();
        _treeList.ItemTemplate = (DataTemplate)XamlReader.Load(
            "<DataTemplate xmlns='http://schemas.microsoft.com/winfx/2006/xaml/presentation'><Grid/></DataTemplate>");
        _treeList.ContainerContentChanging += (_, args) =>
        {
            if (args.InRecycleQueue)
            {
                return;
            }
            if (args.ItemContainer is WorkspaceRowItem rowItem &&
                args.ItemIndex >= 0 &&
                args.ItemIndex < _visibleRows.Count)
            {
                rowItem.Attach();
                rowItem.Bind(_visibleRows[args.ItemIndex]);
            }
        };
        _treeList.ItemClick += (_, args) =>
        {
            if (args.ClickedItem is int idx && idx >= 0 && idx < _visibleRows.Count)
            {
                ActivateRow(_visibleRows[idx]);
            }
        };
        _treeList.PreviewKeyDown += (_, args) =>
        {
            if (HandleTreeKeyDown(args.Key))
            {
                args.Handled = true;
            }
        };

        var headerGrid = new Grid
        {
            Margin = new Thickness(8, 6, 6, 4),
            ColumnSpacing = 4,
        };
        headerGrid.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        headerGrid.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        headerGrid.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        headerGrid.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        headerGrid.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });

        Grid.SetColumn(_headerTitle, 0);
        Grid.SetColumn(_allToggle, 1);
        Grid.SetColumn(_hiddenToggle, 2);
        Grid.SetColumn(_refreshButton, 3);
        Grid.SetColumn(_closeButton, 4);

        headerGrid.Children.Add(_headerTitle);
        headerGrid.Children.Add(_allToggle);
        headerGrid.Children.Add(_hiddenToggle);
        headerGrid.Children.Add(_refreshButton);
        headerGrid.Children.Add(_closeButton);

        _root.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        _root.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        _root.RowDefinitions.Add(new RowDefinition { Height = new GridLength(1, GridUnitType.Star) });

        Grid.SetRow(headerGrid, 0);
        Grid.SetRow(_statusText, 1);
        Grid.SetRow(_treeList, 2);

        _root.Children.Add(headerGrid);
        _root.Children.Add(_statusText);
        _root.Children.Add(_treeList);

        Content = _root;
    }

    public void SetLoading(string folderDisplayName, bool clearTree = true)
    {
        _headerTitle.Text = string.IsNullOrWhiteSpace(folderDisplayName) ? "Workspace" : folderDisplayName;
        _statusText.Text = clearTree ? "Loading workspace…" : "Refreshing workspace…";
        _statusText.Visibility = Visibility.Visible;
        if (clearTree)
        {
            _tree = WorkspaceTree.Empty;
            _expandedDirectoryIds.Clear();
            _visibleRows = [];
            _treeList.ItemsSource = new IndexList(0);
        }
    }

    public void SetError(string message)
    {
        _statusText.Text = string.IsNullOrWhiteSpace(message) ? "Could not load workspace." : message;
        _statusText.Visibility = Visibility.Visible;
    }

    public void SetTree(string folderDisplayName, WorkspaceTree tree, bool resetExpanded = false)
    {
        ArgumentNullException.ThrowIfNull(tree);
        _headerTitle.Text = string.IsNullOrWhiteSpace(folderDisplayName) ? "Workspace" : folderDisplayName;

        if (resetExpanded)
        {
            _expandedDirectoryIds.Clear();
        }
        else if (_expandedDirectoryIds.Count > 0)
        {
            // Preserve expansion by relative path across refreshes.
            var expandedPaths = new HashSet<string>(StringComparer.Ordinal);
            foreach (var entry in _tree.Entries)
            {
                if (entry.IsDirectory && _expandedDirectoryIds.Contains(entry.Id))
                {
                    expandedPaths.Add(entry.RelativePath);
                }
            }
            _expandedDirectoryIds.Clear();
            foreach (var entry in tree.Entries)
            {
                if (entry.IsDirectory && expandedPaths.Contains(entry.RelativePath))
                {
                    _expandedDirectoryIds.Add(entry.Id);
                }
            }
        }

        _tree = tree;
        RebuildVisibleRows();
    }

    public void Clear()
    {
        _headerTitle.Text = "Workspace";
        _statusText.Text = string.Empty;
        _statusText.Visibility = Visibility.Collapsed;
        _tree = WorkspaceTree.Empty;
        _expandedDirectoryIds.Clear();
        _visibleRows = [];
        _treeList.ItemsSource = null;
    }

    public bool ExpandDirectory(ulong entryId)
    {
        if (!_expandedDirectoryIds.Add(entryId))
        {
            return false;
        }
        RebuildVisibleRows(selectedEntryId: entryId);
        return true;
    }

    public bool CollapseDirectory(ulong entryId)
    {
        if (!_expandedDirectoryIds.Remove(entryId))
        {
            return false;
        }
        RebuildVisibleRows(selectedEntryId: entryId);
        return true;
    }

    public bool ToggleDirectory(ulong entryId) =>
        _expandedDirectoryIds.Contains(entryId)
            ? CollapseDirectory(entryId)
            : ExpandDirectory(entryId);

    public bool HandleTreeKeyDown(VirtualKey key)
    {
        var idx = _treeList.SelectedIndex;
        if (idx < 0 || idx >= _visibleRows.Count)
        {
            return false;
        }

        var row = _visibleRows[idx];
        var entry = row.Entry;

        switch (key)
        {
            case VirtualKey.Right:
                if (!entry.IsDirectory)
                {
                    return false;
                }
                if (!row.IsExpanded)
                {
                    ExpandDirectory(entry.Id);
                    return true;
                }
                if (idx + 1 < _visibleRows.Count && _visibleRows[idx + 1].Entry.ParentIndex == entry.Index)
                {
                    _treeList.SelectedIndex = idx + 1;
                    _treeList.ScrollIntoView(idx + 1);
                    return true;
                }
                return false;

            case VirtualKey.Left:
                if (entry.IsDirectory && row.IsExpanded)
                {
                    CollapseDirectory(entry.Id);
                    return true;
                }
                if (entry.ParentIndex != -1)
                {
                    for (var i = idx - 1; i >= 0; i--)
                    {
                        if (_visibleRows[i].Entry.Index == entry.ParentIndex)
                        {
                            _treeList.SelectedIndex = i;
                            _treeList.ScrollIntoView(i);
                            return true;
                        }
                    }
                }
                return false;

            case VirtualKey.Enter:
            case VirtualKey.Space:
                ActivateRow(row);
                return true;

            default:
                return false;
        }
    }

    private void ActivateRow(WorkspaceVisibleRow row)
    {
        if (row.Entry.IsDirectory)
        {
            ToggleDirectory(row.Entry.Id);
        }
        else
        {
            FileActivated?.Invoke(row.Entry);
        }
    }

    private void RebuildVisibleRows(ulong? selectedEntryId = null)
    {
        selectedEntryId ??= SelectedEntry?.Id;
        _visibleRows = _tree.BuildVisibleRows(_expandedDirectoryIds, ShowAllFiles, ShowHidden);
        _treeList.ItemsSource = new IndexList(_visibleRows.Count);

        if (_tree.Truncated)
        {
            _statusText.Text = $"Limit reached ({_tree.Entries.Count:N0} entries scanned)";
            _statusText.Visibility = Visibility.Visible;
        }
        else if (_visibleRows.Count == 0 && _tree.Entries.Count > 0)
        {
            _statusText.Text = "No matching files";
            _statusText.Visibility = Visibility.Visible;
        }
        else if (_tree.Entries.Count == 0 && _headerTitle.Text != "Workspace")
        {
            _statusText.Text = "Folder is empty";
            _statusText.Visibility = Visibility.Visible;
        }
        else
        {
            _statusText.Text = string.Empty;
            _statusText.Visibility = Visibility.Collapsed;
        }

        if (selectedEntryId.HasValue)
        {
            for (var i = 0; i < _visibleRows.Count; i++)
            {
                if (_visibleRows[i].Entry.Id == selectedEntryId.Value)
                {
                    _treeList.SelectedIndex = i;
                    break;
                }
            }
        }
    }
}
