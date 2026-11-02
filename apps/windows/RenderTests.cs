using System.Diagnostics;
using System.IO;
using System.Text;
using Markdown.Windows.Core;
using Markdown.Windows.Document;
using Markdown.Windows.Render;

namespace Markdown.Windows;

internal static class RenderTests
{
    public static int RunAll()
    {
        try
        {
            CoreRuntime.Validate();

            var tests = new (string Name, Action Run)[]
            {
                ("inline_lists_code_html_and_links", InlineListsCodeHtmlAndLinks),
                ("unicode_fragments_styles_budget_and_cancellation", UnicodeFragmentsStylesBudgetAndCancellation),
                ("corpus_outlines_match_published_core_and_expected_contracts", CorpusOutlinesMatchPublishedCoreAndExpectedContracts),
                ("list_only_and_empty_items_keep_markers", ListOnlyAndEmptyItemsKeepMarkers),
                ("nul_and_control_chars_become_visible_replacement", NulAndControlCharsBecomeVisibleReplacement),
                ("soft_breaks_flow_as_spaces_and_hard_breaks_preserve_newlines", SoftBreaksFlowAsSpacesAndHardBreaksPreserveNewlines),
                ("long_heading_keeps_one_semantic_start_and_bounded_styled_continuations", LongHeadingKeepsOneSemanticStartAndBoundedStyledContinuations),
                ("mixed_case_source_schemes_use_canonical_target_and_token_resolution", MixedCaseSourceSchemesUseCanonicalTargetAndTokenResolution),
                ("plan_barrier_replacement_and_close_suppress_publication", PlanBarrierReplacementAndCloseSuppressPublication),
            };

            foreach (var (name, run) in tests)
            {
                run();
                AssertNoLiveHandles(name);
                var callbackEx = CoreRuntime.TakeLastCallbackException();
                Assert(callbackEx is null, $"{name}: unexpected worker callback exception: {callbackEx}");
                Console.WriteLine($"  PASS {name}");
            }

            Console.WriteLine($"RenderTests: all {tests.Length} tests passed.");
            return 0;
        }
        catch (Exception ex)
        {
            Console.Error.WriteLine($"RenderTests FAILED: {ex}");
            return 1;
        }
    }

    private static CoreDocument Document(string source)
    {
        using var session = new CoreSession("render-test");
        var res = session.ParseBlocking(source, null, copyUnchanged: true);
        session.Close();
        return res.Document ?? throw new InvalidOperationException("failed to parse document");
    }

    private static RenderPlan Plan(string source) =>
        RenderPlan.Build(Document(source), () => false);

    private static void InlineListsCodeHtmlAndLinks()
    {
        var p = Plan(
            "# Héllo\n\n[go](#héllo) **bold *both*** ~~gone~~ `a<&`  \nnext\n\n3. first\n   - nested\n\n   second paragraph\n4. last\n\n```rust\n<&>\n```\n\n<script>alert(1)</script>\n\n![image alt](image.png)\n\n[external](https://example.com/a) [mail](mailto:a@example.com) [relative](other.md) [unsafe](javascript:alert) [missing](#absent)\n");

        var text = string.Join("\n", p.Fragments.Select(f => f.Text));
        Assert(text.Contains("3. first"), "missing ordered list item 3");
        Assert(text.Contains("4. last"), "missing ordered list item 4");
        Assert(text.Contains("• nested"), "missing bullet list item");
        Assert(text.Contains("second paragraph"), "missing second paragraph");
        Assert(text.Contains("<script>alert(1)</script>"), "raw html must remain source fallback");
        Assert(text.Contains("image alt"), "image alt text must be present");
        Assert(p.Fragments.Any(f => f.Kind == FragmentKind.Code && f.Text == "<&>"), "code block content missing");

        var allRuns = p.Fragments.SelectMany(f => f.Runs).ToList();
        Assert(allRuns.Any(r => (r.Style & (RunStyle.Strong | RunStyle.Emphasis)) == (RunStyle.Strong | RunStyle.Emphasis) && r.Text.Contains("both")),
            "bold italic combined style missing");
        Assert(allRuns.Any(r => (r.Style & RunStyle.Strikethrough) != 0 && r.Text.Contains("gone")),
            "strikethrough style missing");
        Assert(allRuns.Any(r => (r.Style & RunStyle.Code) != 0 && r.Text.Contains("a<&")),
            "code span style missing");

        // Targets policy
        Assert(p.Targets.Count >= 5, "targets count too low");
        Assert(p.Targets[0].Kind == TargetKind.Fragment && p.Targets[0].Fragment == 0, "fragment link to first heading failed");
        Assert(p.Targets.Any(t => t.Kind == TargetKind.External && t.Uri != null && t.Uri.StartsWith("https:")), "https external link missing");
        Assert(p.Targets.Any(t => t.Kind == TargetKind.External && t.Uri != null && t.Uri.StartsWith("mailto:")), "mailto external link missing");

        // Relative, unsafe (javascript:), and missing anchor links must be Inert
        var inertCount = p.Targets.Count(t => t.Kind == TargetKind.Inert);
        Assert(inertCount == 3, $"expected exactly 3 inert targets, got {inertCount}");

        // Token target verification
        var (fragIdx, fragment) = p.Fragments
            .Select((f, idx) => (idx, f))
            .First(pair => pair.f.HasLink(0));

        Assert(p.TargetFor(fragIdx, 0)?.Kind == TargetKind.Fragment, "TargetFor valid link failed");
        Assert(p.TargetFor(fragIdx, 999) == null, "TargetFor out-of-range link must return null");
        Assert(p.TargetFor(999, 0) == null, "TargetFor out-of-range fragment must return null");
    }

