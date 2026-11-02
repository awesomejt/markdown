using System.Diagnostics;
using System.IO;
using System.Text;
using Markdown.Windows.Core;

namespace Markdown.Windows;

internal static unsafe class CoreConsumerTests
{
    private const string SampleMarkdown =
        "---\ntitle: Sample title\n---\n# Heading one\n\nText with a " +
        "[link](https://example.com/a) and ![alt](images/pic.png).\n\n## Second\n\n- [x] done\n";

    public static int RunAll()
    {
        var tests = new (string Name, Action Body)[]
        {
            ("package_matches_build_profile", PackageMatchesBuildProfile),
            ("negotiation_and_typed_errors", NegotiationAndTypedErrors),
            ("identity_and_abi_mismatch_guards", IdentityAndAbiMismatchGuards),
            ("owned_v2_values_and_generations", OwnedV2ValuesAndGenerations),
            ("parse_failures_and_recovery", ParseFailuresAndRecovery),
            ("copy_budget_is_checked_before_copying", CopyBudgetIsCheckedBeforeCopying),
            ("snapshot_view_validation_guards", SnapshotViewValidationGuards),
            ("queued_cancellation_admits_a_replacement_and_the_queue_is_bounded", QueuedCancellationAdmitsReplacementAndQueueIsBounded),
            ("cancellation_during_a_large_call", CancellationDuringLargeCall),
            ("close_during_work_and_reject_further_use", CloseDuringWorkAndRejectFurtherUse),
            ("concurrent_close_waits_for_cleanup", ConcurrentCloseWaitsForCleanup),
            ("dropping_a_session_releases_it_on_the_worker", DroppingASessionReleasesItOnTheWorker),
            ("availability_follows_recorded_changes", AvailabilityFollowsRecordedChanges),
            ("repeated_lifecycle", RepeatedLifecycle),
            ("blocking_calls_are_refused_on_the_worker", BlockingCallsAreRefusedOnTheWorker),
            ("corpus_and_performance_profiles", CorpusAndPerformanceProfiles),
            ("close_then_does_not_block_and_reports_the_release", CloseThenDoesNotBlockAndReportsRelease),
            ("find_over_current_snapshot", FindOverCurrentSnapshot),
        };

        Console.WriteLine($"running {tests.Length} Windows core consumer tests ({CoreRuntime.ExpectedConfiguration})");
        foreach (var (name, body) in tests)
        {
            DrainWorker();
            var baseline = CoreRuntime.LiveHandles();
            Check(CoreRuntime.PendingParses == 0, $"{name}: admission not zero before test");

            var sw = Stopwatch.StartNew();
            body();
            sw.Stop();

            DrainWorker();
            var after = CoreRuntime.LiveHandles();
            Check(after == baseline, $"{name}: native handles leaked: before={baseline}, after={after}");
            Check(CoreRuntime.PendingParses == 0, $"{name}: admission leaked ({CoreRuntime.PendingParses})");
            Console.WriteLine($"test {name} ... ok ({sw.ElapsedMilliseconds} ms)");
        }

        Console.WriteLine($"core consumer tests ok: {tests.Length} passed");
        return 0;
    }

    private static void Check(bool condition, string message)
    {
        if (!condition)
        {
            throw new InvalidOperationException($"assertion failed: {message}");
        }
    }

    private static CoreException ExpectCoreError(Action action, CoreErrorKind expectedKind)
    {
        try
        {
            action();
        }
        catch (CoreException ex) when (ex.Kind == expectedKind)
        {
            return ex;
        }
        catch (CoreException ex)
        {
            throw new InvalidOperationException(
                $"expected CoreErrorKind.{expectedKind}, got CoreErrorKind.{ex.Kind}: {ex.Message}",
                ex);
        }
        throw new InvalidOperationException($"expected CoreErrorKind.{expectedKind}, but action succeeded");
    }

    private static void DrainWorker()
    {
        using var done = new ManualResetEventSlim(false);
        CoreRuntime.EnqueueWork(done.Set);
        if (!done.Wait(TimeSpan.FromSeconds(60)))
        {
            throw new TimeoutException("timed out waiting for markdown-core worker to drain");
        }
    }

    private static IDisposable HoldWorker()
    {
        var started = new ManualResetEventSlim(false);
        var release = new ManualResetEventSlim(false);
        CoreRuntime.EnqueueWork(() =>
        {
            started.Set();
            release.Wait(TimeSpan.FromSeconds(60));
        });
        if (!started.Wait(TimeSpan.FromSeconds(30)))
        {
            throw new TimeoutException("timed out waiting for worker hold to start");
        }
        return new WorkerReleaseHandle(release, started);
    }

