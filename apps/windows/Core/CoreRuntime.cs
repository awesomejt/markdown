using System.Collections.Concurrent;
using System.IO;
using System.Reflection;
using System.Runtime.InteropServices;
using System.Security.Cryptography;

namespace Markdown.Windows.Core;

public enum CoreErrorKind
{
    PackageIdentityMismatch,
    VersionMismatch,
    StructSizeMismatch,
    MissingModelV2,
    InvalidArgument,
    Busy,
    SnapshotTooLarge,
    InvalidSnapshot,
    Status,
    SessionClosed,
    BlockingWaitOnWorker,
    Cancelled,
    Superseded,
    ParseFailed,
}

public sealed class CoreException : Exception
{
    public CoreErrorKind Kind { get; }
    public string Detail { get; }
    public uint StatusCode { get; }
    public string StatusName { get; }
    public ulong Generation { get; }
    public uint FailureKind { get; }

    public CoreException(
        CoreErrorKind kind,
        string message,
        string detail = "",
        uint statusCode = 0,
        string statusName = "",
        ulong generation = 0,
        uint failureKind = 0)
        : base(message)
    {
        Kind = kind;
        Detail = detail;
        StatusCode = statusCode;
        StatusName = statusName;
        Generation = generation;
        FailureKind = failureKind;
    }

    internal static CoreException PackageIdentityMismatch(string detail) =>
        new(CoreErrorKind.PackageIdentityMismatch, $"core package identity mismatch: {detail}", detail: detail);

    internal static CoreException VersionMismatch(uint expected, uint actual) =>
        new(CoreErrorKind.VersionMismatch, $"core ABI version mismatch: expected {expected}, got {actual}");

    internal static CoreException StructSizeMismatch(string name, int expected, uint actual) =>
        new(CoreErrorKind.StructSizeMismatch, $"core ABI struct size mismatch for {name}: host {expected}, library {actual}", detail: name);

    internal static CoreException MissingModelV2(uint mask) =>
        new(CoreErrorKind.MissingModelV2, $"linked core library does not advertise MD_MODEL_V2 (mask 0x{mask:x})");

    internal static CoreException InvalidArgument(string detail) =>
        new(CoreErrorKind.InvalidArgument, $"invalid core argument: {detail}", detail: detail);

    internal static CoreException Busy() =>
        new(CoreErrorKind.Busy, "core parse queue is full");

    internal static CoreException SnapshotTooLarge(ulong bytes, ulong maxBytes) =>
        new(CoreErrorKind.SnapshotTooLarge, $"core snapshot copy requires {bytes} bytes (limit {maxBytes} bytes)");

    internal static CoreException InvalidSnapshot(string detail) =>
        new(CoreErrorKind.InvalidSnapshot, $"invalid core snapshot view: {detail}", detail: detail);

    internal static CoreException FromStatus(string call, uint status)
    {
        var name = CoreRuntime.StatusName(status);
        return new(
            CoreErrorKind.Status,
            $"{call} failed with status {status} ({name})",
            detail: call,
            statusCode: status,
            statusName: name);
    }

    internal static CoreException SessionClosed() =>
        new(CoreErrorKind.SessionClosed, "core session is closed");

    internal static CoreException BlockingWaitOnWorker() =>
        new(CoreErrorKind.BlockingWaitOnWorker, "cannot block on the markdown-core worker thread");

    internal static CoreException Cancelled(ulong generation) =>
        new(CoreErrorKind.Cancelled, $"parse generation {generation} was cancelled", generation: generation);

    internal static CoreException Superseded(ulong generation, ulong latest) =>
        new(CoreErrorKind.Superseded, $"parse generation {generation} was superseded by {latest}", generation: generation);

    internal static CoreException ParseFailed(ulong generation, uint failureKind) =>
        new(
            CoreErrorKind.ParseFailed,
            $"parse generation {generation} failed (failure kind {failureKind})",
            generation: generation,
            failureKind: failureKind);
}