    private static void UnicodeFragmentsStylesBudgetAndCancellation()
    {
        var textCases = new[]
        {
            string.Concat(Enumerable.Repeat("文🦀", 2000)),
            string.Concat(Enumerable.Repeat("word 文 🦀 ", 2000)) + "end",
        };

        foreach (var text in textCases)
        {
            var p = Plan($"[**{text}**](https://example.com)");
            var reconstructed = string.Concat(p.Fragments.Select(f => f.Text));
            Assert(reconstructed == text, "reconstructed unicode text mismatch");

            foreach (var (idx, f) in p.Fragments.Select((frag, i) => (i, frag)))
            {
                Assert(!string.IsNullOrEmpty(f.Text), "fragment text must not be empty");
                Assert(f.Utf8Length <= RenderPlan.MaxFragmentBytes, $"fragment exceeds {RenderPlan.MaxFragmentBytes} bytes: {f.Utf8Length}");
                Assert(f.Runs.All(r => (r.Style & RunStyle.Strong) != 0 && r.Link == 0), "run style or link mismatch");
                Assert(p.TargetFor(idx, 0) != null, "target resolution for unicode fragment failed");
            }
        }

        var doc = Document(string.Concat(Enumerable.Repeat("word ", 2000)));

        // Budget exhaustion
        try
        {
            _ = RenderPlan.Build(doc, () => false, budget: 128);
            throw new InvalidOperationException("expected RenderException for budget exhaustion");
        }
        catch (RenderException ex)
        {
            Assert(ex.Kind == RenderErrorKind.Budget, $"expected Budget error, got {ex.Kind}");
        }

        // Cancellation
        var calls = 0;
        try
        {
            _ = RenderPlan.Build(doc, () => ++calls > 30);
            throw new InvalidOperationException("expected RenderException for cancellation");
        }
        catch (RenderException ex)
        {
            Assert(ex.Kind == RenderErrorKind.Cancelled, $"expected Cancelled error, got {ex.Kind}");
        }
    }

