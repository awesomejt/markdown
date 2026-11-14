using System.Diagnostics;
using System.Text;

namespace Markdown.Windows.Core;

public sealed record CoreFindOptions(
    bool CaseSensitive = false,
    bool WholeWord = false,
    uint MaxMatches = CoreFfi.MD_FIND_DEFAULT_MAX_MATCHES);

public sealed record CoreFindSegment(
    uint Node,
    Range Text,
    Range Source,
    bool Exact);

public sealed record CoreFindMatch(
    uint Unit,
    Range Source,
    bool Exact,
    int FirstSegment,
    int SegmentCount);

public sealed record CoreFindResult(
    ulong Generation,
    IReadOnlyList<CoreFindMatch> Matches,
    IReadOnlyList<CoreFindSegment> Segments,
    bool Truncated);

internal sealed class FindCancellationGate
{
    private readonly CancellationToken _token;
    public bool IsCancelled => _token.IsCancellationRequested;

    public FindCancellationGate(CancellationToken token)
    {
        _token = token;
    }
}

public sealed unsafe partial class CoreSession
{
    public CoreFindResult Find(
        string query,
        CoreFindOptions? options = null,
        ulong generation = 0,
        CancellationToken cancellationToken = default)
    {
        if (CoreRuntime.OnWorker)
        {
            throw CoreException.BlockingWaitOnWorker();
        }

        return FindAsync(query, options, generation, cancellationToken)
            .GetAwaiter()
            .GetResult();
    }

    public Task<CoreFindResult> FindAsync(
        string query,
        CoreFindOptions? options = null,
        ulong generation = 0,
        CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(query);
        cancellationToken.ThrowIfCancellationRequested();

        var queryBytes = Encoding.UTF8.GetBytes(query);
        if (queryBytes.Length > CoreFfi.MD_FIND_MAX_QUERY_BYTES)
        {
            throw CoreException.InvalidArgument(
                $"query length {queryBytes.Length} exceeds maximum {CoreFfi.MD_FIND_MAX_QUERY_BYTES} bytes");
        }

        var opt = options ?? new CoreFindOptions();
        if (opt.MaxMatches > CoreFfi.MD_FIND_MAX_MATCHES)
        {
            throw CoreException.InvalidArgument(
                $"MaxMatches {opt.MaxMatches} exceeds maximum {CoreFfi.MD_FIND_MAX_MATCHES}");
        }
        var tcs = new TaskCompletionSource<CoreFindResult>(TaskCreationOptions.RunContinuationsAsynchronously);

        var gate = new FindCancellationGate(cancellationToken);
        if (gate.IsCancelled)
        {
            tcs.SetCanceled(cancellationToken);
            return tcs.Task;
        }

        CoreRuntime.EnqueueWork(() =>
        {
            if (gate.IsCancelled)
            {
                tcs.TrySetCanceled(cancellationToken);
                return;
            }

            try
            {
                var result = _inner.RunFind(queryBytes, opt, generation, gate);
                tcs.TrySetResult(result);
            }
            catch (OperationCanceledException)
            {
                tcs.TrySetCanceled(cancellationToken);
            }
            catch (Exception ex)
            {
                tcs.TrySetException(ex);
            }
        });

        return tcs.Task;
    }
}