    private sealed class WorkerReleaseHandle(ManualResetEventSlim release, ManualResetEventSlim started) : IDisposable
    {
        private int _disposed;

        public void Dispose()
        {
            if (Interlocked.Exchange(ref _disposed, 1) == 0)
            {
                release.Set();
                release.Dispose();
                started.Dispose();
            }
        }
    }

    private static (ParseRequest Request, Task<(CoreResult? Result, CoreException? Error)> Completion) ParseAsync(
        CoreSession session,
        byte[] source)
    {
        var tcs = new TaskCompletionSource<(CoreResult? Result, CoreException? Error)>(
            TaskCreationOptions.RunContinuationsAsynchronously);
        var req = session.Parse(source, null, copyUnchanged: true, (res, err) =>
        {
            tcs.TrySetResult((res, err));
        });
        return (req, tcs.Task);
    }

    private static string FindRepoRoot()
    {
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
        throw new DirectoryNotFoundException("unable to locate repository root for test fixtures");
    }

    private static byte[] LoadProfile(string name)
    {
        var path = Path.Combine(FindRepoRoot(), "build", "fixtures", "performance", name);
        if (!File.Exists(path))
        {
            throw new FileNotFoundException(
                $"{path} is missing: run tools/generate_performance_fixtures.py",
                path);
        }
        return File.ReadAllBytes(path);
    }

    private static void PackageMatchesBuildProfile()
    {
        Check(
            CorePackageIdentity.Configuration == CoreRuntime.ExpectedConfiguration,
            $"expected configuration {CoreRuntime.ExpectedConfiguration}, got {CorePackageIdentity.Configuration}");
        Check(CorePackageIdentity.Fingerprint.Length == 64, "fingerprint length must be 64");

        var identityPath = Path.Combine(
            FindRepoRoot(),
            "build",
            "windows",
            "core-package",
            CoreRuntime.ExpectedConfiguration,
            "identity");
        var identity = File.ReadAllText(identityPath).Replace("\r\n", "\n", StringComparison.Ordinal);
        Check(
            identity.Contains($"sha256={CorePackageIdentity.Fingerprint}\n", StringComparison.Ordinal),
            "staged identity file does not match compiled CorePackageIdentity.Fingerprint");
    }

    private static void NegotiationAndTypedErrors()
    {
        CoreRuntime.Validate();

        var info = new CoreFfi.md_abi_info
        {
            struct_size = (uint)sizeof(CoreFfi.md_abi_info),
        };
        var status = CoreFfi.md_abi_negotiate(2, &info);
        Check(status == CoreFfi.MD_ERR_VERSION_MISMATCH, "negotiate v2 should return VERSION_MISMATCH");
        Check(info.abi_version == 1, "mismatch still reports library abi_version 1");
        Check(
            info.model_versions == ((1u << (int)CoreFfi.MD_MODEL_V1) | (1u << (int)CoreFfi.MD_MODEL_V2)),
            "linked package must report model v1 and v2");

        info.struct_size -= 4;
        status = CoreFfi.md_abi_negotiate(1, &info);
        Check(status == CoreFfi.MD_ERR_STRUCT_SIZE, "bad struct_size should return STRUCT_SIZE");
        Check(CoreRuntime.StatusName(CoreFfi.MD_ERR_STRUCT_SIZE) == "struct_size", "status name for STRUCT_SIZE");

        ExpectCoreError(() => _ = new CoreSession(""), CoreErrorKind.InvalidArgument);

        var overCeiling = HostLimits.Default with { MaxSourceBytes = 65UL * 1024UL * 1024UL };
        ExpectCoreError(() => _ = CoreSession.WithLimits("doc", overCeiling), CoreErrorKind.InvalidArgument);

        using var session = new CoreSession("doc");
        var longToken = new byte[257];
        Array.Fill(longToken, (byte)'a');
        ExpectCoreError(
            () => _ = session.ParseBlocking("# x"u8.ToArray(), longToken, copyUnchanged: true),
            CoreErrorKind.InvalidArgument);

        Check(
            CoreFfi.md_snapshot_release(ulong.MaxValue) == CoreFfi.MD_ERR_INVALID_HANDLE,
            "releasing unknown snapshot handle must fail with INVALID_HANDLE");
        session.Close();
    }

