// FEAS-008 scripted checks, ported from the GTK host's `automated()`, and the
// stdin control loop used by the clipboard and UIA probes.
using System.Text;
using System.Text.Json.Nodes;
using Microsoft.UI.Dispatching;
using Microsoft.UI.Xaml;
using Windows.ApplicationModel.DataTransfer;
using Windows.Storage.Streams;

namespace Feas008;

public sealed partial class DocumentWindow
{
    async Task RunAutomated(Launch launch)
    {
        var report = new JsonObject
        {
            ["host"] = Variant,
            ["runtime"] = System.Runtime.InteropServices.RuntimeInformation.FrameworkDescription,
            ["input"] = Path.GetFileName(config.Input),
            ["source_bytes"] = model.SourceBytes,
            ["blocks"] = model.BlockCount,
            ["items"] = model.ItemCount,
            ["launch_from_launcher"] = Clock.FromLauncher,
            ["launch_to_main_ms"] = Clock.SinceLaunch(Program.MainAt),
            ["launch_to_window_loaded_ms"] = Clock.SinceLaunch(launch.WindowAt),
            ["launch_to_parsed_ms"] = Clock.SinceLaunch(loaded.ParsedAt),
            ["launch_to_paint_ms"] = Clock.SinceLaunch(launch.Paint),
            ["launch_to_presented_ms"] = Clock.SinceLaunch(launch.Presented ?? launch.Paint),
            ["presentation_time_available"] = launch.Presented is not null,
            ["read_ms"] = loaded.ReadMs,
            ["parse_ms"] = loaded.ParseMs,
            ["realized_rows_at_launch"] = bound.Count(r => r.Item >= 0),
        };
        try
        {
            await Checks(report);
        }
        catch (Exception error)
        {
            report["error"] = error.ToString();
        }
        report["presented_timeouts"] = TimedOut;
        report["realized_rows_at_end"] = bound.Count(r => r.Item >= 0);
        report["peak_rss_kib"] = Clock.PeakWorkingSetKiB();
        report["link_decisions"] = JsonNode.Parse("[" + string.Join(",", decisions) + "]");
        var text = report.ToJsonString();
        if (config.Report is { } path) File.WriteAllText(path, text);
        Console.Out.Write("REPORT " + text + "\n");
        Console.Out.Flush();
        Application.Current.Exit();
    }

    int HighlightedRows() => bound.Count(r => r.Item >= 0 && r.Text.TextHighlighters.Count > 0);

