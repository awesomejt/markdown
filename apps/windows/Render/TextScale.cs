namespace Markdown.Windows.Render;

/// <summary>
/// The text size steps of View > Actual Size / Make Text Bigger / Smaller and Settings (WIN-001F).
/// </summary>
public static class TextScale
{
    public static readonly double[] Steps = [0.75, 0.85, 1.0, 1.15, 1.3, 1.5, 1.75, 2.0, 2.5];
    public const double Standard = 1.0;

    public static double Clamped(double value)
    {
        if (double.IsNaN(value) || double.IsInfinity(value))
        {
            return Standard;
        }

        var closest = Steps[0];
        var minDiff = Math.Abs(value - closest);
        for (var i = 1; i < Steps.Length; i++)
        {
            var diff = Math.Abs(value - Steps[i]);
            if (diff < minDiff)
            {
                minDiff = diff;
                closest = Steps[i];
            }
        }
        return closest;
    }

    public static double? Larger(double value)
    {
        var current = Clamped(value);
        foreach (var step in Steps)
        {
            if (step > current + 0.001)
            {
                return step;
            }
        }
        return null;
    }

    public static double? Smaller(double value)
    {
        var current = Clamped(value);
        for (var i = Steps.Length - 1; i >= 0; i--)
        {
            if (Steps[i] < current - 0.001)
            {
                return Steps[i];
            }
        }
        return null;
    }
}