    private static void IdentityAndAbiMismatchGuards()
    {
        ExpectCoreError(
            () => CoreRuntime.CheckIdentity("debug", "abc", "release", "abc"),
            CoreErrorKind.PackageIdentityMismatch);
        ExpectCoreError(
            () => CoreRuntime.CheckIdentity("debug", "abc", "debug", "def"),
            CoreErrorKind.PackageIdentityMismatch);

        var info = new CoreFfi.md_abi_info
        {
            struct_size = (uint)sizeof(CoreFfi.md_abi_info),
        };
        Check(CoreFfi.md_abi_negotiate(CoreFfi.MD_ABI_VERSION, &info) == CoreFfi.MD_OK, "negotiate baseline");

        var badAbi = info;
        badAbi.abi_version = 99;
        ExpectCoreError(() => CoreRuntime.ValidateAbiInfo(badAbi), CoreErrorKind.VersionMismatch);

        var badNode = info;
        badNode.node_size += 8;
        ExpectCoreError(() => CoreRuntime.ValidateAbiInfo(badNode), CoreErrorKind.StructSizeMismatch);

        var v1Only = info;
        v1Only.model_versions = 1u << (int)CoreFfi.MD_MODEL_V1;
        ExpectCoreError(() => CoreRuntime.ValidateAbiInfo(v1Only), CoreErrorKind.MissingModelV2);
    }

    private static void OwnedV2ValuesAndGenerations()
    {
        using var session = new CoreSession("sample");
        var sampleBytes = Encoding.UTF8.GetBytes(SampleMarkdown);
        var first = session.ParseBlocking(sampleBytes, "rev-1"u8.ToArray(), copyUnchanged: true);
        Check(first.Outcome == CoreOutcome.Published, "first outcome must be Published");
        Check(first.Generation == 1 && first.Failure is null, "first generation must be 1 with no failure");

        var doc = first.Document!;
        Check(doc.ModelVersion == CoreFfi.MD_MODEL_V2, "document must be model v2");
        Check(doc.Generation == 1, "document generation");
        Check(doc.Source.AsSpan().SequenceEqual(sampleBytes), "owned source matches input");
        Check(doc.DocumentId == "sample", "document_id");
        Check(!string.IsNullOrEmpty(doc.RevisionId), "revision_id");
        Check(doc.Title == "Sample title", "title");
        Check(doc.HasFrontMatter, "has front matter");
        Check(doc.SourceSha256.Length == 64, "source_sha256 length");
        Check(doc.OutlineCount == 2, "outline count");
        Check(doc.Outline[0].level == 1 && doc.OutlineTitle(0) == "Heading one", "first heading");
        Check(doc.Outline[1].level == 2 && doc.OutlineTitle(1) == "Second", "second heading");

        var headingNode = doc.Nodes[doc.Outline[0].node];
        Check(headingNode.kind == CoreFfi.MD_NODE_HEADING && headingNode.value == 1, "heading node kind and level");
        Check(doc.LinkCount == 1 && doc.ResourceCount == 1, "links and resources count");
        Check(doc.LinkTarget(0) == "https://example.com/a", "link target");
        Check(doc.CopiedBytes >= (ulong)sampleBytes.Length, "copied bytes >= source length");

        ExpectCoreError(
            () => _ = doc.DecodeStr(new CoreFfi.md_str { offset = uint.MaxValue, length = 1 }),
            CoreErrorKind.InvalidSnapshot);

        // Same bytes -> Unchanged; copyUnchanged=false skips copying the view.
        var same = session.ParseBlocking(sampleBytes, null, copyUnchanged: false);
        Check(same.Outcome == CoreOutcome.Unchanged && same.Generation == 2, "unchanged outcome gen 2");
        Check(same.Document is null, "unchanged without copy returns null document");

        var copied = session.ParseBlocking(sampleBytes, null, copyUnchanged: true);
        Check(copied.Outcome == CoreOutcome.Unchanged && copied.Document is not null, "unchanged with copy");

        var next = session.ParseBlocking("# Changed\n"u8.ToArray(), null, copyUnchanged: true);
        Check(next.Outcome == CoreOutcome.Published && next.Generation == 4, "next published gen 4");

        // Owned values outlive the session and every native handle.
        session.Close();
        DrainWorker();
        Check(doc.OutlineTitle(1) == "Second", "owned document outlives session release");
    }

