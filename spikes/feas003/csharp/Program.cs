using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
#if !FEAS006
using Markdig;
using Markdig.Syntax;
using Markdig.Syntax.Inlines;
#endif
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Automation.Peers;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Documents;
using Microsoft.UI.Xaml.Markup;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Text;

namespace WindowsRenderingProbe;

static class Program
{
    public static readonly Stopwatch Clock = Stopwatch.StartNew();
    public static string[] Args = [];
    [STAThread]
    static void Main(string[] args)
    {
        Args = args;
        WinRT.ComWrappersSupport.InitializeComWrappers();
        Application.Start(initialization => {
            SynchronizationContext.SetSynchronizationContext(
                new Microsoft.UI.Dispatching.DispatcherQueueSynchronizationContext(
                    Microsoft.UI.Dispatching.DispatcherQueue.GetForCurrentThread()));
            _ = new ProbeApp();
        });
    }
}

public sealed record DocBlock(string Text, int Heading = 0, bool Code = false)
{
    public double Size => Heading > 0 ? 26 - Math.Min(Heading, 4) * 2 : 16;
}

public sealed partial class ProbeApp : Application
{
    Window? window;
    public ProbeApp() => InitializeComponent();
    protected override void OnLaunched(LaunchActivatedEventArgs args)
    {
        window = new ProbeWindow();
        window.Activate();
    }
}

public sealed class ProbeWindow : Window
{
    readonly string mode;
    readonly string input;
    readonly string? output;
    readonly Dictionary<string, object?> metrics = new();
    readonly List<DocBlock> blocks = [];
    readonly Grid root = new();
    readonly TextBlock status = new() { Text = "Loading probe", Margin = new Thickness(12) };
    ScrollViewer? scroll;
    RichTextBlock? rich;
    RichEditBox? edit;
    ListView? list;
    FrameworkElement? surface;
    string plain = "";
    long rendered;
    double firstLoadedMs;
#if FEAS006
    Feas006.NativeCancellation? activeCoreCancellation;
#endif

