using System.Buffers;
using System.Text;

namespace Markdown.Windows.Core;

public readonly record struct WorkspacePathClassification(
    uint ErrorCode,
    bool IsMarkdown,
    bool IsHidden,
    bool HasInvisible,
    uint NameStart)
{
    public bool IsValid => ErrorCode == CoreFfi.MD_PATH_VALID;
}

public static unsafe class CoreWorkspace
{
    public const int MaxPathBytes = (int)CoreFfi.MD_WORKSPACE_MAX_PATH_BYTES;

    public static WorkspacePathClassification ClassifyPath(string relativePath)
    {
        ArgumentNullException.ThrowIfNull(relativePath);
        CoreRuntime.Validate();

        var maxByteCount = Encoding.UTF8.GetMaxByteCount(relativePath.Length);
        if (maxByteCount <= 512)
        {
            Span<byte> stackBytes = stackalloc byte[512];
            var length = Encoding.UTF8.GetBytes(relativePath, stackBytes);
            return ClassifyUtf8Bytes(stackBytes[..length]);
        }

        var rented = ArrayPool<byte>.Shared.Rent(maxByteCount);
        try
        {
            var length = Encoding.UTF8.GetBytes(relativePath, rented);
            return ClassifyUtf8Bytes(rented.AsSpan(0, length));
        }
        finally
        {
            ArrayPool<byte>.Shared.Return(rented);
        }
    }

    public static WorkspacePathClassification ClassifyUtf8Bytes(ReadOnlySpan<byte> utf8Bytes)
    {
        CoreRuntime.Validate();

        var info = new CoreFfi.md_workspace_path_info
        {
            struct_size = CoreFfi.MD_WORKSPACE_PATH_INFO_SIZE,
        };

        uint status;
        fixed (byte* pathPtr = utf8Bytes)
        {
            status = CoreFfi.md_workspace_classify_path(
                pathPtr,
                (nuint)utf8Bytes.Length,
                &info);
        }

        if (status != CoreFfi.MD_OK)
        {
            throw CoreException.FromStatus("md_workspace_classify_path", status);
        }

        return new WorkspacePathClassification(
            ErrorCode: info.error,
            IsMarkdown: (info.flags & CoreFfi.MD_PATH_MARKDOWN) != 0,
            IsHidden: (info.flags & CoreFfi.MD_PATH_HIDDEN) != 0,
            HasInvisible: (info.flags & CoreFfi.MD_PATH_INVISIBLE) != 0,
            NameStart: info.name_start);
    }

