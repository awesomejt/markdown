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
}
