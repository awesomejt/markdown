namespace Markdown.Windows.Render;

/// <summary>
/// A position within a rendered document: fragment index and character offset within that fragment.
/// </summary>
public readonly record struct DocumentPosition(int FragmentIndex, int CharOffset) : IComparable<DocumentPosition>
{
    public int CompareTo(DocumentPosition other)
    {
        var cmp = FragmentIndex.CompareTo(other.FragmentIndex);
        return cmp != 0 ? cmp : CharOffset.CompareTo(other.CharOffset);
    }

    public static bool operator <(DocumentPosition left, DocumentPosition right) => left.CompareTo(right) < 0;
    public static bool operator <=(DocumentPosition left, DocumentPosition right) => left.CompareTo(right) <= 0;
    public static bool operator >(DocumentPosition left, DocumentPosition right) => left.CompareTo(right) > 0;
    public static bool operator >=(DocumentPosition left, DocumentPosition right) => left.CompareTo(right) >= 0;
}

/// <summary>
/// An active document selection defined by anchor and active positions.
/// </summary>
public readonly record struct DocumentSelection(DocumentPosition Anchor, DocumentPosition Active)
{
    public DocumentPosition Start => Anchor <= Active ? Anchor : Active;
    public DocumentPosition End => Anchor <= Active ? Active : Anchor;

    public bool IsEmpty => Start == End;
}
