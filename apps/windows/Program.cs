using System.Diagnostics;
using System.IO;
using System.IO.Pipes;
using System.Runtime.InteropServices;
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
    [DllImport("kernel32.dll", ExactSpelling = true)]
    private static extern nint GetCurrentProcess();

    [DllImport("kernel32.dll", ExactSpelling = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool TerminateProcess(nint hProcess, uint uExitCode);

    internal static void TerminateNow(int exitCode) =>
        _ = TerminateProcess(GetCurrentProcess(), (uint)exitCode);

    internal static bool Smoke;
    internal static bool AssociationSmoke;
    internal static string? AssociationPipeName;
    internal static string? AssociationReportPath;
    internal static int AssociationExpectedDeliveries;
    internal static string? RelaunchCheckPhase;
    internal static string? RelaunchCheckPath;
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
        if (args.Length >= 3 && args[0] == "--relaunch-check")
        {
            RelaunchCheckPhase = args[1];
            RelaunchCheckPath = args[2];
        }
        else if (args.Length >= 4 && args[0] == "--association-smoke")
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

        if (!Smoke && RelaunchCheckPhase is null && !string.IsNullOrEmpty(AssociationPipeName))
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
                // end of Application.Start
            });

            if (Smoke || AssociationSmoke || RelaunchCheckPhase is not null)
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
            if (Smoke)
            {
                // Avoid WinUI 3 unpackaged multi-window ListView DLL_PROCESS_DETACH deadlock after
                // Application.Start has returned, live Rust handles are 0, and streams are flushed.
                Console.Out.Flush();
                Console.Error.Flush();
                _ = TerminateProcess(GetCurrentProcess(), (uint)Result);
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
        if (finalCounts.Sessions != 0 || finalCounts.Jobs != 0 || finalCounts.Snapshots != 0)
        {
            Console.Error.WriteLine($"WaitZeroLiveHandles failed: {finalCounts}");
        }
        return finalCounts.Sessions == 0 && finalCounts.Jobs == 0 && finalCounts.Snapshots == 0;
    }
}

public sealed partial class ReaderApp : Application
{
    private readonly List<ReaderWindow> windows = [];
    private Microsoft.UI.Dispatching.DispatcherQueue? _dispatcher;
    private int _externalDeliveries;
    private bool _shuttingDown;
    private readonly string? _smokeRecentsPath;
    internal bool IsShuttingDown => Volatile.Read(ref _shuttingDown);

    internal IReadOnlyList<ReaderWindow> Windows => windows;
    internal ReaderPreferences Preferences { get; } = new(
        Program.Smoke ? new MemoryPreferenceStore() : new LocalSettingsPreferenceStore());
    internal RecentsStore Recents { get; }
    internal SettingsWindow? SettingsWindow { get; private set; }
    internal RecentItemsWindow? RecentItemsWindow { get; private set; }

    public ReaderApp()
    {
        if (Program.Smoke || Program.AssociationSmoke)
        {
            _smokeRecentsPath = Path.Combine(Path.GetTempPath(), $"markdown-smoke-recents-{Environment.ProcessId}.json");
            Recents = new RecentsStore(_smokeRecentsPath);
        }
        else
        {
            Recents = new RecentsStore();
        }

        UnhandledException += (sender, e) =>
        {
            Console.Error.WriteLine($"WinUI UnhandledException: {e.Message}\n{e.Exception}");
        };
        InitializeComponent();
        Preferences.Changed += () =>
        {
            foreach (var w in windows.ToArray())
            {
                ApplyPreferences(w);
            }
            SettingsWindow?.Refresh();
        };
        Recents.Changed += () =>
        {
            _ = _dispatcher?.TryEnqueue(() =>
            {
                foreach (var w in windows.ToArray())
                {
                    w.RebuildRecentMenu();
                }
            });
        };
    }

    internal void ShowRecentItems(ReaderWindow owner)
    {
        if (RecentItemsWindow is not null)
        {
            RecentItemsWindow.Activate();
            RecentItemsWindow.Refresh();
            return;
        }
        RecentItemsWindow = new RecentItemsWindow(
            Recents,
            entry => OpenRecentEntry(owner, entry),
            (entry, newPath) => RelocateRecentEntry(owner, entry, newPath),
            RevokeRecentEntry);
        RecentItemsWindow.Closed += (_, _) => RecentItemsWindow = null;
        if (RecentItemsWindow.Content is FrameworkElement fe)
        {
            fe.RequestedTheme = Preferences.ToElementTheme();
        }
        RecentItemsWindow.Activate();
    }

    internal void OpenRecentEntry(ReaderWindow preferredWindow, RecentEntry entry)
    {
        var target = preferredWindow;
        if (!windows.Contains(target))
        {
            target = windows.FirstOrDefault() ?? NewWindow();
        }
        if (entry.Kind == RecentKind.Folder)
        {
            if (FolderCapability.TryFromUserInput(entry.Path, out var folderCap, out var folderErr) && folderCap is not null)
            {
                target.OpenFolderCapability(folderCap, isFromRecents: true);
            }
            else
            {
                target.ShowFolderError(folderErr, entry.Path, isFromRecents: true);
            }
        }
        else
        {
            if (DocumentCapability.TryFromUserInput(entry.Path, out var docCap, out var docErr) && docCap is not null)
            {
                target.OpenCapability(docCap, recentSourcePath: entry.Path);
            }
            else
            {
                target.RejectSelection(docErr);
            }
        }
        target.Activate();
    }

    internal void RelocateRecentEntry(ReaderWindow preferredWindow, RecentEntry entry, string newPath)
    {
        if (!Recents.Relocate(entry.Kind, entry.Path, newPath))
        {
            return;
        }
        var relocated = new RecentEntry(entry.Kind, newPath, entry.DisplayName, DateTimeOffset.UtcNow);
        OpenRecentEntry(preferredWindow, relocated);
    }

    internal void RevokeRecentEntry(RecentEntry entry)
    {
        if (entry.Kind == RecentKind.Folder)
        {
            foreach (var w in windows.ToArray())
            {
                if (w.ActiveFolder is not null &&
                    string.Equals(
                        FolderCapability.NormalizeDirectoryRoot(w.ActiveFolder.FullPath).TrimEnd('\\'),
                        FolderCapability.NormalizeDirectoryRoot(entry.Path).TrimEnd('\\'),
                        StringComparison.OrdinalIgnoreCase))
                {
                    w.CloseFolder(revoked: true);
                }
            }
        }
        else
        {
            foreach (var w in windows.ToArray())
            {
                if (w.Loader.Capability is not null &&
                    string.Equals(w.Loader.Capability.FullPath, DocumentCapability.NormalizeLocalPath(entry.Path), StringComparison.OrdinalIgnoreCase))
                {
                    w.Loader.Capability.Revoke();
                    w.ReloadCommand();
                }
            }
        }
        Recents.Remove(entry.Kind, entry.Path);
    }