    private static void ParseFailuresAndRecovery()
    {
        using var session = new CoreSession("failures");
        var invalidUtf8 = session.ParseBlocking([0x23, 0x20, 0xFF], null, copyUnchanged: true);
        Check(invalidUtf8.Outcome == CoreOutcome.Failed, "invalid utf8 outcome");
        Check(invalidUtf8.Failure == CoreParseFailure.InvalidUtf8, "invalid utf8 failure kind");
        Check(invalidUtf8.Document is null, "failed parse returns no document");

        var over = new byte[HostLimits.Default.MaxSourceBytes + 1];
        Array.Fill(over, (byte)'a');
        var overRes = session.ParseBlocking(over, null, copyUnchanged: true);
        Check(overRes.Failure == CoreParseFailure.SourceLimit, "over source limit failure kind");

        var denseCount = (int)(HostLimits.Default.MaxSourceBytes / 2);
        var dense = new byte[denseCount * 2];
        for (int i = 0; i < dense.Length; i += 2)
        {
            dense[i] = (byte)'*';
            dense[i + 1] = (byte)'a';
        }
        var denseRes = session.ParseBlocking(dense, null, copyUnchanged: true);
        Check(denseRes.Failure == CoreParseFailure.NodeLimit, "dense input triggers NodeLimit");

        var good = session.ParseBlocking("# Fine\n"u8.ToArray(), null, copyUnchanged: true);
        Check(good.Outcome == CoreOutcome.Published && good.Generation == 4, "session recovers on gen 4");
        session.Close();
    }

    private static void CopyBudgetIsCheckedBeforeCopying()
    {
        var tiny = HostLimits.Default with { MaxSnapshotCopyBytes = 64 };
        using var session = CoreSession.WithLimits("budget", tiny);
        Check(session.Limits == tiny, "session limits match");
        ExpectCoreError(
            () => _ = session.ParseBlocking(Encoding.UTF8.GetBytes(SampleMarkdown), null, copyUnchanged: true),
            CoreErrorKind.SnapshotTooLarge);
        session.Close();
    }

    private static void SnapshotViewValidationGuards()
    {
        var view = new CoreFfi.md_snapshot_view
        {
            struct_size = (uint)sizeof(CoreFfi.md_snapshot_view),
            model_version = CoreFfi.MD_MODEL_V1,
        };
        ExpectCoreError(() => _ = CoreDocument.CopyFromView(in view, 1024), CoreErrorKind.InvalidSnapshot);

        view.model_version = CoreFfi.MD_MODEL_V2;
        view.node_count = 1;
        view.nodes = null;
        ExpectCoreError(() => _ = CoreDocument.CopyFromView(in view, 1024), CoreErrorKind.InvalidSnapshot);

        byte* unaligned = stackalloc byte[sizeof(CoreFfi.md_node) + 8];
        view.nodes = (CoreFfi.md_node*)(unaligned + 1);
        ExpectCoreError(() => _ = CoreDocument.CopyFromView(in view, 1024), CoreErrorKind.InvalidSnapshot);

        // Two non-root nodes forming a cycle must be rejected even if parent/child pairs match locally.
        Span<byte> shaBytes = stackalloc byte[64];
        shaBytes.Fill((byte)'0');
        CoreFfi.md_node* cyclicNodes = stackalloc CoreFfi.md_node[2];
        cyclicNodes[0] = new CoreFfi.md_node
        {
            kind = CoreFfi.MD_NODE_PARAGRAPH,
            parent = 1,
            first_child = 0,
            child_count = 1,
        };
        cyclicNodes[1] = new CoreFfi.md_node
        {
            kind = CoreFfi.MD_NODE_PARAGRAPH,
            parent = 0,
            first_child = 1,
            child_count = 1,
        };
        uint* cyclicChildren = stackalloc uint[2];
        cyclicChildren[0] = 1;
        cyclicChildren[1] = 0;
        fixed (byte* strPtr = shaBytes)
        {
            var cyclicView = new CoreFfi.md_snapshot_view
            {
                struct_size = (uint)sizeof(CoreFfi.md_snapshot_view),
                model_version = CoreFfi.MD_MODEL_V2,
                strings = strPtr,
                strings_length = 64,
                source_sha256 = new CoreFfi.md_str { offset = 0, length = 64 },
                nodes = cyclicNodes,
                node_count = 2,
                children = cyclicChildren,
                child_count = 2,
            };
            ExpectCoreError(() => _ = CoreDocument.CopyFromView(in cyclicView, 4096), CoreErrorKind.InvalidSnapshot);
        }
    }

