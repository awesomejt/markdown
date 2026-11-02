using System.Diagnostics;
using Markdown.Windows.Core;

namespace Markdown.Windows.Document;

public sealed class Cancellation
{
    private readonly object _lock = new();
    private bool _cancelled;
    private bool _completed;
    private ParseRequest? _parseRequest;

    public bool IsCancelled
    {
        get
        {
            lock (_lock)
            {
                return _cancelled;
            }
        }
    }

    public void Cancel()
    {
        ParseRequest? request;
        lock (_lock)
        {
            _cancelled = true;
            request = _parseRequest;
            _parseRequest = null;
            Monitor.PulseAll(_lock);
        }
        request?.Cancel();
    }

    public void Install(ParseRequest request)
    {
        ArgumentNullException.ThrowIfNull(request);
        var cancelNow = false;
        lock (_lock)
        {
            if (_completed)
            {
                return;
            }
            if (_cancelled)
            {
                cancelNow = true;
            }
            else
            {
                _parseRequest = request;
            }
        }

        if (cancelNow)
        {
            request.Cancel();
        }
    }

    public void MarkCompleted()
    {
        lock (_lock)
        {
            _completed = true;
            _parseRequest = null;
            Monitor.PulseAll(_lock);
        }
    }

    public bool WaitCancelled(TimeSpan timeout)
    {
        lock (_lock)
        {
            if (_cancelled)
            {
                return true;
            }
            _ = Monitor.Wait(_lock, timeout);
            return _cancelled;
        }
    }
}

internal static class FileWorkDispatcher
{
    private sealed record QueuedJob(ulong OwnerId, Action Work);

    private static readonly object s_lock = new();
    private static readonly LinkedList<QueuedJob> s_queue = new();
    private static Thread? s_thread;

    public static void Enqueue(ulong ownerId, Action work)
    {
        lock (s_lock)
        {
            EnsureThreadStartedLocked();
            RemoveLocked(ownerId);
            s_queue.AddLast(new QueuedJob(ownerId, work));
            Monitor.Pulse(s_lock);
        }
    }

    public static int Remove(ulong ownerId)
    {
        lock (s_lock)
        {
            return RemoveLocked(ownerId);
        }
    }

    private static int RemoveLocked(ulong ownerId)
    {
        var removed = 0;
        var node = s_queue.First;
        while (node is not null)
        {
            var next = node.Next;
            if (node.Value.OwnerId == ownerId)
            {
                s_queue.Remove(node);
                removed++;
            }
            node = next;
        }
        return removed;
    }

    private static void EnsureThreadStartedLocked()
    {
        if (s_thread is not null)
        {
            return;
        }
        var thread = new Thread(RunLoop)
        {
            Name = "markdown-files",
            IsBackground = true,
        };
        s_thread = thread;
        thread.Start();
    }

    private static void RunLoop()
    {
        while (true)
        {
            QueuedJob job;
            lock (s_lock)
            {
                while (s_queue.First is null)
                {
                    Monitor.Wait(s_lock);
                }
                job = s_queue.First.Value;
                s_queue.RemoveFirst();
            }

            try
            {
                job.Work();
            }
            catch (Exception ex)
            {
                CoreRuntime.ReportCallbackException(ex);
            }
        }
    }
}

public sealed class DocumentLoader : IDisposable
{
    private static ulong s_nextOwnerId;
    private static ulong s_nextDocumentSeq;

    private sealed record CompletedLoad(
        ulong Serial,
        CoreDocument? Document,
        DocumentError? Error,
        uint VolumeSerial,
        string? VolumeRoot);

    private readonly object _completedLock = new();
    private readonly Queue<CompletedLoad> _completed = new();

    private CoreSession? _session;
    private Cancellation? _active;
    private bool _closed;
    private bool _hadSuccessfulRead;
    private uint _lastVolumeSerial;
    private string? _lastVolumeRoot;

    private ulong _discardedResults;

    public ulong OwnerId { get; } = Interlocked.Increment(ref s_nextOwnerId);
    public HostLimits Limits { get; }
    public TimeSpan BusyRetryTimeout { get; set; } = TimeSpan.FromSeconds(30);

    public DocumentCapability? Capability { get; private set; }
    public CoreDocument? Document { get; private set; }
    public DocumentError? Error { get; private set; }
    public bool IsLoading { get; private set; }
    public ulong Serial { get; private set; }
    public ulong DiscardedResults => Volatile.Read(ref _discardedResults);

    public bool HasFile => Capability is not null;

    public string WindowTitle =>
        Capability is null ? "Markdown" : $"{Capability.DisplayName} — Markdown";

    public Action? OnReady { get; set; }
    public Action? ReadBarrier { get; set; }
    public Func<string, HostLimits, CoreSession>? SessionFactory { get; set; }
    public Func<string?, uint, bool>? VolumeGoneCheck { get; set; }

    public DocumentLoader()
        : this(HostLimits.Default)
    {
    }

    public DocumentLoader(HostLimits limits)
    {
        Limits = limits;
    }

    public CoreAvailability? Availability() => _session?.Availability();