    internal void CleanupSmokeRecents()
    {
        if (_smokeRecentsPath is not null)
        {
            try
            {
                if (File.Exists(_smokeRecentsPath))
                {
                    File.Delete(_smokeRecentsPath);
                }
            }
            catch
            {
            }
        }
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

        if (Program.StartupInputs.Count == 0 &&
            !Program.Smoke &&
            !Program.AssociationSmoke &&
            Program.RelaunchCheckPhase is null)
        {
            var restored = RestorationStore.Load();
            if (restored.Count > 0)
            {
                var isFirst = true;
                foreach (var st in restored)
                {
                    if (DocumentCapability.TryFromUserInput(st.FilePath, out var cap, out _) && cap is not null)
                    {
                        var target = isFirst ? window : NewWindow();
                        isFirst = false;
                        target.RestoreFromState(st, cap);
                    }
                }
            }
        }

        if (!Program.Smoke && Program.AssociationPipeName is not null)
        {
            StartAssociationPipeServer(Program.AssociationPipeName);
        }

        if (Program.RelaunchCheckPhase is not null && Program.RelaunchCheckPath is not null)
        {
            ((FrameworkElement)window.Content).Loaded += async (_, _) =>
            {
                Program.Result = await RelaunchCheck.RunAsync(Program.RelaunchCheckPhase, Program.RelaunchCheckPath, this, window);
                Exit();
            };
            return;
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
            if (windows.Count == 1 && windows.Contains(window) && !_shuttingDown)
            {
                SaveRestorationState();
            }
            windows.Remove(window);
            if (windows.Count == 0)
            {
                SettingsWindow?.Close();
                RecentItemsWindow?.Close();
                CleanupSmokeRecents();
                Volatile.Write(ref _shuttingDown, true);
            }
        };
        ApplyPreferences(window);
        window.Activate();
        return window;
    }

    internal void SaveRestorationState()
    {
        if (Program.Smoke || Program.AssociationSmoke || Program.RelaunchCheckPhase is not null)
        {
            return;
        }

        var activeStates = new List<WindowRestorationState>();
        foreach (var w in windows)
        {
            if (w.Loader.HasFile && w.Loader.Capability is not null && w.Loader.Document is not null)
            {
                var saved = w.Reader.SavePosition(w.Loader.Document);
                activeStates.Add(WindowRestorationState.Create(
                    w.Loader.Capability.FullPath,
                    saved,
                    w.Preferences.TextScaleValue,
                    w.Reader.IsOutlineVisible));
            }
        }

        if (activeStates.Count > 0)
        {
            RestorationStore.Save(activeStates);
        }
        else
        {
            RestorationStore.Clear();
        }
    }