    async Task Checks(JsonObject report)
    {
        Events.Emit("{\"event\":\"stage\",\"stage\":\"highlight\"}");
        // Highlight state as drawn: highlighters on each bound row.
        {
            var first = model.Item(0);
            uint One(uint b) => NextOffset(b);
            var last = model.Item(Math.Min(3, model.ItemCount - 1)).Block;
            SetSelection((new Pos(first.Block, 0), new Pos(last, One(last))));
            var selected = HighlightedRows();
            SetSelection(null);
            var cleared = HighlightedRows();
            var b = model.Item(Math.Min(6, model.ItemCount - 1)).Block;
            SetSelection((new Pos(b, 0), new Pos(b, One(b))));
            var replaced = HighlightedRows();
            SetSelection(null);
            report["highlight_rows"] = new JsonObject { ["selected"] = selected, ["after_clear"] = cleared, ["after_reselect_one_block"] = replaced };
        }

        Events.Emit("{\"event\":\"stage\",\"stage\":\"copy\"}");
        // Whole-document selection and copy, read back through the clipboard.
        var started = Clock.Now;
        SelectAll();
        var (expected, markdownBytes) = Copy() ?? throw new InvalidOperationException("copy failed");
        var copyMs = Clock.Ms(started, Clock.Now);
        var readback = await Retry(async () => await Clipboard.GetContent().GetTextAsync());
        var markdownBack = NativeClipboard.Read(NativeClipboard.MarkdownFormat);
        report["select_all_copy"] = new JsonObject
        {
            ["copy_ms"] = copyMs,
            ["plain_chars"] = expected.Length,
            ["plain_bytes"] = Encoding.UTF8.GetByteCount(expected),
            ["markdown_bytes"] = markdownBytes,
            ["markdown_equals_source"] = markdownBytes == model.SourceBytes,
            ["local_readback_equals_model"] = readback == expected,
            ["local_markdown_readback_equals_source"] = markdownBack is not null && markdownBack.AsSpan().SequenceEqual(model.SourceBytesSpan),
        };
        var expectedBytes = Encoding.UTF8.GetBytes(expected);

        Events.Emit("{\"event\":\"stage\",\"stage\":\"samples\"}");
        // Sampled presentation: displayed fragment equals the copied text at its offset.
        var samples = new JsonArray();
        foreach (var fraction in new[] { 0.1, 0.35, 0.6, 0.85, 1.0 })
        {
            var index = (int)((model.ItemCount - 1) * fraction);
            var (jumpMs, row) = await JumpPresented(index, false);
            var item = model.Item(index);
            var want = model.Fragment(index);
            var at = model.PlainOffset(new Pos(item.Block, item.Start));
            var wantBytes = model.BlockBytes(item.Block, item.Start, item.End);
            samples.Add((JsonNode)new JsonObject
            {
                ["item"] = index,
                ["jump_to_presented_ms"] = jumpMs,
                ["displayed_equals_model"] = row?.Shown == want,
                ["copy_contains_at_offset"] = at + wantBytes.Length <= expectedBytes.Length && expectedBytes.AsSpan((int)at, wantBytes.Length).SequenceEqual(wantBytes),
            });
        }
        report["samples"] = samples;
        SetSelection(null);

        Events.Emit("{\"event\":\"stage\",\"stage\":\"find\"}");
        // Find a phrase near the end and jump to its off-screen row.
        for (var block = (uint)(model.BlockCount * 92 / 100); block < model.BlockCount; block++)
        {
            ref readonly var b = ref model.Block(block);
            var bytes = DocModel.Bytes(b.Text);
            if ((Kind)b.Kind != Kind.Paragraph || bytes.Length <= 60 || (bytes[20] & 0xC0) == 0x80 || (bytes[44] & 0xC0) == 0x80) continue;
            var query = Encoding.UTF8.GetString(bytes[20..44]);
            SetQuery(query);
            var (count, findMs) = RunFind(query);
            var first = Array.FindIndex(Matches, m => m.Block >= block);
            SetMatchIndex(first == 0 ? -1 : first - 1);
            if (first == 0) SetMatchIndex(Matches.Length - 1);
            var m = FindNext(true, false) ?? throw new InvalidOperationException("no match");
            var target = model.ItemFor(new Pos(m.Block, m.Start));
            var (jumpMs, row) = await JumpPresented(target, false);
            report["find"] = new JsonObject
            {
                ["query_bytes"] = Encoding.UTF8.GetByteCount(query),
                ["matches"] = count,
                ["find_ms"] = findMs,
                ["target_block"] = m.Block,
                ["jump_to_presented_ms"] = jumpMs,
                ["presented_row_contains_query"] = row?.Shown.Contains(query, StringComparison.OrdinalIgnoreCase) ?? false,
                ["selection_is_match"] = selection == (new Pos(m.Block, m.Start), new Pos(m.Block, m.End)),
            };
            break;
        }

        Events.Emit("{\"event\":\"stage\",\"stage\":\"outline\"}");
        // Outline jumps: last heading, then middle, then first.
        var jumps = new JsonArray();
        if (model.OutlineCount > 0)
        {
            foreach (var entry in new[] { model.OutlineCount - 1, model.OutlineCount / 2, 0 })
            {
                var heading = model.Outline(entry);
                var item = (int)model.FirstItem(heading.Block);
                var requested = Clock.Now;
                await JumpToBlock(heading.Block, true);
                var (paint, presented) = await Presented(() => VisibleRow(item) is not null);
                var row = VisibleRow(item);
                var focused = Microsoft.UI.Xaml.Input.FocusManager.GetFocusedElement(root.XamlRoot);
                jumps.Add((JsonNode)new JsonObject
                {
                    ["entry"] = entry,
                    ["block"] = heading.Block,
                    ["jump_to_presented_ms"] = Clock.Ms(requested, presented ?? paint),
                    ["heading_presented"] = row?.Shown == DocModel.Text(heading.Title),
                    ["row_focused"] = row is not null && ReferenceEquals(focused, row),
                    ["row_top_px"] = row?.TransformToVisual(scroll).TransformPoint(default).Y,
                });
            }
        }
        report["outline_jumps"] = jumps;

        Events.Emit("{\"event\":\"stage\",\"stage\":\"links\"}");
        // Link policy and fragment activation.
        for (uint i = 0; i < Math.Min(40, model.LinkCount); i++)
        {
            ActivateLink(i);
            var link = model.Link(i);
            if ((Intent)link.Intent == Intent.Fragment)
            {
                var (jumpMs, row) = await JumpPresented((int)model.FirstItem(link.TargetBlock), true, true);
                report["fragment_link"] = new JsonObject { ["dest"] = DocModel.Text(link.Dest), ["jump_to_presented_ms"] = jumpMs, ["presented"] = row is not null };
            }
        }

        Events.Emit("{\"event\":\"stage\",\"stage\":\"images\"}");
        // Images: presented, blocked or refused under policy.
        var imageReports = new JsonArray();
        for (uint block = 0, seen = 0; block < model.BlockCount && seen < 20; block++)
        {
            ref readonly var b = ref model.Block(block);
            if ((Kind)b.Kind != Kind.Image) continue;
            seen++;
            var item = (int)model.FirstItem(block);
            var alt = DocModel.Text(b.Text);
            var request = DocModel.Text(b.Image);
            var local = b.ImageKind == 1;
            await JumpPresented(item, false);
            string? outcome = null;
            for (var i = 0; i < 200 && outcome is null; i++)
            {
                if (!local) outcome = $"blocked by model policy: {request}";
                else if (images.TryGetValue(request, out var state) && state.Done)
                    outcome = state.Bitmap is { } bitmap ? $"presented {bitmap.PixelWidth}x{bitmap.PixelHeight}" : $"refused: {state.Failure}";
                else await Task.Delay(10);
            }
            await Presented(() => VisibleRow(item) is not null);
            var row = VisibleRow(item);
            imageReports.Add((JsonNode)new JsonObject
            {
                ["alt"] = alt,
                ["request"] = local ? $"Local({request})" : $"Blocked({request})",
                ["outcome"] = outcome,
                ["picture_visible"] = row is not null && row.Picture.Visibility == Visibility.Visible && row.Picture.Source is not null,
            });
        }
        report["images"] = imageReports;
    }

