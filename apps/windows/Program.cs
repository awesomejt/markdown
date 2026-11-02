using System.Diagnostics;
using System.IO;
using System.IO.Pipes;
using System.Security.Principal;
using System.Text;
using System.Text.Json;
using Markdown.Windows.Core;
using Markdown.Windows.Document;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Input;
using Windows.System;

namespace Markdown.Windows;

internal static class Program
{
    internal const string NewWindowCommand = "__NEW_WINDOW__";
    internal static bool Smoke;
    internal static bool AssociationSmoke;
    internal static string? AssociationPipeName;
    internal static string? AssociationReportPath;
    internal static int AssociationExpectedDeliveries;
    internal static List<string> StartupInputs = [];
    internal static int Result = 1;
    private static Mutex? s_associationPrimaryMutex;

    [STAThread]
    private static int Main(string[] args)
    {
        if (args.SequenceEqual(["--core-tests"]))
        {
            return CoreConsumerTests.RunAll();
        }
        if (args.SequenceEqual(["--document-tests"]))
        {
            return DocumentTests.RunAll();
        }
        if (args.Length >= 1 && args[0] == "--core-probe")
        {
            return CoreProbe.Run(args[1..]);
        }
        if (args.Length >= 4 && args[0] == "--association-smoke")
        {
            AssociationSmoke = true;
            AssociationPipeName = args[1];
            AssociationReportPath = args[2];
            if (!int.TryParse(args[3], out AssociationExpectedDeliveries) ||
                AssociationExpectedDeliveries < 1)
            {
                return 2;
            }
            StartupInputs = [.. args[4..]];
        }
        else if (args.SequenceEqual(["--smoke"]))
        {
            Smoke = true;
        }
        else
        {
            foreach (var arg in args)
            {
                if (arg.StartsWith("--", StringComparison.Ordinal))
                {
                    return 2;
                }
                StartupInputs.Add(arg);
            }

            var envInstanceKey = Environment.GetEnvironmentVariable("MARKDOWN_INSTANCE_KEY");
            AssociationPipeName = !string.IsNullOrWhiteSpace(envInstanceKey)
                ? envInstanceKey.Trim()
                : DefaultInstancePipeName();
        }

        if (!Smoke && !string.IsNullOrEmpty(AssociationPipeName))
        {
            s_associationPrimaryMutex = new Mutex(
                initiallyOwned: true,
                name: $@"Local\markdown-assoc-mutex-{AssociationPipeName}",
                createdNew: out var isPrimary);
            if (!isPrimary)
            {
                s_associationPrimaryMutex.Dispose();
                s_associationPrimaryMutex = null;
                var payloads = StartupInputs.Count > 0
                    ? StartupInputs
                    : (IReadOnlyList<string>)[NewWindowCommand];
                return ForwardToPrimaryPipe(AssociationPipeName, payloads) ? 0 : 1;
            }
        }

        try
        {
            CoreRuntime.Validate();
            WinRT.ComWrappersSupport.InitializeComWrappers();
            Application.Start(initialization =>
            {
                SynchronizationContext.SetSynchronizationContext(
                    new Microsoft.UI.Dispatching.DispatcherQueueSynchronizationContext(
                        Microsoft.UI.Dispatching.DispatcherQueue.GetForCurrentThread()));
                _ = new ReaderApp();
            });

            if (Smoke || AssociationSmoke)
            {
                if (Result == 0 && !WaitZeroLiveHandles(2000))
                {
                    Result = 1;
                }
                return Result;
            }
            return 0;
        }
        finally
        {
            if (s_associationPrimaryMutex is not null)
            {
                try
                {
                    s_associationPrimaryMutex.ReleaseMutex();
                }
                catch
                {
                    // Ignore release errors on shutdown.
                }
                s_associationPrimaryMutex.Dispose();
                s_associationPrimaryMutex = null;
            }
        }
    }

    private static string DefaultInstancePipeName()
    {
        var sid = WindowsIdentity.GetCurrent().User?.Value ?? Environment.UserName;
        var sb = new StringBuilder("markdown-win-assoc-", 64);
        foreach (var ch in sid)
        {
            sb.Append(char.IsLetterOrDigit(ch) || ch == '-' ? ch : '_');
        }
        return sb.ToString();
    }

