using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Text;

namespace Feas006;

public enum CoreStatus : uint { Ok, BadArgument, VersionMismatch, InvalidUtf8, InputLimit, Cancelled, Panic, InvalidHandle, OutputLimit }
public sealed class CoreException(CoreStatus status) : Exception($"Rust core returned {status}")
{ public CoreStatus Status { get; } = status; }

[StructLayout(LayoutKind.Sequential)]
public struct AbiInfo { public uint Version, PointerBytes, OptionsBytes, BlockBytes, SnapshotBytes, ProbeHooks; }
[StructLayout(LayoutKind.Sequential)]
public struct ParseOptions
{
    public uint Version, StructBytes;
    public ulong MaxInputBytes, MaxOutputBytes, MaxBlocks, CancelHandle;
    public static ParseOptions Default => new() { Version = 1, StructBytes = (uint)Marshal.SizeOf<ParseOptions>(),
        MaxInputBytes = 16 * 1024 * 1024, MaxOutputBytes = 64 * 1024 * 1024, MaxBlocks = 1_000_000 };
}
[StructLayout(LayoutKind.Sequential)]
public struct NativeBlock
{
    public ulong Id;
    public uint Kind, HeadingLevel;
    public ulong SourceStart, SourceEnd, TextOffset, TextLength;
}
[StructLayout(LayoutKind.Sequential)]
public struct NativeSnapshot
{
    public uint Version, StructBytes;
    public ulong Handle;
    public nint Text;
    public ulong TextLength;
    public nint Blocks;
    public ulong BlockCount, CoreNs;
}

public static class NativeApi
{
    const string Library = "feas006_core";
    static NativeApi()
    {
        NativeLibrary.SetDllImportResolver(typeof(NativeApi).Assembly, (name, assembly, searchPath) => {
            if (name != Library) return 0;
            string file = Environment.GetEnvironmentVariable("FEAS006_CORE_DLL")
                ?? Path.Combine(AppContext.BaseDirectory, OperatingSystem.IsWindows() ? "feas006_core.dll"
                    : OperatingSystem.IsMacOS() ? "libfeas006_core.dylib" : "libfeas006_core.so");
            return NativeLibrary.Load(Path.GetFullPath(file));
        });
    }
    [DllImport(Library, CallingConvention = CallingConvention.Cdecl, EntryPoint = "md_get_abi_v1", ExactSpelling = true)]
    public static extern CoreStatus Info(uint version, out AbiInfo info);
    [DllImport(Library, CallingConvention = CallingConvention.Cdecl, EntryPoint = "md_parse_v1", ExactSpelling = true)]
    public static extern CoreStatus Parse(nint input, ulong length, in ParseOptions options, out NativeSnapshot snapshot);
    [DllImport(Library, CallingConvention = CallingConvention.Cdecl, EntryPoint = "md_snapshot_free_v1", ExactSpelling = true)]
    public static extern CoreStatus Free(ulong handle);
    [DllImport(Library, CallingConvention = CallingConvention.Cdecl, EntryPoint = "md_cancel_create_v1", ExactSpelling = true)]
    public static extern CoreStatus CancelCreate(out ulong handle);
    [DllImport(Library, CallingConvention = CallingConvention.Cdecl, EntryPoint = "md_cancel_request_v1", ExactSpelling = true)]
    public static extern CoreStatus CancelRequest(ulong handle);
    [DllImport(Library, CallingConvention = CallingConvention.Cdecl, EntryPoint = "md_cancel_free_v1", ExactSpelling = true)]
    public static extern CoreStatus CancelFree(ulong handle);
    [DllImport(Library, CallingConvention = CallingConvention.Cdecl, EntryPoint = "md_probe_live_v1", ExactSpelling = true)]
    public static extern CoreStatus ProbeLive(out ulong snapshots, out ulong cancellations);
    [DllImport(Library, CallingConvention = CallingConvention.Cdecl, EntryPoint = "md_probe_panic_v1", ExactSpelling = true)]
    public static extern CoreStatus ProbePanic();
    [DllImport(Library, CallingConvention = CallingConvention.Cdecl, EntryPoint = "md_probe_active_v1", ExactSpelling = true)]
    public static extern ulong ProbeActive();
}

