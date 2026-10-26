// FEAS-008 C# WinUI document surface: a ListView over display fragments of
// the Rust block model. Only containers near the viewport exist. Selection,
// copy, find, outline and link jumps, and image loading all work on the model,
// so they reach rows that were never realized.
using System.Runtime.InteropServices.WindowsRuntime;
using System.Text;
using Microsoft.UI;
using Microsoft.UI.Dispatching;
using Microsoft.UI.Input;
using Microsoft.UI.Text;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Documents;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Media.Imaging;
using Windows.ApplicationModel.DataTransfer;
using Windows.Foundation;
using Windows.Storage.Streams;
using Windows.System;
using Windows.UI.Core;

namespace Feas008;

/// Count-only items source. The list asks for indexes; nothing per row is
/// allocated until a container is realized.
public sealed partial class IndexList(int count) : System.Collections.IList
{
    public object? this[int index] { get => index; set => throw new NotSupportedException(); }
    public int Count => count;
    public bool IsFixedSize => true;
    public bool IsReadOnly => true;
    public bool IsSynchronized => false;
    public object SyncRoot => this;
    public int Add(object? value) => throw new NotSupportedException();
    public void Clear() => throw new NotSupportedException();
    public bool Contains(object? value) => value is int i && i >= 0 && i < count;
    public int IndexOf(object? value) => value is int i && i >= 0 && i < count ? i : -1;
    public void Insert(int index, object? value) => throw new NotSupportedException();
    public void Remove(object? value) => throw new NotSupportedException();
    public void RemoveAt(int index) => throw new NotSupportedException();
    public void CopyTo(Array array, int index) { for (var i = 0; i < count; i++) array.SetValue(i, index + i); }
    public System.Collections.IEnumerator GetEnumerator() { for (var i = 0; i < count; i++) yield return i; }
}

public readonly record struct Segment(Run Element, uint ByteStart, string Text, int CharStart);

/// A recycled row container. It owns its visuals so the list never applies
/// a data template or binding.
public sealed partial class RowItem : ListViewItem
{
    public readonly Grid Root = new();
    public readonly RichTextBlock Text = new() { TextWrapping = TextWrapping.Wrap, IsTextSelectionEnabled = false };
    public readonly Image Picture = new() { Stretch = Stretch.None, HorizontalAlignment = HorizontalAlignment.Left, Visibility = Visibility.Collapsed };
    public readonly TextBlock Note = new() { FontSize = 12, Opacity = 0.7, Visibility = Visibility.Collapsed };
    public readonly List<Segment> Segments = [];
    public int Item = -1;
    public string Shown = "";
    public string? ImagePath;

    public RowItem()
    {
        MinHeight = 0;
        Padding = new Thickness(0);
        Margin = new Thickness(0);
        CornerRadius = new CornerRadius(0);
        HorizontalContentAlignment = HorizontalAlignment.Stretch;
        Root.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        Root.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        Root.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        Grid.SetRow(Text, 1);
        Grid.SetRow(Note, 2);
        Root.Children.Add(Picture);
        Root.Children.Add(Text);
        Root.Children.Add(Note);
    }

    /// Moves the row visuals into the container's template root. The list
    /// applies the template once per container and may prepare recycled
    /// containers itself, so the visuals must not be the container content.
    public void Attach()
    {
        if (DocumentWindow.TryAs<Grid>(ContentTemplateRoot) is not { } host || ReferenceEquals(Root.Parent, host)) return;
        if (Root.Parent is Panel old) old.Children.Remove(Root);
        host.Children.Clear();
        host.Children.Add(Root);
    }
}

sealed class ImageState
{
    public BitmapImage? Bitmap;
    public string? Failure;
    public bool Done => Bitmap is not null || Failure is not null;
}