    public void Open(DocumentCapability capability)
    {
        ArgumentNullException.ThrowIfNull(capability);
        if (_closed)
        {
            return;
        }

        var docSeq = Interlocked.Increment(ref s_nextDocumentSeq);
        var documentId = $"win-{OwnerId}-doc-{docSeq}";
        CoreSession newSession;
        try
        {
            var factory = SessionFactory;
            newSession = factory is not null
                ? factory(documentId, Limits)
                : new CoreSession(documentId, Limits);
        }
        catch (CoreException ex)
        {
            if (_session is not null)
            {
                // Preserve the active selection, session, and published document when replacement session creation fails.
                Error = DocumentErrorExtensions.FromCoreException(ex);
                IsLoading = false;
                return;
            }

            // First open in an empty window failed session creation: keep the capability so Reload can retry Open.
            CancelCurrent();
            if (Capability is not null && !ReferenceEquals(Capability, capability))
            {
                Capability.Revoke();
            }
            Capability = capability;
            Document = null;
            Error = DocumentErrorExtensions.FromCoreException(ex);
            IsLoading = false;
            _hadSuccessfulRead = false;
            _lastVolumeSerial = 0;
            _lastVolumeRoot = null;
            return;
        }

        CancelCurrent();

        var oldSession = _session;
        _session = newSession;
        oldSession?.Dispose();

        if (Capability is not null && !ReferenceEquals(Capability, capability))
        {
            Capability.Revoke();
        }

        Capability = capability;
        Document = null;
        Error = null;
        IsLoading = true;
        _hadSuccessfulRead = false;
        _lastVolumeSerial = 0;
        _lastVolumeRoot = null;

        StartLoad(Serial, capability, newSession);
    }

    public void Reload()
    {
        if (_closed || Capability is null)
        {
            return;
        }

        if (_session is null)
        {
            Open(Capability);
            return;
        }

        CancelCurrent();
        Error = null;
        IsLoading = true;

        StartLoad(Serial, Capability, _session);
    }

    public void RejectSelection(DocumentError error)
    {
        if (_closed)
        {
            return;
        }

        CancelCurrent();
        IsLoading = false;
        Error = error;
    }

    public bool Poll()
    {
        var changed = false;
        while (true)
        {
            CompletedLoad? item;
            lock (_completedLock)
            {
                if (!_completed.TryDequeue(out item))
                {
                    break;
                }
            }

            if (_closed || item.Serial != Serial)
            {
                RecordDiscarded();
                continue;
            }

            _active = null;
            IsLoading = false;
            changed = true;

            if (item.Document is not null)
            {
                Document = item.Document;
                Error = null;
                _hadSuccessfulRead = true;
                _lastVolumeSerial = item.VolumeSerial;
                _lastVolumeRoot = item.VolumeRoot;
                continue;
            }

            var rawError = item.Error ?? DocumentError.ReadFailed;
            var effectiveError = PromoteContextualError(rawError);
            Error = effectiveError;

            if (effectiveError.UnavailableReason() is { } reason && _session is not null)
            {
                try
                {
                    _ = _session.RecordUnavailable(reason);
                }
                catch (CoreException)
                {
                    // Session was concurrently closed.
                }
            }
        }
        return changed;
    }

    public void Close()
    {
        if (_closed)
        {
            return;
        }
        _closed = true;
        CancelCurrent();

        Capability?.Revoke();
        Capability = null;
        Document = null;
        Error = null;
        IsLoading = false;

        var oldSession = _session;
        _session = null;
        oldSession?.Dispose();
    }

    public void CloseAndWait()
    {
        var session = _session;
        _session = null;
        Close();
        session?.Close();
    }

    public void Dispose() => Close();

    private DocumentError PromoteContextualError(DocumentError error)
    {
        if (!_hadSuccessfulRead)
        {
            return error;
        }

        if (error == DocumentError.PermissionDenied)
        {
            return DocumentError.AccessRevoked;
        }

        if (error == DocumentError.NotFound)
        {
            var checkVolume = VolumeGoneCheck ?? DocumentReader.IsVolumeGone;
            if (checkVolume(_lastVolumeRoot, _lastVolumeSerial))
            {
                return DocumentError.VolumeUnavailable;
            }
        }

        return error;
    }

    private void CancelCurrent()
    {
        Serial++;
        var oldActive = _active;
        _active = null;
        oldActive?.Cancel();

        var removedFromDispatcher = FileWorkDispatcher.Remove(OwnerId);
        if (removedFromDispatcher > 0)
        {
            _ = Interlocked.Add(ref _discardedResults, (ulong)removedFromDispatcher);
        }

        lock (_completedLock)
        {
            while (_completed.Count > 0)
            {
                _ = _completed.Dequeue();
                _ = Interlocked.Increment(ref _discardedResults);
            }
        }
    }