internal sealed unsafe partial class SessionInner
{
    public CoreFindResult RunFind(
        byte[] queryBytes,
        CoreFindOptions options,
        ulong generation,
        FindCancellationGate gate)
    {
        ulong snapshot;
        lock (_lock)
        {
            if (_closing is not null || _handle == 0)
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

            if (generation != 0 && info.published != generation)
            {
                throw CoreException.Superseded(generation, info.published);
            }
            generation = info.published;

            ulong snap = 0;
            status = CoreFfi.md_session_current(_handle, &snap);
            if (status != CoreFfi.MD_OK)
            {
                throw CoreException.FromStatus("md_session_current", status);
            }
            if (snap == 0)
            {
                throw CoreException.Superseded(generation, info.published);
            }
            snapshot = snap;
        }

        try
        {
            if (gate.IsCancelled)
            {
                throw new OperationCanceledException();
            }

            var settings = new CoreFfi.md_find_options
            {
                struct_size = CoreFfi.MD_FIND_OPTIONS_SIZE,
                flags = (options.CaseSensitive ? CoreFfi.MD_FIND_CASE_SENSITIVE : 0)
                      | (options.WholeWord ? CoreFfi.MD_FIND_WHOLE_WORD : 0),
                max_matches = options.MaxMatches,
                match_size = (uint)sizeof(CoreFfi.md_find_match),
                segment_size = (uint)sizeof(CoreFfi.md_match_segment),
                reserved = 0,
            };

            var limit = options.MaxMatches == 0 ? CoreFfi.MD_FIND_DEFAULT_MAX_MATCHES : options.MaxMatches;
            var initialMatches = (int)Math.Min(limit, 256u);
            var initialSegments = (int)Math.Min((long)limit * 2, 512L);
            var matches = new CoreFfi.md_find_match[initialMatches];
            var segments = new CoreFfi.md_match_segment[initialSegments];
            var result = new CoreFfi.md_find_result { struct_size = CoreFfi.MD_FIND_RESULT_SIZE };

            for (var attempt = 0; attempt < 2; attempt++)
            {
                result = new CoreFfi.md_find_result { struct_size = CoreFfi.MD_FIND_RESULT_SIZE };
                fixed (byte* qPtr = queryBytes)
                fixed (CoreFfi.md_find_match* mPtr = matches)
                fixed (CoreFfi.md_match_segment* sPtr = segments)
                {
                    var status = CoreFfi.md_snapshot_find(
                        snapshot,
                        qPtr,
                        (nuint)queryBytes.Length,
                        &settings,
                        mPtr,
                        (nuint)matches.Length,
                        sPtr,
                        (nuint)segments.Length,
                        &result);

                    if (status == CoreFfi.MD_ERR_BUFFER_TOO_SMALL && attempt == 0)
                    {
                        if (result.match_count > limit || result.segment_count > (ulong)limit * 4UL + 2UL * Limits.MaxNodes)
                        {
                            throw CoreException.InvalidSnapshot("find required buffer sizes exceed safety bounds");
                        }
                        matches = new CoreFfi.md_find_match[result.match_count];
                        segments = new CoreFfi.md_match_segment[result.segment_count];
                        continue;
                    }

                    if (status != CoreFfi.MD_OK)
                    {
                        throw CoreException.FromStatus("md_snapshot_find", status);
                    }
                    break;
                }
            }

            var matchCount = (int)result.match_count;
            var segmentCount = (int)result.segment_count;
            if (matchCount > matches.Length || segmentCount > segments.Length)
            {
                throw CoreException.InvalidSnapshot("find result counts exceed returned arrays");
            }

            var exact = CoreFfi.MD_MATCH_EXACT;
            var ownedSegments = new List<CoreFindSegment>(segmentCount);
            for (var i = 0; i < segmentCount; i++)
            {
                ref readonly var seg = ref segments[i];
                if (seg.text_start > seg.text_end || seg.span_start > seg.span_end)
                {
                    throw CoreException.InvalidSnapshot("match segment bounds corrupted");
                }
                ownedSegments.Add(new CoreFindSegment(
                    seg.node,
                    (int)seg.text_start..(int)seg.text_end,
                    (int)seg.span_start..(int)seg.span_end,
                    (seg.flags & exact) != 0));
            }

            var ownedMatches = new List<CoreFindMatch>(matchCount);
            for (var i = 0; i < matchCount; i++)
            {
                ref readonly var m = ref matches[i];
                var first = (int)m.first_segment;
                var end = first + (int)m.segment_count;
                if (m.span_start > m.span_end || first > end || end > segmentCount)
                {
                    throw CoreException.InvalidSnapshot("match bounds corrupted");
                }
                ownedMatches.Add(new CoreFindMatch(
                    m.unit,
                    (int)m.span_start..(int)m.span_end,
                    (m.flags & exact) != 0,
                    first,
                    (int)m.segment_count));
            }

            return new CoreFindResult(
                generation,
                ownedMatches,
                ownedSegments,
                (result.flags & CoreFfi.MD_FIND_TRUNCATED) != 0);
        }
        finally
        {
            _ = CoreFfi.md_snapshot_release(snapshot);
        }
    }
}