    public ProbeWindow()
    {
        var args = Program.Args;
        mode = args.ElementAtOrDefault(0) ?? "richtext";
        input = Path.GetFullPath(args.ElementAtOrDefault(1) ?? "fixtures/markdown/foundation/gfm-tour.md");
        output = args.ElementAtOrDefault(2);
        Title = $"FEAS-003 {mode} — {Path.GetFileName(input)}";
#if FEAS006
        Title = $"FEAS-006 Rust/{mode} — {Path.GetFileName(input)}";
#endif
        AppWindow.Resize(new Windows.Graphics.SizeInt32(1100, 850));
        metrics["host"] = "C# WinUI";
        metrics["surface"] = mode;
        metrics["input_name"] = Path.GetFileName(input);
        metrics["runtime"] = RuntimeInformation.FrameworkDescription;
        metrics["os"] = Environment.OSVersion.ToString();
        metrics["entry_to_window_ctor_ms"] = Program.Clock.Elapsed.TotalMilliseconds;
        root.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        root.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        root.RowDefinitions.Add(new RowDefinition { Height = new GridLength(1, GridUnitType.Star) });
        root.Children.Add(status);
        var commands = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8, Margin = new Thickness(12, 0, 12, 8) };
        AddButton(commands, "Select all", () => SelectText());
        AddButton(commands, "Go to end", () => GoToEnd());
        AddButton(commands, "Go to top", () => GoToTop());
        AddButton(commands, "Toggle theme", () => root.RequestedTheme = root.ActualTheme == ElementTheme.Dark ? ElementTheme.Light : ElementTheme.Dark);
#if FEAS006
        AddButton(commands, "Cancel parse", () => activeCoreCancellation?.Request());
        Closed += (_, _) => activeCoreCancellation?.Request();
#endif
        Grid.SetRow(commands, 1); root.Children.Add(commands);
        Content = root;
        root.KeyDown += async (_, key) => {
            try {
                if (key.Key == Windows.System.VirtualKey.F8) { await VerifyClipboard(); key.Handled = true; }
                if (key.Key == Windows.System.VirtualKey.F9) { await SweepDisplays(); key.Handled = true; }
            }
            catch (Exception failure) { metrics["manual_check_error"] = failure.ToString(); status.Text = failure.Message; Save(); }
        };
        root.Loaded += OnLoaded;
        Application.Current.UnhandledException += (_, e) => { metrics["error"] = e.Exception.ToString(); Save(); };
    }

    static void AddButton(StackPanel panel, string title, Action action)
    {
        var button = new Button { Content = title };
        button.Click += (_, _) => action();
        panel.Children.Add(button);
    }

    async void OnLoaded(object sender, RoutedEventArgs e)
    {
        root.Loaded -= OnLoaded;
        try
        {
            var timer = Stopwatch.StartNew();
            var bytes = File.ReadAllBytes(input);
#if !FEAS006
            string markdown = Encoding.UTF8.GetString(bytes);
#endif
            metrics["source_bytes"] = bytes.Length;
            metrics["source_sha256"] = Convert.ToHexString(SHA256.HashData(bytes)).ToLowerInvariant();
            metrics["read_ms"] = timer.Elapsed.TotalMilliseconds;
            metrics["phase"] = "parse"; Save();
            timer.Restart();
#if FEAS006
            var uiTicks = 0;
            var heartbeat = DispatcherQueue.CreateTimer();
            heartbeat.Interval = TimeSpan.FromMilliseconds(16);
            heartbeat.Tick += (_, _) => uiTicks++;
            heartbeat.Start();
            Feas006.CoreDocument nativeDocument;
            try
            {
                using var cancellation = new Feas006.NativeCancellation();
                activeCoreCancellation = cancellation;
                if (Program.Args.Contains("--cancel-before-parse")) cancellation.Request();
                nativeDocument = await Task.Run(() => Feas006.Core.ParseAndCopy(bytes, cancellation));
            }
            finally { heartbeat.Stop(); activeCoreCancellation = null; }
            metrics["host"] = "C# WinUI / Rust C ABI v1";
            metrics["core_off_ui_thread"] = true;
            metrics["core_probe_hooks"] = Feas006.Core.Abi.ProbeHooks;
            metrics["ui_heartbeat_ticks_during_core"] = uiTicks;
            metrics["scheduled_core_and_copy_ms"] = timer.Elapsed.TotalMilliseconds;
            metrics["parse_ms"] = nativeDocument.CoreMs;
            metrics["core_parse_model_ms"] = nativeDocument.CoreMs;
            metrics["native_call_ms"] = nativeDocument.NativeCallMs;
            metrics["managed_copy_decode_ms"] = nativeDocument.CopyDecodeMs;
            metrics["native_text_bytes"] = nativeDocument.TextBytes;
            metrics["native_table_bytes"] = nativeDocument.TableBytes;
            timer.Restart();
            blocks.AddRange(nativeDocument.Blocks.Select(b => new DocBlock(b.Text, (int)b.HeadingLevel, b.Kind == 2)));
            plain = string.Join("\n", blocks.Select(b => b.Text));
            metrics["adapter_ms"] = nativeDocument.CopyDecodeMs + timer.Elapsed.TotalMilliseconds;
            if (Feas006.Core.Abi.ProbeHooks != 0)
            {
                Feas006.Core.Ensure(Feas006.NativeApi.ProbeLive(out ulong snapshots, out ulong cancellations));
                metrics["native_snapshots_live_before_ui"] = snapshots;
                metrics["native_tokens_live_before_ui"] = cancellations;
            }
#else
            var document = Markdown.Parse(markdown, new MarkdownPipelineBuilder().UseAdvancedExtensions().Build());
            metrics["parse_ms"] = timer.Elapsed.TotalMilliseconds;
            metrics["phase"] = "adapter"; Save();
            timer.Restart();
            AddBlocks(document);
            plain = string.Join("\n", blocks.Select(b => b.Text));
            metrics["blocks"] = blocks.Count;
            metrics["text_utf16"] = plain.Length;
            metrics["adapter_ms"] = timer.Elapsed.TotalMilliseconds;
#endif
            metrics["blocks"] = blocks.Count;
            metrics["text_utf16"] = plain.Length;
            metrics["phase"] = "surface_setup"; Save();
            timer.Restart();
            MakeSurface();
            Grid.SetRow(surface!, 2); root.Children.Add(surface!);
            AutomationProperties.SetName(surface!, "Markdown document");
            AutomationProperties.SetAutomationId(surface!, "MarkdownDocument");
            metrics["surface_setup_ms"] = timer.Elapsed.TotalMilliseconds;
            metrics["phase"] = "layout"; Save();
            status.Text = $"{mode} | {bytes.Length:N0} bytes | {blocks.Count:N0} blocks | native document probe";
            CompositionTarget.Rendering += OnRendering;
            surface!.Loaded += (_, _) => firstLoadedMs = Program.Clock.Elapsed.TotalMilliseconds;
            await Task.Delay(250);
            metrics["entry_to_surface_loaded_ms"] = firstLoadedMs;
            metrics["entry_to_render_callback_ms"] = rendered == 0 ? null : rendered / 1000.0;
            metrics["dpi_scale"] = root.XamlRoot.RasterizationScale;
            metrics["client_width_dip"] = root.ActualWidth;
            metrics["client_height_dip"] = root.ActualHeight;
            metrics["visual_nodes_initial"] = CountVisual(root);
            metrics["realized_rows_initial"] = CountRows(root);
            var textCheck = CheckBoundText(root);
            metrics["realized_bound_text_nodes"] = textCheck.Nodes;
            metrics["realized_bound_text_matches"] = textCheck.Matches;
            metrics["realized_bound_text_utf16"] = textCheck.Characters;
            if (list != null && blocks.Count > 0 && (textCheck.Nodes == 0 || textCheck.Nodes != textCheck.Matches))
                throw new InvalidOperationException("Virtualized text bindings did not match the adapter model");
            metrics["phase"] = "ready";
            metrics["uia_peer"] = FrameworkElementAutomationPeer.CreatePeerForElement(surface)?.GetClassName();
            var contentPeer = FrameworkElementAutomationPeer.CreatePeerForElement((FrameworkElement?)rich ?? edit ?? (FrameworkElement?)list);
            metrics["content_uia_peer"] = contentPeer?.GetClassName();
            try { metrics["content_uia_text_pattern"] = contentPeer?.GetPattern(PatternInterface.Text) != null; }
            catch (Exception failure) { metrics["uia_probe_error"] = failure.Message; }
            Save();
            if (Program.Args.Contains("--dpi-sweep")) await SweepDisplays();
            if (Program.Args.Contains("--theme-cycle")) await CycleThemes();
            if (output != null && !Program.Args.Contains("--interactive")) await Measure();
        }
#if FEAS006
        catch (Feas006.CoreException ex) when (ex.Status == Feas006.CoreStatus.Cancelled)
        { metrics["cancelled"] = true; Save(); status.Text = "Core parse cancelled"; if (output != null) Close(); }
#endif
        catch (Exception ex) { metrics["error"] = ex.ToString(); Save(); status.Text = ex.Message; if (output != null) Close(); }
    }

    void OnRendering(object? sender, object e)
    {
        rendered = (long)(Program.Clock.Elapsed.TotalMilliseconds * 1000);
        metrics["entry_to_render_callback_ms"] = rendered / 1000.0;
        CompositionTarget.Rendering -= OnRendering;
    }