    private static bool ForwardToPrimaryPipe(string pipeName, IReadOnlyList<string> inputs)
    {
        try
        {
            using var client = new NamedPipeClientStream(
                ".",
                pipeName,
                PipeDirection.Out,
                PipeOptions.CurrentUserOnly);
            client.Connect(timeout: 5000);
            using var writer = new StreamWriter(client, new UTF8Encoding(false)) { AutoFlush = true };
            foreach (var input in inputs)
            {
                writer.WriteLine(input);
            }
            return true;
        }
        catch (Exception ex)
        {
            Console.Error.WriteLine($"Secondary association forwarding failed: {ex}");
            return false;
        }
    }

    internal static bool WaitZeroLiveHandles(int timeoutMs)
    {
        var sw = Stopwatch.StartNew();
        while (sw.ElapsedMilliseconds < timeoutMs)
        {
            var counts = CoreRuntime.LiveHandles();
            if (counts.Sessions == 0 && counts.Jobs == 0 && counts.Snapshots == 0)
            {
                return true;
            }
            Thread.Sleep(5);
        }
        var finalCounts = CoreRuntime.LiveHandles();
        return finalCounts.Sessions == 0 && finalCounts.Jobs == 0 && finalCounts.Snapshots == 0;
    }
}

public sealed partial class ReaderApp : Application
{
    private readonly List<ReaderWindow> windows = [];
    private Microsoft.UI.Dispatching.DispatcherQueue? _dispatcher;
    private int _externalDeliveries;
    private bool _shuttingDown;

    internal IReadOnlyList<ReaderWindow> Windows => windows;

    public ReaderApp() => InitializeComponent();

    protected override void OnLaunched(LaunchActivatedEventArgs args)
    {
        _dispatcher = Microsoft.UI.Dispatching.DispatcherQueue.GetForCurrentThread();
        var window = NewWindow();

        foreach (var rawInput in Program.StartupInputs)
        {
            _ = OpenExternalInput(rawInput);
            _externalDeliveries++;
        }

        if (!Program.Smoke && Program.AssociationPipeName is not null)
        {
            StartAssociationPipeServer(Program.AssociationPipeName);
        }

        if (Program.Smoke)
        {
            ((FrameworkElement)window.Content).Loaded += async (_, _) => await RunSmokeAsync(window);
        }
        else if (Program.AssociationSmoke)
        {
            ((FrameworkElement)window.Content).Loaded += async (_, _) => await RunAssociationSmokeAsync();
        }
    }

    internal ReaderWindow NewWindow()
    {
        var window = new ReaderWindow(this);
        windows.Add(window);
        window.Closed += (_, _) =>
        {
            windows.Remove(window);
            if (windows.Count == 0)
            {
                Volatile.Write(ref _shuttingDown, true);
            }
        };
        window.Activate();
        return window;
    }

    internal void CloseAll()
    {
        Volatile.Write(ref _shuttingDown, true);
        foreach (var window in windows.ToArray())
        {
            window.Close();
        }
    }

    internal ReaderWindow OpenExternalInput(string rawInput)
    {
        if (string.Equals(rawInput, Program.NewWindowCommand, StringComparison.Ordinal))
        {
            return NewWindow();
        }

        if (!DocumentCapability.TryFromUserInput(rawInput, out var capability, out var error) ||
            capability is null)
        {
            var target = FindReusableEmptyWindow() ?? NewWindow();
            target.RejectSelection(error);
            target.Activate();
            return target;
        }

        foreach (var existing in windows)
        {
            if (existing.Loader.Capability?.SameFileAs(capability) == true)
            {
                existing.Activate();
                if (existing.Loader.Document is null || existing.Loader.Error is not null)
                {
                    existing.ReloadCommand();
                }
                return existing;
            }
        }

        var win = FindReusableEmptyWindow() ?? NewWindow();
        win.OpenCapability(capability);
        win.Activate();
        return win;
    }

    private ReaderWindow? FindReusableEmptyWindow() =>
        windows.FirstOrDefault(w =>
            !w.Loader.HasFile &&
            w.Loader.Document is null &&
            w.Loader.Error is null &&
            !w.Loader.IsLoading);

