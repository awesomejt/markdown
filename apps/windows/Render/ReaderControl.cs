using System.Diagnostics;
using Markdown.Windows.Render;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;
using Windows.System;

namespace Markdown.Windows.Render;

/// <summary>
/// Native WinUI presentation control: a virtualized document ListView with recycled RowItems,
/// and an outline ListView sidebar for jumping to headings.
/// </summary>
public sealed partial class ReaderControl : UserControl
{
    private readonly Grid _rootGrid = new();
    private readonly ListView _documentList = new()
    {
        SelectionMode = ListViewSelectionMode.None,
        IsItemClickEnabled = false,
    };
    private readonly ListView _outlineList = new()
    {
        SelectionMode = ListViewSelectionMode.Single,
        IsItemClickEnabled = true,
    };
    private readonly HashSet<RowItem> _boundRows = [];

    private RenderPlan? _plan;
    private ScrollViewer? _scrollViewer;
    private bool _suppressOutlineSelection;
    private int _pendingFocusIndex = -1;

    public Func<Uri, bool>? ExternalUriLauncher { get; set; }

    public ListView DocumentList => _documentList;
    public ListView OutlineList => _outlineList;
    public IReadOnlyCollection<RowItem> BoundRows => _boundRows;
    public RenderPlan? CurrentPlan => _plan;
    public ulong CurrentGeneration { get; private set; }