public readonly record struct HostLimits(
    ulong MaxSourceBytes,
    ulong MaxNodes,
    ulong MaxDepth,
    ulong MaxEvents,
    ulong MaxTextBytes,
    ulong MaxSnapshotCopyBytes)
{
    public static HostLimits Default => new(
        MaxSourceBytes: 5UL * 1024UL * 1024UL,
        MaxNodes: 500_000UL,
        MaxDepth: 0UL,
        MaxEvents: 1_000_000UL,
        MaxTextBytes: 16UL * 1024UL * 1024UL,
        MaxSnapshotCopyBytes: 128UL * 1024UL * 1024UL);

    public static HostLimits CoreDefaults => new(
        MaxSourceBytes: 0UL,
        MaxNodes: 0UL,
        MaxDepth: 0UL,
        MaxEvents: 0UL,
        MaxTextBytes: 0UL,
        MaxSnapshotCopyBytes: 128UL * 1024UL * 1024UL);
}

public readonly record struct HandleCounts(ulong Sessions, ulong Jobs, ulong Snapshots);

public static unsafe class CoreRuntime
{
    internal const int MaxAdmitted = 2;

#if DEBUG
    internal const string ExpectedConfiguration = "debug";
#else
    internal const string ExpectedConfiguration = "release";
#endif

    private static readonly object s_validationLock = new();
    private static bool s_validated;
    private static Exception? s_validationError;

    private static readonly object s_admissionLock = new();
    private static int s_admitted;

    private static readonly BlockingCollection<Action> s_queue = new();
    private static readonly object s_workerInitLock = new();
    private static Thread? s_workerThread;
    private static Exception? s_lastCallbackException;

    [ThreadStatic]
    private static bool s_onWorker;

    public static Exception? TakeLastCallbackException() =>
        Interlocked.Exchange(ref s_lastCallbackException, null);

    internal static void ReportCallbackException(Exception ex) =>
        Interlocked.Exchange(ref s_lastCallbackException, ex);

    static CoreRuntime()
    {
        NativeLibrary.SetDllImportResolver(typeof(CoreRuntime).Assembly, ResolveLibrary);
    }

    private static nint ResolveLibrary(string libraryName, Assembly assembly, DllImportSearchPath? searchPath)
    {
        if (libraryName != CoreFfi.LibraryName)
        {
            return 0;
        }
        var path = Path.Combine(AppContext.BaseDirectory, "markdown_ffi.dll");
        return NativeLibrary.Load(path);
    }

    internal static bool OnWorker => s_onWorker;

    public static void Validate()
    {
        lock (s_validationLock)
        {
            if (s_validated)
            {
                if (s_validationError is not null)
                {
                    throw s_validationError;
                }
                return;
            }

            try
            {
                ValidateOnce();
                s_validated = true;
            }
            catch (Exception ex)
            {
                s_validationError = ex;
                s_validated = true;
                throw;
            }
        }
    }

    private static void ValidateOnce()
    {
        var expectedPath = Path.GetFullPath(Path.Combine(AppContext.BaseDirectory, "markdown_ffi.dll"));
        if (!File.Exists(expectedPath))
        {
            throw CoreException.PackageIdentityMismatch($"missing {expectedPath}");
        }

        // Force library load and verify that the loaded module in this process is
        // the exact file in AppContext.BaseDirectory.
        var insideDll = CoreFfi.md_status_name(CoreFfi.MD_OK);
        var loadedPath = GetLoadedModulePath(insideDll);
        if (!string.Equals(Path.GetFullPath(loadedPath), expectedPath, StringComparison.OrdinalIgnoreCase))
        {
            throw CoreException.PackageIdentityMismatch(
                $"loaded {loadedPath} instead of {expectedPath}");
        }

        string actualFingerprint;
        using (var stream = File.OpenRead(expectedPath))
        {
            var hash = SHA256.HashData(stream);
            actualFingerprint = Convert.ToHexStringLower(hash);
        }

        CheckIdentity(
            ExpectedConfiguration,
            CorePackageIdentity.Fingerprint,
            CorePackageIdentity.Configuration,
            actualFingerprint);

        var info = new CoreFfi.md_abi_info
        {
            struct_size = (uint)sizeof(CoreFfi.md_abi_info),
        };
        var status = CoreFfi.md_abi_negotiate(CoreFfi.MD_ABI_VERSION, &info);
        if (status == CoreFfi.MD_ERR_VERSION_MISMATCH)
        {
            throw CoreException.VersionMismatch(CoreFfi.MD_ABI_VERSION, info.abi_version);
        }
        if (status != CoreFfi.MD_OK)
        {
            throw CoreException.FromStatus("md_abi_negotiate", status);
        }

        ValidateAbiInfo(info);
    }