    private static void QueuedCancellationAdmitsReplacementAndQueueIsBounded()
    {
        using var session = new CoreSession("queue");
        var hold = HoldWorker();
        try
        {
            var (first, firstTask) = ParseAsync(session, "# One\n"u8.ToArray());
            var (_, secondTask) = ParseAsync(session, "# Two\n"u8.ToArray());
            Check(CoreRuntime.PendingParses == 2, "2 admitted parses while worker held");

            ExpectCoreError(
                () => _ = session.Parse("# Three\n"u8.ToArray(), null, copyUnchanged: true, (_, _) => { }),
                CoreErrorKind.Busy);

            Check(!first.IsRunning, "first queued request has not started while worker held");
            first.Cancel();
            Check(firstTask.IsCompleted, "cancelling a queued request completes immediately");
            Check(firstTask.Result.Error?.Kind == CoreErrorKind.Cancelled, "first request reports Cancelled");
            Check(CoreRuntime.PendingParses == 1, "cancelling queued request releases admission slot immediately");

            var (_, thirdTask) = ParseAsync(session, "# Three\n"u8.ToArray());
            first.Cancel();

            // Session-wide Cancel() must also cancel queued requests for that session and release their slot.
            using var otherSession = new CoreSession("queue-session-cancel");
            ExpectCoreError(
                () => _ = otherSession.Parse("# Blocked\n"u8.ToArray(), null, copyUnchanged: true, (_, _) => { }),
                CoreErrorKind.Busy);
            session.Cancel();
            Check(secondTask.IsCompleted && thirdTask.IsCompleted, "session.Cancel completes queued requests immediately");
            Check(
                secondTask.Result.Error?.Kind == CoreErrorKind.Cancelled &&
                thirdTask.Result.Error?.Kind == CoreErrorKind.Cancelled,
                "session.Cancel reports Cancelled for queued requests");
            Check(CoreRuntime.PendingParses == 0, "session.Cancel releases admission slots of queued requests");

            var (_, fourthTask) = ParseAsync(session, "# Four\n"u8.ToArray());
            var (_, fifthTask) = ParseAsync(otherSession, "# Five\n"u8.ToArray());
            hold.Dispose();

            var fourth = fourthTask.GetAwaiter().GetResult();
            var fifth = fifthTask.GetAwaiter().GetResult();
            Check(fourth.Error is null && fifth.Error is null, "fourth and fifth parses succeed");
            Check(
                fourth.Result!.Generation == 1 && fifth.Result!.Generation == 1,
                "cancelled queued requests never consumed a core generation");
            otherSession.Close();
        }
        finally
        {
            hold.Dispose();
        }
        session.Close();
    }

    private static void CancellationDuringLargeCall()
    {
        var source = LoadProfile("prose-5mib.md");
        using var session = new CoreSession("large");
        var (request, task) = ParseAsync(session, source);

        var sw = Stopwatch.StartNew();
        while (!request.IsRunning)
        {
            Check(sw.Elapsed < TimeSpan.FromSeconds(30), "worker never started large parse");
            Thread.SpinWait(200);
        }
        request.Cancel();
        var (_, err) = task.GetAwaiter().GetResult();
        Check(err?.Kind == CoreErrorKind.Cancelled, "in-flight request cancel reports Cancelled");

        var (request2, task2) = ParseAsync(session, source);
        sw.Restart();
        while (!request2.IsRunning)
        {
            Check(sw.Elapsed < TimeSpan.FromSeconds(30), "worker never started second large parse");
            Thread.SpinWait(200);
        }
        session.Cancel();
        var (res2, err2) = task2.GetAwaiter().GetResult();
        Check(
            err2?.Kind == CoreErrorKind.Cancelled ||
            (err2 is null && res2!.Outcome is CoreOutcome.Cancelled or CoreOutcome.Published),
            $"unexpected result after session.Cancel: outcome={res2?.Outcome} error={err2?.Message}");

        var after = session.ParseBlocking("# After\n"u8.ToArray(), null, copyUnchanged: true);
        Check(after.Outcome == CoreOutcome.Published, "session remains usable after cancel");
        session.Close();
    }