    private void StartAssociationPipeServer(string pipeName)
    {
        var serverThread = new Thread(() =>
        {
            while (!Volatile.Read(ref _shuttingDown))
            {
                try
                {
                    using var server = new NamedPipeServerStream(
                        pipeName,
                        PipeDirection.In,
                        NamedPipeServerStream.MaxAllowedServerInstances,
                        PipeTransmissionMode.Byte,
                        PipeOptions.CurrentUserOnly);
                    server.WaitForConnection();
                    if (Volatile.Read(ref _shuttingDown))
                    {
                        break;
                    }
                    using var reader = new StreamReader(server, Encoding.UTF8);
                    while (reader.ReadLine() is { } line)
                    {
                        if (string.IsNullOrWhiteSpace(line))
                        {
                            continue;
                        }
                        var captured = line;
                        _dispatcher?.TryEnqueue(() =>
                        {
                            _ = OpenExternalInput(captured);
                            _externalDeliveries++;
                        });
                    }
                }
                catch
                {
                    if (Volatile.Read(ref _shuttingDown))
                    {
                        break;
                    }
                    Thread.Sleep(20);
                }
            }
        })
        {
            Name = "markdown-association-pipe",
            IsBackground = true,
        };
        serverThread.Start();
    }

    private async Task RunAssociationSmokeAsync()
    {
        try
        {
            var sw = Stopwatch.StartNew();
            while (sw.ElapsedMilliseconds < 10_000)
            {
                foreach (var win in windows)
                {
                    win.SyncFromLoader();
                }

                if (_externalDeliveries >= Program.AssociationExpectedDeliveries &&
                    windows.Count > 0 &&
                    windows.All(w => !w.Loader.IsLoading && (w.Loader.Document is not null || w.Loader.Error is not null)))
                {
                    break;
                }
                await Task.Delay(15);
            }

            if (_externalDeliveries < Program.AssociationExpectedDeliveries ||
                windows.Any(w => w.Loader.IsLoading))
            {
                throw new InvalidOperationException(
                    $"Association smoke timed out: deliveries={_externalDeliveries}/{Program.AssociationExpectedDeliveries}");
            }

            if (!string.IsNullOrEmpty(Program.AssociationReportPath))
            {
                using var stream = File.Create(Program.AssociationReportPath);
                using var writer = new Utf8JsonWriter(stream, new JsonWriterOptions { Indented = true });
                writer.WriteStartObject();
                writer.WriteNumber("deliveries", _externalDeliveries);
                writer.WriteNumber("window_count", windows.Count);
                writer.WriteStartArray("windows");
                foreach (var win in windows)
                {
                    writer.WriteStartObject();
                    writer.WriteString("title", win.Title);
                    writer.WriteString("display_name", win.Loader.Capability?.DisplayName ?? string.Empty);
                    writer.WriteNumber("generation", win.Loader.Document?.Generation ?? 0);
                    writer.WriteString(
                        "heading",
                        win.Loader.Document is { OutlineCount: > 0 } doc ? doc.OutlineTitle(0) : string.Empty);
                    writer.WriteString("error", win.Loader.Error?.ToString() ?? string.Empty);
                    writer.WriteEndObject();
                }
                writer.WriteEndArray();
                writer.WriteEndObject();
            }

            Program.Result = 0;
            CloseAll();
        }
        catch (Exception ex)
        {
            Console.Error.WriteLine($"Association smoke failed: {ex}");
            Program.Result = 1;
            CloseAll();
            Exit();
        }
    }

