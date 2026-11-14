using System.Diagnostics;
using System.Globalization;
using System.IO;
using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;
using System.Text;
using Markdown.Windows.Core;

namespace Markdown.Windows;

internal static unsafe class CoreProbe
{
    [StructLayout(LayoutKind.Sequential)]
    private struct PROCESS_MEMORY_COUNTERS_EX
    {
        public uint cb;
        public uint PageFaultCount;
        public nuint PeakWorkingSetSize;
        public nuint WorkingSetSize;
        public nuint QuotaPeakPagedPoolUsage;
        public nuint QuotaPagedPoolUsage;
        public nuint QuotaPeakNonPagedPoolUsage;
        public nuint QuotaNonPagedPoolUsage;
        public nuint PagefileUsage;
        public nuint PeakPagefileUsage;
        public nuint PrivateUsage;
    }

    [DllImport("kernel32.dll", ExactSpelling = true)]
    private static extern nint GetCurrentProcess();

    [DllImport("kernel32.dll", ExactSpelling = true, SetLastError = true)]
    private static extern int K32GetProcessMemoryInfo(
        nint Process,
        PROCESS_MEMORY_COUNTERS_EX* ppsmemCounters,
        uint cb);

    private readonly record struct MemorySnapshot(
        ulong WorkingSet,
        ulong PeakWorkingSet,
        ulong PrivateBytes,
        ulong ManagedHeapBytes);

    private static MemorySnapshot SampleMemory(bool collectManaged = true)
    {
        if (collectManaged)
        {
            GC.Collect(GC.MaxGeneration, GCCollectionMode.Forced, blocking: true, compacting: true);
            GC.WaitForPendingFinalizers();
            GC.Collect(GC.MaxGeneration, GCCollectionMode.Forced, blocking: true, compacting: true);
        }

        var counters = new PROCESS_MEMORY_COUNTERS_EX
        {
            cb = (uint)sizeof(PROCESS_MEMORY_COUNTERS_EX),
        };
        if (K32GetProcessMemoryInfo(GetCurrentProcess(), &counters, counters.cb) == 0)
        {
            throw new InvalidOperationException("K32GetProcessMemoryInfo failed");
        }

        var managed = (ulong)Math.Max(0, GC.GetTotalMemory(forceFullCollection: false));
        return new MemorySnapshot(
            WorkingSet: counters.WorkingSetSize,
            PeakWorkingSet: counters.PeakWorkingSetSize,
            PrivateBytes: counters.PrivateUsage,
            ManagedHeapBytes: managed);
    }

    public static int Run(string[] args)
    {
        CoreRuntime.Validate();
        var mode = args.Length > 0 ? args[0] : "single";
        return mode switch
        {
            "single" => RunSingle(args.Length > 1 ? args[1] : null),
            "repeated" => RunRepeated(
                args.Length > 1 ? args[1] : null,
                args.Length > 2 ? int.Parse(args[2], CultureInfo.InvariantCulture) : 20),
            "dense-host" => RunDense(hostLimits: true),
            "dense-default" => RunDense(hostLimits: false),
            _ => throw new ArgumentException($"unknown --core-probe mode '{mode}' (expected single, repeated, dense-host, dense-default)"),
        };
    }

    private static byte[] LoadOrSynthesizeInput(string? path)
    {
        if (!string.IsNullOrEmpty(path))
        {
            return File.ReadAllBytes(path);
        }

        // Relocated smoke/probe fallback when no fixture path is provided:
        // deterministic ~256 KiB Markdown document.
        var sb = new StringBuilder(262_144);
        sb.Append("---\ntitle: Synthetic Probe\n---\n");
        for (int i = 0; i < 400; i++)
        {
            sb.Append(CultureInfo.InvariantCulture, $"## Section {i}\n\n");
            sb.Append("Paragraph with *emphasis*, **strong**, `code`, and a [link](https://example.com/item). ");
            sb.Append("Another sentence with an ![image](assets/diagram.png) and a list:\n\n");
            sb.Append("- [x] first task item\n- [ ] second task item\n\n");
        }
        return Encoding.UTF8.GetBytes(sb.ToString());
    }