    internal static void CheckIdentity(
        string expectedConfiguration,
        string expectedFingerprint,
        string actualConfiguration,
        string actualFingerprint)
    {
        if (!string.Equals(expectedConfiguration.Trim(), actualConfiguration.Trim(), StringComparison.Ordinal))
        {
            throw CoreException.PackageIdentityMismatch(
                $"configuration expected '{expectedConfiguration}', got '{actualConfiguration}'");
        }
        if (string.IsNullOrWhiteSpace(expectedFingerprint) ||
            !string.Equals(expectedFingerprint.Trim(), actualFingerprint.Trim(), StringComparison.OrdinalIgnoreCase))
        {
            throw CoreException.PackageIdentityMismatch(
                $"sha256 expected '{expectedFingerprint}', got '{actualFingerprint}'");
        }
    }

    internal static void ValidateAbiInfo(CoreFfi.md_abi_info info)
    {
        if (info.abi_version != CoreFfi.MD_ABI_VERSION)
        {
            throw CoreException.VersionMismatch(CoreFfi.MD_ABI_VERSION, info.abi_version);
        }
        if (info.pointer_size != (uint)sizeof(nint))
        {
            throw CoreException.StructSizeMismatch("pointer", sizeof(nint), info.pointer_size);
        }
        if (info.anchor_version != CoreFfi.MD_ANCHOR_VERSION)
        {
            throw CoreException.VersionMismatch(CoreFfi.MD_ANCHOR_VERSION, info.anchor_version);
        }
        if (info.max_anchor_bytes != CoreFfi.MD_ANCHOR_MAX_BYTES)
        {
            throw CoreException.StructSizeMismatch(
                "max_anchor_bytes",
                (int)CoreFfi.MD_ANCHOR_MAX_BYTES,
                info.max_anchor_bytes);
        }

        CheckSize("md_session_options", sizeof(CoreFfi.md_session_options), info.session_options_size);
        CheckSize("md_session_status_info", sizeof(CoreFfi.md_session_status_info), info.session_status_size);
        CheckSize("md_job_result", sizeof(CoreFfi.md_job_result), info.job_result_size);
        CheckSize("md_snapshot_view", sizeof(CoreFfi.md_snapshot_view), info.snapshot_view_size);
        CheckSize("md_node", sizeof(CoreFfi.md_node), info.node_size);
        CheckSize("md_intent", sizeof(CoreFfi.md_intent), info.intent_size);
        CheckSize("md_outline_entry", sizeof(CoreFfi.md_outline_entry), info.outline_entry_size);
        CheckSize("md_diagnostic", sizeof(CoreFfi.md_diagnostic), info.diagnostic_size);
        CheckSize("md_property", sizeof(CoreFfi.md_property), info.property_size);
        CheckSize("md_anchor_resolution", sizeof(CoreFfi.md_anchor_resolution), info.anchor_resolution_size);

        // D-022 struct size verification.
        CheckSize("md_find_options", sizeof(CoreFfi.md_find_options), CoreFfi.MD_FIND_OPTIONS_SIZE);
        CheckSize("md_find_result", sizeof(CoreFfi.md_find_result), CoreFfi.MD_FIND_RESULT_SIZE);
        CheckSize("md_find_match", sizeof(CoreFfi.md_find_match), CoreFfi.MD_FIND_MATCH_SIZE);
        CheckSize("md_match_segment", sizeof(CoreFfi.md_match_segment), CoreFfi.MD_MATCH_SEGMENT_SIZE);
        CheckSize("md_read_options", sizeof(CoreFfi.md_read_options), CoreFfi.MD_READ_OPTIONS_SIZE);
        CheckSize("md_read_stats", sizeof(CoreFfi.md_read_stats), CoreFfi.MD_READ_STATS_SIZE);
        CheckSize("md_read_section", sizeof(CoreFfi.md_read_section), CoreFfi.MD_READ_SECTION_SIZE);
        CheckSize("md_workspace_path_info", sizeof(CoreFfi.md_workspace_path_info), CoreFfi.MD_WORKSPACE_PATH_INFO_SIZE);
        CheckSize("md_workspace_entry", sizeof(CoreFfi.md_workspace_entry), CoreFfi.MD_WORKSPACE_ENTRY_SIZE);
        CheckSize("md_name_options", sizeof(CoreFfi.md_name_options), CoreFfi.MD_NAME_OPTIONS_SIZE);
        CheckSize("md_name_result", sizeof(CoreFfi.md_name_result), CoreFfi.MD_NAME_RESULT_SIZE);
        CheckSize("md_name_hit", sizeof(CoreFfi.md_name_hit), CoreFfi.MD_NAME_HIT_SIZE);
        CheckSize("md_content_options", sizeof(CoreFfi.md_content_options), CoreFfi.MD_CONTENT_OPTIONS_SIZE);
        CheckSize("md_content_progress", sizeof(CoreFfi.md_content_progress), CoreFfi.MD_CONTENT_PROGRESS_SIZE);
        CheckSize("md_content_result", sizeof(CoreFfi.md_content_result), CoreFfi.MD_CONTENT_RESULT_SIZE);
        CheckSize("md_content_hit", sizeof(CoreFfi.md_content_hit), CoreFfi.MD_CONTENT_HIT_SIZE);

        if ((info.model_versions & (1u << (int)CoreFfi.MD_MODEL_V2)) == 0)
        {
            throw CoreException.MissingModelV2(info.model_versions);
        }
    }