    private async Task RunSmokeAsync(ReaderWindow first)
    {
        var tempDir = Path.Combine(
            Path.GetTempPath(),
            $"markdown-win001c-smoke-{Guid.NewGuid():N}");
        Directory.CreateDirectory(tempDir);

        try
        {
            // 1. Verify initial shell contract and all 5 File menu keyboard accelerators.
            if (windows.Count != 1 ||
                first.Title != "Markdown" ||
                AutomationProperties.GetName(first.Welcome) != "Markdown reader" ||
                first.ReloadItem.IsEnabled)
            {
                throw new InvalidOperationException("Initial shell contract failed.");
            }

            var shortcuts = ((Grid)first.Content).KeyboardAccelerators;
            if (shortcuts.Count != 5 ||
                !shortcuts.Any(k => k.Key == VirtualKey.O && k.Modifiers == VirtualKeyModifiers.Control) ||
                !shortcuts.Any(k => k.Key == VirtualKey.R && k.Modifiers == VirtualKeyModifiers.Control) ||
                !shortcuts.Any(k => k.Key == VirtualKey.N && k.Modifiers == VirtualKeyModifiers.Control) ||
                !shortcuts.Any(k => k.Key == VirtualKey.W && k.Modifiers == VirtualKeyModifiers.Control) ||
                !shortcuts.Any(k => k.Key == VirtualKey.W &&
                    k.Modifiers == (VirtualKeyModifiers.Control | VirtualKeyModifiers.Shift)))
            {
                throw new InvalidOperationException("Root shortcut bindings failed.");
            }

            // 2. Exercise native Win32 file open dialog (GetOpenFileNameW) Cancel path.
            DocumentPicker.EnqueueAutomationCancel();
            first.OpenCommand();
            if (first.Loader.HasFile || first.Title != "Markdown" || first.ReloadItem.IsEnabled)
            {
                throw new InvalidOperationException("Cancelled file dialog must leave window unchanged.");
            }

            // 3. Exercise native Win32 file open dialog (GetOpenFileNameW) Select + Accept path.
            var smokeDocPath = Path.Combine(tempDir, "smoke-doc.md");
            File.WriteAllText(smokeDocPath, "# Windows Smoke\n\nShared core model v2.\n", new UTF8Encoding(false));

            DocumentPicker.EnqueueAutomationSelect(smokeDocPath);
            first.OpenCommand();
            await WaitWindowIdleAsync(first);

            if (first.Title != "smoke-doc.md — Markdown" ||
                !first.ReloadItem.IsEnabled ||
                first.Loader.Error is not null ||
                first.Loader.Document is null ||
                first.Loader.Document.ModelVersion != 2 ||
                first.Loader.Document.Generation != 1 ||
                first.Loader.Document.OutlineCount != 1 ||
                first.Loader.Document.OutlineTitle(0) != "Windows Smoke" ||
                !first.Welcome.Text.Contains("Windows Smoke", StringComparison.Ordinal))
            {
                throw new InvalidOperationException("Native file dialog selection or initial parse failed.");
            }

            // 4. Exercise Reload (Ctrl+R command delegate) after modifying file on disk.
            File.WriteAllText(smokeDocPath, "# Windows Smoke v2\n\nUpdated body.\n", new UTF8Encoding(false));
            first.ReloadCommand();
            await WaitWindowIdleAsync(first);

            if (first.Loader.Error is not null ||
                first.Loader.Document is null ||
                first.Loader.Document.Generation != 2 ||
                first.Loader.Document.OutlineTitle(0) != "Windows Smoke v2")
            {
                throw new InvalidOperationException("Reload after edit failed.");
            }

            // 5. Missing file on Reload reports path-free NotFound while preserving v2 document, then recovers to v3.
            File.Delete(smokeDocPath);
            first.ReloadCommand();
            await WaitWindowIdleAsync(first);

            if (first.Loader.Error != DocumentError.NotFound ||
                first.Loader.Availability() != CoreAvailability.Unavailable(CoreUnavailability.Deleted) ||
                first.Loader.Document is null ||
                first.Loader.Document.Generation != 2 ||
                first.Welcome.Text.Contains(smokeDocPath, StringComparison.OrdinalIgnoreCase))
            {
                throw new InvalidOperationException("Missing-file reload error handling failed.");
            }

            File.WriteAllText(smokeDocPath, "# Windows Smoke v3\n\nRecovered body.\n", new UTF8Encoding(false));
            first.ReloadCommand();
            await WaitWindowIdleAsync(first);

            // Generation 3 was recorded by RecordUnavailable(Deleted); recovered document is Generation 4.
            if (first.Loader.Error is not null ||
                first.Loader.Document is null ||
                first.Loader.Document.Generation != 4 ||
                first.Loader.Availability() != CoreAvailability.Available)
            {
                throw new InvalidOperationException("Recovery reload after missing file failed.");
            }

            // 6. Withdraw OS read permission via NTFS discretionary ACL -> Reload reports AccessRevoked, then recovers.
            DocumentTests.SetReadDenyAcl(smokeDocPath, deny: true);
            try
            {
                first.ReloadCommand();
                await WaitWindowIdleAsync(first);

                if (first.Loader.Error != DocumentError.AccessRevoked ||
                    first.Loader.Availability() != CoreAvailability.Unavailable(CoreUnavailability.AccessRevoked) ||
                    first.Loader.Document is null ||
                    first.Loader.Document.Generation != 4)
                {
                    throw new InvalidOperationException("ACL withdrawal AccessRevoked handling failed.");
                }
            }
            finally
            {
                DocumentTests.SetReadDenyAcl(smokeDocPath, deny: false);
            }

            first.ReloadCommand();
            await WaitWindowIdleAsync(first);
            if (first.Loader.Error is not null ||
                first.Loader.Availability() != CoreAvailability.Available)
            {
                throw new InvalidOperationException("ACL restoration reload failed.");
            }

            // 7. Non-local URI rejection opens in a second window without disturbing the first window.
            var rejectedWin = OpenExternalInput("https://example.com/remote.md");
            if (windows.Count != 2 ||
                rejectedWin.Loader.Error != DocumentError.NonLocalFile ||
                rejectedWin.Welcome.Text.Contains("https://", StringComparison.OrdinalIgnoreCase))
            {
                throw new InvalidOperationException("Non-local URI rejection failed.");
            }
            rejectedWin.CloseCommand();
            if (windows.Count != 1)
            {
                throw new InvalidOperationException("Closing rejected URI window failed.");
            }

            // 8. Stale slow load is superseded when another file is opened before the slow read finishes.
            var secondDocPath = Path.Combine(tempDir, "second-smoke.md");
            File.WriteAllText(secondDocPath, "# Second Smoke\n\nReplacement file.\n", new UTF8Encoding(false));

            using (var entered = new ManualResetEventSlim(false))
            using (var release = new ManualResetEventSlim(false))
            {
                var once = 0;
                first.Loader.ReadBarrier = () =>
                {
                    if (Interlocked.CompareExchange(ref once, 1, 0) == 0)
                    {
                        entered.Set();
                        release.Wait(TimeSpan.FromSeconds(5));
                    }
                };

                first.ReloadCommand();
                while (!entered.IsSet)
                {
                    await Task.Delay(5);
                }

                first.Loader.ReadBarrier = null;
                first.OpenCapability(DocumentCapability.FromPickerPath(secondDocPath));
                release.Set();
                await WaitWindowIdleAsync(first);
            }

            if (first.Title != "second-smoke.md — Markdown" ||
                first.Loader.Error is not null ||
                first.Loader.Document is null ||
                first.Loader.Document.OutlineTitle(0) != "Second Smoke" ||
                first.Loader.DiscardedResults < 1)
            {
                throw new InvalidOperationException("Stale slow-load supersession failed.");
            }

            // 9. Verify New Window, Close Window, and Exit commands close all windows cleanly.
            first.NewCommand();
            if (windows.Count != 2)
            {
                throw new InvalidOperationException("New Window failed.");
            }
            windows[1].CloseCommand();
            if (windows.Count != 1)
            {
                throw new InvalidOperationException("Close failed.");
            }
            first.NewCommand();
            Program.Result = 0;
            first.ExitCommand();
            if (windows.Count != 0)
            {
                Program.Result = 1;
            }
        }
        catch (Exception ex)
        {
            Console.Error.WriteLine($"Smoke failed: {ex}");
            Program.Result = 1;
            CloseAll();
            Exit();
        }
        finally
        {
            try
            {
                if (Directory.Exists(tempDir))
                {
                    Directory.Delete(tempDir, recursive: true);
                }
            }
            catch
            {
                // Best-effort cleanup.
            }
        }
    }

