// FEAS-008 WebView2 document baseline: the same Rust model rendered to HTML
// (f8_html_v1) in a WinUI WebView2. Raw HTML is already inert text and the
// CSP (default-src 'none') blocks page scripts; script execution stays on
// only for the host's injected frame and check probes, because WebView2
// disables ExecuteScriptAsync with IsScriptEnabled. The document and its images are served from a virtual host through
// WebResourceRequested, so images pass the same ImagePolicy as the native
// hosts and nothing is read from disk directly by the engine. Navigation is
// gated by the model's link decisions, which the HTML already applied.
using System.Diagnostics;
using System.Text;
using System.Text.Json.Nodes;
using Microsoft.UI.Xaml;
using Microsoft.Web.WebView2.Core;
using Windows.Storage.Streams;
using Controls = Microsoft.UI.Xaml.Controls;

namespace Feas008;

static class Program
{
    public static string Input = "";
    public static bool Automated;
    public static string? Report;
    public static long MainAt;
    public static Task<(string Html, double ReadMs, double RenderMs, long ReadyAt)> Loading = null!;

    [STAThread]
    static void Main(string[] args)
    {
        MainAt = Clock.Now;
        Events.UseUtf8();
        for (var i = 0; i < args.Length; i++)
        {
            if (args[i] == "--automated") Automated = true;
            else if (args[i] == "--report") Report = args[++i];
            else if (args[i] == "--control") { }
            else Input = Path.GetFullPath(args[i]);
        }
        Loading = Task.Run(() =>
        {
            var started = Stopwatch.StartNew();
            var bytes = File.ReadAllBytes(Input);
            var read = started.Elapsed.TotalMilliseconds;
            var html = DocModel.Html(bytes);
            return (html, read, started.Elapsed.TotalMilliseconds - read, Clock.Now);
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
    WebViewWindow? window;
    public HostApp() => InitializeComponent();
    protected override void OnLaunched(LaunchActivatedEventArgs args)
    {
        window = new WebViewWindow();
        window.Activate();
    }
}

public sealed partial class WebViewWindow : Window
{
    const string Host = "https://feas008.invalid/";
    const string Csp = "default-src 'none'; img-src https://feas008.invalid; style-src 'unsafe-inline'";
    const string Style = "body{font:15px 'Segoe UI',sans-serif;margin:16px 24px;max-width:none}pre,code{font-family:'Cascadia Mono',Consolas,monospace}pre{background:#80808018;padding:8px;white-space:pre-wrap;word-break:break-all}p,li{overflow-wrap:anywhere}";
    readonly Controls.WebView2 view = new();
    readonly List<string> resources = [];
    readonly List<string> navigations = [];
    string html = "";
    TaskCompletionSource<long>? frame;
    long windowAt;

    public WebViewWindow()
    {
        Title = $"FEAS-008 WebView2 — {Path.GetFileName(Program.Input)}";
        AppWindow.Resize(new Windows.Graphics.SizeInt32(1200, 900));
        Content = view;
        view.Loaded += async (_, _) =>
        {
            try { await Load(); }
            catch (Exception error) { Finish(new JsonObject { ["host"] = "WebView2", ["error"] = error.ToString() }); }
        };
    }

    async Task Load()
    {
        windowAt = Clock.Now;
        await view.EnsureCoreWebView2Async();
        var core = view.CoreWebView2;
        core.Settings.AreDefaultScriptDialogsEnabled = false;
        core.Settings.IsWebMessageEnabled = true;
        core.Settings.AreDevToolsEnabled = false;
        core.AddWebResourceRequestedFilter(Host + "*", CoreWebView2WebResourceContext.All);
        core.WebResourceRequested += OnResource;
        core.NavigationStarting += (_, e) =>
        {
            // The HTML already dropped blocked links; anything leaving the
            // document is a decision for the host, never the engine.
            if (e.Uri.StartsWith(Host + "doc.html", StringComparison.Ordinal)) return;
            navigations.Add(e.Uri);
            e.Cancel = true;
        };
        core.WebMessageReceived += (_, _) => frame?.TrySetResult(Clock.Now);
        var loaded = await Program.Loading;
        html = $"<!doctype html><html><head><meta charset=utf-8><meta http-equiv=Content-Security-Policy content=\"{Csp}\"><style>{Style}</style></head><body>{loaded.Html}</body></html>";
        var navigated = new TaskCompletionSource();
        core.NavigationCompleted += (_, _) => navigated.TrySetResult();
        core.Navigate(Host + "doc.html");
        await navigated.Task;
        var (paint, presented) = await NextPresented();
        Events.Emit($"{{\"event\":\"presented\",\"launch_to_presented_ms\":{Clock.SinceLaunch(presented ?? paint):F3}}}");
        if (!Program.Automated) return;
        var report = new JsonObject
        {
            ["host"] = "WebView2",
            ["engine"] = core.Environment.BrowserVersionString,
            ["input"] = Path.GetFileName(Program.Input),
            ["html_bytes"] = Encoding.UTF8.GetByteCount(loaded.Html),
            ["launch_from_launcher"] = Clock.FromLauncher,
            ["launch_to_main_ms"] = Clock.SinceLaunch(Program.MainAt),
            ["launch_to_window_loaded_ms"] = Clock.SinceLaunch(windowAt),
            ["launch_to_parsed_ms"] = Clock.SinceLaunch(loaded.ReadyAt),
            ["launch_to_paint_ms"] = Clock.SinceLaunch(paint),
            ["launch_to_presented_ms"] = Clock.SinceLaunch(presented ?? paint),
            ["presentation_time_available"] = presented is not null,
            ["read_ms"] = loaded.ReadMs,
            ["parse_ms"] = loaded.RenderMs,
        };
        await Checks(core, report);
        report["resources"] = JsonNode.Parse("[" + string.Join(",", resources) + "]");
        report["blocked_navigations"] = JsonNode.Parse("[" + string.Join(",", navigations.Select(Events.Quote)) + "]");
        report["peak_rss_kib"] = Clock.PeakWorkingSetKiB();
        report["process_peak_rss_kib"] = ProcessMemory(core);
        Finish(report);
    }

    /// The engine's first frame after loading: an injected double
    /// requestAnimationFrame posts a message, then DWM presents.
    async Task<(long Paint, long? Presented)> NextPresented()
    {
        frame = new TaskCompletionSource<long>();
        await view.CoreWebView2.ExecuteScriptAsync("requestAnimationFrame(()=>requestAnimationFrame(()=>chrome.webview.postMessage('frame')))");
        var paint = await frame.Task;
        return (paint, await Clock.AfterNextPresent());
    }

    async Task<string> Script(string code) => await view.CoreWebView2.ExecuteScriptAsync(code);

    async Task Checks(CoreWebView2 core, JsonObject report)
    {
        // Whole-document selection through the engine's own selection model.
        var started = Clock.Now;
        var selected = await Script("(()=>{const r=document.createRange();r.selectNodeContents(document.body);const s=getSelection();s.removeAllRanges();s.addRange(r);return s.toString().length})()");
        report["select_all"] = new JsonObject { ["ms"] = Clock.Ms(started, Clock.Now), ["selected_chars"] = int.Parse(selected) };
        await Script("getSelection().removeAllRanges()");

        // Find a phrase near the end, as the native hosts do, with window.find.
        var query = await Script("(()=>{const ps=[...document.querySelectorAll('p')].slice(Math.floor(document.querySelectorAll('p').length*0.92));const p=ps.find(p=>p.textContent.length>60);return p?p.textContent.slice(20,44):''})()");
        query = System.Text.Json.JsonSerializer.Deserialize(query, Feas008Json.Default.String) ?? "";
        if (query.Length > 0)
        {
            started = Clock.Now;
            var found = await Script($"(()=>{{const q={Events.Quote(query)};const all=[...document.querySelectorAll('p')];const t=all.slice(Math.floor(all.length*0.92)).find(p=>p.textContent.includes(q));if(!t)return false;const i=t.textContent.indexOf(q);const w=document.createTreeWalker(t,NodeFilter.SHOW_TEXT);let n,o=0;while((n=w.nextNode())){{if(o+n.length>i){{const r=document.createRange();r.setStart(n,i-o);r.setEnd(n,Math.min(n.length,i-o+q.length));getSelection().removeAllRanges();getSelection().addRange(r);n.parentElement.scrollIntoView();return true}}o+=n.length}}return false}})()");
            var (paint, presented) = await NextPresented();
            report["find"] = new JsonObject { ["query_chars"] = query.Length, ["found"] = found == "true", ["jump_to_presented_ms"] = Clock.Ms(started, presented ?? paint) };
        }

        // Last heading jump through the fragment the HTML carries.
        started = Clock.Now;
        var heading = await Script("(()=>{const h=[...document.querySelectorAll('h1,h2,h3,h4,h5,h6')].pop();if(!h)return '';h.scrollIntoView();return h.id})()");
        var (hp, hpresented) = await NextPresented();
        report["outline_last_jump"] = new JsonObject { ["id"] = heading.Trim('"'), ["jump_to_presented_ms"] = Clock.Ms(started, hpresented ?? hp) };

        // Images: how each request left the custom resource handler.
        var images = await Script("JSON.stringify([...document.images].map(i=>({alt:i.alt,src:i.getAttribute('src'),complete:i.complete,width:i.naturalWidth})))");
        report["images"] = JsonNode.Parse(System.Text.Json.JsonSerializer.Deserialize(images, Feas008Json.Default.String) ?? "[]");
    }

    void OnResource(CoreWebView2 sender, CoreWebView2WebResourceRequestedEventArgs e)
    {
        var path = Uri.UnescapeDataString(new Uri(e.Request.Uri).AbsolutePath.TrimStart('/'));
        var deferral = e.GetDeferral();
        try
        {
            if (path == "doc.html")
            {
                e.Response = Respond(sender, Encoding.UTF8.GetBytes(html), 200, "OK", "text/html; charset=utf-8");
                return;
            }
            try
            {
                var bytes = ImagePolicy.Read(Path.GetDirectoryName(Program.Input)!, path);
                e.Response = Respond(sender, bytes, 200, "OK", "image/png");
                resources.Add($"{{\"path\":{Events.Quote(path)},\"outcome\":\"served\"}}");
            }
            catch (Exception error)
            {
                e.Response = Respond(sender, [], 403, "Forbidden", "text/plain");
                resources.Add($"{{\"path\":{Events.Quote(path)},\"outcome\":{Events.Quote("refused: " + error.Message)}}}");
            }
        }
        finally { deferral.Complete(); }
    }

    static CoreWebView2WebResourceResponse Respond(CoreWebView2 core, byte[] body, int status, string reason, string type)
    {
        var stream = new InMemoryRandomAccessStream();
        using (var writer = new DataWriter(stream.GetOutputStreamAt(0)))
        {
            writer.WriteBytes(body);
            writer.StoreAsync().AsTask().GetAwaiter().GetResult();
            writer.DetachStream();
        }
        return core.Environment.CreateWebResourceResponse(stream, status, reason, $"Content-Type: {type}\nContent-Security-Policy: {Csp}");
    }

    /// Peak working sets of every process the engine runs for this view.
    static JsonObject ProcessMemory(CoreWebView2 core)
    {
        var result = new JsonObject();
        long total = Clock.PeakWorkingSetKiB();
        result["host"] = total;
        foreach (var info in core.Environment.GetProcessInfos())
        {
            try
            {
                var kib = Process.GetProcessById(info.ProcessId).PeakWorkingSet64 / 1024;
                total += kib;
                var key = info.Kind.ToString();
                result[key] = (result[key]?.GetValue<long>() ?? 0) + kib;
            }
            catch (ArgumentException) { }
        }
        result["total"] = total;
        return result;
    }

    void Finish(JsonObject report)
    {
        var text = report.ToJsonString();
        if (Program.Report is { } path) File.WriteAllText(path, text);
        Console.Out.Write("REPORT " + text + "\n");
        Console.Out.Flush();
        Application.Current.Exit();
    }
}

[System.Text.Json.Serialization.JsonSerializable(typeof(string))]
internal sealed partial class Feas008Json : System.Text.Json.Serialization.JsonSerializerContext;
