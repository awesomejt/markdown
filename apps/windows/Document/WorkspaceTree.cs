using System.IO;
using Markdown.Windows.Core;

namespace Markdown.Windows.Document;

public sealed class WorkspaceEnumerationException : Exception
{
    public FolderError Error { get; }

    public WorkspaceEnumerationException(FolderError error, string? message = null)
        : base(message ?? error.Message())
    {
        Error = error;
    }
}

public sealed class WorkspaceEntry
{
    public required ulong Id { get; init; }
    public required int Index { get; init; }
    public required int ParentIndex { get; init; }
    public required int Depth { get; init; }
    public required string RelativePath { get; init; }
    public required string Name { get; init; }
    public required string SortKey { get; init; }
    public required bool IsDirectory { get; init; }
    public required bool IsMarkdown { get; init; }
    public required bool IsHidden { get; init; }
    public required bool HasInvisible { get; init; }
    internal int FirstChildIndex { get; set; } = -1;
    internal int NextSiblingIndex { get; set; } = -1;
}

public readonly record struct WorkspaceVisibleRow(
    WorkspaceEntry Entry,
    bool IsExpanded,
    int PositionInSet,
    int SizeOfSet);

public sealed class WorkspaceTree
{
    public const int MaxEntries = 100_000;
    public const int MaxDepth = 256;

    private readonly List<WorkspaceEntry> _entries;

    public IReadOnlyList<WorkspaceEntry> Entries => _entries;
    public int RootFirstChildIndex { get; }
    public bool Truncated { get; }
    public int SkippedInvalid { get; }
    public int SkippedSymlinks { get; }
    public int TotalMarkdownFiles { get; }

    private WorkspaceTree(
        List<WorkspaceEntry> entries,
        int rootFirstChildIndex,
        bool truncated,
        int skippedInvalid,
        int skippedSymlinks,
        int totalMarkdownFiles)
    {
        _entries = entries;
        RootFirstChildIndex = rootFirstChildIndex;
        Truncated = truncated;
        SkippedInvalid = skippedInvalid;
        SkippedSymlinks = skippedSymlinks;
        TotalMarkdownFiles = totalMarkdownFiles;
    }

    public static WorkspaceTree Empty { get; } = new([], -1, false, 0, 0, 0);

    public static Task<WorkspaceTree> EnumerateAsync(
        FolderCapability folder,
        CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(folder);
        return Task.Run(() => Enumerate(folder, cancellationToken), cancellationToken);
    }

