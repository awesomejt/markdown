using System.Diagnostics;
using System.IO;
using Markdown.Windows.Core;

namespace Markdown.Windows.Document;

public sealed record WorkspaceSearchOptions(
    bool CaseSensitive = false,
    bool WholeWord = false,
    bool IncludeHidden = false,
    bool AllFiles = false,
    uint MaxNameResults = CoreFfi.MD_NAMES_DEFAULT_MAX_RESULTS,
    ulong MaxFiles = CoreFfi.MD_CONTENT_DEFAULT_MAX_FILES,
    ulong MaxTotalBytes = CoreFfi.MD_CONTENT_DEFAULT_MAX_TOTAL_BYTES,
    uint MaxFileBytes = CoreFfi.MD_CONTENT_DEFAULT_MAX_FILE_BYTES,
    uint MaxMatchesPerFile = CoreFfi.MD_CONTENT_DEFAULT_MAX_MATCHES_PER_FILE,
    uint MaxTotalMatches = CoreFfi.MD_CONTENT_DEFAULT_MAX_TOTAL_MATCHES);

public sealed record WorkspaceSearchNameHit(
    string RelativePath,
    string Name,
    uint Rank,
    bool HasInvisible);

public sealed record WorkspaceSearchContentHit(
    string RelativePath,
    string Name,
    ulong Line,
    string Snippet,
    int MatchStartUtf16,
    int MatchLengthUtf16,
    bool LeadingMore,
    bool TrailingMore,
    uint SpanStart,
    uint SpanEnd,
    ulong SourceLength,
    string SourceSha256,
    byte[] Anchor,
    bool HasInvisible);

public sealed record WorkspaceSearchSnapshot(
    string Query,
    bool IsRunning,
    bool IsCancelled,
    bool IsIncomplete,
    uint StopReason,
    bool NameHitsTruncated,
    bool EntriesLimited,
    ulong FilesRead,
    ulong FilesMatched,
    ulong TotalMatches,
    ulong SkippedTooLarge,
    ulong SkippedParseFailed,
    ulong SkippedUnreadable,
    IReadOnlyList<WorkspaceSearchNameHit> NameHits,
    IReadOnlyList<WorkspaceSearchContentHit> ContentHits,
    string? ErrorMessage = null)
{
    public static WorkspaceSearchSnapshot Empty { get; } = new(
        Query: string.Empty,
        IsRunning: false,
        IsCancelled: false,
        IsIncomplete: false,
        StopReason: 0,
        NameHitsTruncated: false,
        EntriesLimited: false,
        FilesRead: 0,
        FilesMatched: 0,
        TotalMatches: 0,
        SkippedTooLarge: 0,
        SkippedParseFailed: 0,
        SkippedUnreadable: 0,
        NameHits: Array.Empty<WorkspaceSearchNameHit>(),
        ContentHits: Array.Empty<WorkspaceSearchContentHit>(),
        ErrorMessage: null);
}

public static class WorkspaceSearch
{
    public static Task<WorkspaceSearchSnapshot> RunAsync(
        FolderCapability folder,
        WorkspaceTree tree,
        string query,
        WorkspaceSearchOptions? options = null,
        Action<WorkspaceSearchSnapshot>? onProgress = null,
        CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(folder);
        ArgumentNullException.ThrowIfNull(tree);
        ArgumentNullException.ThrowIfNull(query);

        return Task.Run(
            () => Run(folder, tree, query, options, onProgress, cancellationToken),
            cancellationToken);
    }