    [MethodImpl(MethodImplOptions.NoInlining)]
    private static (CoreTimings Timings, ulong CopiedBytes, int Nodes, int Outline, int Links, int Resources, MemorySnapshot WithLiveSessionAndDoc, MemorySnapshot AfterSessionClose) ExecuteSingleCycle(byte[] source)
    {
        var session = new CoreSession("probe-single");
        var result = session.ParseBlocking(source, "rev-1"u8.ToArray(), copyUnchanged: true);
        if (result.Outcome != CoreOutcome.Published || result.Document is null)
        {
            throw new InvalidOperationException($"single probe parse failed: {result.Outcome} ({result.Failure})");
        }

        var doc = result.Document;
        var withLive = SampleMemory(collectManaged: true);

        session.Close();
        var afterSessionClose = SampleMemory(collectManaged: true);

        var copiedBytes = doc.CopiedBytes;
        var nodes = doc.NodeCount;
        var outline = doc.OutlineCount;
        var links = doc.LinkCount;
        var resources = doc.ResourceCount;
        GC.KeepAlive(doc);

        return (result.Timings, copiedBytes, nodes, outline, links, resources, withLive, afterSessionClose);
    }

    private static int RunSingle(string? path)
    {
        var beforeLoad = SampleMemory(collectManaged: true);
        var source = LoadOrSynthesizeInput(path);
        var afterLoad = SampleMemory(collectManaged: true);

        var (timings, copiedBytes, nodes, outline, links, resources, withLive, afterSessionClose) =
            ExecuteSingleCycle(source);

        var afterDocDrop = SampleMemory(collectManaged: true);
        _ = Program.WaitZeroLiveHandles(2000);
        var handles = CoreRuntime.LiveHandles();
        if (handles != new HandleCounts(0, 0, 0))
        {
            throw new InvalidOperationException($"handles leaked after single probe: {handles}");
        }

        Console.WriteLine(
            string.Create(
                CultureInfo.InvariantCulture,
                $"mode=single configuration={CoreRuntime.ExpectedConfiguration} " +
                $"source_bytes={source.Length} copied_bytes={copiedBytes} " +
                $"nodes={nodes} outline={outline} links={links} resources={resources} " +
                $"parse_ms={timings.Parse.TotalMilliseconds:F3} " +
                $"encode_ms={timings.Encode.TotalMilliseconds:F3} " +
                $"copy_ms={timings.Copy.TotalMilliseconds:F3} " +
                $"rss_before_load={beforeLoad.WorkingSet} " +
                $"rss_after_load={afterLoad.WorkingSet} " +
                $"rss_live_session_and_doc={withLive.WorkingSet} " +
                $"rss_after_session_close={afterSessionClose.WorkingSet} " +
                $"rss_after_doc_drop={afterDocDrop.WorkingSet} " +
                $"peak_rss={afterDocDrop.PeakWorkingSet} " +
                $"private_after_load={afterLoad.PrivateBytes} " +
                $"private_live_session_and_doc={withLive.PrivateBytes} " +
                $"private_after_session_close={afterSessionClose.PrivateBytes} " +
                $"private_after_doc_drop={afterDocDrop.PrivateBytes} " +
                $"gc_heap_live_doc={withLive.ManagedHeapBytes} " +
                $"gc_heap_after_drop={afterDocDrop.ManagedHeapBytes} " +
                $"handles={handles.Sessions},{handles.Jobs},{handles.Snapshots}"));
        return 0;
    }

    [MethodImpl(MethodImplOptions.NoInlining)]
    private static CoreTimings ExecuteOneIteration(byte[] source, int index)
    {
        using var session = new CoreSession($"probe-repeated-{index}");
        var result = session.ParseBlocking(source, null, copyUnchanged: true);
        if (result.Outcome != CoreOutcome.Published || result.Document is null)
        {
            throw new InvalidOperationException($"iteration {index} failed: {result.Outcome}");
        }
        session.Close();
        return result.Timings;
    }