#if !FEAS006
    void AddBlocks(ContainerBlock container)
    {
        foreach (var block in container)
        {
            if (block is CodeBlock code) blocks.Add(new(code.Lines.ToString(), Code: true));
            else if (block is LeafBlock leaf)
            {
                string text = leaf.Inline == null ? leaf.Lines.ToString() : InlineText(leaf.Inline);
                if (text.Length > 0) blocks.Add(new(text, (block as HeadingBlock)?.Level ?? 0));
            }
            else if (block is ContainerBlock nested) AddBlocks(nested);
        }
    }

    static string InlineText(ContainerInline container)
    {
        var text = new StringBuilder();
        foreach (var inline in container)
            text.Append(inline switch {
                LiteralInline literal => literal.Content.ToString(),
                CodeInline code => code.Content,
                LineBreakInline => "\n",
                HtmlInline html => html.Tag,
                Markdig.Extensions.TaskLists.TaskList task => task.Checked ? "[x] " : "[ ] ",
                AutolinkInline link => link.Url,
                HtmlEntityInline entity => entity.Transcoded.ToString(),
                ContainerInline children => InlineText(children),
                _ => "",
            });
        return text.ToString();
    }

#endif
    void MakeSurface()
    {
        switch (mode)
        {
            case "richtext":
                rich = new RichTextBlock { IsTextSelectionEnabled = true, TextWrapping = TextWrapping.Wrap, FontSize = 16, Margin = new Thickness(16) };
                foreach (var block in blocks)
                {
                    var paragraph = new Paragraph { FontSize = block.Size, Margin = new Thickness(0, 0, 0, 8) };
                    if (block.Code) paragraph.FontFamily = new FontFamily("Consolas");
                    paragraph.Inlines.Add(new Run { Text = block.Text });
                    rich.Blocks.Add(paragraph);
                }
                scroll = new ScrollViewer { Content = rich, HorizontalScrollBarVisibility = ScrollBarVisibility.Disabled };
                surface = scroll;
                break;
            case "richedit":
                edit = new RichEditBox { TextWrapping = TextWrapping.Wrap, FontSize = 16, Margin = new Thickness(12) };
                edit.Document.SetText(TextSetOptions.None, plain);
                int offset = 0;
                foreach (var block in blocks)
                {
                    if (block.Heading > 0 || block.Code)
                    {
                        var range = edit.Document.GetRange(offset, offset + block.Text.Length);
                        range.CharacterFormat.Size = (float)block.Size;
                        if (block.Code) range.CharacterFormat.Name = "Consolas";
                    }
                    offset += block.Text.Length + 1;
                }
                edit.IsReadOnly = true;
                surface = edit;
                break;
            case "blocks":
                list = new ListView { SelectionMode = ListViewSelectionMode.None, ItemsSource = blocks,
                    ItemTemplate = (DataTemplate)XamlReader.Load("<DataTemplate xmlns='http://schemas.microsoft.com/winfx/2006/xaml/presentation'><TextBlock Text='{Binding Text}' FontSize='{Binding Size}' TextWrapping='Wrap' IsTextSelectionEnabled='True' Margin='8'/></DataTemplate>") };
                surface = list;
                break;
            default: throw new ArgumentException("surface must be richtext, richedit or blocks");
        }
    }

    static string Normalize(string text) => text.Replace("\r\n", "\n").Replace('\r', '\n').TrimEnd('\n');
    async Task VerifyClipboard()
    {
        // Invoke F8 only after copying this public fixture through the native UI.
        var clipboard = Windows.ApplicationModel.DataTransfer.Clipboard.GetContent();
        bool available = clipboard.Contains(Windows.ApplicationModel.DataTransfer.StandardDataFormats.Text);
        metrics["clipboard_text_available"] = available;
        metrics["clipboard_matches"] = available && Normalize(await clipboard.GetTextAsync()) == Normalize(plain);
        status.Text = $"Native clipboard matches fixture: {metrics["clipboard_matches"]}";
        Save();
    }

    async Task SweepDisplays()
    {
        var originalPosition = AppWindow.Position; var originalSize = AppWindow.Size;
        var observations = new List<object>();
        try
        {
            var displays = Microsoft.UI.Windowing.DisplayArea.FindAll();
            // Index the WinRT view: this runtime's IEnumerable projection can
            // throw even though Count and indexed access are supported.
            for (int index = 0; index < displays.Count; index++)
            {
                var display = displays[index];
                var area = display.WorkArea;
                AppWindow.MoveAndResize(new Windows.Graphics.RectInt32(area.X + 40, area.Y + 40, 1000, 750));
                await Task.Delay(750);
                observations.Add(new { scale = root.XamlRoot.RasterizationScale,
                    width_dip = root.ActualWidth, height_dip = root.ActualHeight,
                    work_x_px = area.X, work_y_px = area.Y,
                    work_width_px = area.Width, work_height_px = area.Height });
            }
        }
        finally { AppWindow.MoveAndResize(new Windows.Graphics.RectInt32(originalPosition.X, originalPosition.Y, originalSize.Width, originalSize.Height)); }
        await Task.Delay(750);
        metrics["display_sweep"] = observations;
        metrics["display_sweep_restored_geometry"] = AppWindow.Position.X == originalPosition.X && AppWindow.Position.Y == originalPosition.Y
            && AppWindow.Size.Width == originalSize.Width && AppWindow.Size.Height == originalSize.Height;
        status.Text = $"DPI check recorded {observations.Count} display(s); window restored";
        Save();
    }

    async Task CycleThemes()
    {
        var original = root.RequestedTheme;
        try
        {
            root.RequestedTheme = ElementTheme.Dark; await Task.Delay(200);
            metrics["dark_theme_applied"] = root.ActualTheme == ElementTheme.Dark;
            root.RequestedTheme = ElementTheme.Light; await Task.Delay(200);
            metrics["light_theme_applied"] = root.ActualTheme == ElementTheme.Light;
        }
        finally { root.RequestedTheme = original; }
        Save();
    }
    void SelectText()
    {
        string? selected = null;
        if (rich != null) { rich.SelectAll(); selected = rich.SelectedText; }
        if (edit != null) { edit.Document.Selection.SetRange(0, plain.Length); selected = edit.Document.Selection.Text; }
        metrics["selection_supported"] = selected != null;
        metrics["selection_matches"] = selected == null ? null : Normalize(selected) == Normalize(plain);
        metrics["selected_utf16"] = selected?.Length;
        metrics["expected_utf16"] = Normalize(plain).Length;
    }

    void GoToEnd()
    {
        if (scroll != null) scroll.ChangeView(null, scroll.ScrollableHeight, null, true);
        if (edit != null) { edit.Document.Selection.SetRange(plain.Length, plain.Length); edit.Document.Selection.ScrollIntoView(PointOptions.None); }
        if (list != null && blocks.Count > 0) list.ScrollIntoView(blocks[^1], ScrollIntoViewAlignment.Leading);
    }
    void GoToTop()
    {
        if (scroll != null) scroll.ChangeView(null, 0, null, true);
        if (edit != null) { edit.Document.Selection.SetRange(0, 0); edit.Document.Selection.ScrollIntoView(PointOptions.Start); }
        if (list != null && blocks.Count > 0) list.ScrollIntoView(blocks[0], ScrollIntoViewAlignment.Leading);
    }

    async Task Measure()
    {
        var timer = Stopwatch.StartNew(); SelectText(); metrics["select_all_ms"] = timer.Elapsed.TotalMilliseconds;
        GoToTop(); await Task.Delay(200);
        timer.Restart(); GoToEnd(); metrics["end_request_ms"] = timer.Elapsed.TotalMilliseconds;
        await Task.Delay(300);
        metrics["end_reached"] = scroll != null ? scroll.VerticalOffset >= scroll.ScrollableHeight - 2
            : list != null ? blocks.Count == 0 || list.ContainerFromItem(blocks[^1]) != null
            : edit != null ? edit.Document.Selection.StartPosition >= plain.Length : false;
        metrics["scrollable_height"] = scroll?.ScrollableHeight;
        metrics["end_offset"] = scroll?.VerticalOffset;
        metrics["realized_rows_end"] = CountRows(root);
        Save();
        var delays = new List<double>();
        for (int i = 0; i < 10; i++)
        {
            timer.Restart();
            if (i % 2 == 0) GoToTop(); else GoToEnd();
            await Task.Delay(16);
            delays.Add(timer.Elapsed.TotalMilliseconds);
        }
        metrics["navigation_timer_delay_max_ms"] = delays.Max();
        metrics["navigation_timer_delay_median_ms"] = delays.Order().ElementAt(delays.Count / 2);
        Save(); Close();
    }

    static int CountVisual(DependencyObject item)
    {
        int count = 1;
        for (int i = 0; i < VisualTreeHelper.GetChildrenCount(item); i++) count += CountVisual(VisualTreeHelper.GetChild(item, i));
        return count;
    }
    static int CountRows(DependencyObject item)
    {
        int count = item is ListViewItem ? 1 : 0;
        for (int i = 0; i < VisualTreeHelper.GetChildrenCount(item); i++) count += CountRows(VisualTreeHelper.GetChild(item, i));
        return count;
    }
    static (int Nodes, int Matches, int Characters) CheckBoundText(DependencyObject item)
    {
        int nodes = 0, matches = 0, characters = 0;
        if (item is TextBlock text && text.DataContext is DocBlock block)
        {
            nodes = 1; matches = text.Text == block.Text ? 1 : 0; characters = text.Text.Length;
        }
        for (int i = 0; i < VisualTreeHelper.GetChildrenCount(item); i++)
        {
            var child = CheckBoundText(VisualTreeHelper.GetChild(item, i));
            nodes += child.Nodes; matches += child.Matches; characters += child.Characters;
        }
        return (nodes, matches, characters);
    }
    void Save()
    {
        if (output == null) return;
        using var process = Process.GetCurrentProcess(); process.Refresh();
        metrics["working_set_bytes"] = process.WorkingSet64;
        metrics["peak_working_set_bytes"] = process.PeakWorkingSet64;
        metrics["private_bytes"] = process.PrivateMemorySize64;
        metrics["managed_heap_bytes"] = GC.GetTotalMemory(false);
        metrics["process_elapsed_ms"] = Program.Clock.Elapsed.TotalMilliseconds;
        var path = Path.GetFullPath(output); Directory.CreateDirectory(Path.GetDirectoryName(path)!);
        File.WriteAllText(path + ".tmp", JsonSerializer.Serialize(metrics, new JsonSerializerOptions { WriteIndented = true }));
        File.Move(path + ".tmp", path, true);
        if (metrics.GetValueOrDefault("entry_to_render_callback_ms") != null && !File.Exists(path + ".ready"))
            File.WriteAllText(path + ".ready", "render callback checkpoint\n");
    }
}
