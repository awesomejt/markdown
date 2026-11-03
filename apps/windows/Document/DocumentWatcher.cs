using System.IO;

namespace Markdown.Windows.Document;

/// <summary>
/// Watches one document for external changes via Windows FileSystemWatcher.
/// Reports writes, replacements, renames, deletions, and reappearances.
/// Bursts are coalesced with a debounce delay.
/// Callbacks arrive on the thread-pool or caller queue; does not keep any file handle open.
/// </summary>
public sealed class DocumentWatcher : IDisposable
{
    public const int DelayMilliseconds = 100;

    private readonly object _lock = new();
    private readonly string _fullPath;
    private readonly string _directory;
    private readonly string _fileName;
    private readonly Action _callback;

    private FileSystemWatcher? _watcher;
    private Timer? _debounceTimer;
    private bool _disposed;
    private bool _scheduled;

    public string FilePath => _fullPath;

    public bool IsArmed
    {
        get
        {
            lock (_lock)
            {
                return _watcher is not null && _watcher.EnableRaisingEvents;
            }
        }
    }

    public DocumentWatcher(DocumentCapability capability, Action callback)
        : this(capability?.FullPath ?? throw new ArgumentNullException(nameof(capability)), callback)
    {
    }

    public DocumentWatcher(string fullPath, Action callback)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(fullPath);
        ArgumentNullException.ThrowIfNull(callback);

        _fullPath = Path.GetFullPath(fullPath);
        _directory = Path.GetDirectoryName(_fullPath) ?? string.Empty;
        _fileName = Path.GetFileName(_fullPath);
        _callback = callback;

        Arm();
    }

    public void Arm()
    {
        lock (_lock)
        {
            if (_disposed)
            {
                return;
            }

            DisarmLocked();

            if (string.IsNullOrEmpty(_directory) || !Directory.Exists(_directory))
            {
                return;
            }

            try
            {
                var watcher = new FileSystemWatcher(_directory)
                {
                    Filter = _fileName,
                    NotifyFilter = NotifyFilters.FileName |
                                   NotifyFilters.LastWrite |
                                   NotifyFilters.Size |
                                   NotifyFilters.CreationTime,
                    IncludeSubdirectories = false,
                };

                watcher.Changed += OnFileSystemEvent;
                watcher.Created += OnFileSystemEvent;
                watcher.Deleted += OnFileSystemEvent;
                watcher.Renamed += OnFileSystemRenamed;
                watcher.Error += OnFileSystemError;

                watcher.EnableRaisingEvents = true;
                _watcher = watcher;
            }
            catch
            {
                // Directory might be inaccessible or non-watchable; keep disarmed
            }
        }
    }

    private void DisarmLocked()
    {
        if (_watcher is not null)
        {
            try
            {
                _watcher.EnableRaisingEvents = false;
                _watcher.Changed -= OnFileSystemEvent;
                _watcher.Created -= OnFileSystemEvent;
                _watcher.Deleted -= OnFileSystemEvent;
                _watcher.Renamed -= OnFileSystemRenamed;
                _watcher.Error -= OnFileSystemError;
                _watcher.Dispose();
            }
            catch
            {
                // Ignore disposal errors
            }
            _watcher = null;
        }
    }

    private void OnFileSystemEvent(object sender, FileSystemEventArgs e)
    {
        ScheduleReport();
    }

    private void OnFileSystemRenamed(object sender, RenamedEventArgs e)
    {
        // If file was renamed from or to our target name
        if (string.Equals(e.Name, _fileName, StringComparison.OrdinalIgnoreCase) ||
            string.Equals(e.OldName, _fileName, StringComparison.OrdinalIgnoreCase))
        {
            ScheduleReport();
        }
    }

    private void OnFileSystemError(object sender, ErrorEventArgs e)
    {
        lock (_lock)
        {
            if (_disposed)
            {
                return;
            }
            // If watcher suffered an overflow or error, re-arm and schedule a report
            Arm();
        }
        ScheduleReport();
    }

    public void ScheduleReport()
    {
        lock (_lock)
        {
            if (_disposed)
            {
                return;
            }

            if (_debounceTimer is null)
            {
                _debounceTimer = new Timer(OnTimerFired, null, DelayMilliseconds, Timeout.Infinite);
            }
            else
            {
                _debounceTimer.Change(DelayMilliseconds, Timeout.Infinite);
            }
            _scheduled = true;
        }
    }

    private void OnTimerFired(object? state)
    {
        Action cb;
        lock (_lock)
        {
            if (_disposed || !_scheduled)
            {
                return;
            }
            _scheduled = false;
            cb = _callback;
        }

        try
        {
            cb();
        }
        catch (Exception ex)
        {
            Core.CoreRuntime.ReportCallbackException(ex);
        }
    }

    public void Dispose()
    {
        lock (_lock)
        {
            if (_disposed)
            {
                return;
            }
            _disposed = true;
            DisarmLocked();
            if (_debounceTimer is not null)
            {
                _debounceTimer.Dispose();
                _debounceTimer = null;
            }
        }
    }
}
