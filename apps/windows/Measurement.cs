using System.Diagnostics;
using System.IO;
using System.Runtime.InteropServices;
using System.Text.Json;
using System.Text.Json.Nodes;
using Markdown.Windows.Core;
using Markdown.Windows.Document;
using Markdown.Windows.Render;
using Microsoft.UI.Xaml;
using Windows.System;

namespace Markdown.Windows;

/// <summary>
/// Development performance measurements (D-012/D-016) launched by measure.py or directly via --measure.
/// Outputs structured JSON results to stdout.
/// </summary>
public static partial class Measurement
{
    private const long D016TargetBytes = 1024 * 1024;
    [LibraryImport("kernel32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static partial bool QueryPerformanceCounter(out long lpPerformanceCount);

    [LibraryImport("kernel32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static partial bool QueryPerformanceFrequency(out long lpFrequency);

    [LibraryImport("dwmapi.dll")]
    private static partial int DwmFlush();

    private static readonly long s_frequency = GetFrequency();

    private static long GetFrequency()
    {
        _ = QueryPerformanceFrequency(out var freq);
        return freq > 0 ? freq : Stopwatch.Frequency;
    }

    public static long Qpc()
    {
        _ = QueryPerformanceCounter(out var qpc);
        return qpc;
    }

    public static double QpcToMs(long from, long to)
    {
        return (to - from) * 1000.0 / s_frequency;
    }

    private static CoreDocument ParseDoc(byte[] bytes)
    {
        using var session = new CoreSession("measure");
        var res = session.ParseBlocking(bytes, null, copyUnchanged: true);
        session.Close();
        return res.Document ?? throw new InvalidOperationException("Failed to parse document for measurement");
    }

    public static int Run(string[] args)
    {
        if (args.Length < 1)
        {
            Console.Error.WriteLine("Usage: Markdown.Windows.exe --measure <launch|interaction|repeated=N> [document-path]");
            return 2;
        }

        var modeArg = args[0];
        var docPath = args.Length > 1 ? args[1] : null;

        if (modeArg == "launch")
        {
            return RunLaunch(docPath);
        }
        if (modeArg.StartsWith("repeated=", StringComparison.Ordinal) &&
            int.TryParse(modeArg.AsSpan(9), out var count) && count > 0)
        {
            return RunRepeated(docPath, count);
        }
        if (modeArg == "interaction")
        {
            return RunInteraction(docPath);
        }
        if (modeArg.StartsWith("refresh=", StringComparison.Ordinal) &&
            int.TryParse(modeArg.AsSpan(8), out var refreshCount) && refreshCount > 0)
        {
            return RunRefresh(docPath, refreshCount);
        }

        Console.Error.WriteLine($"Unknown measure mode: {modeArg}");
        return 2;
    }

    private static int RunLaunch(string? docPath)
    {
        var proc = Process.GetCurrentProcess();
        var startTime = proc.StartTime;
        var startQpc = Qpc();

        if (string.IsNullOrEmpty(docPath) || !File.Exists(docPath))
        {
            Console.Error.WriteLine($"File not found: {docPath}");
            return 1;
        }

        var sw = Stopwatch.StartNew();
        var bytes = File.ReadAllBytes(docPath);
        var readMs = sw.Elapsed.TotalMilliseconds;

        var coreDoc = ParseDoc(bytes);
        var parseMs = sw.Elapsed.TotalMilliseconds - readMs;

        var plan = RenderPlan.Build(coreDoc, () => false);
        var planMs = sw.Elapsed.TotalMilliseconds - readMs - parseMs;

        // Presentation flush
        var flushSw = Stopwatch.StartNew();
        _ = DwmFlush();
        var flushMs = flushSw.Elapsed.TotalMilliseconds;
        var pipelineMs = sw.Elapsed.TotalMilliseconds;

        proc.Refresh();
        var peakRss = proc.PeakWorkingSet64 / 1024;
        var currentRss = proc.WorkingSet64 / 1024;

        var report = new JsonObject
        {
            ["mode"] = "launch",
            ["document"] = Path.GetFileName(docPath),
            ["source_bytes"] = bytes.Length,
            ["nodes"] = coreDoc.NodeCount,
            ["fragments"] = plan.Fragments.Count,
            ["read_ms"] = readMs,
            ["parse_ms"] = parseMs,
            ["plan_ms"] = planMs,
            ["dwm_flush_ms"] = flushMs,
            ["pipeline_ms"] = pipelineMs,
            ["peak_rss_kib"] = peakRss,
            ["working_set_kib"] = currentRss,
            ["meets_warm_budget_400ms"] = bytes.Length <= D016TargetBytes ? pipelineMs <= 400.0 : null,
            ["meets_cold_budget_700ms"] = bytes.Length <= D016TargetBytes ? pipelineMs <= 700.0 : null,
        };

        Console.WriteLine(report.ToJsonString(new JsonSerializerOptions { WriteIndented = true }));
        return 0;
    }

    private static int RunRepeated(string? docPath, int count)
    {
        if (string.IsNullOrEmpty(docPath) || !File.Exists(docPath))
        {
            Console.Error.WriteLine($"File not found: {docPath}");
            return 1;
        }

        var bytes = File.ReadAllBytes(docPath);
        var proc = Process.GetCurrentProcess();
        proc.Refresh();
        var baselineRss = proc.WorkingSet64 / (1024 * 1024);

        var results = new JsonArray();
        for (var i = 1; i <= count; i++)
        {
            var sw = Stopwatch.StartNew();
            var doc = ParseDoc(bytes);
            var plan = RenderPlan.Build(doc, () => false);
            var elapsedMs = sw.Elapsed.TotalMilliseconds;

            proc.Refresh();
            var rssMib = proc.WorkingSet64 / (1024.0 * 1024.0);

            results.Add((JsonNode)new JsonObject
            {
                ["load"] = i,
                ["elapsed_ms"] = elapsedMs,
                ["retained_rss_mib"] = Math.Round(rssMib, 2),
            });

            // Allow GC trim
            GC.Collect(2, GCCollectionMode.Aggressive, blocking: true, compacting: true);
        }

        proc.Refresh();
        var finalRss = proc.WorkingSet64 / (1024.0 * 1024.0);

        var report = new JsonObject
        {
            ["mode"] = "repeated",
            ["document"] = Path.GetFileName(docPath),
            ["loads"] = count,
            ["baseline_rss_mib"] = baselineRss,
            ["final_rss_mib"] = Math.Round(finalRss, 2),
            ["loads_data"] = results,
        };

        Console.WriteLine(report.ToJsonString(new JsonSerializerOptions { WriteIndented = true }));
        return 0;
    }

    private static int RunInteraction(string? docPath)
    {
        if (string.IsNullOrEmpty(docPath) || !File.Exists(docPath))
        {
            Console.Error.WriteLine($"File not found: {docPath}");
            return 1;
        }

        var bytes = File.ReadAllBytes(docPath);
        var doc = ParseDoc(bytes);
        var plan = RenderPlan.Build(doc, () => false);

        // 1. Off-screen jump timing (simulated navigation to middle and last headings)
        var jumpTimes = new List<double>();
        if (plan.Outline.Count > 0)
        {
            var targets = new[] { plan.Outline.Count / 2, plan.Outline.Count - 1, 0 };
            foreach (var t in targets)
            {
                var targetFrag = plan.Outline[t].Fragment;
                var sw = Stopwatch.StartNew();
                // Model jump target resolution
                var resolved = plan.Fragments[targetFrag];
                _ = resolved.Text;
                jumpTimes.Add(sw.Elapsed.TotalMilliseconds);
            }
        }

        // 2. Select All and Copy plain / source slice timing
        var copySw = Stopwatch.StartNew();
        var allText = plan.TextBetween(new DocumentPosition(0, 0), new DocumentPosition(plan.Fragments.Count - 1, plan.Fragments[^1].Text.Length));
        var allSource = plan.SourceBetween(new DocumentPosition(0, 0), new DocumentPosition(plan.Fragments.Count - 1, plan.Fragments[^1].Text.Length), doc);
        var copyMs = copySw.Elapsed.TotalMilliseconds;

        // Clipboard write
        var clipSw = Stopwatch.StartNew();
        NativeClipboard.Set(0, allText, allSource);
        var clipWriteMs = clipSw.Elapsed.TotalMilliseconds;

        // Clipboard readback
        var readbackText = NativeClipboard.ReadUnicodeText();
        var readbackMarkdown = NativeClipboard.ReadMarkdownBytes();

        var report = new JsonObject
        {
            ["mode"] = "interaction",
            ["document"] = Path.GetFileName(docPath),
            ["fragments"] = plan.Fragments.Count,
            ["outline_count"] = plan.Outline.Count,
            ["target_resolution_ms"] = jumpTimes.Count > 0 ? jumpTimes.Max() : 0.0,
            ["meets_target_resolution_150ms"] = jumpTimes.Count == 0 || jumpTimes.Max() <= 150.0,
            ["copy_slice_ms"] = copyMs,
            ["clipboard_write_ms"] = clipWriteMs,
            ["plain_chars"] = allText.Length,
            ["markdown_bytes"] = allSource.Length,
            ["markdown_equals_source"] = allSource.SequenceEqual(doc.Source),
            ["readback_plain_equals_model"] = readbackText == allText,
            ["readback_markdown_equals_source"] = readbackMarkdown is not null && readbackMarkdown.SequenceEqual(doc.Source),
        };

        Console.WriteLine(report.ToJsonString(new JsonSerializerOptions { WriteIndented = true }));
        return 0;
    }
    private static int RunRefresh(string? docPath, int count)
    {
        if (string.IsNullOrEmpty(docPath) || !File.Exists(docPath))
        {
            Console.Error.WriteLine($"File not found: {docPath}");
            return 1;
        }

        var original = File.ReadAllBytes(docPath);
        var maxLen = Math.Max(original.Length - 64, 0);
        var cut = -1;
        for (var i = maxLen; i >= 0; i--)
        {
            if (original[i] == (byte)'\n')
            {
                cut = i;
                break;
            }
        }
        if (cut < 0) cut = original.Length;

        var prefix = System.Text.Encoding.UTF8.GetBytes("Refreshed from outside.\n\n");
        var edited = new byte[prefix.Length + cut];
        Buffer.BlockCopy(prefix, 0, edited, 0, prefix.Length);
        Buffer.BlockCopy(original, 0, edited, prefix.Length, cut);

        using var session = new CoreSession("measure-refresh");
        var initialRes = session.ParseBlocking(original, null, copyUnchanged: true);
        var initialDoc = initialRes.Document ?? throw new InvalidOperationException("initial parse failed");
        var initialPlan = RenderPlan.Build(initialDoc, () => false);

        if (initialPlan.Outline.Count == 0)
        {
            throw new InvalidOperationException("document has no headings");
        }

        var middleHeading = initialPlan.Outline.Count / 2;
        var middleFrag = initialPlan.Outline[middleHeading].Fragment;
        var middleOffset = initialPlan.SourceOffsetAt(new DocumentPosition(middleFrag, 0), initialDoc);
        var initialAnchor = initialDoc.AnchorAt(middleOffset) ?? throw new InvalidOperationException("failed to anchor middle heading");
        var savedPosition = new SavedPosition(initialAnchor, initialAnchor, null);

        var proc = Process.GetCurrentProcess();
        proc.Refresh();
        var baselineRssMib = Math.Round(proc.WorkingSet64 / (1024.0 * 1024.0), 2);

        var results = new JsonArray();
        for (var iteration = 1; iteration <= count; iteration++)
        {
            var bytesToWrite = (iteration % 2 == 1) ? edited : original;
            var sw = Stopwatch.StartNew();
            File.WriteAllBytes(docPath, bytesToWrite);

            var readBytes = File.ReadAllBytes(docPath);
            var parseRes = session.ParseBlocking(readBytes, null, copyUnchanged: false);
            var newDoc = parseRes.Document ?? throw new InvalidOperationException("refresh parse failed");
            var newPlan = RenderPlan.Build(newDoc, () => false);
            _ = DwmFlush();
            var refreshMs = sw.Elapsed.TotalMilliseconds;

            var topRes = newDoc.ResolveAnchor(savedPosition.Top);
            var restoredPos = topRes is not null ? newPlan.PositionAtSourceOffset(topRes.Value.SourceOffset, newDoc) : (DocumentPosition?)null;
            var keptPlace = restoredPos is not null && restoredPos.Value.FragmentIndex >= 0;

            proc.Refresh();
            var currentRssMib = Math.Round(proc.WorkingSet64 / (1024.0 * 1024.0), 2);

            results.Add((JsonNode)new JsonObject
            {
                ["iteration"] = iteration,
                ["refresh_ms"] = Math.Round(refreshMs, 2),
                ["kept_place"] = keptPlace,
                ["baseline_footprint_mib"] = baselineRssMib,
                ["footprint_mib"] = currentRssMib,
            });

            GC.Collect(2, GCCollectionMode.Aggressive, blocking: true, compacting: true);
        }

        var report = new JsonObject
        {
            ["mode"] = "refresh",
            ["document"] = Path.GetFileName(docPath),
            ["refreshes"] = count,
            ["baseline_footprint_mib"] = baselineRssMib,
            ["refreshes_data"] = results,
        };

        Console.WriteLine(report.ToJsonString(new JsonSerializerOptions { WriteIndented = true }));
        return 0;
    }
}