    private static void CloseDuringWorkAndRejectFurtherUse()
    {
        using var session = new CoreSession("closing");
        var hold = HoldWorker();
        var (_, queuedTask) = ParseAsync(session, "# Queued\n"u8.ToArray());

        var closeTask = Task.Run(session.Close);
        var sw = Stopwatch.StartNew();
        while (true)
        {
            try
            {
                _ = session.Availability();
            }
            catch (CoreException ex) when (ex.Kind == CoreErrorKind.SessionClosed)
            {
                break;
            }
            Check(sw.Elapsed < TimeSpan.FromSeconds(10), "Close did not detach session promptly");
            Thread.Yield();
        }

        ExpectCoreError(session.Cancel, CoreErrorKind.SessionClosed);
        ExpectCoreError(
            () => _ = session.Parse("# x\n"u8.ToArray(), null, copyUnchanged: true, (_, _) => { }),
            CoreErrorKind.SessionClosed);
        ExpectCoreError(() => _ = session.Availability(), CoreErrorKind.SessionClosed);
        ExpectCoreError(() => _ = session.RecordUnavailable(CoreUnavailability.Io), CoreErrorKind.SessionClosed);

        hold.Dispose();
        closeTask.GetAwaiter().GetResult();

        var (_, queuedErr) = queuedTask.GetAwaiter().GetResult();
        Check(queuedErr?.Kind == CoreErrorKind.SessionClosed, "queued work fails with SessionClosed");
        session.Close();
    }

    private static void ConcurrentCloseWaitsForCleanup()
    {
        var before = CoreRuntime.LiveHandles().Sessions;
        using var session = new CoreSession("concurrent");
        _ = session.ParseBlocking("# Ready\n"u8.ToArray(), null, copyUnchanged: true);

        var hold = HoldWorker();
        var closers = Enumerable.Range(0, 4).Select(_ => Task.Run(session.Close)).ToArray();
        Thread.Sleep(20);
        Check(closers.All(t => !t.IsCompleted), "Close must wait for the worker");

        hold.Dispose();
        Task.WaitAll(closers);
        Check(CoreRuntime.LiveHandles().Sessions == before, "session handle released when Close returns");
    }

    private static void DroppingASessionReleasesItOnTheWorker()
    {
        Task<(CoreResult? Result, CoreException? Error)> task;
        var hold = HoldWorker();
        try
        {
            var session = new CoreSession("dropped");
            (_, task) = ParseAsync(session, "# Dropped\n"u8.ToArray());
            session.Dispose();
        }
        finally
        {
            hold.Dispose();
        }

        var (_, err) = task.GetAwaiter().GetResult();
        Check(err?.Kind == CoreErrorKind.SessionClosed, "disposed session fails queued work with SessionClosed");
    }

    private static void AvailabilityFollowsRecordedChanges()
    {
        using var session = new CoreSession("availability");
        Check(session.Availability() == CoreAvailability.Available, "initially Available");

        var gen = session.RecordUnavailable(CoreUnavailability.PermissionDenied);
        Check(gen == 1, "first unavailable generation is 1");
        Check(
            session.Availability() == CoreAvailability.Unavailable(CoreUnavailability.PermissionDenied),
            "availability reflects PermissionDenied");

        _ = session.RecordUnavailable(CoreUnavailability.Deleted);
        Check(
            session.Availability() == CoreAvailability.Unavailable(CoreUnavailability.Deleted),
            "availability reflects Deleted");

        var back = session.ParseBlocking("# Back\n"u8.ToArray(), null, copyUnchanged: true);
        Check(back.Outcome == CoreOutcome.Published, "subsequent parse publishes");
        Check(session.Availability() == CoreAvailability.Available, "published parse restores Available");
        session.Close();
    }

    private static void RepeatedLifecycle()
    {
        for (int i = 0; i < 200; i++)
        {
            var session = new CoreSession($"doc-{i}");
            var source = Encoding.UTF8.GetBytes($"# Document {i}\n\nBody text.\n");
            var res = session.ParseBlocking(source, null, copyUnchanged: true);
            Check(res.Outcome == CoreOutcome.Published, $"iteration {i} published");
            if (i % 2 == 0)
            {
                session.Close();
            }
            else
            {
                session.Dispose();
            }
        }
    }

    private static void BlockingCallsAreRefusedOnTheWorker()
    {
        using var session = new CoreSession("worker");
        var tcs = new TaskCompletionSource<(CoreErrorKind? ParseErr, CoreErrorKind? CloseErr)>(
            TaskCreationOptions.RunContinuationsAsynchronously);

        CoreRuntime.EnqueueWork(() =>
        {
            CoreErrorKind? parseErr = null;
            CoreErrorKind? closeErr = null;
            try
            {
                _ = session.ParseBlocking("# x\n"u8.ToArray(), null, copyUnchanged: true);
            }
            catch (CoreException ex)
            {
                parseErr = ex.Kind;
            }
            try
            {
                session.Close();
            }
            catch (CoreException ex)
            {
                closeErr = ex.Kind;
            }
            tcs.SetResult((parseErr, closeErr));
        });

        var (pErr, cErr) = tcs.Task.GetAwaiter().GetResult();
        Check(
            pErr == CoreErrorKind.BlockingWaitOnWorker && cErr == CoreErrorKind.BlockingWaitOnWorker,
            "ParseBlocking and Close must refuse on the worker thread");
        session.Close();
    }