    private static int RunRepeated(string? path, int iterations)
    {
        if (iterations < 1)
        {
            throw new ArgumentOutOfRangeException(nameof(iterations));
        }

        var source = LoadOrSynthesizeInput(path);
        var afterLoad = SampleMemory(collectManaged: true);

        var parseMs = new double[iterations];
        var encodeMs = new double[iterations];
        var copyMs = new double[iterations];

        MemorySnapshot afterFirst = default;
        MemorySnapshot afterTenth = default;
        MemorySnapshot afterFinal = default;

        for (int i = 0; i < iterations; i++)
        {
            var timings = ExecuteOneIteration(source, i);
            parseMs[i] = timings.Parse.TotalMilliseconds;
            encodeMs[i] = timings.Encode.TotalMilliseconds;
            copyMs[i] = timings.Copy.TotalMilliseconds;

            var snap = SampleMemory(collectManaged: true);
            if (i == 0)
            {
                afterFirst = snap;
            }
            if (i == Math.Min(9, iterations - 1))
            {
                afterTenth = snap;
            }
            if (i == iterations - 1)
            {
                afterFinal = snap;
            }
        }

        Array.Sort(parseMs);
        Array.Sort(encodeMs);
        Array.Sort(copyMs);
        int mid = iterations / 2;

        _ = Program.WaitZeroLiveHandles(2000);
        var handles = CoreRuntime.LiveHandles();
        if (handles != new HandleCounts(0, 0, 0))
        {
            throw new InvalidOperationException($"handles leaked after repeated probe: {handles}");
        }

        Console.WriteLine(
            string.Create(
                CultureInfo.InvariantCulture,
                $"mode=repeated configuration={CoreRuntime.ExpectedConfiguration} " +
                $"iterations={iterations} source_bytes={source.Length} " +
                $"parse_median_ms={parseMs[mid]:F3} parse_max_ms={parseMs[^1]:F3} " +
                $"encode_median_ms={encodeMs[mid]:F3} encode_max_ms={encodeMs[^1]:F3} " +
                $"copy_median_ms={copyMs[mid]:F3} copy_max_ms={copyMs[^1]:F3} " +
                $"rss_after_load={afterLoad.WorkingSet} " +
                $"rss_after_1={afterFirst.WorkingSet} " +
                $"rss_after_10={afterTenth.WorkingSet} " +
                $"rss_after_{iterations}={afterFinal.WorkingSet} " +
                $"peak_rss={afterFinal.PeakWorkingSet} " +
                $"private_after_1={afterFirst.PrivateBytes} " +
                $"private_after_10={afterTenth.PrivateBytes} " +
                $"private_after_{iterations}={afterFinal.PrivateBytes} " +
                $"gc_heap_after_1={afterFirst.ManagedHeapBytes} " +
                $"gc_heap_after_{iterations}={afterFinal.ManagedHeapBytes} " +
                $"handles={handles.Sessions},{handles.Jobs},{handles.Snapshots}"));
        return 0;
    }

    private static int RunDense(bool hostLimits)
    {
        var limits = hostLimits ? HostLimits.Default : HostLimits.CoreDefaults;
        var byteCount = hostLimits ? 5 * 1024 * 1024 : 8 * 1024 * 1024;
        var dense = new byte[byteCount];
        for (int i = 0; i < byteCount; i += 2)
        {
            dense[i] = (byte)'*';
            dense[i + 1] = (byte)'a';
        }

        var before = SampleMemory(collectManaged: true);
        var sw = Stopwatch.StartNew();
        using var session = CoreSession.WithLimits("probe-dense", limits);
        var result = session.ParseBlocking(dense, null, copyUnchanged: false);
        sw.Stop();
        if (result.Outcome != CoreOutcome.Failed || result.Failure != CoreParseFailure.NodeLimit)
        {
            throw new InvalidOperationException(
                $"expected Failed(NodeLimit), got {result.Outcome}({result.Failure})");
        }
        session.Close();
        var after = SampleMemory(collectManaged: true);

        _ = Program.WaitZeroLiveHandles(2000);
        var handles = CoreRuntime.LiveHandles();
        if (handles != new HandleCounts(0, 0, 0))
        {
            throw new InvalidOperationException($"handles leaked after dense probe: {handles}");
        }

        var modeName = hostLimits ? "dense-host" : "dense-default";
        var peakDelta = after.PeakWorkingSet >= before.WorkingSet
            ? after.PeakWorkingSet - before.WorkingSet
            : 0UL;

        Console.WriteLine(
            string.Create(
                CultureInfo.InvariantCulture,
                $"mode={modeName} configuration={CoreRuntime.ExpectedConfiguration} " +
                $"source_bytes={byteCount} max_nodes={limits.MaxNodes} max_events={limits.MaxEvents} " +
                $"outcome={result.Outcome} failure={result.Failure} " +
                $"duration_ms={sw.Elapsed.TotalMilliseconds:F3} " +
                $"rss_before={before.WorkingSet} " +
                $"rss_after_close={after.WorkingSet} " +
                $"peak_rss={after.PeakWorkingSet} " +
                $"peak_delta_bytes={peakDelta} " +
                $"private_before={before.PrivateBytes} " +
                $"private_after_close={after.PrivateBytes} " +
                $"handles={handles.Sessions},{handles.Jobs},{handles.Snapshots}"));
        return 0;
    }
}
