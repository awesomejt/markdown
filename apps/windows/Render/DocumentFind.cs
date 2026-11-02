using System.Globalization;
using System.Text;
using Markdown.Windows.Core;

namespace Markdown.Windows.Render;

public sealed record FindMatchRange(int FragmentIndex, int Start, int Length);

public sealed class FindMatch
{
    public int Index { get; }
    public uint UnitNode { get; }
    public Range Source { get; }
    public bool Exact { get; }
    public int PrimaryFragmentIndex => Ranges.Count > 0 ? Ranges[0].FragmentIndex : 0;
    public IReadOnlyList<FindMatchRange> Ranges { get; }

    public FindMatch(int index, uint unitNode, Range source, bool exact, IReadOnlyList<FindMatchRange> ranges)
    {
        Index = index;
        UnitNode = unitNode;
        Source = source;
        Exact = exact;
        Ranges = ranges;
    }
}

public sealed class FindMatches
{
    private readonly Dictionary<int, List<(int MatchIndex, FindMatchRange Range)>> _rangesByFragment;

    public ulong Generation { get; }
    public IReadOnlyList<FindMatch> Matches { get; }
    public bool Truncated { get; }
    public int Count => Matches.Count;

    public FindMatches(ulong generation, IReadOnlyList<FindMatch> matches, bool truncated)
    {
        Generation = generation;
        Matches = matches;
        Truncated = truncated;

        var byFrag = new Dictionary<int, List<(int MatchIndex, FindMatchRange Range)>>();
        for (var i = 0; i < matches.Count; i++)
        {
            var m = matches[i];
            foreach (var r in m.Ranges)
            {
                if (!byFrag.TryGetValue(r.FragmentIndex, out var list))
                {
                    list = new List<(int MatchIndex, FindMatchRange Range)>();
                    byFrag[r.FragmentIndex] = list;
                }
                list.Add((i, r));
            }
        }
        _rangesByFragment = byFrag;
    }

    public IReadOnlyList<(int MatchIndex, FindMatchRange Range)> RangesForFragment(int fragmentIndex)
    {
        if (_rangesByFragment.TryGetValue(fragmentIndex, out var list))
        {
            return list;
        }
        return Array.Empty<(int, FindMatchRange)>();
    }

    public int? IndexAtOrAfter(int fragmentIndex, int charOffset = 0)
    {
        if (Matches.Count == 0) return null;
        for (var i = 0; i < Matches.Count; i++)
        {
            var m = Matches[i];
            if (m.PrimaryFragmentIndex > fragmentIndex ||
                (m.PrimaryFragmentIndex == fragmentIndex && m.Ranges.Count > 0 && m.Ranges[0].Start >= charOffset))
            {
                return i;
            }
        }
        return 0; // wrap to first
    }

    public int? IndexBefore(int fragmentIndex, int charOffset = int.MaxValue)
    {
        if (Matches.Count == 0) return null;
        for (var i = Matches.Count - 1; i >= 0; i--)
        {
            var m = Matches[i];
            if (m.PrimaryFragmentIndex < fragmentIndex ||
                (m.PrimaryFragmentIndex == fragmentIndex && m.Ranges.Count > 0 && m.Ranges[0].Start < charOffset))
            {
                return i;
            }
        }
        return Matches.Count - 1; // wrap to last
    }

    public int? IndexAfter(int currentIndex, bool backwards = false)
    {
        if (Matches.Count == 0) return null;
        if (backwards)
        {
            return (currentIndex - 1 + Matches.Count) % Matches.Count;
        }
        return (currentIndex + 1) % Matches.Count;
    }

    public string Status(int? currentIndex)
    {
        if (Matches.Count == 0) return "Not found";
        var suffix = Truncated ? "+" : "";
        var total = $"{Matches.Count.ToString("N0", CultureInfo.InvariantCulture)}{suffix}";
        if (currentIndex is null)
        {
            return Matches.Count == 1 ? "1 match" : $"{total} matches";
        }
        return $"{currentIndex.Value + 1} of {total}";
    }
}

public static class DocumentFind
{
    public static (int CharStart, int CharEnd) Utf8ByteRangeToCharRange(string text, int byteStart, int byteEnd)
    {
        var currentByte = 0;
        var charStart = 0;
        var charEnd = text.Length;
        var foundStart = false;

        for (var i = 0; i < text.Length; i++)
        {
            if (!foundStart && currentByte >= byteStart)
            {
                charStart = i;
                foundStart = true;
            }
            if (currentByte >= byteEnd)
            {
                charEnd = i;
                return (charStart, charEnd);
            }

            var ch = text[i];
            if (ch <= 0x7F)
            {
                currentByte += 1;
            }
            else if (ch <= 0x7FF)
            {
                currentByte += 2;
            }
            else if (char.IsSurrogate(ch))
            {
                currentByte += 4;
                i++; // skip trailing surrogate
            }
            else
            {
                currentByte += 3;
            }
        }

        if (!foundStart) charStart = text.Length;
        return (charStart, text.Length);
    }

