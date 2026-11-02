using System.Diagnostics;
using System.Text;

namespace Markdown.Windows.Core;

public enum CoreOutcome : uint
{
    Published = 1,
    Unchanged = 2,
    Superseded = 3,
    Cancelled = 4,
    Failed = 5,
    Closed = 6,
}

public enum CoreParseFailure : uint
{
    InvalidUtf8 = 1,
    SourceLimit = 2,
    EventLimit = 3,
    NodeLimit = 4,
    DepthLimit = 5,
    TextLimit = 6,
    SourceMapping = 7,
    Model = 8,
    Other = 9,
}

public enum CoreUnavailability
{
    Deleted,
    PermissionDenied,
    AccessRevoked,
    Busy,
    Io,
}

public readonly record struct CoreAvailability(bool IsAvailable, CoreUnavailability? Unavailability)
{
    public static CoreAvailability Available => new(true, null);
    public static CoreAvailability Unavailable(CoreUnavailability reason) => new(false, reason);
}

public readonly record struct CoreTimings(TimeSpan Parse, TimeSpan Encode, TimeSpan Copy);

public sealed record CoreResult(
    CoreOutcome Outcome,
    ulong Generation,
    ulong Latest,
    CoreParseFailure? Failure,
    CoreDocument? Document,
    CoreTimings Timings);

public sealed unsafe class CoreSession : IDisposable
{
    private readonly SessionInner _inner;

    public HostLimits Limits => _inner.Limits;

    public CoreSession(string documentId)
        : this(documentId, HostLimits.Default)
    {
    }

    public CoreSession(string documentId, HostLimits limits)
    {
        try
        {
            CoreRuntime.Validate();
            var idBytes = Encoding.UTF8.GetBytes(documentId ?? string.Empty);
            var options = new CoreFfi.md_session_options
            {
                struct_size = (uint)sizeof(CoreFfi.md_session_options),
                model_version = CoreFfi.MD_MODEL_V2,
                max_source_bytes = limits.MaxSourceBytes,
                max_nodes = limits.MaxNodes,
                max_depth = limits.MaxDepth,
                max_events = limits.MaxEvents,
                max_text_bytes = limits.MaxTextBytes,
            };

            ulong handle = 0;
            uint status;
            fixed (byte* idPtr = idBytes)
            {
                status = CoreFfi.md_session_create(idPtr, (nuint)idBytes.Length, &options, &handle);
            }
            if (status == CoreFfi.MD_ERR_INVALID_DOCUMENT_ID || status == CoreFfi.MD_ERR_INVALID_ARGUMENT)
            {
                throw CoreException.InvalidArgument($"invalid document_id '{documentId}' ({CoreRuntime.StatusName(status)})");
            }
            if (status != CoreFfi.MD_OK)
            {
                throw CoreException.FromStatus("md_session_create", status);
            }

            _inner = new SessionInner(handle, limits);
        }
        catch
        {
            GC.SuppressFinalize(this);
            throw;
        }
    }

    public static CoreSession WithLimits(string documentId, HostLimits limits) =>
        new(documentId, limits);

    ~CoreSession()
    {
        _inner?.DropNonBlocking();
    }

    /// <summary>
    /// Non-blocking dispose: detaches and cancels immediately, then releases on the worker.
    /// Safe to call from any thread including the UI thread or the worker thread.
    /// </summary>
    public void Dispose()
    {
        _inner.DropNonBlocking();
        GC.SuppressFinalize(this);
    }

    /// <summary>
    /// Detach and cancel now, then wait for session release on the shared core worker.
    /// Never call on the worker thread (throws <see cref="CoreErrorKind.BlockingWaitOnWorker"/>).
    /// </summary>
    public void Close()
    {
        if (CoreRuntime.OnWorker)
        {
            throw CoreException.BlockingWaitOnWorker();
        }
        GC.SuppressFinalize(this);
        var error = _inner.BeginClose().Wait();
        if (error is not null)
        {
            throw error;
        }
    }