    public static WorkspaceNameSearchResult FindNames(
        IReadOnlyList<WorkspaceNameInput> entries,
        string query,
        WorkspaceNameOptions? options = null)
    {
        ArgumentNullException.ThrowIfNull(entries);
        ArgumentNullException.ThrowIfNull(query);
        CoreRuntime.Validate();

        var opt = options ?? new WorkspaceNameOptions();
        var queryBytes = Encoding.UTF8.GetBytes(query);
        if (queryBytes.Length > CoreFfi.MD_WORKSPACE_MAX_QUERY_BYTES)
        {
            throw CoreException.InvalidArgument(
                $"query length {queryBytes.Length} exceeds maximum {CoreFfi.MD_WORKSPACE_MAX_QUERY_BYTES} bytes");
        }

        var pathBuffer = new MemoryStream(entries.Count * 32);
        var ffiEntries = new CoreFfi.md_workspace_entry[entries.Count];
        for (var i = 0; i < entries.Count; i++)
        {
            var entry = entries[i];
            var utf8 = Encoding.UTF8.GetBytes(entry.RelativePath);
            var offset = (ulong)pathBuffer.Length;
            pathBuffer.Write(utf8, 0, utf8.Length);
            ffiEntries[i] = new CoreFfi.md_workspace_entry
            {
                id = entry.Id,
                path_offset = offset,
                path_length = (uint)utf8.Length,
                reserved = 0,
            };
        }

        var pathsBytes = pathBuffer.ToArray();
        uint flags = 0;
        if (opt.CaseSensitive) flags |= CoreFfi.MD_NAMES_CASE_SENSITIVE;
        if (opt.IncludeHidden) flags |= CoreFfi.MD_NAMES_INCLUDE_HIDDEN;
        if (opt.AllFiles) flags |= CoreFfi.MD_NAMES_ALL_FILES;

        var ffiOptions = new CoreFfi.md_name_options
        {
            struct_size = CoreFfi.MD_NAME_OPTIONS_SIZE,
            flags = flags,
            max_results = opt.MaxResults,
            max_entries = opt.MaxEntries,
            entry_size = CoreFfi.MD_WORKSPACE_ENTRY_SIZE,
            hit_size = CoreFfi.MD_NAME_HIT_SIZE,
        };

        var capacity = opt.MaxResults == 0
            ? (int)CoreFfi.MD_NAMES_DEFAULT_MAX_RESULTS
            : (int)Math.Min(opt.MaxResults, CoreFfi.MD_NAMES_MAX_RESULTS);
        var ffiHits = capacity > 0 ? new CoreFfi.md_name_hit[capacity] : Array.Empty<CoreFfi.md_name_hit>();
        var result = new CoreFfi.md_name_result
        {
            struct_size = CoreFfi.MD_NAME_RESULT_SIZE,
        };

        uint status;
        fixed (byte* pathsPtr = pathsBytes)
        fixed (CoreFfi.md_workspace_entry* entriesPtr = ffiEntries)
        fixed (byte* queryPtr = queryBytes)
        fixed (CoreFfi.md_name_hit* hitsPtr = ffiHits)
        {
            status = CoreFfi.md_workspace_find_names(
                pathsPtr,
                (nuint)pathsBytes.Length,
                entriesPtr,
                (nuint)ffiEntries.Length,
                queryPtr,
                (nuint)queryBytes.Length,
                &ffiOptions,
                hitsPtr,
                (nuint)ffiHits.Length,
                &result);
        }

        if (status == CoreFfi.MD_ERR_BUFFER_TOO_SMALL && result.hit_count > 0)
        {
            var required = checked((int)result.hit_count);
            ffiHits = new CoreFfi.md_name_hit[required];
            result = new CoreFfi.md_name_result
            {
                struct_size = CoreFfi.MD_NAME_RESULT_SIZE,
            };

            fixed (byte* pathsPtr = pathsBytes)
            fixed (CoreFfi.md_workspace_entry* entriesPtr = ffiEntries)
            fixed (byte* queryPtr = queryBytes)
            fixed (CoreFfi.md_name_hit* hitsPtr = ffiHits)
            {
                status = CoreFfi.md_workspace_find_names(
                    pathsPtr,
                    (nuint)pathsBytes.Length,
                    entriesPtr,
                    (nuint)ffiEntries.Length,
                    queryPtr,
                    (nuint)queryBytes.Length,
                    &ffiOptions,
                    hitsPtr,
                    (nuint)ffiHits.Length,
                    &result);
            }
        }

        if (status != CoreFfi.MD_OK)
        {
            throw CoreException.FromStatus("md_workspace_find_names", status);
        }

        var hitCount = checked((int)result.hit_count);
        var hits = new List<WorkspaceNameHit>(hitCount);
        for (var i = 0; i < hitCount; i++)
        {
            var h = ffiHits[i];
            hits.Add(new WorkspaceNameHit(
                Id: h.id,
                EntryIndex: checked((int)h.entry),
                Rank: h.rank,
                MatchStart: h.start,
                MatchEnd: h.end));
        }

        return new WorkspaceNameSearchResult(
            Hits: hits,
            Truncated: (result.flags & CoreFfi.MD_NAMES_TRUNCATED) != 0,
            EntriesLimited: (result.flags & CoreFfi.MD_NAMES_ENTRIES_LIMITED) != 0,
            TotalHits: result.total_hits,
            EntriesExamined: result.entries_examined,
            SkippedHidden: result.skipped_hidden,
            SkippedNotMarkdown: result.skipped_not_markdown,
            SkippedInvalid: result.skipped_invalid);
    }