    uint NextOffset(uint block)
    {
        var bytes = DocModel.Bytes(model.Block(block).Text);
        var next = 1;
        while (next < bytes.Length && (bytes[next] & 0xC0) == 0x80) next++;
        return (uint)next;
    }

    /// The clipboard is briefly held open by other readers (CLIPBRD_E_CANT_OPEN).
    static async Task<T> Retry<T>(Func<Task<T>> read)
    {
        for (var attempt = 0; ; attempt++)
        {
            try { return await read(); }
            catch (System.Runtime.InteropServices.COMException error) when (error.HResult == unchecked((int)0x800401D0) && attempt < 50) { await Task.Delay(20); }
        }
    }

    static async Task<byte[]?> ReadFormat(DataPackageView view, string format)
    {
        if (!view.Contains(format)) return null;
        var data = await view.GetDataAsync(format);
        IRandomAccessStream? stream = data switch
        {
            IRandomAccessStream s => s,
            IRandomAccessStreamReference r => await r.OpenReadAsync(),
            _ => null,
        };
        if (stream is null) return null;
        using var reader = new DataReader(stream.GetInputStreamAt(0));
        var size = (uint)stream.Size;
        await reader.LoadAsync(size);
        var bytes = new byte[size];
        reader.ReadBytes(bytes);
        return bytes;
    }