public sealed class SnapshotOwner : SafeHandle
{
    public SnapshotOwner(ulong value) : base(0, true) => SetHandle(unchecked((nint)(long)value));
    public override bool IsInvalid => handle == 0;
    protected override bool ReleaseHandle() => NativeApi.Free(unchecked((ulong)handle.ToInt64())) == CoreStatus.Ok;
}
public sealed class NativeCancellation : SafeHandle
{
    public NativeCancellation() : base(0, true) { Core.Ensure(NativeApi.CancelCreate(out ulong value)); SetHandle(unchecked((nint)(long)value)); }
    public ulong Id => unchecked((ulong)handle.ToInt64());
    public override bool IsInvalid => handle == 0;
    public CoreStatus Request() => IsClosed ? CoreStatus.InvalidHandle : NativeApi.CancelRequest(Id);
    protected override bool ReleaseHandle() => NativeApi.CancelFree(Id) == CoreStatus.Ok;
}

public sealed record CoreBlock(ulong Id, uint Kind, uint HeadingLevel, ulong SourceStart, ulong SourceEnd, string Text);
public sealed record CoreDocument(List<CoreBlock> Blocks, double CoreMs, double NativeCallMs,
    double CopyDecodeMs, ulong TextBytes, ulong TableBytes);

public static class Core
{
    public static readonly AbiInfo Abi;
    static Core()
    {
        Ensure(NativeApi.Info(1, out Abi));
        if (Abi.Version != 1 || Abi.PointerBytes != (uint)IntPtr.Size
            || Abi.OptionsBytes != Marshal.SizeOf<ParseOptions>() || Abi.BlockBytes != Marshal.SizeOf<NativeBlock>()
            || Abi.SnapshotBytes != Marshal.SizeOf<NativeSnapshot>())
            throw new InvalidOperationException("Native ABI layout negotiation failed");
    }
    public static void Ensure(CoreStatus status) { if (status != CoreStatus.Ok) throw new CoreException(status); }
    public static unsafe CoreDocument ParseAndCopy(byte[] bytes, NativeCancellation? cancellation = null)
    {
        ParseOptions options = ParseOptions.Default;
        options.CancelHandle = cancellation?.Id ?? 0;
        var nativeTimer = Stopwatch.StartNew();
        NativeSnapshot snapshot;
        CoreStatus status;
        fixed (byte* pointer = bytes)
            status = NativeApi.Parse((nint)pointer, (ulong)bytes.Length, options, out snapshot);
        GC.KeepAlive(cancellation);
        double callMs = nativeTimer.Elapsed.TotalMilliseconds;
        Ensure(status);
        using var owner = new SnapshotOwner(snapshot.Handle);
        if (snapshot.Handle == 0 || snapshot.Version != 1 || snapshot.StructBytes != Marshal.SizeOf<NativeSnapshot>()
            || snapshot.TextLength > options.MaxOutputBytes || snapshot.BlockCount > options.MaxBlocks)
            throw new InvalidOperationException("Invalid native snapshot envelope");
        var timer = Stopwatch.StartNew();
        // One bulk copy of each immutable buffer. No per-node FFI calls or UI objects.
        var payload = new ReadOnlySpan<byte>((void*)snapshot.Text, checked((int)snapshot.TextLength)).ToArray();
        var table = new ReadOnlySpan<NativeBlock>((void*)snapshot.Blocks, checked((int)snapshot.BlockCount)).ToArray();
        var blocks = new List<CoreBlock>(table.Length);
        var utf8 = new UTF8Encoding(false, true);
        for (int index = 0; index < table.Length; index++)
        {
            var block = table[index];
            if (block.Id != (ulong)index + 1 || block.Kind > 5 || block.HeadingLevel > 6
                || block.SourceStart > block.SourceEnd || block.SourceEnd > (ulong)bytes.Length
                || block.TextOffset > (ulong)payload.Length || block.TextLength > (ulong)payload.Length - block.TextOffset)
                throw new InvalidOperationException("Native block bounds failed validation");
            blocks.Add(new CoreBlock(block.Id, block.Kind, block.HeadingLevel, block.SourceStart, block.SourceEnd,
                utf8.GetString(payload.AsSpan(checked((int)block.TextOffset), checked((int)block.TextLength)))));
        }
        return new(blocks, snapshot.CoreNs / 1e6, callMs, timer.Elapsed.TotalMilliseconds,
            snapshot.TextLength, snapshot.BlockCount * (ulong)Marshal.SizeOf<NativeBlock>());
    }
}