    public static WorkspaceTree Enumerate(
        FolderCapability folder,
        CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(folder);
        if (!folder.ValidateAccess(out var folderError))
        {
            throw new WorkspaceEnumerationException(folderError);
        }

        var rootPath = folder.CanonicalRoot.TrimEnd('\\', '/');
        var entries = new List<WorkspaceEntry>();
        var rootFirstChildIndex = -1;
        var truncated = false;
        var skippedInvalid = 0;
        var skippedSymlinks = 0;
        var totalMarkdownFiles = 0;
        ulong nextId = 1;

        var stack = new Stack<(string FullPath, string RelativePrefix, int Depth, int ParentIndex)>();
        stack.Push((rootPath, string.Empty, 0, -1));

        while (stack.Count > 0)
        {
            cancellationToken.ThrowIfCancellationRequested();
            if (folder.IsRevoked)
            {
                throw new WorkspaceEnumerationException(FolderError.AccessRevoked);
            }

            var (fullDirPath, relativePrefix, depth, parentIndex) = stack.Pop();
            if (depth >= MaxDepth)
            {
                truncated = true;
                continue;
            }

            IEnumerable<FileSystemInfo> childrenEnum;
            try
            {
                childrenEnum = new DirectoryInfo(fullDirPath).EnumerateFileSystemInfos();
            }
            catch (UnauthorizedAccessException)
            {
                continue;
            }
            catch (DirectoryNotFoundException)
            {
                if (parentIndex == -1)
                {
                    throw new WorkspaceEnumerationException(FolderError.NotFound);
                }
                continue;
            }
            catch (IOException)
            {
                continue;
            }

            var levelItems = new List<(string FullPath, string RelativePath, string Name, string SortKey, bool IsDir, WorkspacePathClassification Classification)>();

            try
            {
                foreach (var info in childrenEnum)
                {
                    cancellationToken.ThrowIfCancellationRequested();
                    if ((info.Attributes & FileAttributes.ReparsePoint) != 0)
                    {
                        skippedSymlinks++;
                        continue;
                    }

                    var isDir = (info.Attributes & FileAttributes.Directory) != 0;
                    var relativePath = string.IsNullOrEmpty(relativePrefix)
                        ? info.Name
                        : $"{relativePrefix}/{info.Name}";

                    var classification = CoreWorkspace.ClassifyPath(relativePath);
                    if (!classification.IsValid)
                    {
                        skippedInvalid++;
                        continue;
                    }

                    levelItems.Add((
                        info.FullName,
                        relativePath,
                        info.Name,
                        info.Name.ToLowerInvariant(),
                        isDir,
                        classification));
                }
            }
            catch (UnauthorizedAccessException)
            {
                continue;
            }
            catch (IOException)
            {
                continue;
            }

            if (levelItems.Count == 0)
            {
                continue;
            }

            levelItems.Sort(static (a, b) =>
            {
                if (a.IsDir != b.IsDir)
                {
                    return a.IsDir ? -1 : 1;
                }
                var cmp = string.CompareOrdinal(a.SortKey, b.SortKey);
                return cmp != 0 ? cmp : string.CompareOrdinal(a.Name, b.Name);
            });

            var firstChild = -1;
            var prevChild = -1;
            var visibleChildDirs = new List<(string FullPath, string RelativePrefix, int Depth, int ParentIndex)>();
            var hiddenChildDirs = new List<(string FullPath, string RelativePrefix, int Depth, int ParentIndex)>();

            foreach (var item in levelItems)
            {
                if (entries.Count >= MaxEntries)
                {
                    truncated = true;
                    break;
                }

                var isMarkdown = !item.IsDir && item.Classification.IsMarkdown;
                if (isMarkdown)
                {
                    totalMarkdownFiles++;
                }

                var entryIndex = entries.Count;
                var entry = new WorkspaceEntry
                {
                    Id = nextId++,
                    Index = entryIndex,
                    ParentIndex = parentIndex,
                    Depth = depth,
                    RelativePath = item.RelativePath,
                    Name = item.Name,
                    SortKey = item.SortKey,
                    IsDirectory = item.IsDir,
                    IsMarkdown = isMarkdown,
                    IsHidden = item.Classification.IsHidden,
                    HasInvisible = item.Classification.HasInvisible,
                };
                entries.Add(entry);

                if (firstChild == -1)
                {
                    firstChild = entryIndex;
                }
                if (prevChild != -1)
                {
                    entries[prevChild].NextSiblingIndex = entryIndex;
                }
                prevChild = entryIndex;

                if (item.IsDir)
                {
                    var childDir = (item.FullPath, item.RelativePath, depth + 1, entryIndex);
                    if (item.Classification.IsHidden)
                    {
                        hiddenChildDirs.Add(childDir);
                    }
                    else
                    {
                        visibleChildDirs.Add(childDir);
                    }
                }
            }

            if (parentIndex == -1)
            {
                rootFirstChildIndex = firstChild;
            }
            else if (firstChild != -1)
            {
                entries[parentIndex].FirstChildIndex = firstChild;
            }

            // Push hidden directories first (LIFO) so visible directories are traversed before hidden subtrees like .git.
            for (var i = hiddenChildDirs.Count - 1; i >= 0; i--)
            {
                stack.Push(hiddenChildDirs[i]);
            }
            for (var i = visibleChildDirs.Count - 1; i >= 0; i--)
            {
                stack.Push(visibleChildDirs[i]);
            }

            if (truncated)
            {
                break;
            }
        }

        return new WorkspaceTree(
            entries,
            rootFirstChildIndex,
            truncated,
            skippedInvalid,
            skippedSymlinks,
            totalMarkdownFiles);
    }

    public List<WorkspaceVisibleRow> BuildVisibleRows(
        IReadOnlySet<ulong> expandedDirectoryIds,
        bool showAllFiles,
        bool showHidden)
    {
        ArgumentNullException.ThrowIfNull(expandedDirectoryIds);
        var rows = new List<WorkspaceVisibleRow>();
        if (RootFirstChildIndex == -1)
        {
            return rows;
        }

        AppendVisibleSiblings(RootFirstChildIndex, expandedDirectoryIds, showAllFiles, showHidden, rows);
        return rows;
    }

    private void AppendVisibleSiblings(
        int firstChildIndex,
        IReadOnlySet<ulong> expandedDirectoryIds,
        bool showAllFiles,
        bool showHidden,
        List<WorkspaceVisibleRow> rows)
    {
        var visibleSiblings = new List<WorkspaceEntry>();
        for (var idx = firstChildIndex; idx != -1; idx = _entries[idx].NextSiblingIndex)
        {
            var entry = _entries[idx];
            if (!showHidden && entry.IsHidden)
            {
                continue;
            }
            if (!entry.IsDirectory && !showAllFiles && !entry.IsMarkdown)
            {
                continue;
            }
            visibleSiblings.Add(entry);
        }

        var sizeOfSet = visibleSiblings.Count;
        for (var i = 0; i < sizeOfSet; i++)
        {
            var entry = visibleSiblings[i];
            var isExpanded = entry.IsDirectory && expandedDirectoryIds.Contains(entry.Id);
            rows.Add(new WorkspaceVisibleRow(entry, isExpanded, i + 1, sizeOfSet));

            if (isExpanded && entry.FirstChildIndex != -1)
            {
                AppendVisibleSiblings(entry.FirstChildIndex, expandedDirectoryIds, showAllFiles, showHidden, rows);
            }
        }
    }
}
