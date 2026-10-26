// FEAS-008 C# WinUI 3 host entry. Reading and parsing start on a worker
// before XAML initializes, as the GTK host overlaps them with GTK startup.
using System.Diagnostics;
using Microsoft.UI.Xaml;

namespace Feas008;

public sealed record Config(string Input, bool Automated, bool Control, string? Report, ulong FragmentBytes)
{
    public static Config Parse(string[] args)
    {
        string? input = null, report = null;
        bool automated = false, control = false;
        for (var i = 0; i < args.Length; i++)
        {
            switch (args[i])
            {
                case "--automated": automated = true; break;
                case "--control": control = true; break;
                case "--report": report = args[++i]; break;
                default: input = args[i]; break;
            }
        }
        ulong.TryParse(Environment.GetEnvironmentVariable("FEAS008_FRAGMENT_BYTES"), out var fragment);
        return new(Path.GetFullPath(input ?? "build/feas008/media/media-links.md"), automated, control, report, fragment);
    }
}

public sealed record Loaded(DocModel Model, double ReadMs, double ParseMs, long ParsedAt);

static class Program
{
    public static Config Config = null!;
    public static Task<Loaded> Loading = null!;
    public static long MainAt;

    [STAThread]
    static void Main(string[] args)
    {
        MainAt = Clock.Now;
        Events.UseUtf8();
        Config = Config.Parse(args);
        Loading = Task.Run(() =>
        {
            var started = Stopwatch.StartNew();
            var bytes = File.ReadAllBytes(Config.Input);
            var read = started.Elapsed.TotalMilliseconds;
            var model = new DocModel(bytes, Config.FragmentBytes);
            return new Loaded(model, read, model.ParseMs, Clock.Now);
        });
        WinRT.ComWrappersSupport.InitializeComWrappers();
        Application.Start(initialization =>
        {
            SynchronizationContext.SetSynchronizationContext(
                new Microsoft.UI.Dispatching.DispatcherQueueSynchronizationContext(
                    Microsoft.UI.Dispatching.DispatcherQueue.GetForCurrentThread()));
            _ = new HostApp();
        });
    }
}

public sealed partial class HostApp : Application
{
    DocumentWindow? window;
    public HostApp()
    {
        InitializeComponent();
        UnhandledException += (_, e) => { e.Handled = true; window?.Fail(e.Exception); };
    }
    protected override void OnLaunched(LaunchActivatedEventArgs args)
    {
        window = new DocumentWindow(Program.Config);
        window.Activate();
    }
}
