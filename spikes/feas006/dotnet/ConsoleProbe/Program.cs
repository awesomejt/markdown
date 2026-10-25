using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Text;
using System.Text.Json;
using Feas006;

static unsafe CoreStatus Raw(byte[] bytes, ParseOptions options, out NativeSnapshot snapshot)
{ fixed (byte* input = bytes) return NativeApi.Parse((nint)input, (ulong)bytes.Length, options, out snapshot); }
static void Check(bool success, string message) { if (!success) throw new Exception(message); }

if (args.Length == 0) throw new ArgumentException("usage: ConsoleProbe <file> [iterations] or --self-test <5MiB-file>");
if (args[0] == "--contract")
{
    var document = Core.ParseAndCopy(File.ReadAllBytes(args[1]));
    Console.WriteLine(JsonSerializer.Serialize(new { block_count = document.Blocks.Count,
        headings = document.Blocks.Where(b => b.Kind == 1).Select(b => new { level = b.HeadingLevel, text = b.Text }) }));
    return;
}
if (args[0] == "--self-test")
{
    _ = Core.Abi;
    Check(NativeApi.Info(2, out _) == CoreStatus.VersionMismatch, "version negotiation");
    var options = ParseOptions.Default; options.StructBytes--;
    Check(Raw([1], options, out _) == CoreStatus.VersionMismatch, "struct size");
    options = ParseOptions.Default;
    Check(Raw([0xff], options, out var empty) == CoreStatus.InvalidUtf8 && empty.Handle == 0, "invalid UTF8");
    Check(NativeApi.Parse(0, 1, options, out _) == CoreStatus.BadArgument, "null input");
    options.MaxInputBytes = 1;
    Check(Raw([1, 2], options, out _) == CoreStatus.InputLimit, "input limit");
    options = ParseOptions.Default; options.MaxOutputBytes = 1;
    Check(Raw(Encoding.UTF8.GetBytes("# x"), options, out _) == CoreStatus.OutputLimit, "output limit");
    options = ParseOptions.Default;
    Core.Ensure(Raw(Encoding.UTF8.GetBytes("# x"), options, out var snapshot));
    Check(NativeApi.Free(snapshot.Handle) == CoreStatus.Ok && NativeApi.Free(snapshot.Handle) == CoreStatus.InvalidHandle, "double release");
    Core.Ensure(Raw(Encoding.UTF8.GetBytes("# owned"), options, out snapshot));
    try { using var owner = new SnapshotOwner(snapshot.Handle); throw new InvalidOperationException("host copy failure probe"); }
    catch (InvalidOperationException) { }
    using (var token = new NativeCancellation())
    {
        Core.Ensure(token.Request());
        options.CancelHandle = token.Id;
        Check(Raw([1], options, out _) == CoreStatus.Cancelled, "pre-cancellation");
    }
    Check(Raw([1], options, out _) == CoreStatus.InvalidHandle, "stale cancellation");
    Check(Core.Abi.ProbeHooks == 1 && NativeApi.ProbePanic() == CoreStatus.Panic, "panic boundary");
    var large = File.ReadAllBytes(args[1]);
    var cancelled = 0;
    var inFlightObserved = 0;
    var cancellationTimes = new List<double>();
    for (int i = 0; i < 10; i++)
    {
        using var token = new NativeCancellation();
        var task = Task.Run(() => { try { Core.ParseAndCopy(large, token); return CoreStatus.Ok; } catch (CoreException e) { return e.Status; } });
        var entered = Stopwatch.StartNew();
        while (NativeApi.ProbeActive() == 0 && !task.IsCompleted && entered.ElapsedMilliseconds < 2000) Thread.Yield();
        if (NativeApi.ProbeActive() > 0) inFlightObserved++;
        var timer = Stopwatch.StartNew(); Core.Ensure(token.Request());
        var status = task.GetAwaiter().GetResult();
        if (status == CoreStatus.Cancelled) { cancelled++; cancellationTimes.Add(timer.Elapsed.TotalMilliseconds); }
        else Check(status == CoreStatus.Ok, "unexpected cancellation result");
    }
    var bytes = Encoding.UTF8.GetBytes("# café\n\nA **document** with `code`.\n");
    var retained = Core.ParseAndCopy(bytes);
    Parallel.For(0, 8, _ => { for (int i = 0; i < 50; i++) Core.ParseAndCopy(bytes); });
    var memory = new List<long>();
    for (int i = 0; i < 1000; i++)
    {
        var document = Core.ParseAndCopy(i % 100 == 0 ? large : bytes);
        if (i % 100 == 99)
        {
            GC.Collect();GC.WaitForPendingFinalizers();
            Core.Ensure(NativeApi.ProbeLive(out ulong snapshots, out ulong tokens));
            Check(snapshots == 0 && tokens == 0, "native ownership leak");
            using var process = Process.GetCurrentProcess(); process.Refresh(); memory.Add(process.PrivateMemorySize64);
        }
    }
    Core.Ensure(NativeApi.ProbeLive(out var liveSnapshots, out var liveTokens));
    Check(cancelled > 0 && inFlightObserved > 0 && liveSnapshots == 0 && liveTokens == 0
        && retained.Blocks[0].Text == "café", "cancellation/lifetime invariants");
    Console.WriteLine(JsonSerializer.Serialize(new { self_test = "passed", lifecycle_iterations = 1000,
        cancellations_observed = cancelled, entered_native_before_cancel = inFlightObserved,
        cancellation_request_to_return_ms = cancellationTimes,
        snapshots_live = liveSnapshots, tokens_live = liveTokens, private_bytes_after_batches = memory }));
    return;
}

var input = File.ReadAllBytes(args[0]);
int iterations = args.Length > 1 ? int.Parse(args[1]) : 5;
var rows = new List<object>();
for (int i = 0; i < iterations; i++)
{
    var total = Stopwatch.StartNew();
    var document = Core.ParseAndCopy(input);
    double totalMs = total.Elapsed.TotalMilliseconds;
    using var process = Process.GetCurrentProcess();process.Refresh();
    rows.Add(new { iteration = i, core_parse_model_ms = document.CoreMs, native_call_ms = document.NativeCallMs,
        end_to_end_ms = totalMs,
        copy_decode_ms = document.CopyDecodeMs, blocks = document.Blocks.Count, text_bytes = document.TextBytes,
        block_table_bytes = document.TableBytes, private_bytes = process.PrivateMemorySize64,
        peak_working_set_bytes = process.PeakWorkingSet64 });
}
Console.WriteLine(JsonSerializer.Serialize(new { abi = Core.Abi.Version, pointer_bytes = Core.Abi.PointerBytes,
    probe_hooks = Core.Abi.ProbeHooks,
    block_bytes = Core.Abi.BlockBytes, options_bytes = Core.Abi.OptionsBytes, snapshot_bytes = Core.Abi.SnapshotBytes,
    runtime = RuntimeInformation.FrameworkDescription, input_bytes = input.Length, rows }));
