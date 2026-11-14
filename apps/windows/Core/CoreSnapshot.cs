using System.Runtime.InteropServices;

namespace Markdown.Windows.Core;

public enum AnchorQuality : uint
{
    Lost = 0,
    Exact = 1,
    Relocated = 2,
    Nearest = 3,
}

public readonly record struct CoreAnchorResolution(AnchorQuality Quality, uint Node, ulong SourceOffset);

public enum CoreChangeDisposition : uint
{
    Unchanged = 1,
    ReadRequired = 2,
    Pending = 3,
}

/// <summary>
/// One published snapshot handle, kept with the copied document so a reading position
/// can be anchored and resolved after the session has moved on. The last release runs
/// on the markdown-core worker.
/// </summary>
public sealed unsafe class CoreSnapshot : IDisposable
{
    private ulong _handle;

    public ulong Handle => Volatile.Read(ref _handle);

    internal CoreSnapshot(ulong handle)
    {
        _handle = handle;
    }

    ~CoreSnapshot()
    {
        ReleaseWorkerNonBlocking();
    }

    public void Dispose()
    {
        ReleaseWorkerNonBlocking();
        GC.SuppressFinalize(this);
    }

    private void ReleaseWorkerNonBlocking()
    {
        var h = Interlocked.Exchange(ref _handle, 0);
        if (h != 0)
        {
            CoreRuntime.EnqueueWork(() =>
            {
                _ = CoreFfi.md_snapshot_release(h);
            });
        }
    }

    public byte[] AnchorAt(ulong sourceOffset)
    {
        var h = Handle;
        if (h == 0)
        {
            throw CoreException.InvalidSnapshot("snapshot handle released");
        }

        var buffer = new byte[CoreFfi.MD_ANCHOR_MAX_BYTES];
        nuint outLength = 0;
        uint status;
        fixed (byte* ptr = buffer)
        {
            status = CoreFfi.md_snapshot_anchor_at(h, sourceOffset, ptr, (nuint)buffer.Length, &outLength);
        }

        if (status == CoreFfi.MD_ERR_NO_BLOCKS)
        {
            throw CoreException.NoBlocks();
        }
        if (status != CoreFfi.MD_OK)
        {
            throw CoreException.FromStatus("md_snapshot_anchor_at", status);
        }

        var result = new byte[(int)Math.Min((ulong)outLength, (ulong)buffer.Length)];
        buffer.AsSpan(0, result.Length).CopyTo(result);
        return result;
    }

    public byte[] AnchorForNode(uint node, ulong offset = 0)
    {
        var h = Handle;
        if (h == 0)
        {
            throw CoreException.InvalidSnapshot("snapshot handle released");
        }

        var buffer = new byte[CoreFfi.MD_ANCHOR_MAX_BYTES];
        nuint outLength = 0;
        uint status;
        fixed (byte* ptr = buffer)
        {
            status = CoreFfi.md_snapshot_anchor_for_node(h, node, offset, ptr, (nuint)buffer.Length, &outLength);
        }

        if (status == CoreFfi.MD_ERR_NO_BLOCKS)
        {
            throw CoreException.NoBlocks();
        }
        if (status != CoreFfi.MD_OK)
        {
            throw CoreException.FromStatus("md_snapshot_anchor_for_node", status);
        }

        var result = new byte[(int)Math.Min((ulong)outLength, (ulong)buffer.Length)];
        buffer.AsSpan(0, result.Length).CopyTo(result);
        return result;
    }

    public CoreAnchorResolution? ResolveAnchor(ReadOnlySpan<byte> anchor)
    {
        var h = Handle;
        if (h == 0)
        {
            throw CoreException.InvalidSnapshot("snapshot handle released");
        }

        var resolution = new CoreFfi.md_anchor_resolution
        {
            struct_size = (uint)sizeof(CoreFfi.md_anchor_resolution),
        };

        uint status;
        fixed (byte* ptr = anchor)
        {
            status = CoreFfi.md_snapshot_resolve_anchor(h, ptr, (nuint)anchor.Length, &resolution);
        }

        if (status == CoreFfi.MD_ERR_NO_BLOCKS)
        {
            return null;
        }
        if (status != CoreFfi.MD_OK)
        {
            throw CoreException.FromStatus("md_snapshot_resolve_anchor", status);
        }

        if (resolution.quality is < CoreFfi.MD_ANCHOR_EXACT or > CoreFfi.MD_ANCHOR_NEAREST)
        {
            return null;
        }

        return new CoreAnchorResolution(
            (AnchorQuality)resolution.quality,
            resolution.node,
            resolution.source_offset);
    }
}
