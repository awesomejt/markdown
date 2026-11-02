namespace Markdown.Windows.Render;

/// <summary>
/// A lightweight count-only items source for the virtualized document ListView.
/// The list queries indices; container instances are realized on demand.
/// </summary>
public sealed partial class IndexList(int count) : System.Collections.IList
{
    public object? this[int index]
    {
        get => index >= 0 && index < count ? index : throw new ArgumentOutOfRangeException(nameof(index));
        set => throw new NotSupportedException();
    }

    public int Count => count;
    public bool IsFixedSize => true;
    public bool IsReadOnly => true;
    public bool IsSynchronized => false;
    public object SyncRoot => this;

    public int Add(object? value) => throw new NotSupportedException();
    public void Clear() => throw new NotSupportedException();
    public bool Contains(object? value) => value is int i && i >= 0 && i < count;
    public int IndexOf(object? value) => value is int i && i >= 0 && i < count ? i : -1;
    public void Insert(int index, object? value) => throw new NotSupportedException();
    public void Remove(object? value) => throw new NotSupportedException();
    public void RemoveAt(int index) => throw new NotSupportedException();

    public void CopyTo(Array array, int index)
    {
        for (var i = 0; i < count; i++)
        {
            array.SetValue(i, index + i);
        }
    }

    public System.Collections.IEnumerator GetEnumerator()
    {
        for (var i = 0; i < count; i++)
        {
            yield return i;
        }
    }
}