    private void StartLoad(ulong serial, DocumentCapability capability, CoreSession session)
    {
        var cancellation = new Cancellation();
        _active = cancellation;

        var maxBytes = Limits.MaxSourceBytes > 0
            ? Limits.MaxSourceBytes
            : HostLimits.Default.MaxSourceBytes;
        var barrier = ReadBarrier;
        var retryTimeout = BusyRetryTimeout;

        FileWorkDispatcher.Enqueue(OwnerId, () =>
        {
            if (cancellation.IsCancelled)
            {
                RecordDiscarded();
                return;
            }

            var readResult = DocumentReader.Read(capability, maxBytes, cancellation, barrier);
            if (cancellation.IsCancelled)
            {
                RecordDiscarded();
                return;
            }

            if (!readResult.IsOk)
            {
                var err = readResult.Error ?? DocumentError.ReadFailed;
                if (err == DocumentError.Cancelled)
                {
                    RecordDiscarded();
                    return;
                }
                PushCompleted(new CompletedLoad(serial, null, err, 0, null));
                return;
            }

            var readDoc = readResult.Document!;
            var startTick = Stopwatch.GetTimestamp();

            while (!cancellation.IsCancelled)
            {
                try
                {
                    var request = session.Parse(
                        readDoc.Bytes,
                        readDoc.RevisionToken,
                        copyUnchanged: true,
                        (coreResult, coreError) =>
                        {
                            cancellation.MarkCompleted();
                            if (cancellation.IsCancelled)
                            {
                                RecordDiscarded();
                                return;
                            }

                            if (coreError is not null)
                            {
                                var mapped = DocumentErrorExtensions.FromCoreException(coreError);
                                if (mapped == DocumentError.Cancelled)
                                {
                                    RecordDiscarded();
                                    return;
                                }
                                PushCompleted(new CompletedLoad(
                                    serial,
                                    null,
                                    mapped,
                                    readDoc.VolumeSerial,
                                    readDoc.VolumeRoot));
                                return;
                            }

                            if (coreResult is null)
                            {
                                PushCompleted(new CompletedLoad(
                                    serial,
                                    null,
                                    DocumentError.ParseFailed,
                                    readDoc.VolumeSerial,
                                    readDoc.VolumeRoot));
                                return;
                            }

                            switch (coreResult.Outcome)
                            {
                                case CoreOutcome.Published:
                                case CoreOutcome.Unchanged:
                                    if (coreResult.Document is not null)
                                    {
                                        PushCompleted(new CompletedLoad(
                                            serial,
                                            coreResult.Document,
                                            null,
                                            readDoc.VolumeSerial,
                                            readDoc.VolumeRoot));
                                    }
                                    else
                                    {
                                        PushCompleted(new CompletedLoad(
                                            serial,
                                            null,
                                            DocumentError.ParseFailed,
                                            readDoc.VolumeSerial,
                                            readDoc.VolumeRoot));
                                    }
                                    break;

                                case CoreOutcome.Failed:
                                    var failureError = coreResult.Failure is { } failure
                                        ? DocumentErrorExtensions.FromParseFailure(failure)
                                        : DocumentError.ParseFailed;
                                    PushCompleted(new CompletedLoad(
                                        serial,
                                        null,
                                        failureError,
                                        readDoc.VolumeSerial,
                                        readDoc.VolumeRoot));
                                    break;

                                case CoreOutcome.Cancelled:
                                case CoreOutcome.Superseded:
                                case CoreOutcome.Closed:
                                    RecordDiscarded();
                                    break;
                            }
                        });

                    cancellation.Install(request);
                    return;
                }
                catch (CoreException ex) when (ex.Kind == CoreErrorKind.Busy)
                {
                    if (Stopwatch.GetElapsedTime(startTick) >= retryTimeout)
                    {
                        if (!cancellation.IsCancelled)
                        {
                            PushCompleted(new CompletedLoad(
                                serial,
                                null,
                                DocumentError.Busy,
                                readDoc.VolumeSerial,
                                readDoc.VolumeRoot));
                        }
                        else
                        {
                            RecordDiscarded();
                        }
                        return;
                    }

                    if (cancellation.WaitCancelled(TimeSpan.FromMilliseconds(15)))
                    {
                        RecordDiscarded();
                        return;
                    }
                }
                catch (CoreException ex)
                {
                    var mapped = DocumentErrorExtensions.FromCoreException(ex);
                    if (mapped != DocumentError.Cancelled && !cancellation.IsCancelled)
                    {
                        PushCompleted(new CompletedLoad(
                            serial,
                            null,
                            mapped,
                            readDoc.VolumeSerial,
                            readDoc.VolumeRoot));
                    }
                    else
                    {
                        RecordDiscarded();
                    }
                    return;
                }
            }

            RecordDiscarded();
        });
    }

    private void RecordDiscarded() =>
        _ = Interlocked.Increment(ref _discardedResults);

    private void PushCompleted(CompletedLoad item)
    {
        if (_closed || item.Serial != Serial)
        {
            RecordDiscarded();
            return;
        }

        lock (_completedLock)
        {
            _completed.Enqueue(item);
        }

        try
        {
            OnReady?.Invoke();
        }
        catch (Exception ex)
        {
            CoreRuntime.ReportCallbackException(ex);
        }
    }
}