    public static WorkspaceSearchSnapshot Run(
        FolderCapability folder,
        WorkspaceTree tree,
        string query,
        WorkspaceSearchOptions? options = null,
        Action<WorkspaceSearchSnapshot>? onProgress = null,
        CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(folder);
        ArgumentNullException.ThrowIfNull(tree);
        ArgumentNullException.ThrowIfNull(query);

        var opt = options ?? new WorkspaceSearchOptions();
        var trimmed = query.Trim();
        if (string.IsNullOrEmpty(trimmed))
        {
            var empty = WorkspaceSearchSnapshot.Empty with { Query = query };
            onProgress?.Invoke(empty);
            return empty;
        }

        if (!folder.ValidateAccess(out var folderError))
        {
            var errSnap = WorkspaceSearchSnapshot.Empty with
            {
                Query = query,
                ErrorMessage = folderError.Message(),
            };
            onProgress?.Invoke(errSnap);
            return errSnap;
        }

        var fileEntries = new List<WorkspaceEntry>();
        var nameInputs = new List<WorkspaceNameInput>();
        foreach (var entry in tree.Entries)
        {
            if (entry.IsDirectory)
            {
                continue;
            }
            fileEntries.Add(entry);
            nameInputs.Add(new WorkspaceNameInput((ulong)fileEntries.Count - 1UL, entry.RelativePath));
        }

        var nameResult = CoreWorkspace.FindNames(
            nameInputs,
            trimmed,
            new WorkspaceNameOptions(
                CaseSensitive: opt.CaseSensitive,
                IncludeHidden: opt.IncludeHidden,
                AllFiles: opt.AllFiles,
                MaxResults: opt.MaxNameResults));

        var nameHits = new List<WorkspaceSearchNameHit>(nameResult.Hits.Count);
        foreach (var hit in nameResult.Hits)
        {
            var idx = checked((int)hit.Id);
            if ((uint)idx < (uint)fileEntries.Count)
            {
                var entry = fileEntries[idx];
                nameHits.Add(new WorkspaceSearchNameHit(
                    entry.RelativePath,
                    entry.Name,
                    hit.Rank,
                    entry.HasInvisible));
            }
        }

        var contentHits = new List<WorkspaceSearchContentHit>();
        var progress = new WorkspaceContentProgress();
        var contentOptions = new WorkspaceContentOptions(
            CaseSensitive: opt.CaseSensitive,
            WholeWord: opt.WholeWord,
            IncludeHidden: opt.IncludeHidden,
            MaxFiles: opt.MaxFiles,
            MaxTotalBytes: opt.MaxTotalBytes,
            MaxFileBytes: opt.MaxFileBytes,
            MaxMatchesPerFile: opt.MaxMatchesPerFile,
            MaxTotalMatches: opt.MaxTotalMatches);

        ulong skippedUnreadable = 0;
        var cancelled = false;
        var rootPath = folder.CanonicalRoot.TrimEnd('\\', '/');
        var lastReportTicks = Stopwatch.GetTimestamp();

        WorkspaceSearchSnapshot BuildSnapshot(bool isRunning, bool isCancelled, string? errorMessage = null)
        {
            var isIncomplete = progress.IsStopped
                || nameResult.Truncated
                || nameResult.EntriesLimited
                || progress.FilesTruncated > 0
                || progress.SkippedTooLarge > 0
                || progress.SkippedParseFailed > 0
                || skippedUnreadable > 0;

            return new WorkspaceSearchSnapshot(
                Query: query,
                IsRunning: isRunning,
                IsCancelled: isCancelled,
                IsIncomplete: isIncomplete,
                StopReason: progress.StoppedReason,
                NameHitsTruncated: nameResult.Truncated,
                EntriesLimited: nameResult.EntriesLimited,
                FilesRead: progress.FilesRead,
                FilesMatched: progress.FilesMatched,
                TotalMatches: progress.Matches,
                SkippedTooLarge: progress.SkippedTooLarge,
                SkippedParseFailed: progress.SkippedParseFailed,
                SkippedUnreadable: skippedUnreadable,
                NameHits: nameHits.ToArray(),
                ContentHits: contentHits.ToArray(),
                ErrorMessage: errorMessage);
        }

        if (onProgress is not null)
        {
            onProgress(BuildSnapshot(isRunning: true, isCancelled: false));
        }

        foreach (var entry in fileEntries)
        {
            if (cancellationToken.IsCancellationRequested)
            {
                cancelled = true;
                break;
            }
            if (folder.IsRevoked)
            {
                var revoked = BuildSnapshot(isRunning: false, isCancelled: false, FolderError.AccessRevoked.Message());
                onProgress?.Invoke(revoked);
                return revoked;
            }

            var fullPath = Path.Combine(rootPath, entry.RelativePath.Replace('/', Path.DirectorySeparatorChar));
            long byteLength;
            try
            {
                var info = new FileInfo(fullPath);
                if (!info.Exists || (info.Attributes & FileAttributes.ReparsePoint) != 0)
                {
                    skippedUnreadable++;
                    continue;
                }
                byteLength = info.Length;
            }
            catch (Exception ex) when (ex is IOException or UnauthorizedAccessException or System.Security.SecurityException)
            {
                skippedUnreadable++;
                continue;
            }

            var admit = CoreWorkspace.ContentAdmit(
                trimmed,
                entry.RelativePath,
                (ulong)Math.Max(0L, byteLength),
                progress,
                contentOptions);

            if (admit.IsStopped)
            {
                break;
            }
            if (admit.IsSkipped)
            {
                continue;
            }

            if (!folder.ResolveDocument(fullPath, out var docCap, out _) || docCap is null)
            {
                skippedUnreadable++;
                continue;
            }

            byte[]? bytes;
            try
            {
                if (!TryReadBoundedBytes(fullPath, opt.MaxFileBytes, out bytes))
                {
                    skippedUnreadable++;
                    continue;
                }
            }
            finally
            {
                docCap.Revoke();
            }

            var fileResult = CoreWorkspace.ContentSearch(
                trimmed,
                entry.RelativePath,
                bytes,
                progress,
                contentOptions);

            if (fileResult.IsSearched && fileResult.Hits.Count > 0)
            {
                foreach (var hit in fileResult.Hits)
                {
                    contentHits.Add(new WorkspaceSearchContentHit(
                        RelativePath: entry.RelativePath,
                        Name: entry.Name,
                        Line: hit.Line,
                        Snippet: hit.Snippet,
                        MatchStartUtf16: hit.MatchStartUtf16,
                        MatchLengthUtf16: hit.MatchLengthUtf16,
                        LeadingMore: hit.LeadingMore,
                        TrailingMore: hit.TrailingMore,
                        SpanStart: hit.SpanStart,
                        SpanEnd: hit.SpanEnd,
                        SourceLength: fileResult.SourceLength,
                        SourceSha256: fileResult.SourceSha256,
                        Anchor: hit.Anchor,
                        HasInvisible: entry.HasInvisible));
                }
            }

            if (progress.IsStopped || fileResult.IsStopped)
            {
                break;
            }

            if (onProgress is not null)
            {
                var now = Stopwatch.GetTimestamp();
                var elapsedMs = (now - lastReportTicks) * 1000.0 / Stopwatch.Frequency;
                if (elapsedMs >= 60.0)
                {
                    lastReportTicks = now;
                    onProgress(BuildSnapshot(isRunning: true, isCancelled: false));
                }
            }
        }

        var finalSnapshot = BuildSnapshot(isRunning: false, isCancelled: cancelled);
        onProgress?.Invoke(finalSnapshot);
        return finalSnapshot;
    }

