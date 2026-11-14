using Markdown.Windows.Core;

namespace Markdown.Windows.Render;

/// <summary>
/// A reading position in rendered text: the document position (fragment and character offset)
/// at the top of the view, and the optional text selection.
/// </summary>
public sealed record ReadingPosition(DocumentPosition Top, DocumentSelection? Selection)
{
    public static ReadingPosition Default => new(new DocumentPosition(0, 0), null);
}

/// <summary>
/// A reading position saved as core block anchors (specs/session), which survive external edits,
/// reloads, and application relaunches where rendered indices and character offsets do not.
/// </summary>
public sealed record SavedPosition(byte[] Top, byte[] SelectionStart, byte[]? SelectionEnd);