    /// <summary>
    /// Non-blocking close with completion callback: detaches and refuses new work immediately,
    /// then invokes <paramref name="done"/> once <c>md_session_release</c> has finished.
    /// </summary>
    public void CloseThen(Action<CoreException?> done)
    {
        GC.SuppressFinalize(this);
        _inner.BeginClose().Then(done);
    }

    public void Cancel() => _inner.Cancel();

    public ulong RecordUnavailable(CoreUnavailability change) => _inner.RecordUnavailable(change);

    public CoreAvailability Availability() => _inner.Availability();

    public ParseRequest Parse(
        byte[] source,
        byte[]? revisionToken,
        bool copyUnchanged,
        Action<CoreResult?, CoreException?> done) =>
        _inner.Parse(source, revisionToken, copyUnchanged, done);

    public CoreResult ParseBlocking(
        byte[] source,
        byte[]? revisionToken = null,
        bool copyUnchanged = false)
    {
        if (CoreRuntime.OnWorker)
        {
            throw CoreException.BlockingWaitOnWorker();
        }

        using var gate = new ManualResetEventSlim(false);
        CoreResult? capturedResult = null;
        CoreException? capturedError = null;

        _inner.Parse(source, revisionToken, copyUnchanged, (res, err) =>
        {
            capturedResult = res;
            capturedError = err;
            gate.Set();
        });

        gate.Wait();
        if (capturedError is not null)
        {
            throw capturedError;
        }
        return capturedResult ?? throw CoreException.SessionClosed();
    }

    public CoreResult ParseBlocking(string markdown, string? revisionToken = null, bool copyUnchanged = false) =>
        ParseBlocking(
            Encoding.UTF8.GetBytes(markdown),
            revisionToken is null ? null : Encoding.UTF8.GetBytes(revisionToken),
            copyUnchanged);
}

public sealed class ParseRequest
{
    private readonly Request _request;

    internal ParseRequest(Request request)
    {
        _request = request;
    }

    public void Cancel() => _request.Cancel();

    public bool IsRunning => _request.IsRunning;
}

internal sealed class Completion<T>
{
    private readonly object _lock = new();
    private bool _hasValue;
    private T? _value;
    private List<Action<T>>? _callbacks = [];

    public void Set(T value)
    {
        List<Action<T>>? callbacks;
        lock (_lock)
        {
            if (_hasValue)
            {
                return;
            }
            _hasValue = true;
            _value = value;
            callbacks = _callbacks;
            _callbacks = null;
            Monitor.PulseAll(_lock);
        }

        if (callbacks is not null)
        {
            foreach (var callback in callbacks)
            {
                try
                {
                    callback(value);
                }
                catch (Exception ex)
                {
                    CoreRuntime.ReportCallbackException(ex);
                }
            }
        }
    }

    public T Wait()
    {
        lock (_lock)
        {
            while (!_hasValue)
            {
                Monitor.Wait(_lock);
            }
            return _value!;
        }
    }

    public void Then(Action<T> callback)
    {
        T value;
        lock (_lock)
        {
            if (!_hasValue)
            {
                _callbacks!.Add(callback);
                return;
            }
            value = _value!;
        }
        try
        {
            callback(value);
        }
        catch (Exception ex)
        {
            CoreRuntime.ReportCallbackException(ex);
        }
    }
}

internal sealed unsafe class SessionInner
{
    private readonly object _lock = new();
    private readonly List<Request> _requests = [];
    private ulong _handle;
    private ulong _cancelEpoch;
    private Completion<CoreException?>? _closing;

    public HostLimits Limits { get; }

    public SessionInner(ulong handle, HostLimits limits)
    {
        _handle = handle;
        Limits = limits;
    }

    public bool IsClosed
    {
        get
        {
            lock (_lock)
            {
                return _handle == 0;
            }
        }
    }