    private static int[] BuildByteToCharMap(string text)
    {
        var byteCount = Encoding.UTF8.GetByteCount(text);
        var map = new int[byteCount + 1];
        var currentByte = 0;

        for (var i = 0; i < text.Length; i++)
        {
            var ch = text[i];
            int charBytes;
            if (ch <= 0x7F)
            {
                charBytes = 1;
            }
            else if (ch <= 0x7FF)
            {
                charBytes = 2;
            }
            else if (char.IsSurrogate(ch))
            {
                charBytes = 4;
                for (var b = 0; b < charBytes && currentByte < byteCount; b++)
                {
                    map[currentByte++] = i;
                }
                i++; // skip trailing surrogate
                continue;
            }
            else
            {
                charBytes = 3;
            }

            for (var b = 0; b < charBytes && currentByte < byteCount; b++)
            {
                map[currentByte++] = i;
            }
        }

        while (currentByte <= byteCount)
        {
            map[currentByte++] = text.Length;
        }

        return map;
    }

    public static FindMatches MapMatches(RenderPlan plan, CoreDocument document, CoreFindResult result)
    {
        // Index fragments and runs by NodeId
        var nodeRunIndex = new Dictionary<int, List<(int FragmentIndex, int FragCharOffset, RenderRun Run)>>();
        for (var f = 0; f < plan.Fragments.Count; f++)
        {
            var frag = plan.Fragments[f];
            var fragCharOffset = 0;
            foreach (var run in frag.Runs)
            {
                if (run.NodeId >= 0)
                {
                    if (!nodeRunIndex.TryGetValue(run.NodeId, out var list))
                    {
                        list = new List<(int, int, RenderRun)>();
                        nodeRunIndex[run.NodeId] = list;
                    }
                    list.Add((f, fragCharOffset, run));
                }
                fragCharOffset += run.Text.Length;
            }
        }

        var nodeTextCache = new Dictionary<int, string>();
        var nodeByteToCharCache = new Dictionary<int, int[]>();

        var matches = new List<FindMatch>(result.Matches.Count);
        for (var mIndex = 0; mIndex < result.Matches.Count; mIndex++)
        {
            var m = result.Matches[mIndex];
            var matchRanges = new List<FindMatchRange>();

            for (var s = m.FirstSegment; s < m.FirstSegment + m.SegmentCount; s++)
            {
                if (s >= result.Segments.Count) break;
                var segment = result.Segments[s];
                var nodeId = (int)segment.Node;

                if (!nodeRunIndex.TryGetValue(nodeId, out var runEntries))
                {
                    continue;
                }

                if (!nodeTextCache.TryGetValue(nodeId, out var nodeText))
                {
                    nodeText = (nodeId >= 0 && nodeId < document.NodeCount)
                        ? document.NodeText(nodeId)
                        : string.Empty;
                    nodeTextCache[nodeId] = nodeText;
                }

                if (!nodeByteToCharCache.TryGetValue(nodeId, out var byteMap))
                {
                    byteMap = BuildByteToCharMap(nodeText);
                    nodeByteToCharCache[nodeId] = byteMap;
                }

                var byteStart = segment.Text.Start.Value;
                var byteEnd = segment.Text.End.Value;
                var charStart = byteStart >= byteMap.Length ? nodeText.Length : byteMap[byteStart];
                var charEnd = byteEnd >= byteMap.Length ? nodeText.Length : byteMap[byteEnd];

                foreach (var (fragmentIndex, fragCharOffset, run) in runEntries)
                {
                    if (run.Atomic)
                    {
                        matchRanges.Add(new FindMatchRange(fragmentIndex, fragCharOffset, run.Text.Length));
                        continue;
                    }

                    var runStartInNode = run.NodeCharOffset;
                    var runEndInNode = runStartInNode + run.Text.Length;

                    var overlapStart = Math.Max(charStart, runStartInNode);
                    var overlapEnd = Math.Min(charEnd, runEndInNode);

                    if (overlapStart < overlapEnd)
                    {
                        var localStart = fragCharOffset + (overlapStart - runStartInNode);
                        var localLength = overlapEnd - overlapStart;
                        matchRanges.Add(new FindMatchRange(fragmentIndex, localStart, localLength));
                    }
                }
            }

            if (matchRanges.Count == 0)
            {
                continue;
            }

            // Merge contiguous ranges within the same fragment
            var merged = new List<FindMatchRange>(matchRanges.Count);
            matchRanges.Sort((a, b) =>
                a.FragmentIndex != b.FragmentIndex
                    ? a.FragmentIndex.CompareTo(b.FragmentIndex)
                    : a.Start.CompareTo(b.Start));

            var current = matchRanges[0];
            for (var i = 1; i < matchRanges.Count; i++)
            {
                var next = matchRanges[i];
                if (next.FragmentIndex == current.FragmentIndex &&
                    next.Start <= current.Start + current.Length)
                {
                    var newEnd = Math.Max(current.Start + current.Length, next.Start + next.Length);
                    current = new FindMatchRange(current.FragmentIndex, current.Start, newEnd - current.Start);
                }
                else
                {
                    merged.Add(current);
                    current = next;
                }
            }
            merged.Add(current);

            matches.Add(new FindMatch(mIndex, m.Unit, m.Source, m.Exact, merged));
        }

        return new FindMatches(plan.Generation, matches, result.Truncated);
    }
}