    public ReaderControl()
    {
        _rootGrid.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(240) });
        _rootGrid.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });

        _outlineList.ItemTemplate = (DataTemplate)Microsoft.UI.Xaml.Markup.XamlReader.Load(
            "<DataTemplate xmlns='http://schemas.microsoft.com/winfx/2006/xaml/presentation'><TextBlock Text='{Binding}' TextWrapping='NoWrap' Margin='8,4,8,4' FontSize='14'/></DataTemplate>");
        _outlineList.SelectionChanged += OnOutlineSelectionChanged;
        _outlineList.ItemClick += (_, args) =>
        {
            if (_plan is null || _outlineList.ItemsSource is not IList<string> items)
            {
                return;
            }
            var index = items.IndexOf(args.ClickedItem as string ?? string.Empty);
            if (index >= 0)
            {
                ActivateOutline(index);
            }
        };
        AutomationProperties.SetName(_outlineList, "Document outline");

        _documentList.ChoosingItemContainer += (_, args) => args.ItemContainer ??= new RowItem();
        _documentList.ItemTemplate = (DataTemplate)Microsoft.UI.Xaml.Markup.XamlReader.Load(
            "<DataTemplate xmlns='http://schemas.microsoft.com/winfx/2006/xaml/presentation'><Grid/></DataTemplate>");
        _documentList.ContainerContentChanging += OnContainerContentChanging;
        AutomationProperties.SetName(_documentList, "Document content");

        Grid.SetColumn(_outlineList, 0);
        Grid.SetColumn(_documentList, 1);

        _rootGrid.Children.Add(_outlineList);
        _rootGrid.Children.Add(_documentList);

        Content = _rootGrid;
    }

    public int FirstVisibleFragmentIndex()
    {
        if (_boundRows.Count == 0)
        {
            return 0;
        }
        var minIndex = int.MaxValue;
        foreach (var row in _boundRows)
        {
            if (row.ItemIndex >= 0 && row.ItemIndex < minIndex)
            {
                minIndex = row.ItemIndex;
            }
        }
        return minIndex == int.MaxValue ? 0 : minIndex;
    }

    public void InstallPlan(RenderPlan plan, ulong generation = 0)
    {
        if (generation != 0 && generation == CurrentGeneration && _plan is not null)
        {
            return;
        }

        var restoreIndex = CurrentGeneration > 0 ? FirstVisibleFragmentIndex() : 0;
        Clear();
        _plan = plan;
        CurrentGeneration = generation;

        var titles = new List<string>(plan.Outline.Count);
        foreach (var entry in plan.Outline)
        {
            var indent = new string(' ', Math.Max(0, entry.Level - 1) * 2);
            titles.Add($"{indent}{entry.Title}");
        }
        _outlineList.ItemsSource = titles;

        _documentList.ItemsSource = new IndexList(plan.Fragments.Count);
        _documentList.UpdateLayout();

        _scrollViewer ??= FindScrollViewer(_documentList);
        Visibility = Visibility.Visible;

        if (restoreIndex > 0 && restoreIndex < plan.Fragments.Count)
        {
            ScrollToFragment(restoreIndex);
        }
    }

    public void Clear()
    {
        _pendingFocusIndex = -1;
        _suppressOutlineSelection = true;
        _outlineList.ItemsSource = null;
        _documentList.ItemsSource = null;
        _suppressOutlineSelection = false;

        foreach (var row in _boundRows)
        {
            row.Unbind();
        }
        _boundRows.Clear();

        _plan = null;
        CurrentGeneration = 0;
        Visibility = Visibility.Collapsed;
    }

    public void ActivateOutline(int index)
    {
        if (_plan is null || (uint)index >= (uint)_plan.Outline.Count)
        {
            return;
        }

        var fragmentIndex = _plan.Outline[index].Fragment;
        ScrollToFragment(fragmentIndex);
    }

    public bool ActivateFragmentLink()
    {
        if (_plan is null)
        {
            return false;
        }

        foreach (var row in _boundRows)
        {
            if (row.ItemIndex < 0 || row.ItemIndex >= _plan.Fragments.Count)
            {
                continue;
            }

            var fragment = _plan.Fragments[row.ItemIndex];
            foreach (var run in fragment.Runs)
            {
                if (run.Link >= 0 && _plan.TargetFor(row.ItemIndex, run.Link) is { } target && target.Kind == TargetKind.Fragment)
                {
                    OnLinkActivated(row.ItemIndex, run.Link);
                    return true;
                }
            }
        }

        return false;
    }

    public void ScrollToFragment(int fragmentIndex)
    {
        if (_plan is null || fragmentIndex < 0 || fragmentIndex >= _plan.Fragments.Count)
        {
            return;
        }

        _documentList.ScrollIntoView(fragmentIndex, ScrollIntoViewAlignment.Leading);
        _documentList.UpdateLayout();
        if (_documentList.ContainerFromIndex(fragmentIndex) is RowItem row)
        {
            _pendingFocusIndex = -1;
            row.Focus(FocusState.Programmatic);
        }
        else
        {
            _pendingFocusIndex = fragmentIndex;
        }
    }

    private void OnOutlineSelectionChanged(object sender, SelectionChangedEventArgs e)
    {
        if (_suppressOutlineSelection || _outlineList.SelectedIndex < 0 || _plan is null)
        {
            return;
        }

        ActivateOutline(_outlineList.SelectedIndex);
    }

    private void OnContainerContentChanging(ListViewBase sender, ContainerContentChangingEventArgs args)
    {
        var row = (RowItem)args.ItemContainer;
        if (args.InRecycleQueue)
        {
            _boundRows.Remove(row);
            row.Unbind();
        }
        else
        {
            row.Attach();
            if (_plan is not null && args.ItemIndex >= 0 && args.ItemIndex < _plan.Fragments.Count)
            {
                row.Bind(args.ItemIndex, _plan.Fragments[args.ItemIndex], OnLinkActivated);
                _boundRows.Add(row);
                if (args.ItemIndex == _pendingFocusIndex)
                {
                    _pendingFocusIndex = -1;
                    row.Focus(FocusState.Programmatic);
                }
            }
        }
        args.Handled = true;
    }

    private void OnLinkActivated(int fragmentIndex, int linkIndex)
    {
        if (_plan is null)
        {
            return;
        }

        var target = _plan.TargetFor(fragmentIndex, linkIndex);
        if (target is null)
        {
            return;
        }

        switch (target.Value.Kind)
        {
            case TargetKind.Fragment:
                ScrollToFragment(target.Value.Fragment);
                break;

            case TargetKind.External:
                if (!string.IsNullOrEmpty(target.Value.Uri) && Uri.TryCreate(target.Value.Uri, UriKind.Absolute, out var uri))
                {
                    if (ExternalUriLauncher is not null)
                    {
                        _ = ExternalUriLauncher(uri);
                    }
                    else
                    {
                        _ = Launcher.LaunchUriAsync(uri);
                    }
                }
                break;

            case TargetKind.Inert:
            default:
                break;
        }
    }

    private static ScrollViewer? FindScrollViewer(DependencyObject node)
    {
        for (var i = 0; i < Microsoft.UI.Xaml.Media.VisualTreeHelper.GetChildrenCount(node); i++)
        {
            var child = Microsoft.UI.Xaml.Media.VisualTreeHelper.GetChild(node, i);
            if (RowItem.TryAs<ScrollViewer>(child) is { } viewer)
            {
                return viewer;
            }
            if (FindScrollViewer(child) is { } found)
            {
                return found;
            }
        }
        return null;
    }
}