    private static bool TryReadBoundedBytes(string fullPath, uint maxFileBytes, out byte[] bytes)
    {
        bytes = Array.Empty<byte>();
        try
        {
            var attrs = File.GetAttributes(fullPath);
            if ((attrs & (FileAttributes.ReparsePoint | FileAttributes.Directory)) != 0)
            {
                return false;
            }

            using var stream = new FileStream(
                fullPath,
                FileMode.Open,
                FileAccess.Read,
                FileShare.ReadWrite | FileShare.Delete,
                bufferSize: 65536,
                FileOptions.SequentialScan);

            var length = stream.Length;
            if (length < 0 || length > maxFileBytes)
            {
                return false;
            }

            var buffer = new byte[length];
            var totalRead = 0;
            while (totalRead < buffer.Length)
            {
                var read = stream.Read(buffer, totalRead, buffer.Length - totalRead);
                if (read == 0)
                {
                    break;
                }
                totalRead += read;
            }

            if (totalRead != buffer.Length)
            {
                Array.Resize(ref buffer, totalRead);
            }

            bytes = buffer;
            return true;
        }
        catch (Exception ex) when (ex is IOException or UnauthorizedAccessException or System.Security.SecurityException)
        {
            return false;
        }
    }
}

public sealed class WorkspaceSearchRunner : IDisposable
{
    private readonly object _lock = new();
    private CancellationTokenSource? _cts;
    private long _serial;

    public long Start(
        FolderCapability folder,
        WorkspaceTree tree,
        string query,
        WorkspaceSearchOptions? options,
        Action<long, WorkspaceSearchSnapshot> onUpdate)
    {
        ArgumentNullException.ThrowIfNull(folder);
        ArgumentNullException.ThrowIfNull(tree);
        ArgumentNullException.ThrowIfNull(query);
        ArgumentNullException.ThrowIfNull(onUpdate);

        CancellationTokenSource cts;
        long serial;
        lock (_lock)
        {
            _cts?.Cancel();
            _cts?.Dispose();
            _cts = new CancellationTokenSource();
            cts = _cts;
            serial = ++_serial;
        }

        var token = cts.Token;
        _ = Task.Run(() =>
        {
            try
            {
                WorkspaceSearch.Run(
                    folder,
                    tree,
                    query,
                    options,
                    snapshot =>
                    {
                        if (!token.IsCancellationRequested)
                        {
                            onUpdate(serial, snapshot);
                        }
                    },
                    token);
            }
            catch (OperationCanceledException)
            {
                // Superseded or stopped.
            }
        }, CancellationToken.None);

        return serial;
    }

    public void Cancel()
    {
        lock (_lock)
        {
            _cts?.Cancel();
            _cts?.Dispose();
            _cts = null;
            _serial++;
        }
    }

    public void Dispose()
    {
        Cancel();
    }
}