    public static WorkspaceContentAdmitResult ContentAdmit(
        string query,
        string relativePath,
        ulong byteLength,
        WorkspaceContentProgress progress,
        WorkspaceContentOptions? options = null)
    {
        ArgumentNullException.ThrowIfNull(query);
        ArgumentNullException.ThrowIfNull(relativePath);
        ArgumentNullException.ThrowIfNull(progress);
        CoreRuntime.Validate();

        var queryBytes = Encoding.UTF8.GetBytes(query);
        var pathBytes = Encoding.UTF8.GetBytes(relativePath);
        var ffiOptions = BuildContentOptions(options);

        uint outcome = 0;
        uint reason = 0;
        uint status;

        fixed (byte* queryPtr = queryBytes)
        fixed (byte* pathPtr = pathBytes)
        fixed (CoreFfi.md_content_progress* progressPtr = &progress.Raw)
        {
            status = CoreFfi.md_workspace_content_admit(
                queryPtr,
                (nuint)queryBytes.Length,
                &ffiOptions,
                pathPtr,
                (nuint)pathBytes.Length,
                byteLength,
                progressPtr,
                &outcome,
                &reason);
        }

        if (status != CoreFfi.MD_OK)
        {
            throw CoreException.FromStatus("md_workspace_content_admit", status);
        }

        return new WorkspaceContentAdmitResult(outcome, reason);
    }

    public static WorkspaceContentFileResult ContentSearch(
        string query,
        string relativePath,
        ReadOnlySpan<byte> content,
        WorkspaceContentProgress progress,
        WorkspaceContentOptions? options = null)
    {
        ArgumentNullException.ThrowIfNull(query);
        ArgumentNullException.ThrowIfNull(relativePath);
        ArgumentNullException.ThrowIfNull(progress);
        CoreRuntime.Validate();

        var opt = options ?? new WorkspaceContentOptions();
        var queryBytes = Encoding.UTF8.GetBytes(query);
        var pathBytes = Encoding.UTF8.GetBytes(relativePath);
        var ffiOptions = BuildContentOptions(opt);

        var maxPerFile = opt.MaxMatchesPerFile == 0
            ? (int)CoreFfi.MD_CONTENT_DEFAULT_MAX_MATCHES_PER_FILE
            : (int)Math.Min(opt.MaxMatchesPerFile, CoreFfi.MD_CONTENT_MAX_MATCHES_PER_FILE);
        var hitCap = Math.Max(1, maxPerFile);
        var textCap = checked(hitCap * (int)CoreFfi.MD_CONTENT_MAX_SNIPPET_BYTES);
        var anchorCap = checked(hitCap * (int)CoreFfi.MD_ANCHOR_MAX_BYTES);

        var ffiHits = new CoreFfi.md_content_hit[hitCap];
        var textBuf = new byte[textCap];
        var anchorBuf = new byte[anchorCap];
        var result = new CoreFfi.md_content_result
        {
            struct_size = CoreFfi.MD_CONTENT_RESULT_SIZE,
        };

        uint status;
        fixed (byte* queryPtr = queryBytes)
        fixed (byte* pathPtr = pathBytes)
        fixed (byte* contentPtr = content)
        fixed (CoreFfi.md_content_progress* progressPtr = &progress.Raw)
        fixed (CoreFfi.md_content_hit* hitsPtr = ffiHits)
        fixed (byte* textPtr = textBuf)
        fixed (byte* anchorPtr = anchorBuf)
        {
            status = CoreFfi.md_workspace_content_search(
                queryPtr,
                (nuint)queryBytes.Length,
                &ffiOptions,
                pathPtr,
                (nuint)pathBytes.Length,
                contentPtr,
                (nuint)content.Length,
                progressPtr,
                hitsPtr,
                (nuint)ffiHits.Length,
                textPtr,
                (nuint)textBuf.Length,
                anchorPtr,
                (nuint)anchorBuf.Length,
                &result);
        }

        if (status == CoreFfi.MD_ERR_BUFFER_TOO_SMALL)
        {
            hitCap = Math.Max(hitCap, checked((int)result.hit_count));
            textCap = Math.Max(textCap, checked((int)result.text_length));
            anchorCap = Math.Max(anchorCap, checked((int)result.anchor_length));
            ffiHits = new CoreFfi.md_content_hit[hitCap];
            textBuf = new byte[textCap];
            anchorBuf = new byte[anchorCap];
            result = new CoreFfi.md_content_result
            {
                struct_size = CoreFfi.MD_CONTENT_RESULT_SIZE,
            };

            fixed (byte* queryPtr = queryBytes)
            fixed (byte* pathPtr = pathBytes)
            fixed (byte* contentPtr = content)
            fixed (CoreFfi.md_content_progress* progressPtr = &progress.Raw)
            fixed (CoreFfi.md_content_hit* hitsPtr = ffiHits)
            fixed (byte* textPtr = textBuf)
            fixed (byte* anchorPtr = anchorBuf)
            {
                status = CoreFfi.md_workspace_content_search(
                    queryPtr,
                    (nuint)queryBytes.Length,
                    &ffiOptions,
                    pathPtr,
                    (nuint)pathBytes.Length,
                    contentPtr,
                    (nuint)content.Length,
                    progressPtr,
                    hitsPtr,
                    (nuint)ffiHits.Length,
                    textPtr,
                    (nuint)textBuf.Length,
                    anchorPtr,
                    (nuint)anchorBuf.Length,
                    &result);
            }
        }

        if (status != CoreFfi.MD_OK)
        {
            throw CoreException.FromStatus("md_workspace_content_search", status);
        }

        var sha256 = result.outcome == CoreFfi.MD_CONTENT_SEARCHED
            ? Encoding.ASCII.GetString(result.source_sha256, 64)
            : string.Empty;

        var hitCount = checked((int)result.hit_count);
        var hits = new List<WorkspaceContentHit>(hitCount);
        for (var i = 0; i < hitCount; i++)
        {
            var h = ffiHits[i];
            var snippetSpan = textBuf.AsSpan(checked((int)h.text_offset), checked((int)h.text_length));
            var snippet = Encoding.UTF8.GetString(snippetSpan);
            var matchStart = Math.Clamp((int)h.match_start, 0, snippetSpan.Length);
            var matchEnd = Math.Clamp((int)h.match_end, matchStart, snippetSpan.Length);
            var matchStartUtf16 = Encoding.UTF8.GetCharCount(snippetSpan[..matchStart]);
            var matchLengthUtf16 = Encoding.UTF8.GetCharCount(snippetSpan[matchStart..matchEnd]);

            var anchor = (h.flags & CoreFfi.MD_HIT_ANCHOR) != 0 && h.anchor_length > 0
                ? anchorBuf.AsSpan(checked((int)h.anchor_offset), checked((int)h.anchor_length)).ToArray()
                : Array.Empty<byte>();

            hits.Add(new WorkspaceContentHit(
                Unit: h.unit,
                Exact: (h.flags & CoreFfi.MD_HIT_EXACT) != 0,
                LeadingMore: (h.flags & CoreFfi.MD_HIT_LEADING_MORE) != 0,
                TrailingMore: (h.flags & CoreFfi.MD_HIT_TRAILING_MORE) != 0,
                SpanStart: h.span_start,
                SpanEnd: h.span_end,
                Line: h.line,
                Snippet: snippet,
                MatchStartUtf16: matchStartUtf16,
                MatchLengthUtf16: matchLengthUtf16,
                Anchor: anchor));
        }

        return new WorkspaceContentFileResult(
            Outcome: result.outcome,
            Reason: result.reason,
            Truncated: (result.flags & CoreFfi.MD_CONTENT_TRUNCATED) != 0,
            SourceLength: result.source_length,
            SourceSha256: sha256,
            Hits: hits);
    }

