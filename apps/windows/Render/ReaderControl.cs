using System.Diagnostics;
using Markdown.Windows.Core;
using Markdown.Windows.Render;
using Microsoft.UI.Dispatching;
using Microsoft.UI.Input;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Input;
using Windows.Foundation;
using Windows.System;
using Windows.UI.Core;

namespace Markdown.Windows.Render;

/// <summary>
/// Native WinUI presentation control: a virtualized document ListView with recycled RowItems,
/// an outline ListView sidebar for jumping to headings, and global multi-row selection with
/// Win32 clipboard copying.
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
    private readonly Microsoft.UI.Dispatching.DispatcherQueueTimer? _autoScrollTimer;

    public ImageSourceCache ImageCache { get; } = new();

    private RenderPlan? _plan;
    private ScrollViewer? _scrollViewer;
    private bool _suppressOutlineSelection;
    private int _pendingFocusIndex = -1;
    private int _lastScrolledFragmentIndex = 0;
    private long _lastScrolledTimestamp;

    private DocumentPosition? _dragAnchor;
    private bool _isDragging;
    private Point _lastPointerPosition;

    public Func<Uri, bool>? ExternalUriLauncher { get; set; }

    public ListView DocumentList => _documentList;
    public bool IsOutlineVisible
    {
        get => _outlineList.Visibility == Visibility.Visible;
        set => _outlineList.Visibility = value ? Visibility.Visible : Visibility.Collapsed;
    }

    public ListView OutlineList => _outlineList;
    public IReadOnlyCollection<RowItem> BoundRows => _boundRows;
    public RenderPlan? CurrentPlan => _plan;
    public ulong CurrentGeneration { get; private set; }
    public double TextScale { get; private set; } = 1.0;
    public FindMatches? CurrentFindMatches { get; private set; }
    public int? CurrentMatchIndex { get; private set; }
    public nint WindowHandle { get; set; }

    public DocumentSelection? Selection { get; private set; }

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
        _documentList.AddHandler(UIElement.PointerPressedEvent, new PointerEventHandler(OnDocumentListPointerPressed), handledEventsToo: true);
        _documentList.AddHandler(UIElement.PointerMovedEvent, new PointerEventHandler(OnDocumentListPointerMoved), handledEventsToo: true);
        _documentList.AddHandler(UIElement.PointerReleasedEvent, new PointerEventHandler(OnDocumentListPointerReleased), handledEventsToo: true);
        _documentList.AddHandler(UIElement.PointerCaptureLostEvent, new PointerEventHandler(OnDocumentListPointerCaptureLost), handledEventsToo: true);
        AutomationProperties.SetName(_documentList, "Document content");

        PreviewKeyDown += OnPreviewKeyDown;

        var queue = Microsoft.UI.Dispatching.DispatcherQueue.GetForCurrentThread();
        if (queue is not null)
        {
            _autoScrollTimer = queue.CreateTimer();
            _autoScrollTimer.Interval = TimeSpan.FromMilliseconds(16);
            _autoScrollTimer.Tick += OnAutoScrollTick;
        }

        Grid.SetColumn(_outlineList, 0);
        Grid.SetColumn(_documentList, 1);

        _rootGrid.Children.Add(_outlineList);
        _rootGrid.Children.Add(_documentList);

        Content = _rootGrid;
    }

    public int FirstVisibleFragmentIndex()
    {
        if (_lastScrolledFragmentIndex > 0)
        {
            var elapsedMs = (Stopwatch.GetTimestamp() - _lastScrolledTimestamp) * 1000.0 / Stopwatch.Frequency;
            if (elapsedMs < 1000 && !_boundRows.Any(r => Math.Abs(r.ItemIndex - _lastScrolledFragmentIndex) <= 1))
            {
                return _lastScrolledFragmentIndex;
            }
        }

        if (_boundRows.Count == 0)
        {
            return Math.Max(0, _lastScrolledFragmentIndex);
        }

        RowItem? topRow = null;
        var minDistance = double.MaxValue;
        foreach (var row in _boundRows)
        {
            if (row.ItemIndex < 0) continue;
            try
            {
                var transform = row.TransformToVisual(_documentList);
                var top = transform.TransformPoint(default).Y;
                var height = row.ActualHeight;
                if (top + height > 0)
                {
                    if (top >= 0 && top < minDistance)
                    {
                        minDistance = top;
                        topRow = row;
                    }
                    else if (topRow is null && top < 0 && (top + height) > 0)
                    {
                        topRow = row;
                    }
                }
            }
            catch
            {
            }
        }

        if (topRow is not null)
        {
            return topRow.ItemIndex;
        }

        return Math.Max(0, _boundRows.Min(r => r.ItemIndex));
    }

    public void SetTextScale(double scale)
    {
        TextScale = Markdown.Windows.Render.TextScale.Clamped(scale);
        if (_plan is null) return;
        foreach (var row in _boundRows)
        {
            if (row.ItemIndex >= 0 && row.ItemIndex < _plan.Fragments.Count)
            {
                row.Bind(row.ItemIndex, _plan.Fragments[row.ItemIndex], OnLinkActivated, ImageCache, TextScale, _plan.Fragments.Count);
                UpdateRowHighlights(row);
                UpdateRowSelection(row);
            }
        }
    }

    public void SetFindMatches(FindMatches? matches, int? currentMatchIndex = null)
    {
        CurrentFindMatches = matches;
        CurrentMatchIndex = currentMatchIndex;
        foreach (var row in _boundRows)
        {
            UpdateRowHighlights(row);
        }
    }

    public void NavigateToMatch(int matchIndex)
    {
        if (CurrentFindMatches is null || matchIndex < 0 || matchIndex >= CurrentFindMatches.Matches.Count)
        {
            return;
        }
        CurrentMatchIndex = matchIndex;
        var match = CurrentFindMatches.Matches[matchIndex];
        ScrollToFragment(match.PrimaryFragmentIndex);
        foreach (var row in _boundRows)
        {
            UpdateRowHighlights(row);
        }
    }

    private void UpdateRowHighlights(RowItem row)
    {
        if (CurrentFindMatches is null || row.ItemIndex < 0)
        {
            row.SetHighlights(null);
            return;
        }

        var fragRanges = CurrentFindMatches.RangesForFragment(row.ItemIndex);
        if (fragRanges.Count == 0)
        {
            row.SetHighlights(null);
            return;
        }

        var list = new List<(int Start, int Length, bool IsCurrent)>(fragRanges.Count);
        foreach (var (mIdx, range) in fragRanges)
        {
            var isCurrent = (CurrentMatchIndex.HasValue && CurrentMatchIndex.Value == mIdx);
            list.Add((range.Start, range.Length, isCurrent));
        }
        row.SetHighlights(list);
    }

    public void SetSelection(DocumentSelection? selection)
    {
        Selection = selection;
        foreach (var row in _boundRows)
        {
            UpdateRowSelection(row);
        }
    }

    public void ClearSelection() => SetSelection(null);

    public void SelectAll()
    {
        if (_plan is null || _plan.Fragments.Count == 0)
        {
            return;
        }

        var lastFrag = _plan.Fragments.Count - 1;
        var lastLen = _plan.Fragments[lastFrag].Text.Length;
        SetSelection(new DocumentSelection(new DocumentPosition(0, 0), new DocumentPosition(lastFrag, lastLen)));
    }

    public (string PlainText, byte[] MarkdownBytes)? CopySelection(CoreDocument? document)
    {
        if (Selection is not { } sel || sel.IsEmpty || _plan is null)
        {
            return null;
        }

        var plain = _plan.TextBetween(sel.Start, sel.End);
        var sourceSpan = _plan.SourceBetween(sel.Start, sel.End, document);
        var markdownBytes = sourceSpan.IsEmpty ? System.Text.Encoding.UTF8.GetBytes(plain) : sourceSpan.ToArray();

        if (!NativeClipboard.TrySet(WindowHandle, plain, markdownBytes))
        {
            return null;
        }

        return (plain, markdownBytes);
    }

    private void UpdateRowSelection(RowItem row)
    {
        if (Selection is not { } sel || sel.IsEmpty || row.ItemIndex < 0 || _plan is null)
        {
            row.SetSelectionRange(null, null);
            return;
        }

        var start = sel.Start;
        var end = sel.End;

        if (row.ItemIndex < start.FragmentIndex || row.ItemIndex > end.FragmentIndex)
        {
            row.SetSelectionRange(null, null);
            return;
        }

        var fragment = _plan.Fragments[row.ItemIndex];
        var textLen = fragment.Text.Length;

        var s = (row.ItemIndex == start.FragmentIndex) ? Math.Clamp(start.CharOffset, 0, textLen) : 0;
        var e = (row.ItemIndex == end.FragmentIndex) ? Math.Clamp(end.CharOffset, 0, textLen) : textLen;

        if (e > s)
        {
            row.SetSelectionRange(s, e - s);
        }
        else
        {
            row.SetSelectionRange(null, null);
        }
    }

    public SavedPosition? SavePosition(CoreDocument? document)
    {
        if (document is null || document.Snapshot is null || _plan is null || _plan.Fragments.Count == 0)
        {
            return null;
        }

        try
        {
            var topFrag = FirstVisibleFragmentIndex();
            var topOffset = _plan.SourceOffsetAt(new DocumentPosition(topFrag, 0), document);
            var topAnchor = document.AnchorAt(topOffset);

            if (Selection is { } sel && !sel.IsEmpty)
            {
                var selStartOffset = _plan.SourceOffsetAt(sel.Start, document);
                var selEndOffset = _plan.SourceOffsetAt(sel.End, document);
                var startAnchor = document.AnchorAt(selStartOffset);
                var endAnchor = document.AnchorAt(selEndOffset);
                return new SavedPosition(topAnchor, startAnchor, endAnchor);
            }

            return new SavedPosition(topAnchor, topAnchor, null);
        }
        catch (CoreException)
        {
            return null;
        }
    }

    public void RestorePosition(SavedPosition? saved, CoreDocument? document)
    {
        if (saved is null || document?.Snapshot is null || _plan is null || _plan.Fragments.Count == 0)
        {
            return;
        }

        try
        {
            var topRes = document.ResolveAnchor(saved.Top);
            if (topRes.HasValue)
            {
                var topPos = _plan.PositionAtSourceOffset(topRes.Value.SourceOffset, document);
                ScrollToFragment(topPos.FragmentIndex);
            }

            if (saved.SelectionEnd is not null)
            {
                var startRes = document.ResolveAnchor(saved.SelectionStart);
                var endRes = document.ResolveAnchor(saved.SelectionEnd);
                if (startRes.HasValue && endRes.HasValue)
                {
                    var startPos = _plan.PositionAtSourceOffset(startRes.Value.SourceOffset, document);
                    var endPos = _plan.PositionAtSourceOffset(endRes.Value.SourceOffset, document);
                    SetSelection(new DocumentSelection(startPos, endPos));
                }
                else
                {
                    ClearSelection();
                }
            }
            else
            {
                ClearSelection();
            }
        }
        catch (CoreException)
        {
            // Position cannot be restored; keep default position
        }
    }

    public void InstallPlan(RenderPlan plan, ulong generation = 0, SavedPosition? savedPosition = null, CoreDocument? document = null)
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

        if (savedPosition is not null && document is not null)
        {
            RestorePosition(savedPosition, document);
        }
        else if (restoreIndex > 0 && restoreIndex < plan.Fragments.Count)
        {
            ScrollToFragment(restoreIndex);
        }
    }

    public void Clear()
    {
        _pendingFocusIndex = -1;
        _lastScrolledFragmentIndex = 0;
        _suppressOutlineSelection = true;
        _outlineList.ItemsSource = null;
        _documentList.ItemsSource = null;
        _suppressOutlineSelection = false;

        foreach (var row in _boundRows)
        {
            row.Unbind();
        }
        _boundRows.Clear();
        ImageCache.Clear();

        ClearSelection();
        CurrentFindMatches = null;
        CurrentMatchIndex = null;
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

        _lastScrolledFragmentIndex = fragmentIndex;
        _lastScrolledTimestamp = Stopwatch.GetTimestamp();
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
                row.Bind(args.ItemIndex, _plan.Fragments[args.ItemIndex], OnLinkActivated, ImageCache, TextScale, _plan.Fragments.Count);
                UpdateRowHighlights(row);
                UpdateRowSelection(row);
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

    public DocumentPosition? HitTest(Point point)
    {
        if (_plan is null || _plan.Fragments.Count == 0 || _boundRows.Count == 0)
        {
            return null;
        }

        RowItem? best = null;
        var bestDistance = double.MaxValue;

        foreach (var row in _boundRows)
        {
            if (row.ItemIndex < 0 || row.ActualHeight <= 0)
            {
                continue;
            }

            var transform = row.TransformToVisual(_documentList);
            var top = transform.TransformPoint(default).Y;
            var height = row.ActualHeight;

            double distance;
            if (point.Y < top)
            {
                distance = top - point.Y;
            }
            else if (point.Y > top + height)
            {
                distance = point.Y - top - height;
            }
            else
            {
                distance = 0;
            }

            if (distance < bestDistance)
            {
                best = row;
                bestDistance = distance;
                if (distance == 0)
                {
                    break;
                }
            }
        }

        if (best is null)
        {
            return null;
        }

        var localPoint = _documentList.TransformToVisual(best.TextBlock).TransformPoint(point);
        return PositionInRow(best, localPoint);
    }

    public DocumentPosition PositionInRow(RowItem row, Point local)
    {
        if (_plan is null || row.ItemIndex < 0 || row.ItemIndex >= _plan.Fragments.Count)
        {
            return default;
        }

        var fragment = _plan.Fragments[row.ItemIndex];
        var text = fragment.Text;
        if (text.Length == 0)
        {
            return new DocumentPosition(row.ItemIndex, 0);
        }

        if (local.Y < 0)
        {
            return new DocumentPosition(row.ItemIndex, 0);
        }
        if (local.Y >= row.TextBlock.ActualHeight)
        {
            return new DocumentPosition(row.ItemIndex, text.Length);
        }

        var x = Math.Clamp(local.X, 0, Math.Max(0, row.TextBlock.ActualWidth - 1));
        var pointer = row.TextBlock.GetPositionFromPoint(new Point(x, local.Y));
        var charOffset = pointer is not null ? Math.Clamp(pointer.Offset, 0, text.Length) : 0;

        // Ensure we do not split a surrogate pair
        if (charOffset > 0 && charOffset < text.Length && char.IsLowSurrogate(text[charOffset]))
        {
            charOffset--;
        }

        return new DocumentPosition(row.ItemIndex, charOffset);
    }

    private void OnDocumentListPointerPressed(object sender, PointerRoutedEventArgs e)
    {
        var point = e.GetCurrentPoint(_documentList);
        if (!point.Properties.IsLeftButtonPressed || _plan is null || _plan.Fragments.Count == 0)
        {
            return;
        }

        var hit = HitTest(point.Position);
        if (hit is null)
        {
            return;
        }

        var shift = (InputKeyboardSource.GetKeyStateForCurrentThread(VirtualKey.Shift) & CoreVirtualKeyStates.Down) != 0;
        if (shift && Selection is { } sel)
        {
            _dragAnchor = sel.Anchor;
            SetSelection(new DocumentSelection(sel.Anchor, hit.Value));
        }
        else
        {
            _dragAnchor = hit.Value;
            SetSelection(null);
        }

        _isDragging = true;
        _lastPointerPosition = point.Position;
        _ = _documentList.CapturePointer(e.Pointer);

        if (_documentList.ContainerFromIndex(hit.Value.FragmentIndex) is RowItem row)
        {
            _ = row.Focus(FocusState.Pointer);
        }
    }

    private void OnDocumentListPointerMoved(object sender, PointerRoutedEventArgs e)
    {
        if (!_isDragging || _dragAnchor is null)
        {
            return;
        }

        _lastPointerPosition = e.GetCurrentPoint(_documentList).Position;
        UpdateDrag();

        if (_lastPointerPosition.Y < 0 || _lastPointerPosition.Y > _documentList.ActualHeight)
        {
            _autoScrollTimer?.Start();
        }
        else
        {
            _autoScrollTimer?.Stop();
        }
    }

    private void OnDocumentListPointerReleased(object sender, PointerRoutedEventArgs e)
    {
        if (!_isDragging)
        {
            return;
        }

        _documentList.ReleasePointerCapture(e.Pointer);
        _isDragging = false;
        _autoScrollTimer?.Stop();
    }

    private void OnDocumentListPointerCaptureLost(object sender, PointerRoutedEventArgs e)
    {
        _isDragging = false;
        _autoScrollTimer?.Stop();
    }

    private void UpdateDrag()
    {
        if (_dragAnchor is not { } anchor || _plan is null)
        {
            return;
        }

        var clamped = new Point(
            _lastPointerPosition.X,
            Math.Clamp(_lastPointerPosition.Y, 0, Math.Max(0, _documentList.ActualHeight - 1)));

        if (HitTest(clamped) is not { } hit)
        {
            return;
        }

        SetSelection(new DocumentSelection(anchor, hit));
    }

    private void OnAutoScrollTick(Microsoft.UI.Dispatching.DispatcherQueueTimer sender, object args)
    {
        if (!_isDragging || _scrollViewer is null)
        {
            _autoScrollTimer?.Stop();
            return;
        }

        var delta = _lastPointerPosition.Y < 0
            ? _lastPointerPosition.Y
            : _lastPointerPosition.Y - _documentList.ActualHeight;

        var step = Math.Clamp(delta, -60, 60);
        var targetOffset = Math.Clamp(_scrollViewer.VerticalOffset + step, 0, _scrollViewer.ScrollableHeight);
        _ = _scrollViewer.ChangeView(null, targetOffset, null, disableAnimation: true);
        _documentList.UpdateLayout();
        UpdateDrag();
    }

    private void OnPreviewKeyDown(object sender, KeyRoutedEventArgs e)
    {
        var shift = (InputKeyboardSource.GetKeyStateForCurrentThread(VirtualKey.Shift) & CoreVirtualKeyStates.Down) != 0;
        var ctrl = (InputKeyboardSource.GetKeyStateForCurrentThread(VirtualKey.Control) & CoreVirtualKeyStates.Down) != 0;

        if (shift)
        {
            if (e.Key == VirtualKey.Home)
            {
                if (ctrl)
                {
                    ExtendSelectionToEdge(toEnd: false);
                }
                else
                {
                    ExtendSelectionToFragmentEdge(toEnd: false);
                }
                e.Handled = true;
            }
            else if (e.Key == VirtualKey.End)
            {
                if (ctrl)
                {
                    ExtendSelectionToEdge(toEnd: true);
                }
                else
                {
                    ExtendSelectionToFragmentEdge(toEnd: true);
                }
                e.Handled = true;
            }
            else if (e.Key == VirtualKey.Left)
            {
                ExtendSelectionByChar(backwards: true);
                e.Handled = true;
            }
            else if (e.Key == VirtualKey.Right)
            {
                ExtendSelectionByChar(backwards: false);
                e.Handled = true;
            }
            else if (e.Key == VirtualKey.Up)
            {
                ExtendSelectionByFragment(delta: -1);
                e.Handled = true;
            }
            else if (e.Key == VirtualKey.Down)
            {
                ExtendSelectionByFragment(delta: 1);
                e.Handled = true;
            }
        }
    }

    public void ExtendSelectionByChar(bool backwards)
    {
        if (_plan is null || _plan.Fragments.Count == 0) return;
        var anchor = Selection?.Anchor ?? CurrentCaretPosition();
        var active = Selection?.Active ?? anchor;

        var frag = active.FragmentIndex;
        var offset = active.CharOffset;
        var text = _plan.Fragments[frag].Text;

        if (backwards)
        {
            if (offset > 0)
            {
                var step = (offset >= 2 && char.IsLowSurrogate(text[offset - 1]) && char.IsHighSurrogate(text[offset - 2])) ? 2 : 1;
                active = new DocumentPosition(frag, offset - step);
            }
            else if (frag > 0)
            {
                var prevText = _plan.Fragments[frag - 1].Text;
                active = new DocumentPosition(frag - 1, prevText.Length);
            }
        }
        else
        {
            if (offset < text.Length)
            {
                var step = (offset + 1 < text.Length && char.IsHighSurrogate(text[offset]) && char.IsLowSurrogate(text[offset + 1])) ? 2 : 1;
                active = new DocumentPosition(frag, offset + step);
            }
            else if (frag + 1 < _plan.Fragments.Count)
            {
                active = new DocumentPosition(frag + 1, 0);
            }
        }

        SetSelection(new DocumentSelection(anchor, active));
        ScrollToFragment(active.FragmentIndex);
    }

    public void ExtendSelectionByFragment(int delta)
    {
        if (_plan is null || _plan.Fragments.Count == 0) return;
        var anchor = Selection?.Anchor ?? CurrentCaretPosition();
        var active = Selection?.Active ?? anchor;

        var targetFrag = Math.Clamp(active.FragmentIndex + delta, 0, _plan.Fragments.Count - 1);
        var targetText = _plan.Fragments[targetFrag].Text;
        var targetOffset = Math.Min(active.CharOffset, targetText.Length);
        if (targetOffset > 0 && targetOffset < targetText.Length && char.IsLowSurrogate(targetText[targetOffset]))
        {
            targetOffset--;
        }

        active = new DocumentPosition(targetFrag, targetOffset);
        SetSelection(new DocumentSelection(anchor, active));
        ScrollToFragment(targetFrag);
    }

    public void ExtendSelectionToFragmentEdge(bool toEnd)
    {
        if (_plan is null || _plan.Fragments.Count == 0) return;
        var anchor = Selection?.Anchor ?? CurrentCaretPosition();
        var active = Selection?.Active ?? anchor;
        var frag = Math.Clamp(active.FragmentIndex, 0, _plan.Fragments.Count - 1);
        var offset = toEnd ? _plan.Fragments[frag].Text.Length : 0;
        SetSelection(new DocumentSelection(anchor, new DocumentPosition(frag, offset)));
    }

    public void ExtendSelectionToEdge(bool toEnd)
    {
        if (_plan is null || _plan.Fragments.Count == 0) return;
        var anchor = Selection?.Anchor ?? CurrentCaretPosition();
        DocumentPosition active;
        if (toEnd)
        {
            var lastFrag = _plan.Fragments.Count - 1;
            active = new DocumentPosition(lastFrag, _plan.Fragments[lastFrag].Text.Length);
        }
        else
        {
            active = new DocumentPosition(0, 0);
        }

        SetSelection(new DocumentSelection(anchor, active));
        ScrollToFragment(active.FragmentIndex);
    }

    private DocumentPosition CurrentCaretPosition()
    {
        var focused = Microsoft.UI.Xaml.Input.FocusManager.GetFocusedElement(XamlRoot) as RowItem;
        var idx = focused?.ItemIndex ?? FirstVisibleFragmentIndex();
        idx = Math.Clamp(idx, 0, Math.Max(0, (_plan?.Fragments.Count ?? 1) - 1));
        return new DocumentPosition(idx, 0);
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