    public Completion<CoreException?> BeginClose()
    {
        ulong old;
        Request[] toCancel;
        Completion<CoreException?> completion;
        lock (_lock)
        {
            if (_closing is not null)
            {
                return _closing;
            }
            completion = new Completion<CoreException?>();
            old = DetachLocked(out toCancel);
            _closing = completion;
        }

        foreach (var request in toCancel)
        {
            request.CancelIfQueued(CoreException.SessionClosed());
        }

        CoreRuntime.EnqueueWork(() =>
        {
            var status = CoreFfi.md_session_release(old);
            completion.Set(status == CoreFfi.MD_OK ? null : CoreException.FromStatus("md_session_release", status));
        });
        return completion;
    }

    public void DropNonBlocking()
    {
        ulong old;
        Request[] toCancel;
        Completion<CoreException?>? completion = null;
        lock (_lock)
        {
            if (_closing is not null)
            {
                return;
            }
            old = DetachLocked(out toCancel);
            if (old == 0)
            {
                return;
            }
            completion = new Completion<CoreException?>();
            _closing = completion;
        }

        foreach (var request in toCancel)
        {
            request.CancelIfQueued(CoreException.SessionClosed());
        }

        CoreRuntime.EnqueueWork(() =>
        {
            var status = CoreFfi.md_session_release(old);
            completion.Set(status == CoreFfi.MD_OK ? null : CoreException.FromStatus("md_session_release", status));
        });
    }

    private ulong DetachLocked(out Request[] toCancel)
    {
        var old = _handle;
        _handle = 0;
        _cancelEpoch++;
        toCancel = _requests.Count == 0 ? [] : [.. _requests];
        _requests.Clear();
        if (old != 0)
        {
            uint cancelled = 0;
            CoreFfi.md_session_cancel(old, &cancelled);
        }
        return old;
    }

    public void Cancel()
    {
        Request[] toCancel;
        uint status;
        lock (_lock)
        {
            if (_handle == 0)
            {
                throw CoreException.SessionClosed();
            }
            _cancelEpoch++;
            toCancel = _requests.Count == 0 ? [] : [.. _requests];
            _requests.Clear();
            uint cancelled = 0;
            status = CoreFfi.md_session_cancel(_handle, &cancelled);
        }

        foreach (var request in toCancel)
        {
            request.CancelIfQueued(CoreException.Cancelled(0));
        }

        if (status != CoreFfi.MD_OK)
        {
            throw CoreException.FromStatus("md_session_cancel", status);
        }
    }

    public ulong RecordUnavailable(CoreUnavailability change)
    {
        var (kind, reason) = change switch
        {
            CoreUnavailability.Deleted => (CoreFfi.MD_CHANGE_DELETED, CoreFfi.MD_REASON_NONE),
            CoreUnavailability.PermissionDenied => (CoreFfi.MD_CHANGE_UNREADABLE, CoreFfi.MD_REASON_PERMISSION_DENIED),
            CoreUnavailability.AccessRevoked => (CoreFfi.MD_CHANGE_UNREADABLE, CoreFfi.MD_REASON_ACCESS_REVOKED),
            CoreUnavailability.Busy => (CoreFfi.MD_CHANGE_UNREADABLE, CoreFfi.MD_REASON_BUSY),
            CoreUnavailability.Io => (CoreFfi.MD_CHANGE_UNREADABLE, CoreFfi.MD_REASON_IO),
            _ => throw CoreException.InvalidArgument($"unknown unavailability {change}"),
        };

        lock (_lock)
        {
            if (_handle == 0)
            {
                throw CoreException.SessionClosed();
            }
            uint disposition = 0;
            ulong generation = 0;
            var status = CoreFfi.md_session_external_change(
                _handle,
                kind,
                reason,
                null,
                0,
                &disposition,
                &generation);
            if (status != CoreFfi.MD_OK)
            {
                throw CoreException.FromStatus("md_session_external_change", status);
            }
            if (disposition != CoreFfi.MD_DISPOSITION_RECORDED)
            {
                throw CoreException.InvalidSnapshot($"unexpected external_change disposition {disposition}");
            }
            return generation;
        }
    }