    private static void CorpusOutlinesMatchPublishedCoreAndExpectedContracts()
    {
        var repoRoot = FindRepoRoot();
        if (repoRoot is null)
        {
            Console.WriteLine("  SKIP corpus_outlines_match_published_core_and_expected_contracts (repo fixtures not found)");
            return;
        }
        var root = Path.Combine(repoRoot, "fixtures");
        var cases = new[]
        {
            ("foundation", "gfm-tour"),
            ("foundation", "code-fences"),
            ("foundation", "unsafe-html"),
            ("advanced", "inline-diagrams"),
            ("advanced", "voice-parity"),
            ("advanced", "math-alerts"),
            ("paths", "unicode-filename"),
            ("paths", "nfd-filename"),
        };

        foreach (var (dir, name) in cases)
        {
            var sourcePath = Path.Combine(root, "markdown", dir, $"{name}.md");
            var source = File.ReadAllText(sourcePath);
            var d = Document(source);
            var p = RenderPlan.Build(d, () => false);

            var expectedPath = Path.Combine(root, "expected", $"{name}.yaml");
            var expectedYaml = File.ReadAllText(expectedPath);

            Assert(p.Outline.Count == d.OutlineCount, $"outline count mismatch for {name}: {p.Outline.Count} vs {d.OutlineCount}");

            for (var i = 0; i < p.Outline.Count; i++)
            {
                var entry = p.Outline[i];
                ref readonly var core = ref d.Outline[i];
                var coreTitle = d.OutlineTitle(i);
                var coreAnchor = d.OutlineAnchor(i);

                Assert(entry.Title == coreTitle, $"title mismatch in {name}: {entry.Title} vs {coreTitle}");
                Assert(entry.Anchor == coreAnchor, $"anchor mismatch in {name}: {entry.Anchor} vs {coreAnchor}");
                Assert(entry.Level == (int)core.level, $"level mismatch in {name}: {entry.Level} vs {core.level}");
                Assert(expectedYaml.Contains(entry.Anchor, StringComparison.Ordinal), $"anchor not found in expected yaml: {entry.Anchor}");
                Assert(p.Fragments[entry.Fragment].HeadingLevel == entry.Level, $"fragment heading level mismatch in {name}");
            }

            Assert(p.Fragments.All(f => f.Utf8Length <= RenderPlan.MaxFragmentBytes), $"fragment length overflow in {name}");
        }
    }

    private static void ListOnlyAndEmptyItemsKeepMarkers()
    {
        var p = Plan("1. - nested\n2.\n");
        var text = string.Join("\n", p.Fragments.Select(f => f.Text));
        Assert(text.Contains("1. "), "missing ordered item 1 marker");
        Assert(text.Contains("2. "), "missing ordered item 2 marker");
        Assert(text.Contains("• nested"), "missing nested item marker");
    }

    private static void NulAndControlCharsBecomeVisibleReplacement()
    {
        var p = Plan("```\ncode \0<&>\u0001\n```\n\n<div>raw \0\u0001</div>\n");
        var text = string.Concat(p.Fragments.Select(f => f.Text));
        Assert(!text.Contains('\0'), "NUL character must not be present in output");
        Assert(!text.Contains('\u0001'), "C0 control character must not be present in output");
        Assert(text.Contains('\uFFFD'), "U+FFFD replacement character must be present");
    }

    private static void SoftBreaksFlowAsSpacesAndHardBreaksPreserveNewlines()
    {
        var p = Plan("ordinary\nwrapped\n\nexplicit  \nbreak\n\nbackslash\\\nbreak\n");
        var texts = p.Fragments.Select(f => f.Text).ToList();
        Assert(texts.Contains("ordinary wrapped"), "soft break must flow as space");
        Assert(texts.Contains("explicit\nbreak"), "hard break with two trailing spaces must preserve newline");
        Assert(texts.Contains("backslash\nbreak"), "hard break with backslash must preserve newline");
    }

    private static void LongHeadingKeepsOneSemanticStartAndBoundedStyledContinuations()
    {
        var title = string.Concat(Enumerable.Repeat("word 文 ", 220)) + "end";
        var p = Plan($"## {title}\n");
        Assert(p.Fragments.Count > 2, "long heading should split into multiple fragments");
        var reconstructed = string.Concat(p.Fragments.Select(f => f.Text));
        Assert(reconstructed == title, "reconstructed long heading title mismatch");
        Assert(p.Fragments.All(f => f.HeadingLevel == 2 && f.Utf8Length <= RenderPlan.MaxFragmentBytes),
            "all heading fragments must have HeadingLevel=2 and bounded byte length");
        Assert(p.Fragments.Count(f => f.HeadingStart) == 1, "exactly one fragment must have HeadingStart=true");
        Assert(p.Fragments[0].HeadingStart, "first fragment must have HeadingStart=true");
        Assert(p.Outline.Count == 1, "outline should contain exactly 1 item");
        Assert(p.Outline[0].Fragment == 0, "outline should point to first fragment");
    }