public sealed partial class DocumentWindow : Window
{
    static readonly FontFamily Mono = new("Cascadia Mono, Consolas");
    static readonly double[] HeadingSizes = [28, 24, 20, 18, 16, 15];
    readonly Config config;
    readonly string folder;
    readonly Grid root = new();
    readonly ListView list = new() { SelectionMode = ListViewSelectionMode.None, IsItemClickEnabled = false };
    readonly ListView outline = new() { SelectionMode = ListViewSelectionMode.Single };
    readonly TextBox search = new() { PlaceholderText = "Find (Ctrl+F, Enter, F3)", Width = 300 };
    readonly TextBlock status = new() { Margin = new Thickness(12, 0, 0, 0), VerticalAlignment = VerticalAlignment.Center };
    readonly HashSet<RowItem> bound = [];
    readonly Dictionary<string, ImageState> images = [];
    readonly List<string> decisions = [];
    readonly SolidColorBrush highlight = new(Windows.UI.Color.FromArgb(0xff, 0x99, 0xc9, 0xff));
    readonly SolidColorBrush quoteBrush = new(Colors.Gray);
    readonly SolidColorBrush codeBrush = new(Windows.UI.Color.FromArgb(0x18, 0x80, 0x80, 0x80));
    readonly Microsoft.UI.Dispatching.DispatcherQueueTimer autoScroll;
    DocModel model = null!;
    Loaded loaded = null!;
    ScrollViewer scroll = null!;
    (Pos Anchor, Pos Active)? selection;
    Pos? dragAnchor;
    bool dragging;
    Point lastPointer;
    F8Match[] matches = [];
    string lastQuery = "";
    int matchIndex = -1;
    bool suppressOutline;

    #if FEAS008_AOT
    public const string Variant = "C# Native AOT";
#else
    public const string Variant = "C# JIT";
#endif