    private static void CheckSize(string name, int hostSize, uint librarySize)
    {
        if ((uint)hostSize != librarySize)
        {
            throw CoreException.StructSizeMismatch(name, hostSize, librarySize);
        }
    }

    public static HandleCounts LiveHandles()
    {
        Validate();
        ulong sessions = 0, jobs = 0, snapshots = 0;
        var status = CoreFfi.md_live_handles(&sessions, &jobs, &snapshots);
        if (status != CoreFfi.MD_OK)
        {
            throw CoreException.FromStatus("md_live_handles", status);
        }
        return new HandleCounts(sessions, jobs, snapshots);
    }

    public static string StatusName(uint status)
    {
        var ptr = CoreFfi.md_status_name(status);
        return ptr is null ? "unknown" : Marshal.PtrToStringUTF8((nint)ptr) ?? "unknown";
    }

    internal static bool TryAcquireSlot()
    {
        lock (s_admissionLock)
        {
            if (s_admitted >= MaxAdmitted)
            {
                return false;
            }
            s_admitted++;
            return true;
        }
    }

    internal static void ReleaseSlot()
    {
        lock (s_admissionLock)
        {
            if (s_admitted > 0)
            {
                s_admitted--;
            }
        }
    }

    internal static int PendingParses
    {
        get
        {
            lock (s_admissionLock)
            {
                return s_admitted;
            }
        }
    }

    internal static void EnqueueWork(Action work)
    {
        EnsureWorkerStarted();
        s_queue.Add(work);
    }

    private static void EnsureWorkerStarted()
    {
        if (s_workerThread is not null)
        {
            return;
        }
        lock (s_workerInitLock)
        {
            if (s_workerThread is not null)
            {
                return;
            }
            var thread = new Thread(WorkerLoop)
            {
                Name = "markdown-core",
                IsBackground = true,
            };
            s_workerThread = thread;
            thread.Start();
        }
    }

    private static void WorkerLoop()
    {
        s_onWorker = true;
        foreach (var work in s_queue.GetConsumingEnumerable())
        {
            try
            {
                work();
            }
            catch (Exception ex)
            {
                ReportCallbackException(ex);
            }
        }
    }

    private const uint GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT = 0x00000002;
    private const uint GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS = 0x00000004;

    [DllImport("kernel32.dll", ExactSpelling = true, SetLastError = true)]
    private static extern int GetModuleHandleExW(uint dwFlags, void* lpModuleName, nint* phModule);

    [DllImport("kernel32.dll", ExactSpelling = true, SetLastError = true)]
    private static extern uint GetModuleFileNameW(nint hModule, char* lpFilename, uint nSize);

    private static string GetLoadedModulePath(void* addressInModule)
    {
        nint module = 0;
        if (GetModuleHandleExW(
                GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
                addressInModule,
                &module) == 0 ||
            module == 0)
        {
            throw CoreException.PackageIdentityMismatch("GetModuleHandleExW failed for markdown_ffi.dll");
        }

        const int capacity = 4096;
        char* buffer = stackalloc char[capacity];
        var length = GetModuleFileNameW(module, buffer, capacity);
        if (length == 0 || length >= capacity)
        {
            throw CoreException.PackageIdentityMismatch("GetModuleFileNameW failed for markdown_ffi.dll");
        }
        return new string(buffer, 0, (int)length);
    }
}