    private static async Task WaitWindowIdleAsync(ReaderWindow window, int timeoutMs = 5000)
    {
        var sw = Stopwatch.StartNew();
        while (sw.ElapsedMilliseconds < timeoutMs)
        {
            window.SyncFromLoader();
            if (!window.Loader.IsLoading)
            {
                return;
            }
            await Task.Delay(10);
        }
        window.SyncFromLoader();
        if (window.Loader.IsLoading)
        {
            throw new TimeoutException($"Window '{window.Title}' did not finish loading within {timeoutMs} ms.");
        }
    }
}

internal sealed class ReaderWindow : Window
{
    internal DocumentLoader Loader { get; } = new();
    internal Action OpenCommand { get; }
    internal Action ReloadCommand { get; }
    internal Action NewCommand { get; }
    internal Action CloseCommand { get; }
    internal Action ExitCommand { get; }
    internal MenuFlyoutItem ReloadItem { get; }
    internal TextBlock Welcome { get; }

    internal ReaderWindow(ReaderApp app)
    {
        Title = "Markdown";
        AppWindow.Resize(new global::Windows.Graphics.SizeInt32(960, 700));

        OpenCommand = PickAndOpenFile;
        ReloadCommand = ReloadDocument;
        NewCommand = () => app.NewWindow();
        CloseCommand = Close;
        ExitCommand = app.CloseAll;

        Loader.OnReady = () =>
        {
            _ = DispatcherQueue?.TryEnqueue(SyncFromLoader);
        };
        Closed += (_, _) => Loader.Close();

        var root = new Grid();
        var file = new MenuBarItem { Title = "File", AccessKey = "F" };
        file.Items.Add(Command(root, "Open…", "Ctrl+O", VirtualKey.O, VirtualKeyModifiers.Control, OpenCommand));
        ReloadItem = Command(root, "Reload", "Ctrl+R", VirtualKey.R, VirtualKeyModifiers.Control, ReloadCommand);
        ReloadItem.IsEnabled = false;
        file.Items.Add(ReloadItem);
        file.Items.Add(new MenuFlyoutSeparator());
        file.Items.Add(Command(root, "New Window", "Ctrl+N", VirtualKey.N, VirtualKeyModifiers.Control, NewCommand));
        file.Items.Add(Command(root, "Close Window", "Ctrl+W", VirtualKey.W, VirtualKeyModifiers.Control, CloseCommand));
        file.Items.Add(new MenuFlyoutSeparator());
        file.Items.Add(Command(root, "Exit", "Ctrl+Shift+W", VirtualKey.W,
            VirtualKeyModifiers.Control | VirtualKeyModifiers.Shift, ExitCommand));
        var menu = new MenuBar();
        menu.Items.Add(file);
        Welcome = new TextBlock
        {
            Text = "Markdown\n\nA native Markdown reader.\nChoose File > Open (Ctrl+O) to open a Markdown document.",
            TextWrapping = TextWrapping.Wrap,
            Margin = new Thickness(32),
            FontSize = 20
        };
        AutomationProperties.SetName(Welcome, "Markdown reader");
        root.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        root.RowDefinitions.Add(new RowDefinition());
        root.Children.Add(menu);
        Grid.SetRow(Welcome, 1);
        root.Children.Add(Welcome);
        Content = root;
    }