    private static CoreFfi.md_content_options BuildContentOptions(WorkspaceContentOptions? options)
    {
        var opt = options ?? new WorkspaceContentOptions();
        uint flags = 0;
        if (opt.CaseSensitive) flags |= CoreFfi.MD_CONTENT_CASE_SENSITIVE;
        if (opt.WholeWord) flags |= CoreFfi.MD_CONTENT_WHOLE_WORD;
        if (opt.IncludeHidden) flags |= CoreFfi.MD_CONTENT_INCLUDE_HIDDEN;

        return new CoreFfi.md_content_options
        {
            struct_size = CoreFfi.MD_CONTENT_OPTIONS_SIZE,
            flags = flags,
            max_files = opt.MaxFiles,
            max_total_bytes = opt.MaxTotalBytes,
            max_file_bytes = opt.MaxFileBytes,
            max_matches_per_file = opt.MaxMatchesPerFile,
            max_total_matches = opt.MaxTotalMatches,
            snippet_context = opt.SnippetContext,
            hit_size = CoreFfi.MD_CONTENT_HIT_SIZE,
            reserved = 0,
        };
    }
}

public readonly record struct WorkspaceNameInput(ulong Id, string RelativePath);

public sealed record WorkspaceNameOptions(
    bool CaseSensitive = false,
    bool IncludeHidden = false,
    bool AllFiles = false,
    uint MaxResults = CoreFfi.MD_NAMES_DEFAULT_MAX_RESULTS,
    uint MaxEntries = CoreFfi.MD_NAMES_DEFAULT_MAX_ENTRIES);