    private static void CorpusAndPerformanceProfiles()
    {
        var repoRoot = FindRepoRoot();
        var corpusDir = Path.Combine(repoRoot, "fixtures", "markdown");
        var corpus = Directory.GetDirectories(corpusDir)
            .SelectMany(d => Directory.GetFiles(d, "*.md"))
            .OrderBy(p => p, StringComparer.Ordinal)
            .ToArray();
        Check(corpus.Length == 8, $"expected 8 markdown corpus files, found {corpus.Length}");

        var profiles = new[]
        {
            "prose-1mib.md",
            "prose-5mib.md",
            "delimiter-storm-1mib.md",
            "long-lines-1mib.md",
        };

        var inputs = corpus.Select(File.ReadAllBytes)
            .Concat(profiles.Select(LoadProfile))
            .ToArray();

        for (int i = 0; i < inputs.Length; i++)
        {
            using var session = new CoreSession($"corpus-{i}");
            var res = session.ParseBlocking(inputs[i], null, copyUnchanged: true);
            Check(res.Outcome == CoreOutcome.Published, $"corpus input {i} failed: {res.Failure}");
            var doc = res.Document!;
            Check(doc.NodeCount > 0, $"corpus input {i} produced empty node table");
            for (int o = 0; o < doc.OutlineCount; o++)
            {
                _ = doc.OutlineTitle(o);
            }
            session.Close();
        }
    }

    private static void CloseThenDoesNotBlockAndReportsRelease()
    {
        using var session = new CoreSession("close-then");
        _ = session.ParseBlocking("# Ready\n"u8.ToArray(), null, copyUnchanged: true);

        var hold = HoldWorker();
        using var closed = new ManualResetEventSlim(false);
        CoreException? closeErr = null;
        session.CloseThen(err =>
        {
            closeErr = err;
            closed.Set();
        });

        ExpectCoreError(session.Cancel, CoreErrorKind.SessionClosed);
        Check(!closed.IsSet, "CloseThen callback waits until worker releases handle");

        hold.Dispose();
        Check(closed.Wait(TimeSpan.FromSeconds(30)), "CloseThen callback timed out");
        Check(closeErr is null, $"CloseThen reported error: {closeErr?.Message}");

        using var again = new ManualResetEventSlim(false);
        session.CloseThen(err =>
        {
            Check(err is null, "second CloseThen error");
            again.Set();
        });
        Check(again.IsSet, "subsequent CloseThen runs immediately");
        session.Close();

        // Throwing user callbacks must not terminate the shared worker thread.
        _ = CoreRuntime.TakeLastCallbackException();
        using var throwingSession = new CoreSession("throwing-callbacks");
        using var parseThrown = new ManualResetEventSlim(false);
        _ = throwingSession.Parse("# Throw\n"u8.ToArray(), null, copyUnchanged: true, (_, _) =>
        {
            parseThrown.Set();
            throw new InvalidOperationException("simulated parse callback exception");
        });
        Check(parseThrown.Wait(TimeSpan.FromSeconds(10)), "throwing parse callback invoked");
        DrainWorker();
        Check(
            CoreRuntime.TakeLastCallbackException()?.Message == "simulated parse callback exception",
            "worker captured throwing parse callback exception");

        var afterThrow = throwingSession.ParseBlocking("# Still alive\n"u8.ToArray(), null, copyUnchanged: true);
        Check(afterThrow.Outcome == CoreOutcome.Published, "worker remains alive after throwing parse callback");

        var holdCloseThrow = HoldWorker();
        using var closeThrown = new ManualResetEventSlim(false);
        throwingSession.CloseThen(_ =>
        {
            closeThrown.Set();
            throw new InvalidOperationException("simulated close callback exception");
        });
        holdCloseThrow.Dispose();
        Check(closeThrown.Wait(TimeSpan.FromSeconds(10)), "throwing CloseThen callback invoked");
        DrainWorker();
        Check(
            CoreRuntime.TakeLastCallbackException()?.Message == "simulated close callback exception",
            "worker captured throwing CloseThen callback exception");
    }