    public DocumentWindow(Config config)
    {
        this.config = config;
        folder = Path.GetDirectoryName(config.Input)!;
        Title = $"FEAS-008 {Variant} — {Path.GetFileName(config.Input)}";
        AppWindow.Resize(new Windows.Graphics.SizeInt32(1200, 900));
        root.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        root.RowDefinitions.Add(new RowDefinition { Height = new GridLength(1, GridUnitType.Star) });
        root.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(260) });
        root.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        var bar = new StackPanel { Orientation = Orientation.Horizontal, Margin = new Thickness(8) };
        bar.Children.Add(search);
        bar.Children.Add(status);
        Grid.SetColumnSpan(bar, 2);
        root.Children.Add(bar);
        Grid.SetRow(outline, 1);
        Grid.SetRow(list, 1);
        Grid.SetColumn(list, 1);
        root.Children.Add(outline);
        root.Children.Add(list);
        AutomationProperties.SetName(list, "Document");
        AutomationProperties.SetName(outline, "Outline");
        AutomationProperties.SetName(search, "Find");
        Content = root;

        list.ChoosingItemContainer += (_, args) => args.ItemContainer ??= new RowItem();
        list.ItemTemplate = (DataTemplate)Microsoft.UI.Xaml.Markup.XamlReader.Load("<DataTemplate xmlns='http://schemas.microsoft.com/winfx/2006/xaml/presentation'><Grid/></DataTemplate>");
        list.ContainerContentChanging += OnContainerContentChanging;
        list.AddHandler(UIElement.PointerPressedEvent, new PointerEventHandler(OnPointerPressed), true);
        list.AddHandler(UIElement.PointerMovedEvent, new PointerEventHandler(OnPointerMoved), true);
        list.AddHandler(UIElement.PointerReleasedEvent, new PointerEventHandler(OnPointerReleased), true);
        list.PointerCaptureLost += (_, _) => EndDrag();
        root.PreviewKeyDown += OnPreviewKeyDown;
        search.KeyDown += (_, e) => { if (e.Key == VirtualKey.Enter) { FindNext(!Down(VirtualKey.Shift)); e.Handled = true; } };
        outline.SelectionChanged += (_, _) => { if (!suppressOutline && outline.SelectedIndex >= 0) _ = JumpToBlock(model.Outline(outline.SelectedIndex).Block, false); };
        // Preview: the list consumes Enter before KeyDown.
        outline.PreviewKeyDown += (_, e) => { if (e.Key == VirtualKey.Enter && outline.SelectedIndex >= 0) { _ = JumpToBlock(model.Outline(outline.SelectedIndex).Block, true); e.Handled = true; } };
        outline.DoubleTapped += (_, _) => { if (outline.SelectedIndex >= 0) _ = JumpToBlock(model.Outline(outline.SelectedIndex).Block, true); };
        autoScroll = DispatcherQueue.CreateTimer();
        autoScroll.Interval = TimeSpan.FromMilliseconds(16);
        autoScroll.Tick += (_, _) => AutoScrollTick();
        root.Loaded += OnLoaded;
    }

    static bool Down(VirtualKey key) => InputKeyboardSource.GetKeyStateForCurrentThread(key).HasFlag(CoreVirtualKeyStates.Down);

    async void OnLoaded(object sender, RoutedEventArgs e)
    {
        try { await Load(); }
        catch (Exception error) { Fail(error); }
    }

    /// An unattended run must end with a report, never a hung window.
    public void Fail(Exception error)
    {
        var text = $"{{\"host\":\"{Variant}\",\"error\":{Events.Quote(error.ToString())}}}";
        Events.Emit($"{{\"event\":\"error\",\"error\":{Events.Quote(error.ToString())}}}");
        if (config.Report is { } path) File.WriteAllText(path, text);
        if (config.Automated) Application.Current.Exit();
    }

    async Task Load()
    {
        var windowAt = Clock.Now;
        loaded = await Program.Loading;
        model = loaded.Model;
        var titles = new List<string>(model.OutlineCount);
        for (var i = 0; i < model.OutlineCount; i++)
        {
            var entry = model.Outline(i);
            titles.Add(new string(' ', (int)(entry.Level - 1) * 3) + DocModel.Text(entry.Title));
        }
        outline.ItemsSource = titles;
        list.ItemsSource = new IndexList(model.ItemCount);
        list.UpdateLayout();
        scroll = FindScrollViewer(list) ?? throw new InvalidOperationException("ListView has no ScrollViewer");
        var (paint, presented) = await Presented(() => bound.Any(r => r.Item >= 0 && r.Text.ActualWidth > 0));
        var launch = new Launch(windowAt, paint, presented);
        Events.Emit($"{{\"event\":\"presented\",\"launch_to_presented_ms\":{Clock.SinceLaunch(presented ?? paint):F3}}}");
        if (config.Automated) await RunAutomated(launch);
        else if (config.Control) _ = Task.Run(ControlLoop);
    }

    public readonly record struct Launch(long WindowAt, long Paint, long? Presented);

    public static T? TryAs<T>(object? value) where T : class
    {
        if (value is null) return null;
        if (value is T typed) return typed;
        try { return WinRT.CastExtensions.As<T>(value); }
        catch (InvalidCastException) { return null; }
    }

    static ScrollViewer? FindScrollViewer(DependencyObject node)
    {
        for (var i = 0; i < VisualTreeHelper.GetChildrenCount(node); i++)
        {
            var child = VisualTreeHelper.GetChild(node, i);
            // Native AOT may wrap template parts as plain DependencyObject, so
            // `is ScrollViewer` fails there; QueryInterface does not.
            if (TryAs<ScrollViewer>(child) is { } viewer) return viewer;
            if (FindScrollViewer(child) is { } found) return found;
        }
        return null;
    }

    // ---- Row binding ----

    void OnContainerContentChanging(ListViewBase sender, ContainerContentChangingEventArgs args)
    {
        var row = (RowItem)args.ItemContainer;
        if (args.InRecycleQueue) { bound.Remove(row); row.Item = -1; }
        else
        {
            row.Attach();
            Bind(row, args.ItemIndex);
            bound.Add(row);
        }
        args.Handled = true;
    }

    void Bind(RowItem row, int index)
    {
        var item = model.Item(index);
        ref readonly var block = ref model.Block(item.Block);
        var kind = (Kind)block.Kind;
        row.Item = index;
        row.Segments.Clear();
        var paragraph = new Paragraph();
        var bytes = DocModel.Bytes(block.Text);
        var shown = new StringBuilder();
        foreach (var run in model.Runs(block))
        {
            var s = Math.Max(run.Start, item.Start);
            var e = Math.Min(run.End, item.End);
            if (s >= e) continue;
            var text = Encoding.UTF8.GetString(bytes[(int)s..(int)e]);
            var element = new Run { Text = text };
            if ((run.Style & Style.Bold) != 0) element.FontWeight = FontWeights.Bold;
            if ((run.Style & Style.Italic) != 0) element.FontStyle = Windows.UI.Text.FontStyle.Italic;
            if ((run.Style & Style.Code) != 0) element.FontFamily = Mono;
            if ((run.Style & Style.Strike) != 0) element.TextDecorations = Windows.UI.Text.TextDecorations.Strikethrough;
            if ((run.Style & Style.Marker) != 0) element.Foreground = quoteBrush;
            if (run.Link != DocModel.None)
            {
                var link = run.Link;
                var hyperlink = new Hyperlink();
                hyperlink.Inlines.Add(element);
                hyperlink.Click += (_, _) => ActivateLink(link);
                paragraph.Inlines.Add(hyperlink);
            }
            else paragraph.Inlines.Add(element);
            row.Segments.Add(new Segment(element, s, text, shown.Length));
            shown.Append(text);
        }
        row.Shown = shown.ToString();
        row.Text.Blocks.Clear();
        row.Text.Blocks.Add(paragraph);
        var heading = kind == Kind.Heading;
        row.Text.FontSize = heading ? HeadingSizes[Math.Clamp((int)block.Level - 1, 0, 5)] : 15;
        row.Text.FontWeight = heading ? FontWeights.SemiBold : FontWeights.Normal;
        var mono = kind is Kind.Code or Kind.Math or Kind.Html;
        row.Text.FontFamily = mono ? Mono : FontFamily.XamlAutoFontFamily;
        row.Root.Background = mono ? codeBrush : null;
        var first = item.Start == 0;
        var last = item.End == block.Text.Len;
        row.Root.Padding = new Thickness(16 + 24 * block.Indent, first ? (heading ? 14 : 6) : 0, 16, last ? 6 : 0);
        row.Root.BorderThickness = new Thickness(block.Quote != 0 ? 3 : 0, 0, 0, 0);
        row.Root.BorderBrush = quoteBrush;
        AutomationProperties.SetName(row, row.Shown);
        AutomationProperties.SetHeadingLevel(row, heading && first ? (Microsoft.UI.Xaml.Automation.Peers.AutomationHeadingLevel)block.Level : Microsoft.UI.Xaml.Automation.Peers.AutomationHeadingLevel.None);
        AutomationProperties.SetItemType(row, kind.ToString().ToLowerInvariant());
        // The list item's own heading level is not exposed through UIA; its
        // text element's is, so Narrator heading navigation can find it.
        AutomationProperties.SetHeadingLevel(row.Text, AutomationProperties.GetHeadingLevel(row));
        row.ImagePath = null;
        row.Picture.Source = null;
        row.Picture.Visibility = Visibility.Collapsed;
        row.Note.Visibility = Visibility.Collapsed;
        if (kind == Kind.Image)
        {
            if ((F8Image)block.ImageKind == F8Image.Local)
            {
                row.ImagePath = DocModel.Text(block.Image);
                RequestImage(row.ImagePath);
            }
            else ShowNote(row, $"Image blocked: {DocModel.Text(block.Image)}");
            UpdatePicture(row);
        }
        ApplyHighlight(row);
    }

    enum F8Image : uint { None, Local, Blocked }

    static void ShowNote(RowItem row, string text)
    {
        row.Note.Text = text;
        row.Note.Visibility = Visibility.Visible;
        AutomationProperties.SetHelpText(row, text);
    }

    void ApplyHighlight(RowItem row)
    {
        row.Text.TextHighlighters.Clear();
        if (selection is not { } sel || row.Item < 0) return;
        var (a, b) = Ordered(sel.Anchor, sel.Active);
        var item = model.Item(row.Item);
        var from = new Pos(item.Block, item.Start);
        var to = new Pos(item.Block, item.End);
        if (b <= from || a >= to) return;
        var s = a > from ? a.Offset : item.Start;
        var e = b < to ? b.Offset : item.End;
        if (s >= e) return;
        var start = Encoding.UTF8.GetCharCount(model.BlockBytes(item.Block, item.Start, s));
        var length = Encoding.UTF8.GetCharCount(model.BlockBytes(item.Block, s, e));
        var highlighter = new TextHighlighter { Background = highlight };
        highlighter.Ranges.Add(new TextRange { StartIndex = start, Length = length });
        row.Text.TextHighlighters.Add(highlighter);
    }

    static (Pos, Pos) Ordered(Pos a, Pos b) => a <= b ? (a, b) : (b, a);

    void SetSelection((Pos, Pos)? value)
    {
        selection = value;
        foreach (var row in bound) ApplyHighlight(row);
    }

    // ---- Images ----

    void RequestImage(string relative)
    {
        if (images.ContainsKey(relative)) return;
        var state = images[relative] = new ImageState();
        var work = Task.Run(() => ImagePolicy.Read(folder, relative));
        work.ContinueWith(async done =>
        {
            if (done.Exception is { } failure) state.Failure = failure.InnerException?.Message ?? failure.Message;
            else
            {
                try
                {
                    var bitmap = new BitmapImage();
                    using var stream = new InMemoryRandomAccessStream();
                    await stream.WriteAsync(done.Result.AsBuffer());
                    stream.Seek(0);
                    await bitmap.SetSourceAsync(stream);
                    if (bitmap.PixelWidth > ImagePolicy.MaxSide || bitmap.PixelHeight > ImagePolicy.MaxSide) state.Failure = "decoded image exceeds the dimension limit";
                    else state.Bitmap = bitmap;
                }
                catch (Exception error) { state.Failure = error.Message; }
            }
            foreach (var row in bound) if (row.ImagePath == relative) UpdatePicture(row);
        }, TaskScheduler.FromCurrentSynchronizationContext());
    }

    void UpdatePicture(RowItem row)
    {
        if (row.ImagePath is not { } path || !images.TryGetValue(path, out var state)) return;
        if (state.Bitmap is { } bitmap)
        {
            row.Picture.Source = bitmap;
            row.Picture.Visibility = Visibility.Visible;
            AutomationProperties.SetName(row.Picture, row.Shown);
        }
        else if (state.Failure is { } failure) ShowNote(row, $"Image refused: {failure}");
    }

    // ---- Hit testing and pointer selection ----

    Pos? HitTest(Point point)
    {
        RowItem? best = null;
        var bestDistance = double.MaxValue;
        foreach (var row in bound)
        {
            if (row.Item < 0 || row.ActualHeight <= 0) continue;
            var top = row.TransformToVisual(list).TransformPoint(default).Y;
            var distance = point.Y < top ? top - point.Y : point.Y > top + row.ActualHeight ? point.Y - top - row.ActualHeight : 0;
            if (distance < bestDistance) { best = row; bestDistance = distance; }
        }
        return best is null ? null : PositionInRow(best, list.TransformToVisual(best.Text).TransformPoint(point));
    }

    Pos PositionInRow(RowItem row, Point local)
    {
        var item = model.Item(row.Item);
        if (row.Segments.Count == 0) return new Pos(item.Block, item.Start);
        if (local.Y < 0) return new Pos(item.Block, item.Start);
        if (local.Y >= row.Text.ActualHeight) return new Pos(item.Block, item.End);
        var x = Math.Clamp(local.X, 0, Math.Max(0, row.Text.ActualWidth - 1));
        var pointer = row.Text.GetPositionFromPoint(new Point(x, local.Y));
        var offset = pointer.Offset;
        var chosen = row.Segments[0];
        var start = chosen.Element.ContentStart.Offset;
        foreach (var segment in row.Segments)
        {
            var at = segment.Element.ContentStart.Offset;
            if (at > offset) break;
            chosen = segment;
            start = at;
        }
        var chars = Math.Clamp(offset - start, 0, chosen.Text.Length);
        if (chars > 0 && chars < chosen.Text.Length && char.IsLowSurrogate(chosen.Text[chars])) chars--;
        return new Pos(item.Block, chosen.ByteStart + (uint)Encoding.UTF8.GetByteCount(chosen.Text.AsSpan(0, chars)));
    }

    void OnPointerPressed(object sender, PointerRoutedEventArgs e)
    {
        var point = e.GetCurrentPoint(list);
        if (!point.Properties.IsLeftButtonPressed || model is null) return;
        if (HitTest(point.Position) is not { } hit) return;
        if (Down(VirtualKey.Shift) && selection is { } sel) { dragAnchor = sel.Anchor; SetSelection((sel.Anchor, hit)); }
        else { dragAnchor = hit; SetSelection(null); }
        dragging = true;
        lastPointer = point.Position;
        list.CapturePointer(e.Pointer);
        // A press moves keyboard focus to the row, so Esc, Ctrl+A/C and the
        // arrow keys act on the document after a click.
        if (list.ContainerFromIndex(model.ItemFor(hit)) is RowItem row) row.Focus(FocusState.Pointer);
    }

    void OnPointerMoved(object sender, PointerRoutedEventArgs e)
    {
        if (!dragging) return;
        lastPointer = e.GetCurrentPoint(list).Position;
        UpdateDrag();
        if (lastPointer.Y < 0 || lastPointer.Y > list.ActualHeight) autoScroll.Start(); else autoScroll.Stop();
    }

    void OnPointerReleased(object sender, PointerRoutedEventArgs e)
    {
        if (!dragging) return;
        list.ReleasePointerCapture(e.Pointer);
        EndDrag();
    }

    void EndDrag() { dragging = false; autoScroll.Stop(); }

    void UpdateDrag()
    {
        if (dragAnchor is not { } anchor) return;
        var clamped = new Point(lastPointer.X, Math.Clamp(lastPointer.Y, 0, list.ActualHeight - 1));
        if (HitTest(clamped) is not { } hit) return;
        // Keep the anchor even when the range is momentarily empty.
        SetSelection(hit == anchor ? null : (anchor, hit));
    }

    void AutoScrollTick()
    {
        if (!dragging) { autoScroll.Stop(); return; }
        var delta = lastPointer.Y < 0 ? lastPointer.Y : lastPointer.Y - list.ActualHeight;
        scroll.ChangeView(null, Math.Clamp(scroll.VerticalOffset + Math.Clamp(delta, -60, 60), 0, scroll.ScrollableHeight), null, true);
        list.UpdateLayout();
        UpdateDrag();
    }

    // ---- Keyboard ----

    bool SearchFocused() => ReferenceEquals(FocusManager.GetFocusedElement(root.XamlRoot), search);

    bool FocusInDocument()
    {
        var focused = FocusManager.GetFocusedElement(root.XamlRoot) as DependencyObject;
        for (var node = focused; node is not null; node = VisualTreeHelper.GetParent(node))
        {
            if (ReferenceEquals(node, list)) return true;
            if (ReferenceEquals(node, search) || ReferenceEquals(node, outline)) return false;
        }
        return false;
    }

    void OnPreviewKeyDown(object sender, KeyRoutedEventArgs e)
    {
        var ctrl = Down(VirtualKey.Control);
        var shift = Down(VirtualKey.Shift);
        if (ctrl && e.Key == VirtualKey.F) { search.Focus(FocusState.Keyboard); search.SelectAll(); e.Handled = true; return; }
        if (e.Key == VirtualKey.F3) { FindNext(!shift); e.Handled = true; return; }
        if (e.Key == VirtualKey.Escape && selection is not null && !SearchFocused()) { SetSelection(null); e.Handled = true; return; }
        if (!FocusInDocument()) return;
        if (ctrl && e.Key == VirtualKey.A) { SelectAll(); e.Handled = true; }
        else if (ctrl && e.Key == VirtualKey.C) { Copy(); e.Handled = true; }
        else if (shift && e.Key is VirtualKey.Left or VirtualKey.Right or VirtualKey.Up or VirtualKey.Down
                 || shift && ctrl && e.Key is VirtualKey.Home or VirtualKey.End)
        {
            ExtendSelection(e.Key, ctrl);
            e.Handled = true;
        }
    }

    Pos Caret()
    {
        var focused = FocusManager.GetFocusedElement(root.XamlRoot) as RowItem;
        var index = focused?.Item ?? -1;
        if (index < 0) index = bound.Where(r => r.Item >= 0).Select(r => r.Item).DefaultIfEmpty(0).Min();
        var item = model.Item(index);
        return new Pos(item.Block, item.Start);
    }

    void ExtendSelection(VirtualKey key, bool ctrl)
    {
        var anchor = selection?.Anchor ?? Caret();
        var active = selection?.Active ?? anchor;
        active = key switch
        {
            VirtualKey.Home when ctrl => model.Start,
            VirtualKey.End when ctrl => model.End,
            VirtualKey.Left => PreviousChar(active),
            VirtualKey.Right => NextChar(active),
            VirtualKey.Up => ByFragment(active, -1),
            _ => ByFragment(active, 1),
        };
        SetSelection((anchor, active));
        _ = JumpTo(model.ItemFor(active), true, false);
    }

    Pos PreviousChar(Pos p)
    {
        if (p.Offset > 0) return p with { Offset = model.Boundary(p.Block, p.Offset - 1) };
        return p.Block == 0 ? p : new Pos(p.Block - 1, (uint)model.Block(p.Block - 1).Text.Len);
    }

    Pos NextChar(Pos p)
    {
        var length = (uint)model.Block(p.Block).Text.Len;
        if (p.Offset < length)
        {
            var bytes = DocModel.Bytes(model.Block(p.Block).Text);
            var next = p.Offset + 1;
            while (next < length && (bytes[(int)next] & 0xC0) == 0x80) next++;
            return p with { Offset = next };
        }
        return p.Block + 1 >= model.BlockCount ? p : new Pos(p.Block + 1, 0);
    }

    Pos ByFragment(Pos p, int step)
    {
        var index = model.ItemFor(p);
        var current = model.Item(index);
        var target = Math.Clamp(index + step, 0, model.ItemCount - 1);
        if (target == index) return step < 0 ? new Pos(current.Block, current.Start) : new Pos(current.Block, current.End);
        var item = model.Item(target);
        var offset = Math.Min(item.Start + (p.Offset - current.Start), item.End);
        return new Pos(item.Block, model.Boundary(item.Block, offset));
    }

    public void SelectAll() => SetSelection((model.Start, model.End));

    /// Rendered text as text/plain and the covered Markdown as text/markdown.
    public (string Plain, long MarkdownBytes)? Copy()
    {
        if (selection is not { } sel) return null;
        var (a, b) = Ordered(sel.Anchor, sel.Active);
        var plain = model.TextBetween(a, b);
        var markdown = model.SourceBetween(a, b);
        NativeClipboard.Set(WinRT.Interop.WindowNative.GetWindowHandle(this), plain, markdown);
        return (plain, markdown.Length);
    }

    // ---- Find, outline and links ----

    public (int Count, double Ms) RunFind(string query)
    {
        var started = Clock.Now;
        matches = model.Find(query);
        lastQuery = query;
        matchIndex = -1;
        return (matches.Length, Clock.Ms(started, Clock.Now));
    }

    public F8Match? FindNext(bool forward, bool jump = true)
    {
        if (search.Text != lastQuery) RunFind(search.Text);
        if (matches.Length == 0) { status.Text = lastQuery.Length == 0 ? "" : "No matches"; return null; }
        matchIndex = matchIndex < 0 ? (forward ? 0 : matches.Length - 1) : (matchIndex + (forward ? 1 : matches.Length - 1)) % matches.Length;
        var m = matches[matchIndex];
        SetSelection((new Pos(m.Block, m.Start), new Pos(m.Block, m.End)));
        status.Text = $"{matchIndex + 1} of {matches.Length}";
        if (jump) _ = JumpTo(model.ItemFor(new Pos(m.Block, m.Start)), false, false);
        return m;
    }

    public void SetMatchIndex(int index) => matchIndex = index;
    public F8Match[] Matches => matches;
    public void SetQuery(string text) => search.Text = text;

    public Task<RowItem?> JumpToBlock(uint block, bool focus)
    {
        var index = Array.FindIndex(Enumerable.Range(0, model.OutlineCount).ToArray(), i => model.Outline(i).Block == block);
        status.Text = index >= 0 ? $"Heading {index + 1} of {model.OutlineCount}, item {model.FirstItem(block) + 1} of {model.ItemCount}" : $"Item {model.FirstItem(block) + 1} of {model.ItemCount}";
        if (index >= 0 && outline.SelectedIndex != index) { suppressOutline = true; outline.SelectedIndex = index; suppressOutline = false; }
        return JumpTo((int)model.FirstItem(block), focus, true);
    }

    /// Scroll an item into view; with `align`, put its top at the viewport top
    /// unless the document end prevents it.
    public async Task<RowItem?> JumpTo(int item, bool focus, bool align)
    {
        var boxed = (object)item;
        list.ScrollIntoView(boxed, align ? ScrollIntoViewAlignment.Leading : ScrollIntoViewAlignment.Default);
        RowItem? row = null;
        for (var attempt = 0; attempt < 12; attempt++)
        {
            await NextFrame();
            row = list.ContainerFromIndex(item) as RowItem;
            if (row is null || row.Item != item) { list.ScrollIntoView(boxed, align ? ScrollIntoViewAlignment.Leading : ScrollIntoViewAlignment.Default); continue; }
            var top = row.TransformToVisual(scroll).TransformPoint(default).Y;
            if (align && Math.Abs(top) > 0.5 && !(top > 0 && scroll.VerticalOffset >= scroll.ScrollableHeight - 0.5))
            {
                scroll.ChangeView(null, scroll.VerticalOffset + top, null, true);
                continue;
            }
            // Realized but outside the viewport: correct by the measured offset
            // rather than asking the list to estimate again.
            if (!align && top < -0.5) { scroll.ChangeView(null, scroll.VerticalOffset + top, null, true); continue; }
            var overflow = top + Math.Min(row.ActualHeight, scroll.ViewportHeight) - scroll.ViewportHeight;
            if (!align && overflow > 0.5) { scroll.ChangeView(null, scroll.VerticalOffset + overflow, null, true); continue; }
            break;
        }
        if (focus && row is not null) row.Focus(FocusState.Programmatic);
        return row;
    }

    public void ActivateLink(uint index)
    {
        var link = model.Link(index);
        var intent = (Intent)link.Intent;
        var dest = DocModel.Text(link.Dest);
        var detail = DocModel.Text(link.Detail);
        decisions.Add($"{{\"dest\":{Events.Quote(dest)},\"intent\":\"{intent}\",\"detail\":{Events.Quote(detail)}}}");
        Events.Emit($"{{\"event\":\"link\",\"dest\":{Events.Quote(dest)},\"intent\":\"{intent}\"}}");
        switch (intent)
        {
            case Intent.Fragment: _ = JumpToBlock(link.TargetBlock, true); break;
            case Intent.External when !config.Automated: _ = Launcher.LaunchUriAsync(new Uri(dest)); break;
            case Intent.External: status.Text = $"Would open {dest}"; break;
            case Intent.LocalDocument: status.Text = $"Local document {detail} (opening is not implemented in the spike)"; break;
            default: status.Text = $"Link not followed: {intent} {detail}"; break;
        }
    }

    // ---- Frames ----

    public Task NextFrame()
    {
        var done = new TaskCompletionSource();
        EventHandler<object>? handler = null;
        handler = (_, _) => { CompositionTarget.Rendering -= handler; done.TrySetResult(); };
        CompositionTarget.Rendering += handler;
        return done.Task;
    }

    /// Waits for the first frame where `condition` holds, then for the DWM
    /// present after the UI thread has committed it.
    public int TimedOut;

    public Task<(long Paint, long? Presented)> Presented(Func<bool> condition)
    {
        var done = new TaskCompletionSource<(long, long?)>();
        EventHandler<object>? handler = null;
        var deadline = Clock.Now + 5 * System.Diagnostics.Stopwatch.Frequency;
        handler = (_, _) =>
        {
            if (Clock.Now > deadline)
            {
                // Report the miss as a timeout instead of hanging the run.
                CompositionTarget.Rendering -= handler;
                TimedOut++;
                done.TrySetResult((Clock.Now, null));
                return;
            }
            if (!condition()) return;
            CompositionTarget.Rendering -= handler;
            var paint = Clock.Now;
            DispatcherQueue.TryEnqueue(Microsoft.UI.Dispatching.DispatcherQueuePriority.Low, async () => done.TrySetResult((paint, await Clock.AfterNextPresent())));
        };
        CompositionTarget.Rendering += handler;
        return done.Task;
    }

    public RowItem? VisibleRow(int item)
    {
        if (list.ContainerFromIndex(item) is not RowItem row || row.Item != item || row.ActualHeight <= 0 || (row.Shown.Length > 0 && row.Text.ActualWidth <= 0)) return null;
        var top = row.TransformToVisual(scroll).TransformPoint(default).Y;
        return top + row.ActualHeight > 0 && top < scroll.ViewportHeight ? row : null;
    }

    public async Task<(double Ms, RowItem? Row)> JumpPresented(int item, bool focus, bool align = false)
    {
        var requested = Clock.Now;
        await JumpTo(item, focus, align);
        var (paint, presented) = await Presented(() => VisibleRow(item) is not null);
        return (Clock.Ms(requested, presented ?? paint), VisibleRow(item));
    }
}