public readonly record struct WorkspaceNameHit(
    ulong Id,
    int EntryIndex,
    uint Rank,
    uint MatchStart,
    uint MatchEnd);

public sealed record WorkspaceNameSearchResult(
    IReadOnlyList<WorkspaceNameHit> Hits,
    bool Truncated,
    bool EntriesLimited,
    ulong TotalHits,
    ulong EntriesExamined,
    ulong SkippedHidden,
    ulong SkippedNotMarkdown,
    ulong SkippedInvalid);

public sealed record WorkspaceContentOptions(
    bool CaseSensitive = false,
    bool WholeWord = false,
    bool IncludeHidden = false,
    ulong MaxFiles = CoreFfi.MD_CONTENT_DEFAULT_MAX_FILES,
    ulong MaxTotalBytes = CoreFfi.MD_CONTENT_DEFAULT_MAX_TOTAL_BYTES,
    uint MaxFileBytes = CoreFfi.MD_CONTENT_DEFAULT_MAX_FILE_BYTES,
    uint MaxMatchesPerFile = CoreFfi.MD_CONTENT_DEFAULT_MAX_MATCHES_PER_FILE,
    uint MaxTotalMatches = CoreFfi.MD_CONTENT_DEFAULT_MAX_TOTAL_MATCHES,
    uint SnippetContext = CoreFfi.MD_CONTENT_DEFAULT_SNIPPET_CONTEXT);

public readonly record struct WorkspaceContentAdmitResult(uint Outcome, uint Reason)
{
    public bool ShouldRead => Outcome == CoreFfi.MD_CONTENT_READ;
    public bool IsSkipped => Outcome == CoreFfi.MD_CONTENT_SKIPPED;
    public bool IsStopped => Outcome == CoreFfi.MD_CONTENT_STOPPED;
}

public sealed record WorkspaceContentHit(
    uint Unit,
    bool Exact,
    bool LeadingMore,
    bool TrailingMore,
    uint SpanStart,
    uint SpanEnd,
    ulong Line,
    string Snippet,
    int MatchStartUtf16,
    int MatchLengthUtf16,
    byte[] Anchor);

public sealed record WorkspaceContentFileResult(
    uint Outcome,
    uint Reason,
    bool Truncated,
    ulong SourceLength,
    string SourceSha256,
    IReadOnlyList<WorkspaceContentHit> Hits)
{
    public bool IsSearched => Outcome == CoreFfi.MD_CONTENT_SEARCHED;
    public bool IsSkipped => Outcome == CoreFfi.MD_CONTENT_SKIPPED;
    public bool IsStopped => Outcome == CoreFfi.MD_CONTENT_STOPPED;
}

public sealed class WorkspaceContentProgress
{
    internal CoreFfi.md_content_progress Raw;

    public WorkspaceContentProgress()
    {
        Raw = new CoreFfi.md_content_progress
        {
            struct_size = CoreFfi.MD_CONTENT_PROGRESS_SIZE,
        };
    }

    public uint StoppedReason => Raw.stopped;
    public bool IsStopped => Raw.stopped != 0;
    public ulong FilesRead => Raw.files_read;
    public ulong FilesMatched => Raw.files_matched;
    public ulong BytesRead => Raw.bytes_read;
    public ulong Matches => Raw.matches;
    public ulong FilesTruncated => Raw.files_truncated;
    public ulong SkippedHidden => Raw.skipped_hidden;
    public ulong SkippedNotMarkdown => Raw.skipped_not_markdown;
    public ulong SkippedTooLarge => Raw.skipped_too_large;
    public ulong SkippedInvalidUtf8 => Raw.skipped_invalid_utf8;
    public ulong SkippedParseFailed => Raw.skipped_parse_failed;
}