    /// Line commands on stdin for the external clipboard, UIA and Narrator probes.
    async Task ControlLoop()
    {
        Events.Emit("{\"event\":\"ready\"}");
        while (await Console.In.ReadLineAsync() is { } line)
        {
            var done = new TaskCompletionSource<bool>();
            DispatcherQueue.TryEnqueue(async () =>
            {
                try { done.SetResult(await Command(line)); }
                catch (Exception error) { Events.Emit($"{{\"event\":\"error\",\"line\":{Events.Quote(line)},\"error\":{Events.Quote(error.Message)}}}"); done.SetResult(true); }
            });
            if (!await done.Task) break;
        }
        DispatcherQueue.TryEnqueue(() => Application.Current.Exit());
    }

    async Task<bool> Command(string line)
    {
        var parts = line.Split(' ', StringSplitOptions.RemoveEmptyEntries);
        int Arg(int i, int fallback) => parts.Length > i && int.TryParse(parts[i], out var n) ? n : fallback;
        switch (parts.FirstOrDefault())
        {
            case "copy-all":
                SelectAll();
                var (plain, markdown) = Copy()!.Value;
                Events.Emit($"{{\"event\":\"copied\",\"plain_chars\":{plain.Length},\"plain_bytes\":{Encoding.UTF8.GetByteCount(plain)},\"markdown_bytes\":{markdown}}}");
                break;
            case "item" or "focus":
                var (ms, _) = await JumpPresented(Arg(1, 0), parts[0] == "focus", parts[0] == "focus");
                Events.Emit($"{{\"event\":\"presented\",\"item\":{Arg(1, 0)},\"jump_to_presented_ms\":{ms:F3}}}");
                break;
            case "outline":
                var block = model.Outline(Arg(1, 0)).Block;
                var item = (int)model.FirstItem(block);
                await JumpToBlock(block, true);
                await Presented(() => VisibleRow(item) is not null);
                Events.Emit($"{{\"event\":\"presented\",\"item\":{item}}}");
                break;
            case "texts":
                var from = Arg(1, 0);
                var to = Math.Min(Arg(2, from + 1), model.ItemCount);
                var texts = string.Join(",", Enumerable.Range(from, Math.Max(0, to - from)).Select(i => Events.Quote(model.Fragment(i))));
                Events.Emit($"{{\"event\":\"texts\",\"from\":{from},\"texts\":[{texts}],\"items\":{model.ItemCount}}}");
                break;
            case "select":
                SetSelection((new Pos((uint)Arg(1, 0), (uint)Arg(2, 0)), new Pos((uint)Arg(3, 0), (uint)Arg(4, 0))));
                Events.Emit("{\"event\":\"selected\"}");
                break;
            case "geom":
                var lb = list.TransformToVisual(root).TransformBounds(new Windows.Foundation.Rect(0, 0, list.ActualWidth, list.ActualHeight));
                var rows = string.Join(",", bound.Where(r => r.Item >= 0).OrderBy(r => r.Item).Take(4).Select(r =>
                {
                    var b = r.TransformToVisual(root).TransformBounds(new Windows.Foundation.Rect(0, 0, r.ActualWidth, r.ActualHeight));
                    return $"{{\"item\":{r.Item},\"x\":{b.X:F0},\"y\":{b.Y:F0},\"w\":{b.Width:F0},\"h\":{b.Height:F0},\"text_w\":{r.Text.ActualWidth:F0},\"vis\":\"{r.Visibility}\",\"opacity\":{r.Opacity},\"content_attached\":{(r.Root.Parent is null ? "false" : "true")}}}";
                }));
                Events.Emit($"{{\"event\":\"texts\",\"list\":[{lb.X:F0},{lb.Y:F0},{lb.Width:F0},{lb.Height:F0}],\"root_w\":{root.ActualWidth:F0},\"rows\":[{rows}]}}");
                break;
            case "quit":
                return false;
            default:
                Events.Emit($"{{\"event\":\"error\",\"line\":{Events.Quote(line)}}}");
                break;
        }
        return true;
    }
}