    private static void MixedCaseSourceSchemesUseCanonicalTargetAndTokenResolution()
    {
        var doc = Document(
            "[secure](HTTPS://example.com/a) [web](HtTp://example.com/b) [mail](MAILTO:Person@example.com) [mixed](MailTo:Other@example.com)");
        var p = RenderPlan.Build(doc, () => false);
        Assert(p.Targets.Count == 4, $"expected 4 targets, got {p.Targets.Count}");

        for (var i = 0; i < p.Targets.Count; i++)
        {
            var target = p.Targets[i];
            var canonical = doc.LinkTarget(i);
            Assert(target.Kind == TargetKind.External && target.Uri == canonical,
                $"expected External target with canonical URI {canonical}, got {target.Uri}");
            Assert(new[] { "https:", "http:", "mailto:" }.Any(scheme => canonical.StartsWith(scheme, StringComparison.Ordinal)),
                $"canonical URI must start with valid scheme: {canonical}");
        }
    }

    private static void PlanBarrierReplacementAndCloseSuppressPublication()
    {
        var tempDir = Path.Combine(Path.GetTempPath(), $"markdown-win001d-tests-{Guid.NewGuid():N}");
        Directory.CreateDirectory(tempDir);
        try
        {
            var path = Path.Combine(tempDir, "plan-barrier.md");
            File.WriteAllText(path, "# Barrier Doc\n\nBody.\n", new UTF8Encoding(false));

            using var enteredBarrier = new ManualResetEventSlim(false);
            using var releaseBarrier = new ManualResetEventSlim(false);

            var loader = new DocumentLoader();
            loader.PlanBarrier = () =>
            {
                enteredBarrier.Set();
                releaseBarrier.Wait(TimeSpan.FromSeconds(5));
            };

            loader.Open(DocumentCapability.FromPickerPath(path));
            Assert(enteredBarrier.Wait(TimeSpan.FromSeconds(5)), "timed out waiting for plan barrier");

            // Close while background preparation is blocked at plan barrier
            loader.CloseAndWait();
            releaseBarrier.Set();

            Thread.Sleep(25);
            Assert(!loader.Poll(), "closed loader must not publish results on Poll");
            Assert(loader.Document is null && loader.Plan is null && loader.Error is null && !loader.HasFile,
                "closed loader must remain completely empty");
        }
        finally
        {
            try
            {
                if (Directory.Exists(tempDir))
                {
                    Directory.Delete(tempDir, recursive: true);
                }
            }
            catch
            {
                // Best-effort cleanup
            }
        }
    }

    private static string? FindRepoRoot()
    {
        if (Environment.GetEnvironmentVariable("MARKDOWN_REPO_ROOT") is { Length: > 0 } envRoot &&
            Directory.Exists(envRoot))
        {
            return envRoot;
        }

        var dir = new DirectoryInfo(AppContext.BaseDirectory);
        while (dir is not null)
        {
            if (File.Exists(Path.Combine(dir.FullName, "Cargo.toml")) &&
                Directory.Exists(Path.Combine(dir.FullName, "core", "include")))
            {
                return dir.FullName;
            }
            dir = dir.Parent;
        }
        var cwd = new DirectoryInfo(Directory.GetCurrentDirectory());
        while (cwd is not null)
        {
            if (File.Exists(Path.Combine(cwd.FullName, "Cargo.toml")) &&
                Directory.Exists(Path.Combine(cwd.FullName, "core", "include")))
            {
                return cwd.FullName;
            }
            cwd = cwd.Parent;
        }
        return null;
    }

    private static void AssertNoLiveHandles(string context)
    {
        var sw = Stopwatch.StartNew();
        while (sw.ElapsedMilliseconds < 2000)
        {
            var counts = CoreRuntime.LiveHandles();
            if (counts.Sessions == 0 && counts.Jobs == 0 && counts.Snapshots == 0)
            {
                return;
            }
            Thread.Sleep(5);
        }
        var finalCounts = CoreRuntime.LiveHandles();
        Assert(
            finalCounts.Sessions == 0 && finalCounts.Jobs == 0 && finalCounts.Snapshots == 0,
            $"{context}: leaked core handles {finalCounts}");
    }

    private static void Assert([System.Diagnostics.CodeAnalysis.DoesNotReturnIf(false)] bool condition, string message)
    {
        if (!condition)
        {
            throw new InvalidOperationException(message);
        }
    }
}