    public CoreAvailability Availability()
    {
        lock (_lock)
        {
            if (_handle == 0)
            {
                throw CoreException.SessionClosed();
            }
            var info = new CoreFfi.md_session_status_info
            {
                struct_size = (uint)sizeof(CoreFfi.md_session_status_info),
            };
            var status = CoreFfi.md_session_get_status(_handle, &info);
            if (status != CoreFfi.MD_OK)
            {
                throw CoreException.FromStatus("md_session_get_status", status);
            }

            return (info.availability, info.unreadable_reason) switch
            {
                (CoreFfi.MD_AVAILABLE, _) => CoreAvailability.Available,
                (CoreFfi.MD_DELETED, _) => CoreAvailability.Unavailable(CoreUnavailability.Deleted),
                (CoreFfi.MD_UNREADABLE, CoreFfi.MD_REASON_PERMISSION_DENIED) =>
                    CoreAvailability.Unavailable(CoreUnavailability.PermissionDenied),
                (CoreFfi.MD_UNREADABLE, CoreFfi.MD_REASON_ACCESS_REVOKED) =>
                    CoreAvailability.Unavailable(CoreUnavailability.AccessRevoked),
                (CoreFfi.MD_UNREADABLE, CoreFfi.MD_REASON_BUSY) =>
                    CoreAvailability.Unavailable(CoreUnavailability.Busy),
                (CoreFfi.MD_UNREADABLE, CoreFfi.MD_REASON_IO) =>
                    CoreAvailability.Unavailable(CoreUnavailability.Io),
                _ => throw CoreException.InvalidSnapshot(
                    $"unexpected availability ({info.availability}, {info.unreadable_reason})"),
            };
        }
    }

    public ParseRequest Parse(
        byte[] source,
        byte[]? revisionToken,
        bool copyUnchanged,
        Action<CoreResult?, CoreException?> done)
    {
        ArgumentNullException.ThrowIfNull(source);
        ArgumentNullException.ThrowIfNull(done);

        lock (_lock)
        {
            if (_handle == 0)
            {
                throw CoreException.SessionClosed();
            }
        }

        if (!CoreRuntime.TryAcquireSlot())
        {
            throw CoreException.Busy();
        }

        var tokenCopy = revisionToken is null ? null : (byte[])revisionToken.Clone();
        Request request;
        lock (_lock)
        {
            if (_handle == 0)
            {
                CoreRuntime.ReleaseSlot();
                throw CoreException.SessionClosed();
            }
            request = new Request(source, tokenCopy, done, _cancelEpoch);
            _requests.RemoveAll(static r => r.IsFinished);
            _requests.Add(request);
        }

        CoreRuntime.EnqueueWork(() =>
        {
            if (!request.TryBegin(out var reqSource, out var reqToken))
            {
                return;
            }

            CoreResult? res = null;
            CoreException? err = null;
            try
            {
                res = RunParse(reqSource!, reqToken, copyUnchanged, request);
            }
            catch (CoreException ex)
            {
                err = ex;
            }
            catch (Exception ex)
            {
                err = new CoreException(CoreErrorKind.Status, $"unexpected worker exception: {ex.Message}");
            }
            request.Complete(res, err);
        });

        return new ParseRequest(request);
    }

