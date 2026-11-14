using System.Diagnostics;
using System.IO;
using Markdown.Windows.Document;
using Markdown.Windows.Render;

namespace Markdown.Windows;

/// <summary>
/// Opt-in verification of real state restoration (WIN-001H), launched
/// twice by tools/check.py.
/// 'prepare' opens the document, scrolls Part 80 to top with 'marked' selected,
/// saves the position to RestorationStore, and closes.
/// 'verify' loads RestorationStore, restores the window, verifies the reading position
/// and selection, clears the store, and exits cleanly.
/// </summary>
internal static class RelaunchCheck
{
    internal static async Task<int> RunAsync(string phase, string filePath, ReaderApp app, ReaderWindow window)
    {
        try
        {
            if (phase == "prepare")
            {
                if (!DocumentCapability.TryFromUserInput(filePath, out var cap, out var err) || cap is null)
                {
                    Console.Error.WriteLine($"relaunch-check: invalid file {filePath}: {err}");
                    return 1;
                }
                window.OpenCapability(cap);
                await WaitSettledAsync(window);

                var doc = window.Loader.Document ?? throw new InvalidOperationException("document is null");
                var headingIndex = -1;
                for (var i = 0; i < doc.OutlineCount; i++)
                {
                    if (doc.OutlineTitle(i) == "Part 80")
                    {
                        headingIndex = i;
                        break;
                    }
                }
                if (headingIndex < 0)
                {
                    throw new InvalidOperationException("missing Part 80 heading");
                }

                window.Reader.ActivateOutline(headingIndex);

                var plan = window.Loader.Plan ?? throw new InvalidOperationException("plan is null");
                var startFrag = plan.Outline[headingIndex].Fragment;
                var foundFrag = -1;
                var wordStart = -1;
                for (var f = startFrag; f < Math.Min(startFrag + 10, plan.Fragments.Count); f++)
                {
                    var idx = plan.Fragments[f].Text.IndexOf("marked", StringComparison.Ordinal);
                    if (idx >= 0)
                    {
                        foundFrag = f;
                        wordStart = idx;
                        break;
                    }
                }
                if (foundFrag < 0)
                {
                    throw new InvalidOperationException("missing 'marked' word");
                }

                window.Reader.SetSelection(new DocumentSelection(
                    new DocumentPosition(foundFrag, wordStart),
                    new DocumentPosition(foundFrag, wordStart + "marked".Length)));

                var pos = window.Reader.SavePosition(doc);
                if (pos is null)
                {
                    throw new InvalidOperationException("position not saved");
                }

                var state = WindowRestorationState.Create(
                    filePath,
                    pos,
                    window.Preferences.TextScaleValue,
                    outlineShown: true);

                RestorationStore.Save([state]);
                Console.WriteLine("relaunch-check: prepared");
                app.CloseAll();
                Program.Result = 0;
                return 0;
            }
            else if (phase == "verify")
            {
                var restored = RestorationStore.Load();
                if (restored.Count != 1)
                {
                    throw new InvalidOperationException($"expected 1 restored state, got {restored.Count}");
                }
                var state = restored[0];
                if (!string.Equals(Path.GetFullPath(state.FilePath), Path.GetFullPath(filePath), StringComparison.OrdinalIgnoreCase))
                {
                    throw new InvalidOperationException($"restored path mismatch: {state.FilePath} vs {filePath}");
                }

                if (window.Loader.Document is null)
                {
                    if (!DocumentCapability.TryFromUserInput(state.FilePath, out var cap, out _) || cap is null)
                    {
                        throw new InvalidOperationException($"failed to create capability from {state.FilePath}");
                    }
                    window.PendingRestorePosition = state.ToSavedPosition();
                    window.OpenCapability(cap);
                }

                await WaitSettledAsync(window);

                var doc = window.Loader.Document ?? throw new InvalidOperationException("restored document is null");
                var headingIndex = -1;
                for (var i = 0; i < doc.OutlineCount; i++)
                {
                    if (doc.OutlineTitle(i) == "Part 80")
                    {
                        headingIndex = i;
                        break;
                    }
                }
                if (headingIndex < 0)
                {
                    throw new InvalidOperationException("missing heading Part 80 in restored document");
                }

                var plan = window.Loader.Plan ?? throw new InvalidOperationException("restored plan is null");
                var targetFrag = plan.Outline[headingIndex].Fragment;
                var firstVisible = window.Reader.FirstVisibleFragmentIndex();

                if (Math.Abs(firstVisible - targetFrag) > 1)
                {
                    throw new InvalidOperationException($"top fragment mismatch: firstVisible={firstVisible}, targetFrag={targetFrag}");
                }

                var sel = window.Reader.Selection;
                if (sel is null || sel.Value.IsEmpty)
                {
                    throw new InvalidOperationException("restored selection is empty or null");
                }
                var selText = plan.TextBetween(sel.Value.Anchor, sel.Value.Active);
                if (selText != "marked")
                {
                    throw new InvalidOperationException($"restored selection text expected 'marked', got '{selText}'");
                }

                RestorationStore.Clear();
                Console.WriteLine("relaunch-check: pass (restored window, document, top line and selection)");
                app.CloseAll();
                Program.Result = 0;
                return 0;
            }
            else
            {
                Console.Error.WriteLine($"relaunch-check: unknown phase {phase}");
                return 2;
            }
        }
        catch (Exception ex)
        {
            Console.Error.WriteLine($"relaunch-check: FAIL: {ex.Message}");
            app.CloseAll();
            Program.Result = 1;
            return 1;
        }
    }

    private static async Task WaitSettledAsync(ReaderWindow window, int timeoutMs = 10000)
    {
        var sw = Stopwatch.StartNew();
        while (sw.ElapsedMilliseconds < timeoutMs)
        {
            window.SyncFromLoader();
            if (!window.Loader.IsLoading && window.Loader.Document is not null)
            {
                return;
            }
            await Task.Delay(20);
        }
        throw new TimeoutException("document did not settle in time");
    }
}
