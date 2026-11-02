using System.Diagnostics;
using System.IO;
using System.IO.Pipes;
using System.Security.Principal;
using System.Text;
using System.Text.Json;
using Markdown.Windows.Core;
using Markdown.Windows.Document;
using Markdown.Windows.Render;
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
        if (args.SequenceEqual(["--render-tests"]))
        {
            return RenderTests.RunAll();
        }
        if (args.Length >= 1 && args[0] == "--core-probe")
        {
            return CoreProbe.Run(args[1..]);
        }
        if (args.Length >= 2 && args[0] == "--measure")
        {
            return Measurement.Run(args[1..]);
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
    internal ReaderPreferences Preferences { get; } = new(
        Program.Smoke ? new MemoryPreferenceStore() : new LocalSettingsPreferenceStore());
    internal SettingsWindow? SettingsWindow { get; private set; }

    public ReaderApp()
    {
        InitializeComponent();
        Preferences.Changed += () =>
        {
            foreach (var w in windows.ToArray())
            {
                ApplyPreferences(w);
            }
            SettingsWindow?.Refresh();
        };
    }

    internal void ShowSettings()
    {
        if (SettingsWindow is not null)
        {
            SettingsWindow.Activate();
            SettingsWindow.Refresh();
            return;
        }
        SettingsWindow = new SettingsWindow(Preferences);
        SettingsWindow.Closed += (_, _) => SettingsWindow = null;
        if (SettingsWindow.Content is FrameworkElement fe)
        {
            fe.RequestedTheme = Preferences.ToElementTheme();
        }
        SettingsWindow.Activate();
    }

    internal void ApplyPreferences(ReaderWindow window)
    {
        if (window.Content is FrameworkElement fe)
        {
            fe.RequestedTheme = Preferences.ToElementTheme();
        }
        window.Reader.SetTextScale(Preferences.TextScaleValue);
    }

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
        var window = new ReaderWindow(this, Preferences);
        windows.Add(window);
        window.Closed += (_, _) =>
        {
            windows.Remove(window);
            if (windows.Count == 0)
            {
                SettingsWindow?.Close();
                Volatile.Write(ref _shuttingDown, true);
            }
        };
        ApplyPreferences(window);
        window.Activate();
        return window;
    }

    internal void CloseAll()
    {
        Volatile.Write(ref _shuttingDown, true);
        SettingsWindow?.Close();
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
            if (shortcuts.Count != 15 ||
                !shortcuts.Any(k => k.Key == VirtualKey.O && k.Modifiers == VirtualKeyModifiers.Control) ||
                !shortcuts.Any(k => k.Key == VirtualKey.R && k.Modifiers == VirtualKeyModifiers.Control) ||
                !shortcuts.Any(k => k.Key == VirtualKey.N && k.Modifiers == VirtualKeyModifiers.Control) ||
                !shortcuts.Any(k => k.Key == VirtualKey.W && k.Modifiers == VirtualKeyModifiers.Control) ||
                !shortcuts.Any(k => k.Key == VirtualKey.W &&
                    k.Modifiers == (VirtualKeyModifiers.Control | VirtualKeyModifiers.Shift)) ||
                !shortcuts.Any(k => k.Key == VirtualKey.C && k.Modifiers == VirtualKeyModifiers.Control) ||
                !shortcuts.Any(k => k.Key == VirtualKey.A && k.Modifiers == VirtualKeyModifiers.Control) ||
                !shortcuts.Any(k => k.Key == VirtualKey.Escape && k.Modifiers == VirtualKeyModifiers.None) ||
                !shortcuts.Any(k => k.Key == VirtualKey.F && k.Modifiers == VirtualKeyModifiers.Control) ||
                !shortcuts.Any(k => k.Key == VirtualKey.F3 && k.Modifiers == VirtualKeyModifiers.None) ||
                !shortcuts.Any(k => k.Key == VirtualKey.F3 && k.Modifiers == VirtualKeyModifiers.Shift) ||
                !shortcuts.Any(k => k.Key == VirtualKey.Number0 && k.Modifiers == VirtualKeyModifiers.Control))
            {
                throw new InvalidOperationException($"Root shortcut bindings failed: count={shortcuts.Count}");
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
                !first.Welcome.Text.Contains("Windows Smoke", StringComparison.Ordinal) ||
                first.Loader.Plan is null ||
                first.Reader.Visibility != Visibility.Visible ||
                first.Welcome.Visibility != Visibility.Collapsed ||
                first.Reader.OutlineList.Items.Count != 1 ||
                first.Reader.DocumentList.Items.Count < 2)
            {
                throw new InvalidOperationException("Native file dialog selection or initial parse failed.");
            }

            first.Reader.ActivateOutline(0);
            Uri? smokeLaunchedUri = null;
            first.Reader.ExternalUriLauncher = uri => { smokeLaunchedUri = uri; return true; };

            // 4. Exercise Reload (Ctrl+R command delegate) after modifying file on disk.
            var smokeImgPath = Path.Combine(tempDir, "smoke-img.png");
            File.WriteAllBytes(smokeImgPath, RenderTests.CreatePng(64, 64));

            var richSmokeDoc =
                "# Windows Smoke v2\n\n" +
                "Updated body with [Internal](#windows-smoke-v2) and [External](https://example.com).\n\n" +
                "- [x] Task done\n" +
                "- [ ] Task open\n" +
                "- List item 1\n\n" +
                "| Header 1 | Header 2 |\n| :--- | ---: |\n| Val 1 | 100 |\n\n" +
                "![smoke alt](smoke-img.png)\n\n" +
                "```csharp\nvar answer = 42;\n```\n\n" +
                "> Blockquote line\n";
            File.WriteAllText(smokeDocPath, richSmokeDoc, new UTF8Encoding(false));
            first.ReloadCommand();
            await WaitWindowIdleAsync(first);

            if (first.Loader.Error is not null ||
                first.Loader.Document is null ||
                first.Loader.Document.Generation != 2 ||
                first.Loader.Document.OutlineTitle(0) != "Windows Smoke v2" ||
                first.Loader.Plan is null ||
                first.Reader.Visibility != Visibility.Visible ||
                first.Reader.OutlineList.Items.Count != 1 ||
                first.Reader.DocumentList.Items.Count < 5 ||
                first.Loader.Plan.Images.Count != 1 ||
                first.Loader.Plan.Images[0].Failure is not null ||
                !first.Loader.Plan.Fragments.Any(f => f.Kind == FragmentKind.TableRow) ||
                !first.Loader.Plan.Fragments.Any(f => f.Runs.Any(r => (r.Style & RunStyle.TaskChecked) != 0)))
            {
                throw new InvalidOperationException("Reload after edit with rich blocks (tables, tasks, images) failed.");
            }

            first.Reader.ActivateOutline(0);

            // 5. Missing file on Reload reports path-free NotFound while preserving v2 document, then recovers to v3.
            File.Delete(smokeDocPath);
            first.ReloadCommand();
            await WaitWindowIdleAsync(first);

            if (first.Loader.Error != DocumentError.NotFound ||
                first.Loader.Availability() != CoreAvailability.Unavailable(CoreUnavailability.Deleted) ||
                first.Loader.Document is null ||
                first.Loader.Document.Generation != 2 ||
                !first.ErrorBanner.IsOpen ||
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

            // 9. Document find, text scale, theme switching, and Settings window.
            first.FindCommand();
            if (!first.IsFindBarVisible)
            {
                throw new InvalidOperationException("Find command must show FindBar.");
            }

            first.FindBar.Query = "second smoke";
            first.FindBar.MatchCase = false;
            first.ExecuteFind();
            await WaitWindowIdleAsync(first);
            var swFind = Stopwatch.StartNew();
            while (swFind.ElapsedMilliseconds < 3000 && (first.Reader.CurrentFindMatches is null || first.Reader.CurrentFindMatches.Count != 1))
            {
                await Task.Delay(20);
            }
            if (first.Reader.CurrentFindMatches is null ||
                first.Reader.CurrentFindMatches.Count != 1 ||
                first.FindBar.StatusText.Text != "1 of 1")
            {
                throw new InvalidOperationException($"Case-insensitive find failed: matches={first.Reader.CurrentFindMatches?.Count}, status={first.FindBar.StatusText.Text}");
            }

            first.FindNextCommand();
            first.FindPreviousCommand();

            first.FindBar.MatchCase = true;
            first.ExecuteFind();
            await WaitWindowIdleAsync(first);
            var swCase = Stopwatch.StartNew();
            while (swCase.ElapsedMilliseconds < 3000 && (first.Reader.CurrentFindMatches is null || first.Reader.CurrentFindMatches.Count != 0))
            {
                await Task.Delay(20);
            }
            if (first.Reader.CurrentFindMatches?.Count != 0 || first.FindBar.StatusText.Text != "Not found")
            {
                throw new InvalidOperationException($"Case sensitive find failed: expected 0 matches, got {first.Reader.CurrentFindMatches?.Count}");
            }

            first.FindBar.MatchCase = false;
            first.ExecuteFind();
            await WaitWindowIdleAsync(first);
            var swRestore = Stopwatch.StartNew();
            while (swRestore.ElapsedMilliseconds < 3000 && (first.Reader.CurrentFindMatches is null || first.Reader.CurrentFindMatches.Count != 1))
            {
                await Task.Delay(20);
            }
            if (first.Reader.CurrentFindMatches?.Count != 1 || first.FindBar.StatusText.Text != "1 of 1")
            {
                throw new InvalidOperationException("Restoring case-insensitive find failed.");
            }

            first.CloseFindCommand();
            if (first.IsFindBarVisible || first.Reader.CurrentFindMatches is not null)
            {
                throw new InvalidOperationException("Close find must hide FindBar and clear highlights.");
            }

            // Text scaling
            first.BiggerCommand();
            if (Math.Abs(first.Preferences.TextScaleValue - 1.15) > 0.001 ||
                Math.Abs(first.Reader.TextScale - 1.15) > 0.001)
            {
                throw new InvalidOperationException("Bigger command failed.");
            }

            first.SmallerCommand();
            if (Math.Abs(first.Preferences.TextScaleValue - 1.0) > 0.001)
            {
                throw new InvalidOperationException("Smaller command failed.");
            }

            // Appearance & theme
            first.Preferences.Theme = AppTheme.Dark;
            if (first.Preferences.Theme != AppTheme.Dark)
            {
                throw new InvalidOperationException("Theme switch to Dark failed.");
            }
            first.Preferences.Theme = AppTheme.System;

            // Settings window
            first.SettingsCommand();
            if (SettingsWindow is null)
            {
                throw new InvalidOperationException("Settings window not opened.");
            }
            SettingsWindow.Close();

            // 10. Global Selection, Copy to clipboard (CF_UNICODETEXT & text/markdown), Escape clear, and UIA accessibility.
            first.SelectAllCommand();
            if (first.Reader.Selection is null || first.Reader.Selection.Value.IsEmpty)
            {
                throw new InvalidOperationException("SelectAllCommand must establish a non-empty selection.");
            }

            var copyRes = first.Reader.CopySelection(first.Loader.Document);
            if (copyRes is null)
            {
                throw new InvalidOperationException("CopySelection must return copied plain and markdown contents.");
            }

            var cbPlain = NativeClipboard.ReadUnicodeText();
            var cbMarkdown = NativeClipboard.ReadMarkdownBytes();
            if (cbPlain != copyRes.Value.PlainText)
            {
                throw new InvalidOperationException("Clipboard CF_UNICODETEXT does not match copied text.");
            }
            if (cbMarkdown is null || !cbMarkdown.SequenceEqual(first.Loader.Document.Source))
            {
                throw new InvalidOperationException("Clipboard text/markdown does not match document source.");
            }

            first.Reader.ClearSelection();
            if (first.Reader.Selection is not null)
            {
                throw new InvalidOperationException("ClearSelection must clear active selection.");
            }

            // UIA accessibility verification:
            // Ensure RowItem container exposes ShownText as its Name for focus announcements,
            // while child TextBlock uses AccessibilityView.Raw so Narrator ignores duplicate text.
            // Expose item type, position in set, size of set, and heading level.
            if (first.Reader.WindowHandle == 0)
            {
                throw new InvalidOperationException("Reader WindowHandle must be non-zero for native clipboard ownership.");
            }

            foreach (var row in first.Reader.BoundRows)
            {
                if (row.ItemIndex < 0) continue;
                var rowName = AutomationProperties.GetName(row);
                var expectedName = row.ShownText.Length <= 512 ? row.ShownText : row.ShownText[..512] + "…";
                if (rowName != expectedName)
                {
                    throw new InvalidOperationException($"RowItem Name expected '{expectedName}', got '{rowName}'.");
                }
                var textBlockView = AutomationProperties.GetAccessibilityView(row.TextBlock);
                if (textBlockView != Microsoft.UI.Xaml.Automation.Peers.AccessibilityView.Raw)
                {
                    throw new InvalidOperationException("RowItem TextBlock must have AccessibilityView.Raw to prevent Narrator double reading.");
                }
                var itemType = AutomationProperties.GetItemType(row);
                if (string.IsNullOrEmpty(itemType))
                {
                    throw new InvalidOperationException("RowItem must expose ItemType for screen readers.");
                }
                var posInSet = AutomationProperties.GetPositionInSet(row);
                if (posInSet != row.ItemIndex + 1)
                {
                    throw new InvalidOperationException($"RowItem PositionInSet expected {row.ItemIndex + 1}, got {posInSet}.");
                }
                var sizeOfSet = AutomationProperties.GetSizeOfSet(row);
                if (sizeOfSet <= 0)
                {
                    throw new InvalidOperationException("RowItem SizeOfSet must be greater than 0.");
                }
            }

            // Verify selection highlight and find highlight coexist without interference.
            if (first.Reader.BoundRows.Count > 0)
            {
                var testRow = first.Reader.BoundRows.First();
                testRow.SetHighlights([(0, 2, false)]);
                if (testRow.TextBlock.TextHighlighters.Count != 1)
                {
                    throw new InvalidOperationException("Find highlighter failed to attach.");
                }
                testRow.SetSelectionRange(2, 3);
                if (testRow.TextBlock.TextHighlighters.Count != 2)
                {
                    throw new InvalidOperationException("Selection highlighter failed to attach alongside find highlighter.");
                }
                testRow.SetSelectionRange(null, null);
                if (testRow.TextBlock.TextHighlighters.Count != 1)
                {
                    throw new InvalidOperationException("Selection highlighter clear removed find highlighter.");
                }
                testRow.SetHighlights([]);
                if (testRow.TextBlock.TextHighlighters.Count != 0)
                {
                    throw new InvalidOperationException("Find highlighters failed to clear.");
                }
            }

            // 11. Verify New Window, Close Window, and Exit commands close all windows cleanly.
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
    internal ReaderPreferences Preferences { get; }
    internal Action OpenCommand { get; }
    internal Action ReloadCommand { get; }
    internal Action NewCommand { get; }
    internal Action CloseCommand { get; }
    internal Action ExitCommand { get; }
    internal Action CopyCommand { get; }
    internal Action SelectAllCommand { get; }
    internal Action FindCommand { get; }
    internal Action FindNextCommand { get; }
    internal Action FindPreviousCommand { get; }
    internal Action CloseFindCommand { get; }
    internal Action ActualSizeCommand { get; }
    internal Action BiggerCommand { get; }
    internal Action SmallerCommand { get; }
    internal Action SettingsCommand { get; }

    internal MenuFlyoutItem ReloadItem { get; }
    internal TextBlock Welcome { get; }
    internal ReaderControl Reader { get; } = new();
    internal FindBar FindBar { get; } = new();
    internal InfoBar ErrorBanner { get; } = new()
    {
        IsOpen = false,
        Severity = InfoBarSeverity.Error,
        IsClosable = true,
    };

    public bool IsFindBarVisible => FindBar.Visibility == Visibility.Visible;
    private uint _findRequestId;
    private CancellationTokenSource? _findCts;

    internal ReaderWindow(ReaderApp app, ReaderPreferences preferences)
    {
        Preferences = preferences;
        Title = "Markdown";
        AppWindow.Resize(new global::Windows.Graphics.SizeInt32(960, 700));
        Reader.WindowHandle = WinRT.Interop.WindowNative.GetWindowHandle(this);

        OpenCommand = PickAndOpenFile;
        ReloadCommand = ReloadDocument;
        NewCommand = () => app.NewWindow();
        CloseCommand = Close;
        ExitCommand = app.CloseAll;
        CopyCommand = () =>
        {
            _ = Reader.CopySelection(Loader.Document);
        };
        SelectAllCommand = () =>
        {
            Reader.SelectAll();
        };
        FindCommand = ShowFind;
        FindNextCommand = NavigateFindNext;
        FindPreviousCommand = NavigateFindPrevious;
        CloseFindCommand = CloseFind;
        ActualSizeCommand = () => Preferences.TextScaleValue = 1.0;
        BiggerCommand = () => Preferences.TextScaleValue = TextScale.Larger(Preferences.TextScaleValue) ?? Preferences.TextScaleValue;
        SmallerCommand = () => Preferences.TextScaleValue = TextScale.Smaller(Preferences.TextScaleValue) ?? Preferences.TextScaleValue;
        SettingsCommand = () => app.ShowSettings();

        Loader.OnReady = () =>
        {
            _ = DispatcherQueue?.TryEnqueue(SyncFromLoader);
        };
        Closed += (_, _) =>
        {
            _findCts?.Cancel();
            _findCts?.Dispose();
            _findCts = null;
            Loader.Close();
            Reader.Clear();
        };

        FindBar.Visibility = Visibility.Collapsed;
        FindBar.QueryChanged += (_, _, _) => ExecuteFind();
        FindBar.FindNext += NavigateFindNext;
        FindBar.FindPrevious += NavigateFindPrevious;
        FindBar.CloseRequested += CloseFind;

        var root = new Grid();
        var menu = new MenuBar();

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
        menu.Items.Add(file);

        var edit = new MenuBarItem { Title = "Edit", AccessKey = "E" };
        edit.Items.Add(Command(root, "Copy", "Ctrl+C", VirtualKey.C, VirtualKeyModifiers.Control, CopyCommand, bypassWhenTextFocused: true));
        edit.Items.Add(Command(root, "Select All", "Ctrl+A", VirtualKey.A, VirtualKeyModifiers.Control, SelectAllCommand, bypassWhenTextFocused: true));
        edit.Items.Add(new MenuFlyoutSeparator());
        edit.Items.Add(Command(root, "Find…", "Ctrl+F", VirtualKey.F, VirtualKeyModifiers.Control, FindCommand));
        edit.Items.Add(Command(root, "Find Next", "F3", VirtualKey.F3, VirtualKeyModifiers.None, FindNextCommand));
        edit.Items.Add(Command(root, "Find Previous", "Shift+F3", VirtualKey.F3, VirtualKeyModifiers.Shift, FindPreviousCommand));
        menu.Items.Add(edit);

        var escAccelerator = new KeyboardAccelerator { Key = VirtualKey.Escape };
        escAccelerator.Invoked += (_, args) =>
        {
            if (IsFindBarVisible)
            {
                args.Handled = true;
                CloseFind();
            }
            else if (Reader.Selection is not null && !Reader.Selection.Value.IsEmpty)
            {
                args.Handled = true;
                Reader.ClearSelection();
            }
        };
        root.KeyboardAccelerators.Add(escAccelerator);

        var view = new MenuBarItem { Title = "View", AccessKey = "V" };
        view.Items.Add(Command(root, "Actual Size", "Ctrl+0", VirtualKey.Number0, VirtualKeyModifiers.Control, ActualSizeCommand));
        view.Items.Add(Command(root, "Make Text Bigger", "Ctrl+Plus", (VirtualKey)187, VirtualKeyModifiers.Control, BiggerCommand));
        view.Items.Add(Command(root, "Make Text Smaller", "Ctrl+Minus", (VirtualKey)189, VirtualKeyModifiers.Control, SmallerCommand));
        view.Items.Add(new MenuFlyoutSeparator());

        var themeSub = new MenuFlyoutSubItem { Text = "Theme" };
        var sysItem = new MenuFlyoutItem { Text = "System" };
        sysItem.Click += (_, _) => Preferences.Theme = AppTheme.System;
        var lightItem = new MenuFlyoutItem { Text = "Light" };
        lightItem.Click += (_, _) => Preferences.Theme = AppTheme.Light;
        var darkItem = new MenuFlyoutItem { Text = "Dark" };
        darkItem.Click += (_, _) => Preferences.Theme = AppTheme.Dark;
        themeSub.Items.Add(sysItem);
        themeSub.Items.Add(lightItem);
        themeSub.Items.Add(darkItem);
        view.Items.Add(themeSub);

        view.Items.Add(new MenuFlyoutSeparator());
        view.Items.Add(Command(root, "Settings…", "Ctrl+,", (VirtualKey)188, VirtualKeyModifiers.Control, SettingsCommand));
        menu.Items.Add(view);

        Welcome = new TextBlock
        {
            Text = "Markdown\n\nA native Markdown reader.\nChoose File > Open (Ctrl+O) to open a Markdown document.",
            TextWrapping = TextWrapping.Wrap,
            Margin = new Thickness(32),
            FontSize = 20
        };
        AutomationProperties.SetName(Welcome, "Markdown reader");
        AutomationProperties.SetName(Reader, "Document reader");
        Reader.Visibility = Visibility.Collapsed;

        root.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        root.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        root.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        root.RowDefinitions.Add(new RowDefinition());

        root.Children.Add(menu);
        Grid.SetRow(FindBar, 1);
        root.Children.Add(FindBar);
        Grid.SetRow(ErrorBanner, 2);
        root.Children.Add(ErrorBanner);
        Grid.SetRow(Welcome, 3);
        Grid.SetRow(Reader, 3);
        root.Children.Add(Welcome);
        root.Children.Add(Reader);
        Content = root;
    }

    internal void ShowFind()
    {
        FindBar.Visibility = Visibility.Visible;
        FindBar.FocusQuery();
        if (!string.IsNullOrEmpty(FindBar.Query))
        {
            ExecuteFind();
        }
    }

    internal void CloseFind()
    {
        FindBar.Visibility = Visibility.Collapsed;
        Reader.SetFindMatches(null);
        FindBar.SetStatus(string.Empty);
        Reader.Focus(FocusState.Programmatic);
    }

    internal void NavigateFindNext()
    {
        if (!IsFindBarVisible)
        {
            ShowFind();
            return;
        }
        var matches = Reader.CurrentFindMatches;
        if (matches is null || matches.Count == 0)
        {
            return;
        }
        var current = Reader.CurrentMatchIndex ?? -1;
        var next = matches.IndexAfter(current, backwards: false);
        if (next.HasValue)
        {
            Reader.NavigateToMatch(next.Value);
            FindBar.SetStatus(matches.Status(next.Value));
        }
    }

    internal void NavigateFindPrevious()
    {
        if (!IsFindBarVisible)
        {
            ShowFind();
            return;
        }
        var matches = Reader.CurrentFindMatches;
        if (matches is null || matches.Count == 0)
        {
            return;
        }
        var current = Reader.CurrentMatchIndex ?? 0;
        var prev = matches.IndexAfter(current, backwards: true);
        if (prev.HasValue)
        {
            Reader.NavigateToMatch(prev.Value);
            FindBar.SetStatus(matches.Status(prev.Value));
        }
    }

    internal void ExecuteFind()
    {
        var doc = Loader.Document;
        var plan = Loader.Plan;
        var session = Loader.Session;
        var query = FindBar.Query;
        if (doc is null || plan is null || session is null || string.IsNullOrEmpty(query))
        {
            _findCts?.Cancel();
            _findCts?.Dispose();
            _findCts = null;
            Reader.SetFindMatches(null);
            FindBar.SetStatus(string.Empty);
            return;
        }

        _findCts?.Cancel();
        _findCts?.Dispose();
        var cts = new CancellationTokenSource();
        _findCts = cts;

        var reqId = unchecked(++_findRequestId);
        var generation = doc.Generation;
        var options = new CoreFindOptions(CaseSensitive: FindBar.MatchCase, WholeWord: FindBar.WholeWord);

        _ = DispatcherQueue?.TryEnqueue(async () =>
        {
            try
            {
                var result = await session.FindAsync(query, options, generation, cts.Token);
                if (reqId != _findRequestId || doc != Loader.Document || cts.IsCancellationRequested)
                {
                    return;
                }

                var matches = DocumentFind.MapMatches(plan, doc, result);
                var currentIndex = matches.Count > 0 ? (int?)0 : null;
                Reader.SetFindMatches(matches, currentIndex);
                FindBar.SetStatus(matches.Status(currentIndex));
                if (currentIndex.HasValue)
                {
                    Reader.NavigateToMatch(currentIndex.Value);
                }
            }
            catch (OperationCanceledException)
            {
                // Canceled by newer query or window close
            }
            catch (CoreException cex) when (cex.Kind == CoreErrorKind.Superseded)
            {
                // Superseded by reload/new document
            }
            catch (Exception ex)
            {
                if (reqId == _findRequestId && doc == Loader.Document && !cts.IsCancellationRequested)
                {
                    FindBar.SetStatus($"Find error: {ex.Message}");
                }
            }
        });
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
            ErrorBanner.IsOpen = false;
            Reader.Clear();
            Welcome.Visibility = Visibility.Visible;
            Welcome.Text =
                "Markdown\n\nA native Markdown reader.\nChoose File > Open (Ctrl+O) to open a Markdown document.";
            return;
        }

        if (Loader.IsLoading && Loader.Document is null)
        {
            ErrorBanner.IsOpen = false;
            Reader.Clear();
            Welcome.Visibility = Visibility.Visible;
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
                ErrorBanner.Title = label;
                ErrorBanner.Message = activeError.ToDisplayString();
                ErrorBanner.IsOpen = true;
            }
            else
            {
                ErrorBanner.IsOpen = false;
            }

            Welcome.Text = summary;

            if (Loader.Plan is { } plan)
            {
                Reader.InstallPlan(plan, doc.Generation);
                Reader.SetTextScale(Preferences.TextScaleValue);
                Reader.Visibility = Visibility.Visible;
                Welcome.Visibility = Visibility.Collapsed;
                if (IsFindBarVisible && !string.IsNullOrEmpty(FindBar.Query))
                {
                    ExecuteFind();
                }
            }
            return;
        }

        if (Loader.Error is { } error)
        {
            ErrorBanner.IsOpen = false;
            Reader.Clear();
            Welcome.Visibility = Visibility.Visible;
            Welcome.Text = error.ToDisplayString();
        }
    }

    private static MenuFlyoutItem Command(Grid root, string text, string shortcut, VirtualKey key,
        VirtualKeyModifiers modifiers, Action invoke, bool bypassWhenTextFocused = false)
    {
        var item = new MenuFlyoutItem { Text = text, KeyboardAcceleratorTextOverride = shortcut };
        item.Click += (_, _) => invoke();
        var accelerator = new KeyboardAccelerator { Key = key, Modifiers = modifiers };
        accelerator.Invoked += (_, args) =>
        {
            if (bypassWhenTextFocused)
            {
                var focused = FocusManager.GetFocusedElement(root.XamlRoot);
                if (focused is TextBox or PasswordBox)
                {
                    return; // Do NOT handle: let the focused text control handle Ctrl+C / Ctrl+A
                }
            }
            args.Handled = true;
            invoke();
        };
        root.KeyboardAccelerators.Add(accelerator);
        return item;
    }
}