    internal void OpenCapability(DocumentCapability capability)
    {
        Loader.Open(capability);
        SyncFromLoader();
    }

    internal void RejectSelection(DocumentError error)
    {
        Loader.RejectSelection(error);
        SyncFromLoader();
    }

    private void PickAndOpenFile()
    {
        var pickedPath = DocumentPicker.PickFile(this);
        if (pickedPath is null)
        {
            return;
        }
        if (!DocumentCapability.TryFromUserInput(pickedPath, out var capability, out var error) ||
            capability is null)
        {
            RejectSelection(error);
            return;
        }
        OpenCapability(capability);
    }

    private void ReloadDocument()
    {
        if (!Loader.HasFile)
        {
            return;
        }
        Loader.Reload();
        SyncFromLoader();
    }

    internal void SyncFromLoader()
    {
        _ = Loader.Poll();
        Title = Loader.WindowTitle;
        ReloadItem.IsEnabled = Loader.HasFile;

        if (!Loader.HasFile && Loader.Error is null)
        {
            Welcome.Text =
                "Markdown\n\nA native Markdown reader.\nChoose File > Open (Ctrl+O) to open a Markdown document.";
            return;
        }

        if (Loader.IsLoading && Loader.Document is null)
        {
            Welcome.Text = $"Loading {Loader.Capability?.DisplayName ?? "document"}…";
            return;
        }

        if (Loader.Document is { } doc)
        {
            var heading = doc.Title
                ?? (doc.OutlineCount > 0 ? doc.OutlineTitle(0) : null)
                ?? Loader.Capability?.DisplayName
                ?? "Document";

            var summary =
                $"{heading}\n\n" +
                $"Document: {Loader.Capability?.DisplayName ?? "Untitled"}\n" +
                $"Generation: {doc.Generation} · Nodes: {doc.NodeCount} · Headings: {doc.OutlineCount} · Bytes: {doc.SourceByteCount}";

            if (Loader.Error is { } activeError)
            {
                var label = Loader.LastErrorWasOpenFailure ? "Open issue" : "Reload issue";
                summary += $"\n\n{label}:\n{activeError.ToDisplayString()}";
            }

            Welcome.Text = summary;
            return;
        }

        if (Loader.Error is { } error)
        {
            Welcome.Text = error.ToDisplayString();
        }
    }

    private static MenuFlyoutItem Command(Grid root, string text, string shortcut, VirtualKey key,
        VirtualKeyModifiers modifiers, Action invoke)
    {
        var item = new MenuFlyoutItem { Text = text, KeyboardAcceleratorTextOverride = shortcut };
        item.Click += (_, _) => invoke();
        var accelerator = new KeyboardAccelerator { Key = key, Modifiers = modifiers };
        accelerator.Invoked += (_, args) => { args.Handled = true; invoke(); };
        root.KeyboardAccelerators.Add(accelerator);
        return item;
    }
}
