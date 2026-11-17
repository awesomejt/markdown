using System.Diagnostics;
using System.IO;

namespace Markdown.Windows.Document;

/// <summary>
/// Watches an open workspace folder for filesystem changes and access loss.
/// Uses a recursive root FileSystemWatcher (64 KiB buffer) plus a parent watcher
/// for root deletion/rename, coalesces change storms with debounce and max-coalesce
/// bounds, and gates refreshes against in-flight enumerations.
/// </summary>
public sealed class WorkspaceWatcher : IDisposable
{
    public const int DebounceMilliseconds = 150;
    public const int MaxCoalesceMilliseconds = 1000;
    public const int BufferSizeBytes = 65536;

    private readonly object _lock = new();
    private readonly FolderCapability _folder;
    private readonly string _rootPath;
    private readonly string _parentDir;
    private readonly string _rootName;
    private readonly Action _onChanged;
    private readonly Action<FolderError> _onAccessLost;
    private readonly int _debounceMs;
    private readonly int _maxCoalesceMs;

    private FileSystemWatcher? _rootWatcher;
    private FileSystemWatcher? _parentWatcher;
    private Timer? _debounceTimer;
    private bool _disposed;
    private bool _accessLostReported;
    private bool _pendingChange;
    private bool _refreshInFlight;
    private bool _refreshQueuedWhileInFlight;
    private long _burstStartTimestamp;

    public bool IsArmed
    {
        get
        {
            lock (_lock)
            {
                return _rootWatcher is not null && _rootWatcher.EnableRaisingEvents;
            }
        }
    }

    public WorkspaceWatcher(
        FolderCapability folder,
        Action onChanged,
        Action<FolderError> onAccessLost,
        int debounceMs = DebounceMilliseconds,
        int maxCoalesceMs = MaxCoalesceMilliseconds)
    {
        ArgumentNullException.ThrowIfNull(folder);
        ArgumentNullException.ThrowIfNull(onChanged);
        ArgumentNullException.ThrowIfNull(onAccessLost);

        _folder = folder;
        _rootPath = folder.CanonicalRoot.TrimEnd('\\', '/');
        _parentDir = Path.GetDirectoryName(_rootPath) ?? string.Empty;
        _rootName = Path.GetFileName(_rootPath);
        _onChanged = onChanged;
        _onAccessLost = onAccessLost;
        _debounceMs = Math.Max(10, debounceMs);
        _maxCoalesceMs = Math.Max(_debounceMs, maxCoalesceMs);

        Arm();
    }

    public void Arm()
    {
        lock (_lock)
        {
            if (_disposed || _accessLostReported)
            {
                return;
            }

            DisarmLocked();

            if (!Directory.Exists(_rootPath))
            {
                return;
            }

            try
            {
                var rootWatcher = new FileSystemWatcher(_rootPath)
                {
                    Filter = "*.*",
                    IncludeSubdirectories = true,
                    InternalBufferSize = BufferSizeBytes,
                    NotifyFilter = NotifyFilters.FileName |
                                   NotifyFilters.DirectoryName |
                                   NotifyFilters.LastWrite |
                                   NotifyFilters.Size |
                                   NotifyFilters.CreationTime,
                };
                rootWatcher.Changed += OnRootEvent;
                rootWatcher.Created += OnRootEvent;
                rootWatcher.Deleted += OnRootEvent;
                rootWatcher.Renamed += OnRootRenamed;
                rootWatcher.Error += OnRootError;
                rootWatcher.EnableRaisingEvents = true;
                _rootWatcher = rootWatcher;
            }
            catch
            {
                DisarmLocked();
                return;
            }

            if (!string.IsNullOrEmpty(_parentDir) &&
                !string.IsNullOrEmpty(_rootName) &&
                Directory.Exists(_parentDir))
            {
                try
                {
                    var parentWatcher = new FileSystemWatcher(_parentDir)
                    {
                        Filter = _rootName,
                        IncludeSubdirectories = false,
                        NotifyFilter = NotifyFilters.DirectoryName |
                                       NotifyFilters.FileName |
                                       NotifyFilters.Attributes,
                    };
                    parentWatcher.Deleted += OnParentRootEvent;
                    parentWatcher.Renamed += OnParentRootRenamed;
                    parentWatcher.Error += OnParentError;
                    parentWatcher.EnableRaisingEvents = true;
                    _parentWatcher = parentWatcher;
                }
                catch
                {
                    // Parent directory may be inaccessible (e.g. drive root or restricted parent);
                    // root watcher remains active.
                }
            }
        }
    }

    /// <summary>
    /// Marks the end of an in-flight workspace enumeration/refresh.
    /// If additional filesystem events arrived while refreshing, schedules a single follow-up refresh.
    /// </summary>
    public void NotifyRefreshCompleted()
    {
        lock (_lock)
        {
            _refreshInFlight = false;
            if (_disposed || _accessLostReported || !_refreshQueuedWhileInFlight)
            {
                _refreshQueuedWhileInFlight = false;
                return;
            }

            _refreshQueuedWhileInFlight = false;
            _pendingChange = true;
            _burstStartTimestamp = 0;
            EnsureTimerLocked(_debounceMs);
        }
    }