    private CoreResult RunParse(
        byte[] source,
        byte[]? token,
        bool copyUnchanged,
        Request request)
    {
        request.ThrowIfCancelled();

        ulong job = 0;
        ulong generation = 0;
        lock (_lock)
        {
            if (_handle == 0)
            {
                throw CoreException.SessionClosed();
            }
            if (request.SessionCancelEpoch != _cancelEpoch)
            {
                throw CoreException.Cancelled(0);
            }

            uint submitStatus;
            fixed (byte* srcPtr = source)
            fixed (byte* tokPtr = token)
            {
                // Empty byte[] pins to null in C#; pass a non-null sentinel pointer for 0-length source.
                byte dummy = 0;
                byte* nonNullSrc = srcPtr is not null ? srcPtr : &dummy;
                submitStatus = CoreFfi.md_session_submit(
                    _handle,
                    nonNullSrc,
                    (nuint)source.Length,
                    tokPtr,
                    token is null ? 0 : (nuint)token.Length,
                    &job,
                    &generation);
            }

            if (submitStatus == CoreFfi.MD_ERR_INVALID_REVISION_TOKEN ||
                submitStatus == CoreFfi.MD_ERR_INVALID_ARGUMENT)
            {
                throw CoreException.InvalidArgument(
                    $"md_session_submit rejected input ({CoreRuntime.StatusName(submitStatus)})");
            }
            if (submitStatus != CoreFfi.MD_OK)
            {
                throw CoreException.FromStatus("md_session_submit", submitStatus);
            }
        }

        request.InstallJob(job);
        ulong snapshot = 0;
        try
        {
            var jobResult = new CoreFfi.md_job_result
            {
                struct_size = (uint)sizeof(CoreFfi.md_job_result),
            };

            var t0 = Stopwatch.GetTimestamp();
            var runStatus = CoreFfi.md_job_run(job, &jobResult);
            var tParsed = Stopwatch.GetTimestamp();

            snapshot = jobResult.snapshot;
            if (runStatus != CoreFfi.MD_OK)
            {
                throw CoreException.FromStatus("md_job_run", runStatus);
            }

            request.ThrowIfCancelled();
            if (IsClosed)
            {
                throw CoreException.SessionClosed();
            }

            if (jobResult.outcome is < 1 or > 6)
            {
                throw CoreException.InvalidSnapshot($"unknown job outcome {jobResult.outcome}");
            }
            var outcome = (CoreOutcome)jobResult.outcome;
            CoreParseFailure? failure = jobResult.failure switch
            {
                0 => null,
                >= 1 and <= 9 => (CoreParseFailure)jobResult.failure,
                _ => throw CoreException.InvalidSnapshot($"unknown parse failure {jobResult.failure}"),
            };

            if (outcome == CoreOutcome.Failed && failure is null)
            {
                throw CoreException.InvalidSnapshot("Failed outcome missing failure kind");
            }

            var tEncoded = tParsed;
            var tCopied = tParsed;
            CoreDocument? document = null;

            if (outcome == CoreOutcome.Published || (outcome == CoreOutcome.Unchanged && copyUnchanged))
            {
                if (snapshot == 0)
                {
                    throw CoreException.InvalidSnapshot("Published outcome returned null snapshot");
                }
                var view = new CoreFfi.md_snapshot_view
                {
                    struct_size = (uint)sizeof(CoreFfi.md_snapshot_view),
                };
                var viewStatus = CoreFfi.md_snapshot_get_view(snapshot, &view);
                if (viewStatus != CoreFfi.MD_OK)
                {
                    throw CoreException.FromStatus("md_snapshot_get_view", viewStatus);
                }
                tEncoded = Stopwatch.GetTimestamp();

                document = CoreDocument.CopyFromView(in view, Limits.MaxSnapshotCopyBytes);
                tCopied = Stopwatch.GetTimestamp();
            }

            if (snapshot != 0)
            {
                var relSnap = snapshot;
                snapshot = 0;
                CoreFfi.md_snapshot_release(relSnap);
            }

            request.ThrowIfCancelled();
            if (IsClosed)
            {
                throw CoreException.SessionClosed();
            }

            return new CoreResult(
                Outcome: outcome,
                Generation: jobResult.generation,
                Latest: jobResult.latest,
                Failure: failure,
                Document: document,
                Timings: new CoreTimings(
                    Parse: Stopwatch.GetElapsedTime(t0, tParsed),
                    Encode: Stopwatch.GetElapsedTime(tParsed, tEncoded),
                    Copy: Stopwatch.GetElapsedTime(tEncoded, tCopied)));
        }
        finally
        {
            if (snapshot != 0)
            {
                CoreFfi.md_snapshot_release(snapshot);
            }
            request.ReleaseJob();
        }
    }
}

internal enum RequestPhase
{
    Queued,
    Running,
    Finished,
}