    internal void CloseAll()
    {
        SaveRestorationState();
        Volatile.Write(ref _shuttingDown, true);
        SettingsWindow?.Close();
        RecentItemsWindow?.Close();
        CleanupSmokeRecents();
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
            if (shortcuts.Count != 18 ||
                !shortcuts.Any(k => k.Key == VirtualKey.B && k.Modifiers == VirtualKeyModifiers.Control) ||
                !shortcuts.Any(k => k.Key == VirtualKey.O && k.Modifiers == VirtualKeyModifiers.Control) ||
                !shortcuts.Any(k => k.Key == VirtualKey.O &&
                    k.Modifiers == (VirtualKeyModifiers.Control | VirtualKeyModifiers.Shift)) ||
                !shortcuts.Any(k => k.Key == VirtualKey.R && k.Modifiers == VirtualKeyModifiers.Control) ||
                !shortcuts.Any(k => k.Key == VirtualKey.N && k.Modifiers == VirtualKeyModifiers.Control) ||
                !shortcuts.Any(k => k.Key == VirtualKey.W && k.Modifiers == VirtualKeyModifiers.Control) ||
                !shortcuts.Any(k => k.Key == VirtualKey.W &&
                    k.Modifiers == (VirtualKeyModifiers.Control | VirtualKeyModifiers.Shift)) ||
                !shortcuts.Any(k => k.Key == VirtualKey.C && k.Modifiers == VirtualKeyModifiers.Control) ||
                !shortcuts.Any(k => k.Key == VirtualKey.A && k.Modifiers == VirtualKeyModifiers.Control) ||
                !shortcuts.Any(k => k.Key == VirtualKey.Escape && k.Modifiers == VirtualKeyModifiers.None) ||
                !shortcuts.Any(k => k.Key == VirtualKey.F && k.Modifiers == VirtualKeyModifiers.Control) ||
                !shortcuts.Any(k => k.Key == VirtualKey.F &&
                    k.Modifiers == (VirtualKeyModifiers.Control | VirtualKeyModifiers.Shift)) ||
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

            // 12. External refresh, anchors and reading restoration (WIN-001H)
            static string SmokeSections(int start, int count)
            {
                var sb = new StringBuilder();
                for (var i = start; i < start + count; i++)
                {
                    sb.Append($"## Part {i}\n\nProse for part {i}, with a *marked* word.\n\n");
                }
                return sb.ToString();
            }

            var livePath = Path.Combine(tempDir, "live.md");
            var originalLive = "# Live\n\n" + SmokeSections(0, 120);
            File.WriteAllText(livePath, originalLive, new UTF8Encoding(false));

            first.OpenCapability(DocumentCapability.FromPickerPath(livePath));
            await WaitWindowIdleAsync(first);

            if (first.Loader.Document is null || first.Loader.Plan is null)
            {
                throw new InvalidOperationException("Live document initial open failed.");
            }

            var part80OutlineIndex = -1;
            for (var i = 0; i < first.Loader.Document.OutlineCount; i++)
            {
                if (first.Loader.Document.OutlineTitle(i) == "Part 80")
                {
                    part80OutlineIndex = i;
                    break;
                }
            }
            if (part80OutlineIndex < 0)
            {
                throw new InvalidOperationException("Missing Part 80 in live document outline.");
            }

            first.Reader.ActivateOutline(part80OutlineIndex);
            await Task.Delay(50);
            var livePlan = first.Loader.Plan!;
            var part80Frag = livePlan.Outline[part80OutlineIndex].Fragment;
            var markedFrag = -1;
            var markedOffset = -1;
            for (var f = part80Frag; f < Math.Min(part80Frag + 10, livePlan.Fragments.Count); f++)
            {
                var idx = livePlan.Fragments[f].Text.IndexOf("marked", StringComparison.Ordinal);
                if (idx >= 0)
                {
                    markedFrag = f;
                    markedOffset = idx;
                    break;
                }
            }
            if (markedFrag < 0)
            {
                throw new InvalidOperationException("Missing 'marked' in Part 80 section.");
            }

            first.Reader.SetSelection(new DocumentSelection(
                new DocumentPosition(markedFrag, markedOffset),
                new DocumentPosition(markedFrag, markedOffset + "marked".Length)));

            void CheckKeptPlace(string msg)
            {
                var vis = first.Reader.FirstVisibleFragmentIndex();
                var outlineIdx = -1;
                for (var i = 0; i < first.Loader.Document!.OutlineCount; i++)
                {
                    if (first.Loader.Document.OutlineTitle(i) == "Part 80")
                    {
                        outlineIdx = i;
                        break;
                    }
                }
                if (outlineIdx < 0) throw new InvalidOperationException($"{msg}: Part 80 outline missing");
                var expectedFrag = first.Loader.Plan!.Outline[outlineIdx].Fragment;
                if (Math.Abs(vis - expectedFrag) > 1)
                {
                    throw new InvalidOperationException($"{msg}: top line not kept (firstVis={vis}, target={expectedFrag})");
                }
                var sel = first.Reader.Selection;
                if (sel is null || sel.Value.IsEmpty)
                {
                    throw new InvalidOperationException($"{msg}: selection lost");
                }
                var text = first.Loader.Plan.TextBetween(sel.Value.Anchor, sel.Value.Active);
                if (text != "marked")
                {
                    throw new InvalidOperationException($"{msg}: selection text mismatch '{text}'");
                }
            }

            CheckKeptPlace("Initial placement");

            var prevGen = first.Loader.Document.Generation;
            var editedLive = "# Live\n\nA new introduction.\n\n" + SmokeSections(200, 30) + SmokeSections(0, 120);
            File.WriteAllText(livePath, editedLive, new UTF8Encoding(false));

            var swLive = Stopwatch.StartNew();
            while (swLive.ElapsedMilliseconds < 5000 &&
                   (first.Loader.Document.Generation == prevGen || first.Loader.IsLoading))
            {
                first.SyncFromLoader();
                await Task.Delay(20);
            }
            if (first.Loader.Document.Generation == prevGen)
            {
                first.Loader.RefreshExternal();
                await WaitWindowIdleAsync(first);
            }
            CheckKeptPlace("After external edit");
            if (first.Loader.Document.OutlineCount != 151)
            {
                throw new InvalidOperationException($"Expected 151 outlines after external edit, got {first.Loader.Document.OutlineCount}");
            }
            if (first.ErrorBanner.IsOpen)
            {
                throw new InvalidOperationException("Notice banner should be closed after successful edit.");
            }

            var keptGen = first.Loader.Document.Generation;
            File.Delete(livePath);
            first.Loader.RefreshExternal();
            var swDel = Stopwatch.StartNew();
            while (swDel.ElapsedMilliseconds < 5000 &&
                   (first.Loader.Error is null || first.Loader.IsLoading))
            {
                first.SyncFromLoader();
                await Task.Delay(20);
            }

            if (first.Loader.Error != DocumentError.NotFound ||
                !first.ErrorBanner.IsOpen ||
                first.Loader.Document is null ||
                first.Loader.Document.Generation != keptGen ||
                first.Loader.Availability() != CoreAvailability.Unavailable(CoreUnavailability.Deleted))
            {
                throw new InvalidOperationException($"External deletion notice handling failed: Error={first.Loader.Error}, ErrorBanner.IsOpen={first.ErrorBanner.IsOpen}, DocGen={first.Loader.Document?.Generation} vs keptGen={keptGen}, Avail={first.Loader.Availability()}");
            }

            File.WriteAllText(livePath, editedLive, new UTF8Encoding(false));
            first.Loader.RefreshExternal();
            var swRet = Stopwatch.StartNew();
            while (swRet.ElapsedMilliseconds < 5000 &&
                   (first.Loader.Error is not null || first.Loader.IsLoading))
            {
                first.SyncFromLoader();
                await Task.Delay(20);
            }

            if (first.Loader.Error is not null ||
                first.ErrorBanner.IsOpen ||
                first.Loader.Document.Generation != keptGen)
            {
                throw new InvalidOperationException("Reappearance did not clear notice cleanly without re-parsing.");
            }
            CheckKeptPlace("After file reappearance");

            first.ReloadCommand();
            await WaitWindowIdleAsync(first);
            CheckKeptPlace("After manual Reload");

            var savedPosition = first.Reader.SavePosition(first.Loader.Document);
            if (savedPosition is null)
            {
                throw new InvalidOperationException("SavePosition returned null.");
            }
            var restState = WindowRestorationState.Create(
                livePath,
                savedPosition,
                first.Preferences.TextScaleValue,
                outlineShown: true);

            var customRestPath = Path.Combine(tempDir, "restoration-test.json");
            RestorationStore.Save([restState], customRestPath);
            var loadedList = RestorationStore.Load(customRestPath);
            if (loadedList.Count != 1 || loadedList[0].FilePath != livePath)
            {
                throw new InvalidOperationException("RestorationStore load/save roundtrip failed.");
            }

            var restoredWindow = NewWindow();
            restoredWindow.PendingRestorePosition = loadedList[0].ToSavedPosition();
            restoredWindow.OpenCapability(DocumentCapability.FromPickerPath(livePath));
            await WaitWindowIdleAsync(restoredWindow);
            await Task.Delay(50);

            var restoredPlan = restoredWindow.Loader.Plan!;
            var restoredPart80Idx = -1;
            for (var i = 0; i < restoredWindow.Loader.Document!.OutlineCount; i++)
            {
                if (restoredWindow.Loader.Document.OutlineTitle(i) == "Part 80")
                {
                    restoredPart80Idx = i;
                    break;
                }
            }
            var restoredTargetFrag = restoredPlan.Outline[restoredPart80Idx].Fragment;
            var restoredVis = -1;
            var swRestored = Stopwatch.StartNew();
            while (swRestored.ElapsedMilliseconds < 3000)
            {
                restoredVis = restoredWindow.Reader.FirstVisibleFragmentIndex();
                if (Math.Abs(restoredVis - restoredTargetFrag) <= 1)
                {
                    break;
                }
                await Task.Delay(25);
            }
            if (Math.Abs(restoredVis - restoredTargetFrag) > 1)
            {
                throw new InvalidOperationException($"Restored window top mismatch: vis={restoredVis}, target={restoredTargetFrag}");
            }
            var restoredSel = restoredWindow.Reader.Selection;
            if (restoredSel is null || restoredSel.Value.IsEmpty ||
                restoredPlan.TextBetween(restoredSel.Value.Anchor, restoredSel.Value.Active) != "marked")
            {
                throw new InvalidOperationException("Restored window selection mismatch.");
            }
            restoredWindow.CloseCommand();
            await Task.Delay(50);

            var garbageState = new WindowRestorationState(
                livePath,
                Convert.ToBase64String(Encoding.UTF8.GetBytes("not-an-anchor")),
                null,
                null,
                1.0,
                true);
            var garbageWin = NewWindow();
            garbageWin.PendingRestorePosition = garbageState.ToSavedPosition();
            garbageWin.OpenCapability(DocumentCapability.FromPickerPath(livePath));
            await WaitWindowIdleAsync(garbageWin);
            if (garbageWin.Reader.FirstVisibleFragmentIndex() != 0)
            {
                throw new InvalidOperationException("Garbage anchor should open at top.");
            }
            garbageWin.CloseCommand();
            await Task.Delay(50);

            // 12b. WIN-002A: Folder capability grants, recents, Close Folder revocation, moved-folder recovery, and Recent Items window.
            var wsDir = Path.Combine(tempDir, "win002a-ws");
            var wsMovedDir = Path.Combine(tempDir, "win002a-ws-moved");
            Directory.CreateDirectory(wsDir);
            var wsDocPath = Path.Combine(wsDir, "inside.md");
            File.WriteAllText(wsDocPath, "# Workspace Doc\n\nInside approved folder.\n", new UTF8Encoding(false));

            var win002a = NewWindow();
            DocumentPicker.EnqueueFolderAutomationCancel();
            win002a.OpenFolderCommand();
            if (win002a.ActiveFolder is not null || win002a.CloseFolderItem.IsEnabled)
            {
                throw new InvalidOperationException("Cancelled Open Folder should not activate a folder.");
            }

            DocumentPicker.EnqueueFolderAutomationSelect(wsDir);
            win002a.OpenFolderCommand();
            if (win002a.ActiveFolder is null || !win002a.CloseFolderItem.IsEnabled)
            {
                throw new InvalidOperationException("Selected Open Folder did not activate folder grant.");
            }
            if (Recents.Folders.Count == 0 || win002a.OpenRecentMenu.Items.Count < 3)
            {
                throw new InvalidOperationException("Recents store or Open Recent menu was not updated for folder.");
            }

            // 12c. WIN-002B: Native workspace browser sidebar populated on folder open.
            var swWs = Stopwatch.StartNew();
            while (swWs.ElapsedMilliseconds < 3000 && win002a.WorkspaceSidebar.VisibleRows.Count < 1)
            {
                await Task.Delay(20);
            }
            if (!win002a.IsWorkspaceSidebarVisible ||
                !win002a.ToggleWorkspaceSidebarItem.IsEnabled ||
                !win002a.FindInWorkspaceItem.IsEnabled ||
                win002a.WorkspaceSidebar.VisibleRows.Count != 1 ||
                win002a.WorkspaceSidebar.VisibleRows[0].Entry.Name != "inside.md")
            {
                throw new InvalidOperationException($"Unexpected workspace sidebar state for win002a: visible={win002a.IsWorkspaceSidebarVisible}, count={win002a.WorkspaceSidebar.VisibleRows.Count}");
            }

            // 12d. WIN-002C: Native workspace search UI, bounded search, and result navigation.
            win002a.ShowWorkspaceSearch();
            if (win002a.WorkspaceSidebar.Mode != WorkspaceSidebarMode.Search)
            {
                throw new InvalidOperationException("Find in Workspace did not switch sidebar to Search mode.");
            }
            win002a.WorkspaceSidebar.SearchPanel.Query = "approved";
            win002a.WorkspaceSidebar.SearchPanel.RequestSearch();
            var swSearch = Stopwatch.StartNew();
            while (swSearch.ElapsedMilliseconds < 3000 &&
                (win002a.WorkspaceSidebar.SearchPanel.CurrentSnapshot.IsRunning ||
                 win002a.WorkspaceSidebar.SearchPanel.CurrentSnapshot.ContentHits.Count < 1))
            {
                await Task.Delay(20);
            }
            var searchSnap = win002a.WorkspaceSidebar.SearchPanel.CurrentSnapshot;
            if (searchSnap.IsRunning || searchSnap.ContentHits.Count != 1 || searchSnap.ContentHits[0].RelativePath != "inside.md")
            {
                throw new InvalidOperationException(
                    $"Workspace search failed: running={searchSnap.IsRunning}, contentHits={searchSnap.ContentHits.Count}");
            }
            win002a.OpenWorkspaceContentHit(searchSnap.ContentHits[0]);
            await WaitWindowIdleAsync(win002a);
            if (win002a.Loader.Document is null || win002a.Reader.Visibility != Visibility.Visible)
            {
                throw new InvalidOperationException("Workspace search content hit navigation failed to open document.");
            }
            win002a.WorkspaceSidebar.SetMode(WorkspaceSidebarMode.Files);

            // Open inside.md via File > Open while ActiveFolder is set -> binds to folder grant.
            DocumentPicker.EnqueueAutomationSelect(wsDocPath);
            win002a.OpenCommand();
            await WaitWindowIdleAsync(win002a);
            if (win002a.Loader.Document is null ||
                win002a.Loader.Error is not null ||
                win002a.Loader.Capability?.ParentGrant != win002a.ActiveFolder.Grant)
            {
                throw new InvalidOperationException("File > Open inside active folder did not bind to folder grant.");
            }

            // Close Folder revokes the folder grant and its bound document.
            win002a.CloseFolderCommand();
            await WaitWindowIdleAsync(win002a);
            if (win002a.ActiveFolder is not null || win002a.CloseFolderItem.IsEnabled)
            {
                throw new InvalidOperationException("Close Folder did not clear active folder.");
            }
            if (win002a.Loader.Error != DocumentError.AccessRevoked)
            {
                throw new InvalidOperationException($"Close Folder should revoke bound document (got {win002a.Loader.Error}).");
            }

            // Moved folder triggers ErrorBanner recovery, then Locate... relocates and opens it.
            Directory.Move(wsDir, wsMovedDir);
            var staleFolderEntry = Recents.Folders[0];
            OpenRecentEntry(win002a, staleFolderEntry);
            if (!win002a.ErrorBanner.IsOpen || win002a.RecoveryPrimaryButton.Visibility != Visibility.Visible)
            {
                throw new InvalidOperationException("Moved recent folder did not open inline recovery banner.");
            }
            RelocateRecentEntry(win002a, staleFolderEntry, wsMovedDir);
            if (win002a.ActiveFolder is null || win002a.ErrorBanner.Title == "Folder access issue")
            {
                throw new InvalidOperationException("Moved recent folder recovery failed.");
            }
            if (!win002a.ActiveFolder.ResolveDocument("inside.md", out var movedDocCap, out _) || movedDocCap is null)
            {
                throw new InvalidOperationException("Failed to resolve inside.md from relocated folder.");
            }
            win002a.OpenCapability(movedDocCap);
            await WaitWindowIdleAsync(win002a);
            if (win002a.Loader.Document is null || win002a.Loader.Error is not null || win002a.ErrorBanner.IsOpen)
            {
                throw new InvalidOperationException("Document open after moved-folder recovery failed.");
            }

            win002a.RecentItemsCommand();
            if (RecentItemsWindow is null)
            {
                throw new InvalidOperationException("RecentItemsWindow did not open.");
            }
            await Task.Delay(50);
            RecentItemsWindow.Close();
            win002a.CloseCommand();
            await Task.Delay(50);

            // 13. Verify New Window, Close Window, and Exit commands close all windows cleanly.
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
            await Task.Delay(50);
            Program.Result = 0;
            first.ExitCommand();
            if (windows.Count != 0)
            {
                Program.Result = 1;
            }
            if (Program.Result == 0 && !Program.WaitZeroLiveHandles(2000))
            {
                Program.Result = 1;
            }
            var exitCode = Program.Result;
            var watchdog = new Thread(() =>
            {
                Thread.Sleep(250);
                Console.Out.Flush();
                Console.Error.Flush();
                Program.TerminateNow(exitCode);
            })
            {
                IsBackground = true,
                Name = "smoke-exit-watchdog",
            };
            watchdog.Start();
            Exit();
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
    private readonly ReaderApp _app;
    internal DocumentLoader Loader { get; } = new();
    internal ReaderPreferences Preferences { get; }
    internal FolderCapability? ActiveFolder { get; private set; }
    private DocumentWatcher? _watcher;
    private bool _isClosing;
    private CoreDocument? _displayedDoc;
    private DocumentCapability? _recordedRecentCap;
    private string? _pendingRecentFilePath;
    private string? _lastFailedFolderPath;
    private bool _lastFailedFolderFromRecents;
    private FolderError? _activeFolderError;
    internal SavedPosition? PendingRestorePosition { get; set; }
    internal Action OpenCommand { get; }
    internal Action OpenFolderCommand { get; }
    internal Action CloseFolderCommand { get; }
    internal Action RecentItemsCommand { get; }
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
    internal Action FindInWorkspaceCommand { get; }
    internal Action ActualSizeCommand { get; }
    internal Action BiggerCommand { get; }
    internal Action SmallerCommand { get; }
    internal Action ToggleWorkspaceSidebarCommand { get; }
    internal Action SettingsCommand { get; }

    internal MenuFlyoutItem ReloadItem { get; }
    internal MenuFlyoutItem OpenFolderItem { get; }
    internal MenuFlyoutItem CloseFolderItem { get; }
    internal MenuFlyoutItem FindInWorkspaceItem { get; }
    internal MenuFlyoutItem ToggleWorkspaceSidebarItem { get; }
    internal MenuFlyoutSubItem OpenRecentMenu { get; }
    internal TextBlock Welcome { get; }
    internal WorkspaceSidebar WorkspaceSidebar { get; } = new();
    internal ReaderControl Reader { get; } = new();
    internal FindBar FindBar { get; } = new();
    internal InfoBar ErrorBanner { get; } = new()
    {
        IsOpen = false,
        Severity = InfoBarSeverity.Error,
        IsClosable = true,
    };
    private readonly StackPanel _recoveryActionsPanel = new()
    {
        Orientation = Orientation.Horizontal,
        Spacing = 8,
        Margin = new Thickness(0, 0, 0, 8),
        Visibility = Visibility.Collapsed,
    };
    internal Button RecoveryPrimaryButton { get; } = new() { Content = "Locate…" };
    internal Button RecoveryRetryButton { get; } = new() { Content = "Try Again" };
    internal Button RecoveryRemoveButton { get; } = new() { Content = "Remove from Recents" };

    public bool IsFindBarVisible => FindBar.Visibility == Visibility.Visible;
    public bool IsWorkspaceSidebarVisible => WorkspaceSidebar.Visibility == Visibility.Visible;
    private uint _findRequestId;
    private CancellationTokenSource? _findCts;
    private ulong _workspaceGeneration;
    private CancellationTokenSource? _workspaceCts;
    private bool _workspaceSidebarUserVisible = true;
    private readonly WorkspaceSearchRunner _workspaceSearchRunner = new();
    private long _workspaceSearchSerial;
    private WorkspaceSearchContentHit? _pendingSearchHit;

    internal ReaderWindow(ReaderApp app, ReaderPreferences preferences)
    {
        _app = app;
        Preferences = preferences;
        Title = "Markdown";
        AppWindow.Resize(new global::Windows.Graphics.SizeInt32(960, 700));
        Reader.WindowHandle = WinRT.Interop.WindowNative.GetWindowHandle(this);

        OpenCommand = PickAndOpenFile;
        OpenFolderCommand = PickAndOpenFolder;
        CloseFolderCommand = () => CloseFolder(revoked: false);
        RecentItemsCommand = () => app.ShowRecentItems(this);
        ReloadCommand = ReloadDocument;
        NewCommand = () => app.NewWindow();
        CloseCommand = () =>
        {
            _isClosing = true;
            Close();
        };
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
        FindInWorkspaceCommand = ShowWorkspaceSearch;
        ActualSizeCommand = () => Preferences.TextScaleValue = 1.0;
        BiggerCommand = () => Preferences.TextScaleValue = TextScale.Larger(Preferences.TextScaleValue) ?? Preferences.TextScaleValue;
        SmallerCommand = () => Preferences.TextScaleValue = TextScale.Smaller(Preferences.TextScaleValue) ?? Preferences.TextScaleValue;
        ToggleWorkspaceSidebarCommand = ToggleWorkspaceSidebar;
        SettingsCommand = () => app.ShowSettings();

        Loader.OnReady = () =>
        {
            _ = DispatcherQueue?.TryEnqueue(SyncFromLoader);
        };
        Activated += (_, args) =>
        {
            if (!_isClosing && !app.IsShuttingDown && args.WindowActivationState != WindowActivationState.Deactivated && Loader.HasFile)
            {
                Loader.RefreshExternal();
                SyncFromLoader();
            }
        };

        Closed += (_, _) =>
        {
            _isClosing = true;
            ActiveFolder?.Revoke();
            ActiveFolder = null;
            _watcher?.Dispose();
            _watcher = null;
            _findCts?.Cancel();
            _findCts?.Dispose();
            _findCts = null;
            _workspaceCts?.Cancel();
            _workspaceCts?.Dispose();
            _workspaceCts = null;
            _workspaceSearchRunner.Dispose();
            _pendingSearchHit = null;
            var oldDoc = _displayedDoc;
            _displayedDoc = null;
            oldDoc?.Dispose();
            Loader.Close();
            Reader.Clear();
            WorkspaceSidebar.Clear();
        };

        FindBar.Visibility = Visibility.Collapsed;
        FindBar.QueryChanged += (_, _, _) => ExecuteFind();
        FindBar.FindNext += NavigateFindNext;
        FindBar.FindPrevious += NavigateFindPrevious;
        FindBar.CloseRequested += CloseFind;

        var root = new Grid();
        var menu = new MenuBar();

        _recoveryActionsPanel.Children.Add(RecoveryPrimaryButton);
        _recoveryActionsPanel.Children.Add(RecoveryRetryButton);
        _recoveryActionsPanel.Children.Add(RecoveryRemoveButton);
        ErrorBanner.Content = _recoveryActionsPanel;
        ErrorBanner.CloseButtonClick += (_, _) => _activeFolderError = null;

        var file = new MenuBarItem { Title = "File", AccessKey = "F" };
        file.Items.Add(Command(root, "Open…", "Ctrl+O", VirtualKey.O, VirtualKeyModifiers.Control, OpenCommand));
        OpenFolderItem = Command(root, "Open Folder…", "Ctrl+Shift+O", VirtualKey.O,
            VirtualKeyModifiers.Control | VirtualKeyModifiers.Shift, OpenFolderCommand);
        file.Items.Add(OpenFolderItem);
        OpenRecentMenu = new MenuFlyoutSubItem { Text = "Open Recent" };
        file.Items.Add(OpenRecentMenu);
        CloseFolderItem = new MenuFlyoutItem { Text = "Close Folder", IsEnabled = false };
        CloseFolderItem.Click += (_, _) => CloseFolderCommand();
        file.Items.Add(CloseFolderItem);
        file.Items.Add(new MenuFlyoutSeparator());
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
        RebuildRecentMenu();

        var edit = new MenuBarItem { Title = "Edit", AccessKey = "E" };
        edit.Items.Add(Command(root, "Copy", "Ctrl+C", VirtualKey.C, VirtualKeyModifiers.Control, CopyCommand, bypassWhenTextFocused: true));
        edit.Items.Add(Command(root, "Select All", "Ctrl+A", VirtualKey.A, VirtualKeyModifiers.Control, SelectAllCommand, bypassWhenTextFocused: true));
        edit.Items.Add(new MenuFlyoutSeparator());
        edit.Items.Add(Command(root, "Find…", "Ctrl+F", VirtualKey.F, VirtualKeyModifiers.Control, FindCommand));
        edit.Items.Add(Command(root, "Find Next", "F3", VirtualKey.F3, VirtualKeyModifiers.None, FindNextCommand));
        edit.Items.Add(Command(root, "Find Previous", "Shift+F3", VirtualKey.F3, VirtualKeyModifiers.Shift, FindPreviousCommand));
        FindInWorkspaceItem = Command(
            root,
            "Find in Workspace…",
            "Ctrl+Shift+F",
            VirtualKey.F,
            VirtualKeyModifiers.Control | VirtualKeyModifiers.Shift,
            FindInWorkspaceCommand);
        FindInWorkspaceItem.IsEnabled = false;
        edit.Items.Add(FindInWorkspaceItem);
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
        ToggleWorkspaceSidebarItem = Command(root, "Toggle Workspace Sidebar", "Ctrl+B", VirtualKey.B, VirtualKeyModifiers.Control, ToggleWorkspaceSidebarCommand);
        ToggleWorkspaceSidebarItem.IsEnabled = false;
        view.Items.Add(ToggleWorkspaceSidebarItem);
        view.Items.Add(Command(root, "Settings…", "Ctrl+,", (VirtualKey)188, VirtualKeyModifiers.Control, SettingsCommand));
        menu.Items.Add(view);

        WorkspaceSidebar.Visibility = Visibility.Collapsed;
        WorkspaceSidebar.FileActivated += OpenWorkspaceEntry;
        WorkspaceSidebar.RefreshRequested += () => RefreshWorkspaceTree(resetExpanded: false);
        WorkspaceSidebar.CloseRequested += () =>
        {
            _workspaceSidebarUserVisible = false;
            UpdateWorkspaceSidebarVisibility();
        };
        WorkspaceSidebar.SearchPanel.SearchRequested += ExecuteWorkspaceSearch;
        WorkspaceSidebar.SearchPanel.StopRequested += StopWorkspaceSearch;
        WorkspaceSidebar.SearchPanel.NameHitActivated += OpenWorkspaceNameHit;
        WorkspaceSidebar.SearchPanel.ContentHitActivated += OpenWorkspaceContentHit;

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

        var contentGrid = new Grid();
        contentGrid.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        contentGrid.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        Grid.SetColumn(WorkspaceSidebar, 0);
        Grid.SetColumn(Welcome, 1);
        Grid.SetColumn(Reader, 1);
        contentGrid.Children.Add(WorkspaceSidebar);
        contentGrid.Children.Add(Welcome);
        contentGrid.Children.Add(Reader);

        root.Children.Add(menu);
        Grid.SetRow(FindBar, 1);
        root.Children.Add(FindBar);
        Grid.SetRow(ErrorBanner, 2);
        root.Children.Add(ErrorBanner);
        Grid.SetRow(contentGrid, 3);
        root.Children.Add(contentGrid);
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

    internal void RestoreFromState(WindowRestorationState state, DocumentCapability cap)
    {
        PendingRestorePosition = state.ToSavedPosition();
        if (state.TextScale > 0)
        {
            Preferences.TextScaleValue = state.TextScale;
            Reader.SetTextScale(state.TextScale);
        }
        Reader.IsOutlineVisible = state.OutlineShown;
        OpenCapability(cap);
    }

    internal void OpenCapability(DocumentCapability capability, string? recentSourcePath = null)
    {
        _activeFolderError = null;
        _pendingRecentFilePath = recentSourcePath;
        _watcher?.Dispose();
        _watcher = null;
        if (!string.IsNullOrEmpty(capability.FullPath))
        {
            _watcher = new DocumentWatcher(capability.FullPath, () =>
            {
                _ = DispatcherQueue?.TryEnqueue(() =>
                {
                    Loader.RefreshExternal();
                    SyncFromLoader();
                });
            });
        }
        Loader.Open(capability);
        SyncFromLoader();
    }

    internal void OpenFolderCapability(FolderCapability folder, bool isFromRecents = false)
    {
        if (!folder.ValidateAccess(out var folderError))
        {
            ShowFolderError(folderError, folder.FullPath, isFromRecents);
            return;
        }

        _activeFolderError = null;
        _lastFailedFolderPath = null;
        _lastFailedFolderFromRecents = false;
        if (ActiveFolder is not null)
        {
            if (ActiveFolder.SameFolderAs(folder))
            {
                _app.Recents.RecordFolder(folder);
                SyncFromLoader();
                return;
            }
            CloseFolder(revoked: false);
        }
        ActiveFolder = folder;
        CloseFolderItem.IsEnabled = true;
        ToggleWorkspaceSidebarItem.IsEnabled = true;
        FindInWorkspaceItem.IsEnabled = true;
        _workspaceSidebarUserVisible = true;
        UpdateWorkspaceSidebarVisibility();
        _app.Recents.RecordFolder(folder);
        RefreshWorkspaceTree(resetExpanded: true);
        SyncFromLoader();
    }

    internal void ToggleWorkspaceSidebar()
    {
        if (ActiveFolder is null)
        {
            return;
        }
        _workspaceSidebarUserVisible = !_workspaceSidebarUserVisible;
        UpdateWorkspaceSidebarVisibility();
    }

    private void UpdateWorkspaceSidebarVisibility()
    {
        WorkspaceSidebar.Visibility = ActiveFolder is not null && _workspaceSidebarUserVisible
            ? Visibility.Visible
            : Visibility.Collapsed;
    }

    internal async void RefreshWorkspaceTree(bool resetExpanded)
    {
        var folder = ActiveFolder;
        if (folder is null)
        {
            return;
        }

        _workspaceCts?.Cancel();
        _workspaceCts?.Dispose();
        var cts = new CancellationTokenSource();
        _workspaceCts = cts;
        var generation = ++_workspaceGeneration;
        WorkspaceSidebar.SetLoading(folder.DisplayName, clearTree: resetExpanded);

        try
        {
            var tree = await WorkspaceTree.EnumerateAsync(folder, cts.Token);
            if (_isClosing || generation != _workspaceGeneration || ActiveFolder != folder)
            {
                return;
            }
            WorkspaceSidebar.SetTree(folder.DisplayName, tree, resetExpanded);
        }
        catch (OperationCanceledException)
        {
        }
        catch (WorkspaceEnumerationException ex)
        {
            if (_isClosing || generation != _workspaceGeneration || ActiveFolder != folder)
            {
                return;
            }
            WorkspaceSidebar.SetError(ex.Error.Message());
            ShowFolderError(ex.Error, folder.FullPath, isFromRecents: false);
        }
        catch (Exception)
        {
            if (_isClosing || generation != _workspaceGeneration || ActiveFolder != folder)
            {
                return;
            }
            WorkspaceSidebar.SetError("Could not load workspace.");
            ShowFolderError(FolderError.OpenFailed, folder.FullPath, isFromRecents: false);
        }
    }

    private void OpenWorkspaceEntry(WorkspaceEntry entry)
    {
        var folder = ActiveFolder;
        if (folder is null || entry.IsDirectory)
        {
            return;
        }

        if (!folder.ResolveDocument(entry.RelativePath, out var docCap, out var error) || docCap is null)
        {
            ShowFolderError(error, folder.FullPath, isFromRecents: false);
            return;
        }

        OpenCapability(docCap);
    }

    internal void ShowWorkspaceSearch()
    {
        if (ActiveFolder is null)
        {
            return;
        }
        _workspaceSidebarUserVisible = true;
        UpdateWorkspaceSidebarVisibility();
        WorkspaceSidebar.SetMode(WorkspaceSidebarMode.Search);
        WorkspaceSidebar.SearchPanel.FocusQuery();
        if (!string.IsNullOrWhiteSpace(WorkspaceSidebar.SearchPanel.Query))
        {
            WorkspaceSidebar.SearchPanel.RequestSearch();
        }
    }

    internal void ExecuteWorkspaceSearch(string query, WorkspaceSearchOptions options)
    {
        var folder = ActiveFolder;
        if (folder is null)
        {
            _workspaceSearchRunner.Cancel();
            WorkspaceSidebar.SearchPanel.ApplySnapshot(WorkspaceSearchSnapshot.Empty);
            return;
        }

        var trimmed = query.Trim();
        if (string.IsNullOrEmpty(trimmed))
        {
            _workspaceSearchRunner.Cancel();
            _workspaceSearchSerial++;
            WorkspaceSidebar.SearchPanel.ApplySnapshot(WorkspaceSearchSnapshot.Empty with { Query = query });
            return;
        }

        var tree = WorkspaceSidebar.CurrentTree;
        var serial = _workspaceSearchRunner.Start(folder, tree, query, options, (updateSerial, snapshot) =>
        {
            _ = DispatcherQueue?.TryEnqueue(() =>
            {
                if (_isClosing || updateSerial != _workspaceSearchSerial || ActiveFolder != folder)
                {
                    return;
                }
                WorkspaceSidebar.SearchPanel.ApplySnapshot(snapshot);
            });
        });
        _workspaceSearchSerial = serial;
    }

    internal void StopWorkspaceSearch()
    {
        _workspaceSearchRunner.Cancel();
        _workspaceSearchSerial++;
        var current = WorkspaceSidebar.SearchPanel.CurrentSnapshot;
        if (current.IsRunning)
        {
            WorkspaceSidebar.SearchPanel.ApplySnapshot(current with
            {
                IsRunning = false,
                IsCancelled = true,
            });
        }
    }

    private void OpenWorkspaceNameHit(WorkspaceSearchNameHit hit)
    {
        var folder = ActiveFolder;
        if (folder is null)
        {
            return;
        }
        if (!folder.ResolveDocument(hit.RelativePath, out var docCap, out var error) || docCap is null)
        {
            ShowFolderError(error, folder.FullPath, isFromRecents: false);
            return;
        }
        _pendingSearchHit = null;
        OpenCapability(docCap);
    }

    internal void OpenWorkspaceContentHit(WorkspaceSearchContentHit hit)
    {
        var folder = ActiveFolder;
        if (folder is null)
        {
            return;
        }
        if (!folder.ResolveDocument(hit.RelativePath, out var docCap, out var error) || docCap is null)
        {
            ShowFolderError(error, folder.FullPath, isFromRecents: false);
            return;
        }

        if (Loader.HasFile &&
            Loader.Capability is { } currentCap &&
            string.Equals(currentCap.FullPath, docCap.FullPath, StringComparison.OrdinalIgnoreCase) &&
            _displayedDoc is not null &&
            Loader.Plan is { } currentPlan)
        {
            docCap.Revoke();
            ApplyContentHitNavigation(_displayedDoc, currentPlan, hit);
            return;
        }

        _pendingSearchHit = hit;
        OpenCapability(docCap);
    }

    private void ApplyContentHitNavigation(CoreDocument doc, RenderPlan plan, WorkspaceSearchContentHit hit)
    {
        if (plan.Fragments.Count == 0)
        {
            return;
        }

        if ((ulong)doc.SourceByteCount == hit.SourceLength &&
            string.Equals(doc.SourceSha256, hit.SourceSha256, StringComparison.OrdinalIgnoreCase))
        {
            var pos = plan.PositionAtSourceOffset(hit.SpanStart, doc);
            Reader.ScrollToFragment(pos.FragmentIndex);
            return;
        }

        if (hit.Anchor.Length > 0)
        {
            try
            {
                var resolved = doc.ResolveAnchor(hit.Anchor);
                if (resolved.HasValue)
                {
                    var pos = plan.PositionAtSourceOffset(resolved.Value.SourceOffset, doc);
                    Reader.ScrollToFragment(pos.FragmentIndex);
                }
            }
            catch (CoreException)
            {
                // Anchor could not be resolved; keep document top.
            }
        }
    }

    internal void CloseFolder(bool revoked = false)
    {
        if (ActiveFolder is null)
        {
            return;
        }

        _workspaceGeneration++;
        _workspaceCts?.Cancel();
        _workspaceCts?.Dispose();
        _workspaceCts = null;
        _workspaceSearchRunner.Cancel();
        _workspaceSearchSerial++;
        _pendingSearchHit = null;

        var hadFolderBoundDoc = Loader.Capability?.ParentGrant == ActiveFolder.Grant;
        ActiveFolder.Revoke();
        ActiveFolder = null;
        CloseFolderItem.IsEnabled = false;
        ToggleWorkspaceSidebarItem.IsEnabled = false;
        FindInWorkspaceItem.IsEnabled = false;
        WorkspaceSidebar.Clear();
        UpdateWorkspaceSidebarVisibility();

        if (hadFolderBoundDoc)
        {
            Loader.Reload();
        }

        if (revoked && !hadFolderBoundDoc)
        {
            ShowFolderError(FolderError.AccessRevoked, null, isFromRecents: false);
            return;
        }

        SyncFromLoader();
    }

    internal void ShowFolderError(FolderError error, string? folderPath, bool isFromRecents)
    {
        _activeFolderError = error;
        _lastFailedFolderPath = folderPath;
        _lastFailedFolderFromRecents = isFromRecents;
        ErrorBanner.Title = "Folder access issue";
        ErrorBanner.Message = error.ToDisplayString();
        ConfigureFolderRecoveryActions(error);
        ErrorBanner.IsOpen = true;
    }

    internal void RebuildRecentMenu()
    {
        OpenRecentMenu.Items.Clear();

        var folders = _app.Recents.Folders;
        var files = _app.Recents.Files;

        if (folders.Count > 0)
        {
            var foldersHeader = new MenuFlyoutItem { Text = "Folders", IsEnabled = false };
            OpenRecentMenu.Items.Add(foldersHeader);
            foreach (var entry in folders)
            {
                var captured = entry;
                var item = new MenuFlyoutItem { Text = entry.DisplayName };
                item.Click += (_, _) => _app.OpenRecentEntry(this, captured);
                OpenRecentMenu.Items.Add(item);
            }
            OpenRecentMenu.Items.Add(new MenuFlyoutSeparator());
        }

        if (files.Count > 0)
        {
            var filesHeader = new MenuFlyoutItem { Text = "Files", IsEnabled = false };
            OpenRecentMenu.Items.Add(filesHeader);
            foreach (var entry in files)
            {
                var captured = entry;
                var item = new MenuFlyoutItem { Text = entry.DisplayName };
                item.Click += (_, _) => _app.OpenRecentEntry(this, captured);
                OpenRecentMenu.Items.Add(item);
            }
            OpenRecentMenu.Items.Add(new MenuFlyoutSeparator());
        }

        if (folders.Count == 0 && files.Count == 0)
        {
            OpenRecentMenu.Items.Add(new MenuFlyoutItem { Text = "(No Recent Items)", IsEnabled = false });
            OpenRecentMenu.Items.Add(new MenuFlyoutSeparator());
        }

        var manageItem = new MenuFlyoutItem { Text = "Recent Items…" };
        manageItem.Click += (_, _) => RecentItemsCommand();
        OpenRecentMenu.Items.Add(manageItem);

        var clearItem = new MenuFlyoutItem
        {
            Text = "Clear Recent",
            IsEnabled = folders.Count > 0 || files.Count > 0,
        };
        clearItem.Click += (_, _) => _app.Recents.Clear();
        OpenRecentMenu.Items.Add(clearItem);
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
        if (ActiveFolder is not null &&
            ActiveFolder.ResolveDocument(pickedPath, out var boundCap, out _) &&
            boundCap is not null)
        {
            OpenCapability(boundCap);
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

    private void PickAndOpenFolder()
    {
        var pickedPath = DocumentPicker.PickFolder(this);
        if (pickedPath is null)
        {
            return;
        }
        if (!FolderCapability.TryFromUserInput(pickedPath, out var folder, out var error) ||
            folder is null)
        {
            ShowFolderError(error, pickedPath, isFromRecents: false);
            return;
        }
        OpenFolderCapability(folder);
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
        if (_isClosing || (Application.Current as ReaderApp)?.IsShuttingDown == true)
        {
            return;
        }

        SavedPosition? savedPos = PendingRestorePosition;
        if (savedPos is null && _displayedDoc is not null)
        {
            savedPos = Reader.SavePosition(_displayedDoc);
        }

        _ = Loader.Poll();
        Title = ActiveFolder is not null && !Loader.HasFile
            ? $"Markdown — {ActiveFolder.DisplayName}"
            : Loader.WindowTitle;
        ReloadItem.IsEnabled = Loader.HasFile;
        CloseFolderItem.IsEnabled = ActiveFolder is not null;

        if (!Loader.HasFile && Loader.Error is null)
        {
            if (_activeFolderError is null)
            {
                ErrorBanner.IsOpen = false;
                _recoveryActionsPanel.Visibility = Visibility.Collapsed;
            }
            Reader.Clear();
            Welcome.Visibility = Visibility.Visible;
            Welcome.Text = ActiveFolder is not null
                ? $"Folder workspace: {ActiveFolder.DisplayName}\n\nChoose File > Open (Ctrl+O) to open a document, or File > Close Folder to revoke folder access."
                : "Markdown\n\nA native Markdown reader.\nChoose File > Open (Ctrl+O) to open a Markdown document, or File > Open Folder (Ctrl+Shift+O) to open a folder workspace.";
            return;
        }

        if (Loader.IsLoading && Loader.Document is null)
        {
            ErrorBanner.IsOpen = false;
            _recoveryActionsPanel.Visibility = Visibility.Collapsed;
            Reader.Clear();
            Welcome.Visibility = Visibility.Visible;
            Welcome.Text = $"Loading {Loader.Capability?.DisplayName ?? "document"}…";
            return;
        }

        if (Loader.Document is { } doc)
        {
            if (Loader.Error is null &&
                Loader.Capability is { IsRevoked: false } loadedCap &&
                !ReferenceEquals(loadedCap, _recordedRecentCap))
            {
                _recordedRecentCap = loadedCap;
                _pendingRecentFilePath = null;
                _app.Recents.RecordFile(loadedCap);
            }

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
                ConfigureDocumentRecoveryActions(activeError);
                ErrorBanner.IsOpen = true;
            }
            else if (_activeFolderError is null)
            {
                ErrorBanner.IsOpen = false;
                _recoveryActionsPanel.Visibility = Visibility.Collapsed;
            }

            Welcome.Text = summary;

            if (Loader.Plan is { } plan)
            {
                PendingRestorePosition = null;
                var oldDoc = _displayedDoc;
                _displayedDoc = doc;
                Reader.InstallPlan(plan, doc.Generation, savedPos, doc);
                if (oldDoc is not null && oldDoc != doc)
                {
                    oldDoc.Dispose();
                }
                Reader.SetTextScale(Preferences.TextScaleValue);
                Reader.Visibility = Visibility.Visible;
                Welcome.Visibility = Visibility.Collapsed;
                if (_pendingSearchHit is { } pendingHit)
                {
                    _pendingSearchHit = null;
                    ApplyContentHitNavigation(doc, plan, pendingHit);
                }
                if (IsFindBarVisible && !string.IsNullOrEmpty(FindBar.Query))
                {
                    ExecuteFind();
                }
            }
            return;
        }

        if (Loader.Error is { } error)
        {
            var oldDoc = _displayedDoc;
            _displayedDoc = null;
            oldDoc?.Dispose();
            Reader.Clear();
            Welcome.Visibility = Visibility.Visible;
            Welcome.Text = error.ToDisplayString();
            if (IsRecoverableDocumentError(error))
            {
                ErrorBanner.Title = "Open issue";
                ErrorBanner.Message = error.ToDisplayString();
                ConfigureDocumentRecoveryActions(error);
                ErrorBanner.IsOpen = true;
            }
            else
            {
                ErrorBanner.IsOpen = false;
                _recoveryActionsPanel.Visibility = Visibility.Collapsed;
            }
        }
    }

    private static bool IsRecoverableDocumentError(DocumentError error) => error switch
    {
        DocumentError.NotFound or
        DocumentError.PermissionDenied or
        DocumentError.AccessRevoked or
        DocumentError.VolumeUnavailable => true,
        _ => false,
    };

    private RoutedEventHandler? _primaryRecoveryHandler;
    private RoutedEventHandler? _retryRecoveryHandler;
    private RoutedEventHandler? _removeRecoveryHandler;

    private void ClearRecoveryHandlers()
    {
        if (_primaryRecoveryHandler is not null)
        {
            RecoveryPrimaryButton.Click -= _primaryRecoveryHandler;
            _primaryRecoveryHandler = null;
        }
        if (_retryRecoveryHandler is not null)
        {
            RecoveryRetryButton.Click -= _retryRecoveryHandler;
            _retryRecoveryHandler = null;
        }
        if (_removeRecoveryHandler is not null)
        {
            RecoveryRemoveButton.Click -= _removeRecoveryHandler;
            _removeRecoveryHandler = null;
        }
    }

    private void ConfigureDocumentRecoveryActions(DocumentError error)
    {
        ClearRecoveryHandlers();
        if (!IsRecoverableDocumentError(error))
        {
            _recoveryActionsPanel.Visibility = Visibility.Collapsed;
            return;
        }

        RecoveryPrimaryButton.Content = error is DocumentError.PermissionDenied or DocumentError.AccessRevoked
            ? "Grant Access…"
            : "Locate…";
        RecoveryPrimaryButton.Visibility = Visibility.Visible;
        _primaryRecoveryHandler = (_, _) =>
        {
            var picked = DocumentPicker.PickFile(this);
            if (string.IsNullOrWhiteSpace(picked))
            {
                return;
            }
            var oldPath = _pendingRecentFilePath ?? Loader.Capability?.FullPath;
            if (oldPath is not null)
            {
                _ = _app.Recents.Relocate(RecentKind.File, oldPath, picked);
            }
            if (DocumentCapability.TryFromUserInput(picked, out var cap, out var pickErr) && cap is not null)
            {
                OpenCapability(cap);
            }
            else
            {
                RejectSelection(pickErr);
            }
        };
        RecoveryPrimaryButton.Click += _primaryRecoveryHandler;

        RecoveryRetryButton.Visibility = Loader.HasFile ? Visibility.Visible : Visibility.Collapsed;
        _retryRecoveryHandler = (_, _) => ReloadCommand();
        RecoveryRetryButton.Click += _retryRecoveryHandler;

        var removablePath = _pendingRecentFilePath;
        RecoveryRemoveButton.Visibility = removablePath is not null ? Visibility.Visible : Visibility.Collapsed;
        if (removablePath is not null)
        {
            _removeRecoveryHandler = (_, _) =>
            {
                _app.Recents.Remove(RecentKind.File, removablePath);
                _pendingRecentFilePath = null;
                ErrorBanner.IsOpen = false;
                _recoveryActionsPanel.Visibility = Visibility.Collapsed;
            };
            RecoveryRemoveButton.Click += _removeRecoveryHandler;
        }

        _recoveryActionsPanel.Visibility = Visibility.Visible;
    }

    private void ConfigureFolderRecoveryActions(FolderError error)
    {
        ClearRecoveryHandlers();
        if (!error.IsRecoverable())
        {
            _recoveryActionsPanel.Visibility = Visibility.Collapsed;
            return;
        }

        RecoveryPrimaryButton.Content = error is FolderError.PermissionDenied or FolderError.AccessRevoked
            ? "Grant Access…"
            : "Locate…";
        RecoveryPrimaryButton.Visibility = Visibility.Visible;
        _primaryRecoveryHandler = (_, _) =>
        {
            var picked = DocumentPicker.PickFolder(this);
            if (string.IsNullOrWhiteSpace(picked))
            {
                return;
            }
            if (_lastFailedFolderPath is not null && _lastFailedFolderFromRecents)
            {
                _ = _app.Recents.Relocate(RecentKind.Folder, _lastFailedFolderPath, picked);
            }
            if (FolderCapability.TryFromUserInput(picked, out var folder, out var pickErr) && folder is not null)
            {
                OpenFolderCapability(folder);
            }
            else
            {
                ShowFolderError(pickErr, picked, _lastFailedFolderFromRecents);
            }
        };
        RecoveryPrimaryButton.Click += _primaryRecoveryHandler;

        var retryPath = _lastFailedFolderPath;
        RecoveryRetryButton.Visibility = retryPath is not null ? Visibility.Visible : Visibility.Collapsed;
        if (retryPath is not null)
        {
            _retryRecoveryHandler = (_, _) =>
            {
                if (FolderCapability.TryFromUserInput(retryPath, out var folder, out var err) && folder is not null)
                {
                    OpenFolderCapability(folder, _lastFailedFolderFromRecents);
                }
                else
                {
                    ShowFolderError(err, retryPath, _lastFailedFolderFromRecents);
                }
            };
            RecoveryRetryButton.Click += _retryRecoveryHandler;
        }

        RecoveryRemoveButton.Visibility = retryPath is not null && _lastFailedFolderFromRecents
            ? Visibility.Visible
            : Visibility.Collapsed;
        if (retryPath is not null && _lastFailedFolderFromRecents)
        {
            _removeRecoveryHandler = (_, _) =>
            {
                _app.Recents.Remove(RecentKind.Folder, retryPath);
                _lastFailedFolderPath = null;
                _activeFolderError = null;
                ErrorBanner.IsOpen = false;
                _recoveryActionsPanel.Visibility = Visibility.Collapsed;
            };
            RecoveryRemoveButton.Click += _removeRecoveryHandler;
        }

        _recoveryActionsPanel.Visibility = Visibility.Visible;
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