    private static void FindOverCurrentSnapshot()
    {
        using var session = new CoreSession("find-test");
        var source = "# Straße\n\nCafé and cafe\u0301 and CAFÉ.\n\n𝒳𝒴 target\n\n" +
                     "👨\u200D👩\u200D👧 family, 中文 and עברית.\n\n`code café` and **caf**é\n";
        var bytes = Encoding.UTF8.GetBytes(source);
        var first = session.ParseBlocking(bytes, null, copyUnchanged: true);
        Check(first.Outcome == CoreOutcome.Published, "parse succeeded");
        var generation = first.Generation;

        List<string> Find(string query, CoreFindOptions? options = null)
        {
            var res = session.Find(query, options, generation);
            Check(res.Generation == generation, "find generation matches");
            foreach (var match in res.Matches)
            {
                Check(match.SegmentCount > 0 && match.FirstSegment + match.SegmentCount <= res.Segments.Count, "match segments in bounds");
            }
            var list = new List<string>(res.Matches.Count);
            foreach (var m in res.Matches)
            {
                list.Add(Encoding.UTF8.GetString(bytes, m.Source.Start.Value, m.Source.End.Value - m.Source.Start.Value));
            }
            return list;
        }

        var cafe = Find("café");
        Check(cafe.SequenceEqual(["Café", "CAFÉ", "café", "caf**é"]), "café matches");
        var sensitive = Find("café", new CoreFindOptions(CaseSensitive: true));
        Check(sensitive.SequenceEqual(["café", "caf**é"]), "case sensitive café");
        var nfd = Find("cafe\u0301");
        Check(nfd.SequenceEqual(["cafe\u0301"]), "decomposed cafe");
        Check(Find("cafe").Count == 0, "no match inside a combining sequence");
        Check(Find("STRAßE").SequenceEqual(["Straße"]), "Straße case folded");
        Check(Find("STRASSE").Count == 0, "simple folding only");
        Check(Find("𝒳𝒴").SequenceEqual(["𝒳𝒴"]), "astral chars matched");
        Check(Find("👨").Count == 0, "no match inside emoji sequence");
        Check(Find("👨\u200D👩\u200D👧").SequenceEqual(["👨\u200D👩\u200D👧"]), "emoji sequence matched");
        Check(Find("family", new CoreFindOptions(WholeWord: true)).SequenceEqual(["family"]), "whole word matched");
        Check(Find("fam", new CoreFindOptions(WholeWord: true)).Count == 0, "subword not matched with WholeWord");
        Check(Find("中文").SequenceEqual(["中文"]), "CJK matched");
        Check(Find("עברית").SequenceEqual(["עברית"]), "RTL Hebrew matched");
        Check(Find("   ").Count == 0, "whitespace query returns empty");

        var limited = session.Find("caf", new CoreFindOptions(MaxMatches: 2), generation);
        Check(limited.Matches.Count == 2 && limited.Truncated, "truncated at maxMatches");

        try
        {
            _ = session.Find(new string('a', 4097), generation: generation);
            throw new InvalidOperationException("over-long query should throw");
        }
        catch (CoreException ex) when (ex.Kind == CoreErrorKind.InvalidArgument)
        {
        }

        try
        {
            _ = session.Find("café", generation: generation + 10);
            throw new InvalidOperationException("unpublished generation should throw");
        }
        catch (CoreException ex) when (ex.Kind == CoreErrorKind.Superseded)
        {
        }

        var second = session.ParseBlocking("café again\n"u8.ToArray(), null, copyUnchanged: true);
        Check(second.Generation > generation, "second generation increased");

        try
        {
            _ = session.Find("café", generation: generation);
            throw new InvalidOperationException("superseded generation should throw");
        }
        catch (CoreException ex) when (ex.Kind == CoreErrorKind.Superseded)
        {
        }

        var current = session.Find("café", generation: second.Generation);
        Check(current.Matches.Count == 1, "find over current generation");

        using (var hold = HoldWorker())
        using (var cts = new CancellationTokenSource())
        {
            var queuedTask = session.FindAsync("café", generation: second.Generation, cancellationToken: cts.Token);
            cts.Cancel();
            hold.Dispose();
            try
            {
                queuedTask.GetAwaiter().GetResult();
                throw new InvalidOperationException("cancelled find should throw");
            }
            catch (OperationCanceledException)
            {
            }
        }

        session.Close();
        try
        {
            _ = session.Find("café", generation: second.Generation);
            throw new InvalidOperationException("closed session should throw");
        }
        catch (CoreException ex) when (ex.Kind == CoreErrorKind.SessionClosed)
        {
        }
    }
}
