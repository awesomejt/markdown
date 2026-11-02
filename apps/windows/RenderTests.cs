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
                ("tables_align_header_and_tsv_copy", TablesAlignHeaderAndTsvCopy),
                ("task_markers_and_accessible_text", TaskMarkersAndAccessibleText),
                ("images_follow_resource_policy_and_limits", ImagesFollowResourcePolicyAndLimits),
                ("document_resources_filesystem_security", DocumentResourcesFilesystemSecurity),
                ("text_scale_clamped_and_steps", TextScaleClampedAndSteps),
                ("preferences_memory_and_local_settings", PreferencesMemoryAndLocalSettings),
                ("document_find_unicode_mappings_and_wrapping", DocumentFindUnicodeMappingsAndWrapping),
                ("global_selection_and_text_between", GlobalSelectionAndTextBetween),
                ("native_clipboard_roundtrip", NativeClipboardRoundtrip),
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

    private static byte[] CreateBmp(int width, int height)
    {
        var data = new byte[54];
        data[0] = 0x42;
        data[1] = 0x4D;
        BitConverter.GetBytes(54).CopyTo(data, 2);
        BitConverter.GetBytes(54).CopyTo(data, 10);
        BitConverter.GetBytes(40).CopyTo(data, 14);
        BitConverter.GetBytes(width).CopyTo(data, 18);
        BitConverter.GetBytes(height).CopyTo(data, 22);
        BitConverter.GetBytes((short)1).CopyTo(data, 26);
        BitConverter.GetBytes((short)24).CopyTo(data, 28);
        return data;
    }

    internal static byte[] CreatePng(int width, int height)
    {
        using var ms = new MemoryStream();
        ms.Write([0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]);
        ms.Write([0x00, 0x00, 0x00, 0x0D]);
        ms.Write(Encoding.ASCII.GetBytes("IHDR"));
        var wBytes = BitConverter.GetBytes(width);
        if (BitConverter.IsLittleEndian) Array.Reverse(wBytes);
        ms.Write(wBytes);
        var hBytes = BitConverter.GetBytes(height);
        if (BitConverter.IsLittleEndian) Array.Reverse(hBytes);
        ms.Write(hBytes);
        ms.Write([8, 2, 0, 0, 0]);
        ms.Write([0, 0, 0, 0]);
        ms.Write([0, 0, 0, 0]);
        ms.Write(Encoding.ASCII.GetBytes("IEND"));
        ms.Write([0xAE, 0x42, 0x60, 0x82]);
        return ms.ToArray();
    }

    private static void TablesAlignHeaderAndTsvCopy()
    {
        var source =
            "| Name   | Type    | Score |\n" +
            "| :----- | :-----: | ----: |\n" +
            "| Alpha  | string  |    10 |\n" +
            "| Beta   | number  |   200 |\n" +
            "| Gamma  | boolean |     3 |\n";

        var p = Plan(source);
        var tableFragments = p.Fragments.Where(f => f.Kind == FragmentKind.TableRow).ToList();
        Assert(tableFragments.Count == 4, $"expected 4 table row fragments, got {tableFragments.Count}");

        Assert(tableFragments[0].IsHeader, "row 0 must be header");
        Assert(!tableFragments[1].IsHeader, "row 1 must not be header");
        Assert(!tableFragments[2].IsHeader, "row 2 must not be header");
        Assert(!tableFragments[3].IsHeader, "row 3 must not be header");

        Assert(tableFragments[0].Runs.Any(r => (r.Style & RunStyle.Strong) != 0 && r.Text.Contains("Score")),
            "header row cells must have Strong style");
        Assert(tableFragments[1].Runs.All(r => (r.Style & RunStyle.Strong) == 0),
            "body row cells must not have Strong style");

        var expectedRows = new[]
        {
            "Name\tType\tScore",
            "Alpha\tstring\t10",
            "Beta\tnumber\t200",
            "Gamma\tboolean\t3"
        };
        for (var i = 0; i < expectedRows.Length; i++)
        {
            Assert(tableFragments[i].Text == expectedRows[i],
                $"row {i} mismatch: expected '{expectedRows[i]}', got '{tableFragments[i].Text}'");
        }

        var shortTable = Plan("| # | Item |\n| -: | --- |\n| 1 | one |\n| 22 |\n");
        var shortRows = shortTable.Fragments.Where(f => f.Kind == FragmentKind.TableRow).ToList();
        Assert(shortRows.Count == 3, "expected 3 rows for short table");
        Assert(shortRows[2].Text == "22\t", $"expected padded short row '22\\t', got '{shortRows[2].Text}'");

        var quoted = Plan("> | q |\n> | - |\n> | x |\n");
        var quotedRows = quoted.Fragments.Where(f => f.Kind == FragmentKind.TableRow).ToList();
        Assert(quotedRows.Count == 2, "expected 2 rows in quoted table");
        Assert(quotedRows.All(r => r.Quote > 0), "quoted table rows must have quote depth > 0");
    }

    private static void TaskMarkersAndAccessibleText()
    {
        var p = Plan("- [x] done\n- [ ] open\n- plain\n");
        Assert(p.Fragments.Count == 3, $"expected 3 list fragments, got {p.Fragments.Count}");

        Assert(p.Fragments[0].Text == "☑ done", $"expected '☑ done', got '{p.Fragments[0].Text}'");
        Assert(p.Fragments[1].Text == "☐ open", $"expected '☐ open', got '{p.Fragments[1].Text}'");
        Assert(p.Fragments[2].Text == "• plain", $"expected '• plain', got '{p.Fragments[2].Text}'");

        var f0Marker = p.Fragments[0].Runs[0];
        Assert((f0Marker.Style & RunStyle.TaskChecked) != 0, "checked item must have TaskChecked style");

        var f1Marker = p.Fragments[1].Runs[0];
        Assert((f1Marker.Style & RunStyle.TaskUnchecked) != 0, "unchecked item must have TaskUnchecked style");

        var f2Marker = p.Fragments[2].Runs[0];
        Assert((f2Marker.Style & (RunStyle.TaskChecked | RunStyle.TaskUnchecked)) == 0, "plain item must not have task style");
    }

    private static void ImagesFollowResourcePolicyAndLimits()
    {
        var smallPng = CreatePng(64, 64);
        var widePng = CreatePng(2000, 100);
        var hugeBomb = CreatePng(20000, 20000);

        var images = new Dictionary<string, byte[]>(StringComparer.Ordinal)
        {
            ["small.png"] = smallPng,
            ["wide.png"] = widePng,
            ["huge.png"] = hugeBomb,
            ["vector.svg"] = Encoding.UTF8.GetBytes("<svg xmlns='http://www.w3.org/2000/svg'/>"),
            ["disguised.png"] = Encoding.UTF8.GetBytes("<?xml version='1.0'?><svg></svg>"),
            ["corrupt.png"] = Encoding.UTF8.GetBytes("not an image at all"),
        };

        var loadCount = 0;
        (byte[]? Data, ResourceError? Error) MockLoader(string path, int limit)
        {
            loadCount++;
            if (images.TryGetValue(path, out var data))
            {
                return (data, null);
            }
            if (path == "../up.png")
            {
                return (null, ResourceError.OutsideFolder);
            }
            return (null, ResourceError.NotFound);
        }

        var md =
            "![small](small.png)\n\n" +
            "![huge](huge.png)\n\n" +
            "![svg](vector.svg)\n\n" +
            "![disguised](disguised.png)\n\n" +
            "![corrupt](corrupt.png)\n\n" +
            "![up](../up.png)\n\n" +
            "![missing](missing.png)\n\n" +
            "![remote](https://example.com/pic.png)\n\n" +
            "![denied](data:image/png;base64,AAAA)\n";

        var doc = Document(md);
        var plan = RenderPlan.Build(doc, () => false, RenderPlan.DefaultBudget, MockLoader);



        Assert(plan.Images.Count == 9, $"expected 9 images in plan, got {plan.Images.Count}");
        Assert(plan.Images[0].Failure == null, "small.png should succeed");
        Assert(plan.Images[0].Width == 64 && plan.Images[0].Height == 64, "small.png dimensions mismatch");
        Assert(plan.Images[1].Failure == ResourceError.TooLarge, "huge bomb should fail with TooLarge");
        Assert(plan.Images[2].Failure == ResourceError.UnsupportedType, "vector.svg should fail with UnsupportedType");
        Assert(plan.Images[3].Failure == ResourceError.UnsupportedType, "disguised.png should fail with UnsupportedType");
        Assert(plan.Images[4].Failure == ResourceError.Undecodable, "corrupt.png should fail with Undecodable");
        Assert(plan.Images[5].Failure == ResourceError.OutsideFolder, "../up.png should fail with OutsideFolder");
        Assert(plan.Images[6].Failure == ResourceError.NotFound, "missing.png should fail with NotFound");
        Assert(plan.Images[7].Failure == ResourceError.Remote, "https image should fail with Remote");
        Assert(plan.Images[8].Failure == ResourceError.Blocked, "data: image should fail with Blocked");

        var budget = new ImageBudget { MaxImages = 3 };
        var multiMd = "![1](small.png) ![2](wide.png) ![3](i3.png) ![4](i4.png) ![5](i5.png)\n";
        var multiDoc = Document(multiMd);
        images["i3.png"] = smallPng;
        images["i4.png"] = smallPng;
        images["i5.png"] = smallPng;
        var budgetPlan = RenderPlan.Build(multiDoc, () => false, RenderPlan.DefaultBudget, MockLoader, budget);
        Assert(budgetPlan.Images.Count == 5, "expected 5 images in multi plan");
        Assert(budgetPlan.Images[0].Failure == null, "image 1 should succeed");
        Assert(budgetPlan.Images[1].Failure == null, "image 2 should succeed");
        Assert(budgetPlan.Images[2].Failure == null, "image 3 should succeed");
        Assert(budgetPlan.Images[3].Failure == ResourceError.Limit, "image 4 should fail with Limit");
        Assert(budgetPlan.Images[4].Failure == ResourceError.Limit, "image 5 should fail with Limit");

        var memBudget = new ImageBudget { MaxDecodedBytes = 64 * 64 * 4 * 2 };
        var memMd = "![w](wide.png) ![s1](s1.png) ![s2](s2.png) ![s3](s3.png)\n";
        images["s1.png"] = smallPng;
        images["s2.png"] = smallPng;
        images["s3.png"] = smallPng;
        var memPlan = RenderPlan.Build(Document(memMd), () => false, RenderPlan.DefaultBudget, MockLoader, memBudget);
        Assert(memPlan.Images[0].Failure == ResourceError.Limit, "wide image should exceed small memory budget");
        Assert(memPlan.Images[1].Failure == null, "s1 should fit");
        Assert(memPlan.Images[2].Failure == null, "s2 should fit");
        Assert(memPlan.Images[3].Failure == ResourceError.Limit, "s3 should exceed memory budget");

        loadCount = 0;
        var dedupMd = "![first](small.png)\n\n![second](small.png)\n";
        var dedupPlan = RenderPlan.Build(Document(dedupMd), () => false, RenderPlan.DefaultBudget, MockLoader);
        Assert(dedupPlan.Images.Count == 2, "expected 2 image occurrences");
        Assert(loadCount == 1, $"expected exactly 1 loader call for duplicate image, got {loadCount}");
        Assert(dedupPlan.Images[0].Failure == null && dedupPlan.Images[1].Failure == null, "both occurrences should succeed");

        // Mixed paragraph with text, link, and multiple images
        var mixedMd = "See ![first](small.png) for details and [docs](#test) plus ![second](wide.png)\n";
        var mixedPlan = RenderPlan.Build(Document(mixedMd), () => false, RenderPlan.DefaultBudget, MockLoader);
        var mixedFrag = mixedPlan.Fragments.First(f => f.Text.Contains("See"));
        Assert(mixedFrag.Images.Count == 2, $"expected 2 images in mixed paragraph, got {mixedFrag.Images.Count}");
        Assert(mixedFrag.Runs.Any(r => r.Text == "docs" && r.Link != RenderRun.NoLink), "mixed paragraph must retain clickable link");
        Assert(mixedFrag.Text.Contains("See ") && mixedFrag.Text.Contains(" for details and "), "mixed paragraph must retain text runs");

        // Image in table cell must not leak into subsequent paragraph
        var tableWithImgMd = "| Header |\n| --- |\n| ![cell](small.png) |\n\nNext paragraph text\n";
        var tablePlan = RenderPlan.Build(Document(tableWithImgMd), () => false, RenderPlan.DefaultBudget, MockLoader);
        var nextFrag = tablePlan.Fragments.First(f => f.Text.Contains("Next paragraph"));
        Assert(nextFrag.Images.Count == 0, "table cell image must not leak into subsequent paragraph");

        // MaxPixelSide (2048) enforcement: 2049x10 (under 32 MP, but side > 2048)
        var longSidePng = CreatePng(2049, 10);
        images["longside.png"] = longSidePng;
        var sidePlan = RenderPlan.Build(Document("![side](longside.png)\n"), () => false, RenderPlan.DefaultBudget, MockLoader);
        Assert(sidePlan.Images[0].Failure == ResourceError.TooLarge, "image with side > 2048 must fail with TooLarge");

        // Crafted BMP with int.MinValue height must not throw OverflowException
        var minBmp = CreateBmp(100, int.MinValue);
        var bmpOk = ImageFormatInspector.Inspect(minBmp, 32L * 1024 * 1024, 2048, out _, out _, out var bmpErr);
        Assert(!bmpOk && bmpErr == ResourceError.TooLarge, "crafted BMP with int.MinValue height must fail with TooLarge without throwing");


    }

    private static void DocumentResourcesFilesystemSecurity()
    {
        var tempDir = Path.Combine(Path.GetTempPath(), $"markdown-resources-test-{Guid.NewGuid():N}");
        Directory.CreateDirectory(tempDir);
        var subDir = Path.Combine(tempDir, "sub");
        Directory.CreateDirectory(subDir);
        var outsideDir = Path.Combine(Path.GetTempPath(), $"markdown-outside-test-{Guid.NewGuid():N}");
        Directory.CreateDirectory(outsideDir);

        try
        {
            var validPng = CreatePng(32, 32);
            var testPngPath = Path.Combine(tempDir, "test.png");
            File.WriteAllBytes(testPngPath, validPng);

            var subPngPath = Path.Combine(subDir, "sub.png");
            File.WriteAllBytes(subPngPath, validPng);

            var outsidePngPath = Path.Combine(outsideDir, "outside.png");
            File.WriteAllBytes(outsidePngPath, validPng);

            var svgPath = Path.Combine(tempDir, "vector.svg");
            File.WriteAllText(svgPath, "<svg xmlns='http://www.w3.org/2000/svg'/>");

            var fakePngPath = Path.Combine(tempDir, "fake.png");
            File.WriteAllText(fakePngPath, "this is not a png");

            var dirAsPngPath = Path.Combine(tempDir, "dir.png");
            Directory.CreateDirectory(dirAsPngPath);

            var docPath = Path.Combine(tempDir, "doc.md");
            File.WriteAllText(docPath, "# Test\n");

            var capability = DocumentCapability.FromPickerPath(docPath);
            var resources = new DocumentResources(capability);

            var (d1, e1) = resources.ReadImage("test.png");
            Assert(e1 == null && d1 != null && d1.Length > 0, "test.png should succeed");

            var (d2, e2) = resources.ReadImage("sub/sub.png");
            Assert(e2 == null && d2 != null && d2.Length > 0, "sub/sub.png should succeed");

            var (d3, e3) = resources.ReadImage("../outside.png");
            Assert(e3 == ResourceError.OutsideFolder, "escaped path must fail with OutsideFolder");

            var (d4, e4) = resources.ReadImage("/test.png");
            Assert(e4 == ResourceError.OutsideFolder, "absolute path must fail with OutsideFolder");

            var (d5, e5) = resources.ReadImage("C:test.png");
            Assert(e5 == ResourceError.OutsideFolder, "drive path must fail with OutsideFolder");

            var (d6, e6) = resources.ReadImage("dir.png");
            Assert(e6 == ResourceError.NotRegularFile, "directory must fail with NotRegularFile");

            var (d7, e7) = resources.ReadImage("vector.svg");
            Assert(e7 == ResourceError.UnsupportedType, "svg file must fail with UnsupportedType");

            var (d8, e8) = resources.ReadImage("fake.png");
            Assert(e8 == ResourceError.Undecodable, "fake png must fail with Undecodable");

            var (dIco, eIco) = resources.ReadImage("icon.ico");
            Assert(eIco == ResourceError.UnsupportedType, "ico file must fail with UnsupportedType");

            var (d9, e9) = resources.ReadImage("missing.png");
            Assert(e9 == ResourceError.NotFound, "missing file must fail with NotFound");

            var (d10, e10) = resources.ReadImage("test.png", byteLimit: 10);
            Assert(e10 == ResourceError.TooLarge, "file exceeding byteLimit must fail with TooLarge");

            // Directory junction escape test (via mklink /J)
            var juncDir = Path.Combine(tempDir, "junc");
            var mklink = System.Diagnostics.Process.Start(new System.Diagnostics.ProcessStartInfo
            {
                FileName = "cmd.exe",
                Arguments = $"/c mklink /J \"{juncDir}\" \"{outsideDir}\"",
                CreateNoWindow = true,
                UseShellExecute = false,
            });
            mklink?.WaitForExit();
            Assert(Directory.Exists(juncDir), "junction directory creation must succeed");
            var (dj, ej) = resources.ReadImage("junc/outside.png");
            Assert(ej == ResourceError.OutsideFolder, "junction escape outside document folder must fail with OutsideFolder");
        }
        finally
        {
            try { Directory.Delete(tempDir, recursive: true); } catch { }
            try { Directory.Delete(outsideDir, recursive: true); } catch { }
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

    private static void TextScaleClampedAndSteps()
    {
        Assert(Math.Abs(TextScale.Clamped(1.0) - 1.0) < 0.001, "clamped standard");
        Assert(Math.Abs(TextScale.Clamped(1.04) - 1.0) < 0.001, "clamped near 1.0");
        Assert(Math.Abs(TextScale.Clamped(9.0) - TextScale.Steps[^1]) < 0.001, "clamped max");
        Assert(Math.Abs(TextScale.Clamped(-1.0) - TextScale.Steps[0]) < 0.001, "clamped min");
        Assert(Math.Abs(TextScale.Clamped(double.NaN) - TextScale.Standard) < 0.001, "clamped NaN");
        Assert(Math.Abs(TextScale.Clamped(double.PositiveInfinity) - TextScale.Standard) < 0.001, "clamped inf");

        var larger = TextScale.Larger(1.0);
        Assert(larger.HasValue && Math.Abs(larger.Value - 1.15) < 0.001, "larger step");
        var smaller = TextScale.Smaller(1.0);
        Assert(smaller.HasValue && Math.Abs(smaller.Value - 0.85) < 0.001, "smaller step");

        Assert(TextScale.Larger(TextScale.Steps[^1]) is null, "larger at max is null");
        Assert(TextScale.Smaller(TextScale.Steps[0]) is null, "smaller at min is null");
    }

    private static void PreferencesMemoryAndLocalSettings()
    {
        var store = new MemoryPreferenceStore();
        var prefs = new ReaderPreferences(store);
        Assert(prefs.Theme == AppTheme.System, "default theme system");
        Assert(Math.Abs(prefs.TextScaleValue - 1.0) < 0.001, "default text scale 1.0");

        var changedCount = 0;
        prefs.Changed += () => changedCount++;

        prefs.Theme = AppTheme.Dark;
        Assert(changedCount == 1, "theme change fired event");
        Assert(prefs.Theme == AppTheme.Dark, "theme is Dark");
        Assert(prefs.ToElementTheme() == Microsoft.UI.Xaml.ElementTheme.Dark, "element theme dark");

        prefs.TextScaleValue = 1.5;
        Assert(changedCount == 2, "scale change fired event");
        Assert(Math.Abs(prefs.TextScaleValue - 1.5) < 0.001, "scale is 1.5");

        // Reread from same store
        var prefs2 = new ReaderPreferences(store);
        Assert(prefs2.Theme == AppTheme.Dark, "persisted theme Dark");
        Assert(Math.Abs(prefs2.TextScaleValue - 1.5) < 0.001, "persisted scale 1.5");

        // File-backed LocalSettingsPreferenceStore persistence
        var tempFile = Path.Combine(Path.GetTempPath(), $"prefs-{Guid.NewGuid():N}.json");
        try
        {
            var fileStore = new LocalSettingsPreferenceStore(tempFile);
            var filePrefs = new ReaderPreferences(fileStore);
            filePrefs.Theme = AppTheme.Light;
            filePrefs.TextScaleValue = 1.3;

            // Reread from a fresh store pointing to the same file
            var fileStore2 = new LocalSettingsPreferenceStore(tempFile);
            var filePrefs2 = new ReaderPreferences(fileStore2);
            Assert(filePrefs2.Theme == AppTheme.Light, "reloaded theme Light from file");
            Assert(Math.Abs(filePrefs2.TextScaleValue - 1.3) < 0.001, "reloaded scale 1.3 from file");

            // Corrupt file fallback
            File.WriteAllText(tempFile, "{ broken json !!");
            var corruptStore = new LocalSettingsPreferenceStore(tempFile);
            var corruptPrefs = new ReaderPreferences(corruptStore);
            Assert(corruptPrefs.Theme == AppTheme.System, "corrupt file falls back to default system theme");
            Assert(Math.Abs(corruptPrefs.TextScaleValue - 1.0) < 0.001, "corrupt file falls back to 1.0 scale");
        }
        finally
        {
            if (File.Exists(tempFile))
            {
                try { File.Delete(tempFile); } catch { }
            }
        }
    }

    private static void DocumentFindUnicodeMappingsAndWrapping()
    {
        var source =
            "# Find 𝒳 heading\n\n" +
            "Plain foo **bar** baz, 𝒳𝒴 target, AT&amp;T and `code span`.\n" +
            "soft\nbreak and hard\\\nline.\n\n" +
            "> ```\n> quoted code\n> second line\n> ```\n\n" +
            "![alt words](missing.png) and <span>raw html</span>\n\n" +
            "| Cell one | Cell two |\n| - | - |\n| table value | 中文 |\n\n" +
            "Café and cafe\u0301; 👨\u200D👩\u200D👧 family; עברית שלום.\n";

        using var session = new CoreSession("render-find-test");
        var parseRes = session.ParseBlocking(Encoding.UTF8.GetBytes(source), null, copyUnchanged: true);
        Assert(parseRes.Document is not null, "parsed doc");
        var doc = parseRes.Document;
        var plan = RenderPlan.Build(doc, () => false);

        FindMatches DoFind(string query, CoreFindOptions? options = null)
        {
            var res = session.Find(query, options, doc.Generation);
            return DocumentFind.MapMatches(plan, doc, res);
        }

        List<string> MatchTexts(FindMatches matches)
        {
            var list = new List<string>(matches.Count);
            foreach (var m in matches.Matches)
            {
                var sb = new StringBuilder();
                foreach (var r in m.Ranges)
                {
                    var frag = plan.Fragments[r.FragmentIndex];
                    sb.Append(frag.Text.Substring(r.Start, r.Length));
                }
                list.Add(sb.ToString());
            }
            return list;
        }

        var cases = new (string Query, string[] Expected)[]
        {
            ("foo bar", ["foo bar"]),
            ("target", ["target"]),
            ("𝒳", ["𝒳", "𝒳"]),
            ("AT&T", ["AT&T"]),
            ("code span", ["code span"]),
            ("soft break", ["soft break"]),
            ("alt words", ["[alt words]"]),
            ("table value", ["table value"]),
            ("中文", ["中文"]),
            ("café", ["Café"]),
            ("cafe\u0301", ["cafe\u0301"]),
            ("family", ["family"]),
            ("שלום", ["שלום"]),
            ("heading", ["heading"]),
            ("missing", []),
        };

        foreach (var (query, expected) in cases)
        {
            var matches = DoFind(query);
            var texts = MatchTexts(matches);
            Assert(texts.SequenceEqual(expected), $"query '{query}' expected [{string.Join(", ", expected)}] got [{string.Join(", ", texts)}]");
        }

        // Navigation wrapping and status
        var multiMatches = DoFind("𝒳");
        Assert(multiMatches.Count == 2, "2 matches for 𝒳");
        Assert(multiMatches.Status(null) == "2 matches", "status plural");
        Assert(multiMatches.Status(0) == "1 of 2", "status 1 of 2");
        Assert(multiMatches.Status(1) == "2 of 2", "status 2 of 2");

        Assert(multiMatches.IndexAtOrAfter(0, 0) == 0, "at or after 0");
        Assert(multiMatches.IndexAfter(0, backwards: false) == 1, "next after 0");
        Assert(multiMatches.IndexAfter(1, backwards: false) == 0, "next wraps to 0");
        Assert(multiMatches.IndexAfter(0, backwards: true) == 1, "prev wraps to 1");

        var emptyMatches = DoFind("nonexistentquery");
        Assert(emptyMatches.Count == 0, "0 matches");
        Assert(emptyMatches.Status(null) == "Not found", "not found status");
        Assert(emptyMatches.IndexAtOrAfter(0) is null, "empty index is null");

        // Stale generation
        var staleResult = new CoreFindResult(doc.Generation + 1, [], [], false);
        var staleMatches = DocumentFind.MapMatches(plan, doc, staleResult);
        Assert(staleMatches.Count == 0, "stale generation returns 0 matches");

        session.Close();
    }

    private static void GlobalSelectionAndTextBetween()
    {
        var p0 = new DocumentPosition(0, 0);
        var p1 = new DocumentPosition(0, 5);
        var p2 = new DocumentPosition(1, 0);
        var p3 = new DocumentPosition(1, 10);

        Assert(p0 < p1, "p0 < p1");
        var p1Copy = new DocumentPosition(0, 5);
        Assert(p1 <= p1Copy && p1 >= p1Copy, "p1 <= p1Copy");
        Assert(p2 > p1, "p2 > p1");
        Assert(p3 >= p2, "p3 >= p2");
        Assert(p0.CompareTo(p0) == 0, "p0 compare");

        var selEmpty = new DocumentSelection(p1, p1);
        Assert(selEmpty.IsEmpty, "selEmpty is empty");
        Assert(selEmpty.Start == p1 && selEmpty.End == p1, "empty start and end");

        var selForward = new DocumentSelection(p0, p3);
        Assert(!selForward.IsEmpty, "selForward not empty");
        Assert(selForward.Start == p0 && selForward.End == p3, "forward start and end");

        var selReverse = new DocumentSelection(p3, p0);
        Assert(!selReverse.IsEmpty, "selReverse not empty");
        Assert(selReverse.Start == p0 && selReverse.End == p3, "reverse start and end");

        var doc = Document("# Title\n\nFirst paragraph.\n\nSecond paragraph with **bold** text.\n\n| Col A | Col B |\n| :--- | :--- |\n| Val 1 | Val 2 |\n");
        var plan = RenderPlan.Build(doc, () => false);

        // Full document text
        var fullText = plan.TextBetween(new DocumentPosition(0, 0), new DocumentPosition(plan.Fragments.Count - 1, plan.Fragments[^1].Text.Length));
        Assert(fullText.Contains("Title"), "contains Title");
        Assert(fullText.Contains("First paragraph."), "contains First paragraph");
        Assert(fullText.Contains("Second paragraph with bold text."), "contains Second paragraph");
        Assert(fullText.Contains("Val 1	Val 2"), "table row contains tab separator");

        // Full document source returns doc.Source exactly
        var fullSource = plan.SourceBetween(new DocumentPosition(0, 0), new DocumentPosition(plan.Fragments.Count - 1, plan.Fragments[^1].Text.Length), doc);
        Assert(fullSource.SequenceEqual(doc.Source), "full source equals doc.Source");

        // Sub-range within first fragment
        var subFirst = plan.TextBetween(new DocumentPosition(0, 0), new DocumentPosition(0, 5));
        Assert(subFirst == "Title", $"expected 'Title', got '{subFirst}'");

        // Sub-range across paragraph fragments
        var subRange = plan.TextBetween(new DocumentPosition(1, 0), new DocumentPosition(2, 6));
        Assert(subRange.StartsWith("First paragraph."), "starts with First paragraph.");
        Assert(subRange.Contains("\n\n"), "contains block separator");
        Assert(subRange.EndsWith("Second"), "ends with Second");

        // 1. Mid-paragraph literal text selection: narrows exact source bytes
        var midText = plan.TextBetween(new DocumentPosition(1, 6), new DocumentPosition(1, 15));
        Assert(midText == "paragraph", $"expected 'paragraph', got '{midText}'");

        var midSource = plan.SourceBetween(new DocumentPosition(1, 6), new DocumentPosition(1, 15), doc);
        var expectedSource = Encoding.UTF8.GetBytes("paragraph");
        Assert(midSource.SequenceEqual(expectedSource), $"midSource expected 'paragraph', got '{Encoding.UTF8.GetString(midSource)}'");

        // 2. Mid-paragraph selection across code span, escapes, and entities:
        // verifies fallback to whole-node spans without byte offset drift or splitting
        var doc2 = Document("Word with `code` and \\*star\\* and &amp; entity.\n");
        var plan2 = RenderPlan.Build(doc2, () => false);
        var frag2Text = plan2.Fragments[0].Text;
        Assert(frag2Text.Contains("code"), "contains code");

        // Select across `code` and \*star\*
        var codeIdx = frag2Text.IndexOf("code", StringComparison.Ordinal);
        var starIdx = frag2Text.IndexOf("*star*", StringComparison.Ordinal);
        var subText2 = plan2.TextBetween(new DocumentPosition(0, codeIdx), new DocumentPosition(0, starIdx + 6));
        Assert(subText2.StartsWith("code") && subText2.EndsWith("*star*"), $"subText2 unexpected: '{subText2}'");

        var subSource2 = plan2.SourceBetween(new DocumentPosition(0, codeIdx), new DocumentPosition(0, starIdx + 6), doc2);
        var subSource2Str = Encoding.UTF8.GetString(subSource2);
        Assert(subSource2Str.Contains("`code`"), $"source must contain full code span with backticks: '{subSource2Str}'");
        Assert(subSource2Str.Contains("\\*star\\*"), $"source must contain full escape without drift: '{subSource2Str}'");

        // AccessibleItemType verification
        Assert(RowItem.AccessibleItemType(FragmentKind.Heading) == "heading", "accessible heading");
        Assert(RowItem.AccessibleItemType(FragmentKind.Paragraph) == "paragraph", "accessible paragraph");
        Assert(RowItem.AccessibleItemType(FragmentKind.Code) == "code block", "accessible code block");
        Assert(RowItem.AccessibleItemType(FragmentKind.ThematicBreak) == "thematic break", "accessible thematic break");
        Assert(RowItem.AccessibleItemType(FragmentKind.TableRow) == "table row", "accessible table row");
        Assert(RowItem.AccessibleItemType(FragmentKind.Image) == "image", "accessible image");
        Assert(RowItem.AccessibleItemType(FragmentKind.Fallback) == "raw html fallback", "accessible fallback");
    }

    private static void NativeClipboardRoundtrip()
    {
        var testText = "Rendered Markdown Line 1\nLine 2 ❤";
        var testMarkdown = Encoding.UTF8.GetBytes("# Header\n\nMarkdown source with ❤.\n");

        var ok = NativeClipboard.TrySet(0, testText, testMarkdown);
        Assert(ok, "TrySet returned true");
        var readText = NativeClipboard.ReadUnicodeText();
        var readMarkdown = NativeClipboard.ReadMarkdownBytes();

        Assert(readText == testText, $"clipboard plain text roundtrip expected '{testText}', got '{readText}'");
        Assert(readMarkdown is not null && readMarkdown.SequenceEqual(testMarkdown), "clipboard markdown bytes roundtrip failed");
    }
}