internal sealed unsafe class Request
{
    private readonly object _lock = new();
    private RequestPhase _phase = RequestPhase.Queued;
    private byte[]? _source;
    private byte[]? _token;
    private Action<CoreResult?, CoreException?>? _done;
    private bool _cancelled;
    private ulong _job;

    public ulong SessionCancelEpoch { get; }

    public Request(byte[] source, byte[]? token, Action<CoreResult?, CoreException?> done, ulong sessionCancelEpoch)
    {
        _source = source;
        _token = token;
        _done = done;
        SessionCancelEpoch = sessionCancelEpoch;
    }

    public bool IsRunning
    {
        get
        {
            lock (_lock)
            {
                return _phase == RequestPhase.Running;
            }
        }
    }

    public bool IsFinished
    {
        get
        {
            lock (_lock)
            {
                return _phase == RequestPhase.Finished;
            }
        }
    }

    public bool TryBegin(out byte[]? source, out byte[]? token)
    {
        lock (_lock)
        {
            if (_phase != RequestPhase.Queued)
            {
                source = null;
                token = null;
                return false;
            }
            _phase = RequestPhase.Running;
            source = _source;
            token = _token;
            _source = null;
            _token = null;
            return source is not null;
        }
    }

    public void ThrowIfCancelled()
    {
        lock (_lock)
        {
            if (_cancelled)
            {
                throw CoreException.Cancelled(0);
            }
        }
    }

    public void InstallJob(ulong job)
    {
        lock (_lock)
        {
            _job = job;
            if (_cancelled && job != 0)
            {
                CoreFfi.md_job_cancel(job);
            }
        }
    }

    public void ReleaseJob()
    {
        lock (_lock)
        {
            if (_job != 0)
            {
                CoreFfi.md_job_release(_job);
                _job = 0;
            }
        }
    }

    public void CancelIfQueued(CoreException reason)
    {
        Action<CoreResult?, CoreException?>? done = null;
        lock (_lock)
        {
            if (_phase != RequestPhase.Queued)
            {
                return;
            }
            _cancelled = true;
            _phase = RequestPhase.Finished;
            _source = null;
            _token = null;
            CoreRuntime.ReleaseSlot();
            done = _done;
            _done = null;
        }

        if (done is not null)
        {
            try
            {
                done(null, reason);
            }
            catch (Exception ex)
            {
                CoreRuntime.ReportCallbackException(ex);
            }
        }
    }

    public void Cancel()
    {
        Action<CoreResult?, CoreException?>? done = null;
        lock (_lock)
        {
            _cancelled = true;
            switch (_phase)
            {
                case RequestPhase.Queued:
                    _phase = RequestPhase.Finished;
                    _source = null;
                    _token = null;
                    CoreRuntime.ReleaseSlot();
                    done = _done;
                    _done = null;
                    break;
                case RequestPhase.Running:
                    if (_job != 0)
                    {
                        CoreFfi.md_job_cancel(_job);
                    }
                    break;
                case RequestPhase.Finished:
                    break;
            }
        }

        if (done is not null)
        {
            try
            {
                done(null, CoreException.Cancelled(0));
            }
            catch (Exception ex)
            {
                CoreRuntime.ReportCallbackException(ex);
            }
        }
    }

    public void Complete(CoreResult? result, CoreException? error)
    {
        Action<CoreResult?, CoreException?>? done;
        CoreResult? finalResult;
        CoreException? finalError;

        lock (_lock)
        {
            _phase = RequestPhase.Finished;
            CoreRuntime.ReleaseSlot();
            if (_cancelled)
            {
                finalResult = null;
                finalError = CoreException.Cancelled(result?.Generation ?? 0);
            }
            else
            {
                finalResult = result;
                finalError = error;
            }
            done = _done;
            _done = null;
        }

        if (done is not null)
        {
            try
            {
                done(finalResult, finalError);
            }
            catch (Exception ex)
            {
                CoreRuntime.ReportCallbackException(ex);
            }
        }
    }
}