    public void ScheduleChange()
    {
        lock (_lock)
        {
            if (_disposed || _accessLostReported)
            {
                return;
            }

            _pendingChange = true;
            if (_burstStartTimestamp == 0)
            {
                _burstStartTimestamp = Stopwatch.GetTimestamp();
            }

            var elapsedMs = (int)Stopwatch.GetElapsedTime(_burstStartTimestamp).TotalMilliseconds;
            var remainingCoalesce = Math.Max(0, _maxCoalesceMs - elapsedMs);
            var dueMs = remainingCoalesce == 0 ? 0 : Math.Min(_debounceMs, remainingCoalesce);

            EnsureTimerLocked(dueMs);
        }
    }

    private void EnsureTimerLocked(int dueMs)
    {
        if (_debounceTimer is null)
        {
            _debounceTimer = new Timer(OnTimerFired, null, dueMs, Timeout.Infinite);
        }
        else
        {
            _debounceTimer.Change(dueMs, Timeout.Infinite);
        }
    }

    private void OnTimerFired(object? state)
    {
        lock (_lock)
        {
            if (_disposed || _accessLostReported || !_pendingChange)
            {
                return;
            }

            _pendingChange = false;
            _burstStartTimestamp = 0;

            if (_refreshInFlight)
            {
                _refreshQueuedWhileInFlight = true;
                return;
            }

            _refreshInFlight = true;
        }

        if (!_folder.ValidateAccess(out var folderError))
        {
            ReportAccessLost(folderError);
            return;
        }

        _onChanged();
    }

    private void OnRootEvent(object sender, FileSystemEventArgs e)
    {
        ScheduleChange();
    }

    private void OnRootRenamed(object sender, RenamedEventArgs e)
    {
        ScheduleChange();
    }

    private void OnRootError(object sender, ErrorEventArgs e)
    {
        _ = ThreadPool.QueueUserWorkItem(_ => HandleWatcherError());
    }

    private void OnParentRootEvent(object sender, FileSystemEventArgs e)
    {
        _ = ThreadPool.QueueUserWorkItem(_ => HandleWatcherError());
    }

    private void OnParentRootRenamed(object sender, RenamedEventArgs e)
    {
        if (string.Equals(e.OldName, _rootName, StringComparison.OrdinalIgnoreCase) ||
            string.Equals(e.Name, _rootName, StringComparison.OrdinalIgnoreCase))
        {
            _ = ThreadPool.QueueUserWorkItem(_ => HandleWatcherError());
        }
    }

    private void OnParentError(object sender, ErrorEventArgs e)
    {
        _ = ThreadPool.QueueUserWorkItem(_ => HandleWatcherError());
    }

    private void HandleWatcherError()
    {
        lock (_lock)
        {
            if (_disposed || _accessLostReported)
            {
                return;
            }
        }

        if (!_folder.ValidateAccess(out var folderError))
        {
            ReportAccessLost(folderError);
            return;
        }

        // Transient error or buffer overflow during a change storm: re-arm and refresh.
        Arm();
        ScheduleChange();
    }

    private void ReportAccessLost(FolderError error)
    {
        lock (_lock)
        {
            if (_disposed || _accessLostReported)
            {
                return;
            }
            _accessLostReported = true;
            _refreshInFlight = false;
            _refreshQueuedWhileInFlight = false;
            _pendingChange = false;
            DisarmLocked();
        }

        _onAccessLost(error);
    }

    private void DisarmLocked()
    {
        DisposeWatcher(ref _rootWatcher, OnRootEvent, OnRootRenamed, OnRootError);
        DisposeWatcher(ref _parentWatcher, OnParentRootEvent, OnParentRootRenamed, OnParentError);
    }

    private static void DisposeWatcher(
        ref FileSystemWatcher? watcher,
        FileSystemEventHandler onEvent,
        RenamedEventHandler onRenamed,
        ErrorEventHandler onError)
    {
        if (watcher is null)
        {
            return;
        }

        try
        {
            watcher.EnableRaisingEvents = false;
            watcher.Changed -= onEvent;
            watcher.Created -= onEvent;
            watcher.Deleted -= onEvent;
            watcher.Renamed -= onRenamed;
            watcher.Error -= onError;
            watcher.Dispose();
        }
        catch
        {
            // Ignore disposal errors
        }

        watcher = null;
    }

    public void Dispose()
    {
        Timer? timerToDispose;
        lock (_lock)
        {
            if (_disposed)
            {
                return;
            }

            _disposed = true;
            _pendingChange = false;
            _refreshInFlight = false;
            _refreshQueuedWhileInFlight = false;
            timerToDispose = _debounceTimer;
            _debounceTimer = null;
            DisarmLocked();
        }

        timerToDispose?.Dispose();
    }
}
