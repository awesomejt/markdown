using System.Diagnostics;
using System.Diagnostics.CodeAnalysis;
using System.IO;
using System.Security.AccessControl;
using System.Security.Principal;
using System.Text;
using Markdown.Windows.Core;
using Markdown.Windows.Document;

namespace Markdown.Windows;

internal static class DocumentTests
{
    public static int RunAll()
    {
        try
        {
            CoreRuntime.Validate();

            var tests = new (string Name, Action Run)[]
            {
                ("capability_redaction_display_name_and_uri_rejection", TestCapabilityRedactionDisplayNameAndUriRejection),
                ("regular_files_directories_devices_and_size_limits", TestRegularFilesDirectoriesDevicesAndSizeLimits),
                ("bounded_growth_and_cancellation_between_chunks", TestBoundedGrowthAndCancellationBetweenChunks),
                ("permission_denial_acl_revocation_and_recovery", TestPermissionDenialAclRevocationAndRecovery),
                ("volume_unavailable_mapping_and_detached_volume_promotion", TestVolumeUnavailableMappingAndDetachedVolumePromotion),
                ("failed_open_preserves_selection_and_retries_first_open", TestFailedOpenPreservesSelectionAndRetriesFirstOpen),
                ("reload_lineage_missing_file_recovery_and_availability", TestReloadLineageMissingFileRecoveryAndAvailability),
                ("invalid_utf8_and_complexity_do_not_corrupt_availability", TestInvalidUtf8AndComplexityDoNotCorruptAvailability),
                ("stale_errors_and_queued_results_are_suppressed", TestStaleErrorsAndQueuedResultsAreSuppressed),
                ("read_barrier_replacement_and_close_suppress_publication", TestReadBarrierReplacementAndCloseSuppressPublication),
                ("callback_before_install_and_synchronous_cancel_do_not_deadlock", TestCallbackBeforeInstallAndSynchronousCancelDoNotDeadlock),
                ("busy_admission_retry_recovers_when_slot_releases", TestBusyAdmissionRetryRecoversWhenSlotReleases),
                ("document_watcher_modification_deletion_and_reappearance", TestDocumentWatcherModificationDeletionAndReappearance),
                ("external_refresh_dispositions_and_stale_retention", TestExternalRefreshDispositionsAndStaleRetention),
                ("folder_capability_containment_grant_revocation_and_rejection", TestFolderCapabilityContainmentGrantRevocationAndRejection),
                ("folder_acl_denial_revocation_and_move_recovery", TestFolderAclDenialRevocationAndMoveRecovery),
                ("recents_store_caps_dedup_untrusted_json_and_future_version", TestRecentsStoreCapsDedupUntrustedJsonAndFutureVersion),
                ("recents_availability_checks_for_available_missing_denied_and_type", TestRecentsAvailabilityChecks),
                ("workspace_tree_enumeration_filtering_and_large_folder", TestWorkspaceTreeEnumerationFilteringAndLargeFolder),
                ("workspace_search_bounded_cancellation_and_revocation", TestWorkspaceSearchBoundedCancellationAndRevocation),
                ("workspace_watcher_change_storm_and_in_flight_gating", TestWorkspaceWatcherChangeStormAndInFlightGating),
                ("workspace_watcher_access_loss_on_deletion_and_revocation", TestWorkspaceWatcherAccessLossOnDeletionAndRevocation),
            };

            foreach (var (name, run) in tests)
            {
                run();
                AssertNoLiveHandles(name);
                var callbackEx = CoreRuntime.TakeLastCallbackException();
                Assert(callbackEx is null, $"{name}: unexpected worker callback exception: {callbackEx}");
                Console.WriteLine($"  PASS {name}");
            }

            Console.WriteLine($"DocumentTests: all {tests.Length} tests passed.");
            return 0;
        }
        catch (Exception ex)
        {
            Console.Error.WriteLine($"DocumentTests FAILED: {ex}");
            return 1;
        }
    }

    private static void TestCapabilityRedactionDisplayNameAndUriRejection()
    {
        var secretPath = @"C:\Users\Alice\PrivateSecretFolder\confidential-notes.md";
        var cap = DocumentCapability.FromPickerPath(secretPath);
        Assert(cap.ToString() == "DocumentCapability(<private>)", $"unexpected ToString: {cap}");
        Assert(!cap.ToString().Contains("Alice", StringComparison.OrdinalIgnoreCase), "ToString leaked user path");
        Assert(!cap.ToString().Contains("confidential", StringComparison.OrdinalIgnoreCase), "ToString leaked filename");
        Assert(cap.DisplayName == "confidential-notes.md", $"unexpected DisplayName: {cap.DisplayName}");

        // Control characters and Unicode bidi overrides are replaced with U+FFFD, and long names truncate at 160 scalars.
        var dirtyName = "bad\r\n\t\u0000\u001b\u202E\u2066name.md";
        var sanitized = DocumentCapability.SanitizeDisplayName(dirtyName);
        Assert(sanitized == "bad\uFFFD\uFFFD\uFFFD\uFFFD\uFFFD\uFFFD\uFFFDname.md", $"unexpected sanitized name: {sanitized}");

        var longEmojiName = string.Concat(Enumerable.Repeat("😀a", 100)) + ".md";
        var truncated = DocumentCapability.SanitizeDisplayName(longEmojiName);
        Assert(truncated.EnumerateRunes().Count() == DocumentCapability.MaxDisplayScalars,
            $"expected {DocumentCapability.MaxDisplayScalars} scalars, got {truncated.EnumerateRunes().Count()}");
        // Verify valid UTF-8 / no broken surrogate pairs.
        var utf8 = Encoding.UTF8.GetBytes(truncated);
        Assert(Utf8IsValid(utf8), "truncated display name produced invalid UTF-16/UTF-8");

        // Local file URI is accepted.
        Assert(DocumentCapability.TryFromUserInput("file:///C:/Temp/readme%20file.md", out var localUriCap, out _),
            "expected local file:// URI to succeed");
        Assert(localUriCap is not null && localUriCap.DisplayName == "readme file.md",
            $"unexpected local URI display name: {localUriCap?.DisplayName}");

        // Non-local URIs and network shares are rejected with NonLocalFile.
        string[] nonLocalInputs =
        [
            "https://example.com/readme.md",
            "http://example.com/readme.md",
            "mailto:user@example.com",
            "ftp://files.example.com/doc.md",
            "file://fileserver/share/doc.md",
            @"\\fileserver\share\doc.md",
            @"\\?\UNC\fileserver\share\doc.md",
        ];
        foreach (var input in nonLocalInputs)
        {
            var ok = DocumentCapability.TryFromUserInput(input, out var rejectedCap, out var err);
            Assert(!ok && rejectedCap is null && err == DocumentError.NonLocalFile,
                $"expected NonLocalFile for '{input}', got ok={ok}, err={err}");
        }

        // Empty/whitespace input returns NotFound.
        Assert(!DocumentCapability.TryFromUserInput("   ", out _, out var emptyErr) && emptyErr == DocumentError.NotFound,
            "expected NotFound for whitespace input");

        // Every DocumentError has path-free message and recovery text.
        foreach (var err in Enum.GetValues<DocumentError>())
        {
            var msg = err.Message();
            var rec = err.Recovery();
            var display = err.ToDisplayString();
            Assert(!string.IsNullOrWhiteSpace(msg), $"empty message for {err}");
            Assert(!string.IsNullOrWhiteSpace(rec), $"empty recovery for {err}");
            Assert(!display.Contains(@"C:\", StringComparison.OrdinalIgnoreCase), $"display leaked path for {err}");
        }
    }

    private static void TestRegularFilesDirectoriesDevicesAndSizeLimits()
    {
        using var temp = new TempDir();
        var goodPath = temp.WriteFile("good.md", "# Regular File\n\nBody.\n");
        var goodCap = DocumentCapability.FromPickerPath(goodPath);

        var readRes = DocumentReader.Read(goodCap, HostLimits.Default.MaxSourceBytes);
        Assert(readRes.IsOk && readRes.Document is not null, "expected regular file read to succeed");
        var tokenText = Encoding.UTF8.GetString(readRes.Document!.RevisionToken);
        Assert(!tokenText.Contains("good.md", StringComparison.OrdinalIgnoreCase),
            $"revision token leaked filename: {tokenText}");
        Assert(readRes.Document.VolumeSerial != 0, "expected non-zero volume serial");
        Assert(!string.IsNullOrEmpty(readRes.Document.VolumeRoot), "expected non-empty volume root");

        // Directory is rejected as NotRegularFile.
        var dirCap = DocumentCapability.FromPickerPath(temp.Path);
        var dirRes = DocumentReader.Read(dirCap, HostLimits.Default.MaxSourceBytes);
        Assert(!dirRes.IsOk && dirRes.Error == DocumentError.NotRegularFile,
            $"expected NotRegularFile for directory, got {dirRes.Error}");

        // Device paths and reserved DOS device names are rejected as NotRegularFile without opening/blocking.
        string[] devicePaths =
        [
            @"\\.\NUL",
            @"\\.\CON",
            @"\\.\pipe\markdown-nonexistent-pipe",
            @"\\?\pipe\markdown-nonexistent-pipe",
            Path.Combine(temp.Path, "CON"),
            Path.Combine(temp.Path, "NUL.md"),
            Path.Combine(temp.Path, "COM1.txt"),
            Path.Combine(temp.Path, "LPT1.markdown"),
        ];
        foreach (var dev in devicePaths)
        {
            var devCap = DocumentCapability.FromPickerPath(dev);
            var devRes = DocumentReader.Read(devCap, HostLimits.Default.MaxSourceBytes);
            Assert(!devRes.IsOk && devRes.Error == DocumentError.NotRegularFile,
                $"expected NotRegularFile for device path '{dev}', got {devRes.Error}");
        }

        // File exceeding size limit is rejected before reading.
        var bigPath = temp.WriteBytes("too-big.md", new byte[2049]);
        var bigCap = DocumentCapability.FromPickerPath(bigPath);
        var bigRes = DocumentReader.Read(bigCap, maxBytes: 2048);
        Assert(!bigRes.IsOk && bigRes.Error == DocumentError.TooLarge,
            $"expected TooLarge, got {bigRes.Error}");
    }

    private static void TestBoundedGrowthAndCancellationBetweenChunks()
    {
        using var temp = new TempDir();
        var growingPath = temp.WriteBytes("growing.md", Encoding.UTF8.GetBytes(new string('a', DocumentReader.ChunkBytes)));
        var growingCap = DocumentCapability.FromPickerPath(growingPath);

        // Grow the file past the 96 KiB cap right after the first 64 KiB chunk is read.
        var hookCalls = 0;
        var growRes = DocumentReader.Read(
            growingCap,
            maxBytes: 96 * 1024,
            cancellation: null,
            chunkHook: () =>
            {
                hookCalls++;
                if (hookCalls == 2)
                {
                    using var fs = new FileStream(
                        growingPath,
                        FileMode.Append,
                        FileAccess.Write,
                        FileShare.ReadWrite | FileShare.Delete);
                    var extra = Encoding.UTF8.GetBytes(new string('b', DocumentReader.ChunkBytes));
                    fs.Write(extra, 0, extra.Length);
                    fs.Flush();
                }
            });
        Assert(!growRes.IsOk && growRes.Error == DocumentError.TooLarge,
            $"expected TooLarge on mid-read growth, got {growRes.Error}");

        // Cancellation between chunks stops immediately with DocumentError.Cancelled.
        var cancelToken = new Cancellation();
        var cancelRes = DocumentReader.Read(
            growingCap,
            maxBytes: HostLimits.Default.MaxSourceBytes,
            cancellation: cancelToken,
            chunkHook: () => cancelToken.Cancel());
        Assert(!cancelRes.IsOk && cancelRes.Error == DocumentError.Cancelled,
            $"expected Cancelled on mid-read cancel, got {cancelRes.Error}");

        // Explicit capability revocation between chunks stops immediately with DocumentError.AccessRevoked.
        var revokeCap = DocumentCapability.FromPickerPath(growingPath);
        var revokeRes = DocumentReader.Read(
            revokeCap,
            maxBytes: HostLimits.Default.MaxSourceBytes,
            cancellation: null,
            chunkHook: () => revokeCap.Revoke());
        Assert(!revokeRes.IsOk && revokeRes.Error == DocumentError.AccessRevoked,
            $"expected AccessRevoked on mid-read revoke, got {revokeRes.Error}");
    }

    private static void TestPermissionDenialAclRevocationAndRecovery()
    {
        using var temp = new TempDir();
        var filePath = temp.WriteFile("acl.md", "# Initial Secret\n\nAccessible.\n");

        // 1. Deny read before first open -> PermissionDenied.
        SetReadDenyAcl(filePath, deny: true);
        try
        {
            using var deniedLoader = new DocumentLoader();
            deniedLoader.Open(DocumentCapability.FromPickerPath(filePath));
            WaitForIdle(deniedLoader);

            Assert(deniedLoader.Error == DocumentError.PermissionDenied,
                $"expected PermissionDenied on initial open, got {deniedLoader.Error}");
            Assert(deniedLoader.Document is null, "expected null Document on initial PermissionDenied");
            Assert(deniedLoader.Availability() == CoreAvailability.Unavailable(CoreUnavailability.PermissionDenied),
                $"unexpected availability: {deniedLoader.Availability()}");
            deniedLoader.CloseAndWait();
        }
        finally
        {
            SetReadDenyAcl(filePath, deny: false);
        }

        // 2. Open readable file -> Generation 1, then withdraw read permission via ACL -> Reload reports AccessRevoked,
        //    preserves last good Document, and restoring ACL + Reload recovers to Generation 2.
        using var loader = new DocumentLoader();
        loader.Open(DocumentCapability.FromPickerPath(filePath));
        WaitForIdle(loader);
        Assert(loader.Error is null && loader.Document is not null && loader.Document.Generation == 1,
            "expected initial open to publish Generation 1");
        Assert(loader.Availability() == CoreAvailability.Available, "expected Available after first open");

        SetReadDenyAcl(filePath, deny: true);
        try
        {
            loader.Reload();
            WaitForIdle(loader);

            Assert(loader.Error == DocumentError.AccessRevoked,
                $"expected AccessRevoked after ACL withdrawal, got {loader.Error}");
            Assert(loader.Document is not null && loader.Document.Generation == 1,
                "expected last good Document to stay preserved on AccessRevoked");
            Assert(loader.Availability() == CoreAvailability.Unavailable(CoreUnavailability.AccessRevoked),
                $"expected AccessRevoked availability, got {loader.Availability()}");
        }
        finally
        {
            SetReadDenyAcl(filePath, deny: false);
        }

        File.WriteAllText(filePath, "# Restored Secret\n\nUpdated after ACL restore.\n", new UTF8Encoding(false));
        loader.Reload();
        WaitForIdle(loader);
        // Generation 1 was initial parse, Generation 2 was recorded by RecordUnavailable(AccessRevoked),
        // so the recovered document is Generation 3.
        Assert(loader.Error is null && loader.Document is not null && loader.Document.Generation == 3,
            $"expected recovery to Generation 3, got gen={loader.Document?.Generation}, err={loader.Error}");
        Assert(loader.Availability() == CoreAvailability.Available, "expected Available after ACL recovery");

        // 3. Explicit capability.Revoke() -> Reload reports AccessRevoked.
        loader.Capability!.Revoke();
        loader.Reload();
        WaitForIdle(loader);
        Assert(loader.Error == DocumentError.AccessRevoked,
            $"expected AccessRevoked after Capability.Revoke(), got {loader.Error}");
        Assert(loader.Availability() == CoreAvailability.Unavailable(CoreUnavailability.AccessRevoked),
            "expected AccessRevoked availability after Capability.Revoke()");

        loader.CloseAndWait();
    }

    private static void TestVolumeUnavailableMappingAndDetachedVolumePromotion()
    {
        int[] volumeErrors =
        [
            DocumentReader.ERROR_INVALID_DRIVE,
            DocumentReader.ERROR_NOT_READY,
            DocumentReader.ERROR_SECTOR_NOT_FOUND,
            DocumentReader.ERROR_GEN_FAILURE,
            DocumentReader.ERROR_BAD_NETPATH,
            DocumentReader.ERROR_DEV_NOT_EXIST,
            DocumentReader.ERROR_NETNAME_DELETED,
            DocumentReader.ERROR_UNRECOGNIZED_VOLUME,
            DocumentReader.ERROR_NO_MEDIA_IN_DRIVE,
            DocumentReader.ERROR_DEVICE_NOT_CONNECTED,
            DocumentReader.ERROR_FILE_CORRUPT,
            DocumentReader.ERROR_DISK_CORRUPT,
        ];
        foreach (var code in volumeErrors)
        {
            var mapped = DocumentReader.MapWin32Error(code);
            Assert(mapped == DocumentError.VolumeUnavailable,
                $"expected VolumeUnavailable for Win32 error {code}, got {mapped}");
            Assert(mapped.UnavailableReason() == CoreUnavailability.Io,
                "expected VolumeUnavailable to map to CoreUnavailability.Io");
        }

        Assert(DocumentReader.IsVolumeGone(@"Z:\nonexistent_volume_root_check\", 0x12345678u),
            "expected nonexistent volume root to report IsVolumeGone == true");

        using var temp = new TempDir();
        var docPath = temp.WriteFile("removable.md", "# Removable Drive\n");
        using var loader = new DocumentLoader();
        loader.Open(DocumentCapability.FromPickerPath(docPath));
        WaitForIdle(loader);
        Assert(loader.Document is not null && loader.Document.Generation == 1, "expected initial load");

        // Simulate volume detachment when the file disappears on reload.
        File.Delete(docPath);
        loader.VolumeGoneCheck = (_, _) => true;
        loader.Reload();
        WaitForIdle(loader);

        Assert(loader.Error == DocumentError.VolumeUnavailable,
            $"expected NotFound on detached volume to promote to VolumeUnavailable, got {loader.Error}");
        Assert(loader.Availability() == CoreAvailability.Unavailable(CoreUnavailability.Io),
            $"expected Io availability for VolumeUnavailable, got {loader.Availability()}");
        Assert(loader.Document is not null && loader.Document.Generation == 1,
            "expected previous Document preserved when volume disappears");

        loader.CloseAndWait();
    }

    private static void TestFailedOpenPreservesSelectionAndRetriesFirstOpen()
    {
        using var temp = new TempDir();
        var firstPath = temp.WriteFile("first.md", "# First\n");
        var secondPath = temp.WriteFile("second.md", "# Second\n");

        using var loader = new DocumentLoader();

        // 1. First open fails session creation -> capability is retained so Reload retries Open.
        loader.SessionFactory = (_, _) => throw new CoreException(CoreErrorKind.Status, "simulated session create failure");
        var firstCap = DocumentCapability.FromPickerPath(firstPath);
        loader.Open(firstCap);
        Assert(loader.HasFile && ReferenceEquals(loader.Capability, firstCap),
            "expected failed first open to retain capability for retry");
        Assert(loader.Error == DocumentError.ParseFailed && !loader.IsLoading && loader.Document is null,
            "expected ParseFailed on simulated session failure");

        // Restore session creation and Reload -> succeeds and publishes Generation 1.
        loader.SessionFactory = null;
        loader.Reload();
        WaitForIdle(loader);
        Assert(loader.Error is null && loader.Document is not null && loader.Document.Generation == 1,
            "expected Reload after failed first open to succeed");
        Assert(loader.Document!.OutlineTitle(0) == "First", $"unexpected outline title: {loader.Document.OutlineTitle(0)}");

        // 2. Replacement open fails session creation while an in-flight reload of the first file is active ->
        //    cancels the in-flight reload, revokes the rejected capability, and preserves the first selection/document/error.
        using (var enteredReload = new ManualResetEventSlim(false))
        using (var releaseReload = new ManualResetEventSlim(false))
        {
            loader.ReadBarrier = () =>
            {
                enteredReload.Set();
                releaseReload.Wait(TimeSpan.FromSeconds(5));
            };
            loader.Reload();
            Assert(enteredReload.Wait(TimeSpan.FromSeconds(5)), "timed out waiting for reload barrier");
            loader.ReadBarrier = null;

            var serialBefore = loader.Serial;
            loader.SessionFactory = (_, _) => throw new CoreException(CoreErrorKind.Status, "simulated replacement failure");
            var secondCap = DocumentCapability.FromPickerPath(secondPath);
            loader.Open(secondCap);

            releaseReload.Set();
            Thread.Sleep(25);
            _ = loader.Poll();

            Assert(ReferenceEquals(loader.Capability, firstCap), "replacement failure must preserve previous capability");
            Assert(!firstCap.IsRevoked, "previous capability must not be revoked when replacement session creation fails");
            Assert(secondCap.IsRevoked, "rejected replacement capability must be revoked");
            Assert(loader.Document is not null && loader.Document.OutlineTitle(0) == "First",
                "replacement failure must preserve previous document");
            Assert(loader.Serial > serialBefore, "replacement failure must advance serial to cancel in-flight load");
            Assert(loader.Error == DocumentError.ParseFailed, "expected ParseFailed error preserved after in-flight reload finishes");
            Assert(loader.LastErrorWasOpenFailure, "expected LastErrorWasOpenFailure == true on replacement open failure");
        }

        loader.SessionFactory = null;
        loader.CloseAndWait();
    }

    private static void TestReloadLineageMissingFileRecoveryAndAvailability()
    {
        using var temp = new TempDir();
        var path = temp.WriteFile("lineage.md", "# v1\n\nInitial text.\n");
        var cap = DocumentCapability.FromPickerPath(path);

        using var loader = new DocumentLoader();
        loader.Open(cap);
        WaitForIdle(loader);

        Assert(loader.Document is not null && loader.Document.Generation == 1, "expected Generation 1");
        Assert(loader.Document!.OutlineTitle(0) == "v1", "expected v1 heading");
        Assert(!loader.Document.DocumentId.Contains("lineage", StringComparison.OrdinalIgnoreCase),
            $"DocumentId leaked filename: {loader.Document.DocumentId}");
        Assert(loader.WindowTitle == "lineage.md — Markdown", $"unexpected WindowTitle: {loader.WindowTitle}");

        // Unchanged reload preserves snapshot generation 1 (while consuming session submit generation 2).
        loader.Reload();
        WaitForIdle(loader);
        Assert(loader.Error is null && loader.Document is not null && loader.Document.Generation == 1,
            $"expected unchanged reload to stay at Generation 1, got {loader.Document?.Generation}");

        // Modified reload advances published snapshot to generation 3.
        File.WriteAllText(path, "# v2\n\nUpdated text.\n", new UTF8Encoding(false));
        loader.Reload();
        WaitForIdle(loader);
        Assert(loader.Error is null && loader.Document is not null && loader.Document.Generation == 3,
            $"expected Generation 3 after edit, got {loader.Document?.Generation}");
        Assert(loader.Document!.OutlineTitle(0) == "v2", "expected v2 heading");

        // Deleting file and reloading reports NotFound, marks session Deleted (generation 4), and keeps v2 document in memory.
        File.Delete(path);
        loader.Reload();
        WaitForIdle(loader);
        Assert(loader.Error == DocumentError.NotFound, $"expected NotFound, got {loader.Error}");
        Assert(loader.Availability() == CoreAvailability.Unavailable(CoreUnavailability.Deleted),
            $"expected Deleted availability, got {loader.Availability()}");
        Assert(loader.Document is not null && loader.Document.Generation == 3,
            "expected v2 document preserved during NotFound");

        // Recreating file and reloading recovers to generation 5 and restores Available status.
        File.WriteAllText(path, "# v3\n\nRecovered text.\n", new UTF8Encoding(false));
        loader.Reload();
        WaitForIdle(loader);
        Assert(loader.Error is null && loader.Document is not null && loader.Document.Generation == 5,
            $"expected Generation 5 after recovery, got {loader.Document?.Generation}");
        Assert(loader.Document!.OutlineTitle(0) == "v3", "expected v3 heading");
        Assert(loader.Availability() == CoreAvailability.Available, "expected Available after recovery");

        loader.CloseAndWait();
    }

    private static void TestInvalidUtf8AndComplexityDoNotCorruptAvailability()
    {
        using var temp = new TempDir();
        var path = temp.WriteFile("encoding.md", "# Valid UTF-8\n");

        using var loader = new DocumentLoader();
        loader.Open(DocumentCapability.FromPickerPath(path));
        WaitForIdle(loader);
        Assert(loader.Document is not null && loader.Document.Generation == 1, "expected Generation 1");
        Assert(loader.Availability() == CoreAvailability.Available, "expected Available");

        // Write invalid UTF-8 bytes and reload -> InvalidText, but availability remains Available.
        File.WriteAllBytes(path, [0xFF, 0xFE, 0x80, 0x41]);
        loader.Reload();
        WaitForIdle(loader);
        Assert(loader.Error == DocumentError.InvalidText, $"expected InvalidText, got {loader.Error}");
        Assert(loader.Availability() == CoreAvailability.Available,
            $"InvalidText must not mark session unavailable: {loader.Availability()}");

        // Non-local selection rejection also preserves session availability.
        loader.RejectSelection(DocumentError.NonLocalFile);
        Assert(loader.Error == DocumentError.NonLocalFile, "expected NonLocalFile");
        Assert(loader.Availability() == CoreAvailability.Available,
            "RejectSelection must not mark session unavailable");
        loader.CloseAndWait();

        // Complexity failure (MaxNodes = 2) maps to TooComplex and keeps availability Available.
        var complexPath = temp.WriteFile("complex.md", "# H1\n\nP1\n\nP2\n\nP3\n\nP4\n");
        var tightLimits = HostLimits.Default with { MaxNodes = 2 };
        using var complexLoader = new DocumentLoader(tightLimits);
        complexLoader.Open(DocumentCapability.FromPickerPath(complexPath));
        WaitForIdle(complexLoader);
        Assert(complexLoader.Error == DocumentError.TooComplex,
            $"expected TooComplex, got {complexLoader.Error}");
        Assert(complexLoader.Availability() == CoreAvailability.Available,
            $"TooComplex must not mark session unavailable: {complexLoader.Availability()}");
        complexLoader.CloseAndWait();
    }

    private static void TestStaleErrorsAndQueuedResultsAreSuppressed()
    {
        using var temp = new TempDir();
        var firstPath = temp.WriteFile("first.md", "# First\n");
        var secondPath = temp.WriteFile("second.md", "# Second\n");
        var thirdPath = temp.WriteFile("third.md", "# Third\n");

        using var enteredBarrier = new ManualResetEventSlim(false);
        using var releaseBarrier = new ManualResetEventSlim(false);
        var barrierOnce = 0;

        using var loader = new DocumentLoader();
        loader.ReadBarrier = () =>
        {
            if (Interlocked.CompareExchange(ref barrierOnce, 1, 0) == 0)
            {
                enteredBarrier.Set();
                releaseBarrier.Wait(TimeSpan.FromSeconds(5));
            }
        };

        loader.Open(DocumentCapability.FromPickerPath(firstPath));
        Assert(enteredBarrier.Wait(TimeSpan.FromSeconds(5)), "timed out waiting for first read to hit barrier");

        // While first read is stalled in-flight, rapidly open second then third so second is deduplicated in the queue.
        loader.Open(DocumentCapability.FromPickerPath(secondPath));
        loader.Open(DocumentCapability.FromPickerPath(thirdPath));
        releaseBarrier.Set();

        WaitForIdle(loader);
        Assert(loader.Error is null && loader.Document is not null, "expected third open to succeed");
        Assert(loader.Document!.OutlineTitle(0) == "Third", $"expected Third, got {loader.Document.OutlineTitle(0)}");
        Assert(loader.DiscardedResults >= 1, $"expected DiscardedResults >= 1, got {loader.DiscardedResults}");

        loader.CloseAndWait();
    }

    private static void TestReadBarrierReplacementAndCloseSuppressPublication()
    {
        using var temp = new TempDir();
        var path = temp.WriteFile("slow.md", "# Slow\n");

        using var enteredBarrier = new ManualResetEventSlim(false);
        using var releaseBarrier = new ManualResetEventSlim(false);

        var loader = new DocumentLoader();
        loader.ReadBarrier = () =>
        {
            enteredBarrier.Set();
            releaseBarrier.Wait(TimeSpan.FromSeconds(5));
        };

        loader.Open(DocumentCapability.FromPickerPath(path));
        Assert(enteredBarrier.Wait(TimeSpan.FromSeconds(5)), "timed out waiting for read barrier");

        // Close while the background read is still blocked at the barrier, then release the barrier.
        loader.CloseAndWait();
        releaseBarrier.Set();

        // Give the background thread a brief moment to finish unwinding and verify nothing is published.
        Thread.Sleep(25);
        Assert(!loader.Poll(), "closed loader must not publish results on Poll");
        Assert(loader.Document is null && loader.Error is null && !loader.HasFile,
            "closed loader must remain empty");
    }

    private static void TestCallbackBeforeInstallAndSynchronousCancelDoNotDeadlock()
    {
        using var session = new CoreSession("cancel-race-test");
        var source = Encoding.UTF8.GetBytes("# Cancel Race\n");

        // 1. MarkCompleted before Install must be a no-op and must not cancel or deadlock.
        var c1 = new Cancellation();
        c1.MarkCompleted();
        using var doneGate1 = new ManualResetEventSlim(false);
        var req1 = session.Parse(source, null, copyUnchanged: true, (_, _) => doneGate1.Set());
        c1.Install(req1);
        Assert(doneGate1.Wait(TimeSpan.FromSeconds(5)), "req1 did not complete");

        // 2. Cancel before Install must cancel the ParseRequest immediately inside Install without deadlock.
        var c2 = new Cancellation();
        c2.Cancel();
        using var doneGate2 = new ManualResetEventSlim(false);
        var req2 = session.Parse(source, null, copyUnchanged: true, (_, _) => doneGate2.Set());
        c2.Install(req2);
        Assert(doneGate2.Wait(TimeSpan.FromSeconds(5)), "req2 did not complete after synchronous cancel");

        session.Close();
    }

    private static void TestBusyAdmissionRetryRecoversWhenSlotReleases()
    {
        using var temp = new TempDir();
        var path = temp.WriteFile("busy.md", "# Busy Recovery\n");

        // Acquire all core admission slots directly to simulate saturation.
        var acquired = 0;
        while (CoreRuntime.TryAcquireSlot())
        {
            acquired++;
        }
        Assert(acquired == CoreRuntime.MaxAdmitted, $"expected {CoreRuntime.MaxAdmitted} slots, got {acquired}");

        try
        {
            // Release one slot after 60 ms on a timer thread while DocumentLoader retries admission.
            var releaseThread = new Thread(() =>
            {
                Thread.Sleep(60);
                CoreRuntime.ReleaseSlot();
            })
            {
                IsBackground = true,
            };
            releaseThread.Start();
            acquired--;

            using var loader = new DocumentLoader();
            loader.Open(DocumentCapability.FromPickerPath(path));
            WaitForIdle(loader);

            Assert(loader.Error is null && loader.Document is not null && loader.Document.Generation == 1,
                $"expected Busy retry to recover and publish Generation 1, got err={loader.Error}");
            loader.CloseAndWait();
            releaseThread.Join();

            // Now hold all slots again and verify:
            // (a) another window's file read executes on markdown-files without waiting behind the first window's Busy retry, and
            // (b) a short BusyRetryTimeout reports DocumentError.Busy cleanly.
            Assert(CoreRuntime.TryAcquireSlot(), "expected to re-acquire released slot");
            acquired++;

            var missingPath = System.IO.Path.Combine(temp.Path, "other-window-missing.md");
            using var otherWindowLoader = new DocumentLoader();
            using var timeoutLoader = new DocumentLoader
            {
                BusyRetryTimeout = TimeSpan.FromMilliseconds(150),
            };
            timeoutLoader.Open(DocumentCapability.FromPickerPath(path));

            // While timeoutLoader is actively waiting in its Busy admission retry, a second window's file read
            // (which completes on markdown-files before parse submission) must finish promptly without waiting 150 ms.
            otherWindowLoader.Open(DocumentCapability.FromPickerPath(missingPath));
            WaitForIdle(otherWindowLoader, timeoutMs: 100);
            Assert(otherWindowLoader.Error == DocumentError.NotFound,
                $"expected second window read to complete with NotFound while first window retries Busy, got {otherWindowLoader.Error}");
            otherWindowLoader.CloseAndWait();

            WaitForIdle(timeoutLoader);

            Assert(timeoutLoader.Error == DocumentError.Busy,
                $"expected DocumentError.Busy after retry timeout, got {timeoutLoader.Error}");
            timeoutLoader.CloseAndWait();
        }
        finally
        {
            for (var i = 0; i < acquired; i++)
            {
                CoreRuntime.ReleaseSlot();
            }
        }
    }

    private static void WaitForIdle(DocumentLoader loader, int timeoutMs = 5000)
    {
        var sw = Stopwatch.StartNew();
        while (sw.ElapsedMilliseconds < timeoutMs)
        {
            _ = loader.Poll();
            if (!loader.IsLoading)
            {
                return;
            }
            Thread.Sleep(5);
        }
        _ = loader.Poll();
        Assert(!loader.IsLoading, "timed out waiting for DocumentLoader to finish loading");
    }

    internal static void SetReadDenyAcl(string path, bool deny)
    {
        var currentSid = WindowsIdentity.GetCurrent().User
            ?? throw new InvalidOperationException("Current WindowsIdentity has no User SID.");
        var fileInfo = new FileInfo(path);
        var security = fileInfo.GetAccessControl();
        var rule = new FileSystemAccessRule(
            currentSid,
            FileSystemRights.ReadData | FileSystemRights.ReadAttributes | FileSystemRights.ReadExtendedAttributes,
            InheritanceFlags.None,
            PropagationFlags.None,
            AccessControlType.Deny);

        if (deny)
        {
            security.AddAccessRule(rule);
        }
        else
        {
            security.RemoveAccessRuleSpecific(rule);
        }

        fileInfo.SetAccessControl(security);
    }

    private static bool Utf8IsValid(byte[] bytes)
    {
        try
        {
            var utf8 = new UTF8Encoding(encoderShouldEmitUTF8Identifier: false, throwOnInvalidBytes: true);
            _ = utf8.GetString(bytes);
            return true;
        }
        catch
        {
            return false;
        }
    }

    private static void AssertNoLiveHandles(string context)
    {
        var sw = Stopwatch.StartNew();
        while (sw.ElapsedMilliseconds < 2000)
        {
            GC.Collect();
            GC.WaitForPendingFinalizers();
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

    private static void Assert([DoesNotReturnIf(false)] bool condition, string message)
    {
        if (!condition)
        {
            throw new InvalidOperationException(message);
        }
    }

    private sealed class TempDir : IDisposable
    {
        public string Path { get; }

        public TempDir()
        {
            Path = System.IO.Path.Combine(
                System.IO.Path.GetTempPath(),
                $"markdown-win001c-tests-{Guid.NewGuid():N}");
            Directory.CreateDirectory(Path);
        }

        public string WriteFile(string name, string content)
        {
            var full = System.IO.Path.Combine(Path, name);
            File.WriteAllText(full, content, new UTF8Encoding(encoderShouldEmitUTF8Identifier: false));
            return full;
        }

        public string WriteBytes(string name, byte[] bytes)
        {
            var full = System.IO.Path.Combine(Path, name);
            File.WriteAllBytes(full, bytes);
            return full;
        }

        public void Dispose()
        {
            try
            {
                if (Directory.Exists(Path))
                {
                    Directory.Delete(Path, recursive: true);
                }
            }
            catch
            {
                // Best effort cleanup for temp test directory.
            }
        }
    }

    private static void TestDocumentWatcherModificationDeletionAndReappearance()
    {
        var tempDir = Path.Combine(Path.GetTempPath(), $"doc-watcher-test-{Guid.NewGuid():N}");
        Directory.CreateDirectory(tempDir);
        try
        {
            var filePath = Path.Combine(tempDir, "watched.md");
            File.WriteAllText(filePath, "# Initial\n", new UTF8Encoding(false));

            var changeSignal = new AutoResetEvent(false);
            using var watcher = new DocumentWatcher(filePath, () => changeSignal.Set());

            // 1. Modification
            File.WriteAllText(filePath, "# Modified\n", new UTF8Encoding(false));
            Assert(changeSignal.WaitOne(TimeSpan.FromSeconds(3)), "watcher signaled on modification");

            // 2. Deletion
            File.Delete(filePath);
            Assert(changeSignal.WaitOne(TimeSpan.FromSeconds(3)), "watcher signaled on deletion");

            // 3. Reappearance
            File.WriteAllText(filePath, "# Reappeared\n", new UTF8Encoding(false));
            Assert(changeSignal.WaitOne(TimeSpan.FromSeconds(3)), "watcher signaled on reappearance");
        }
        finally
        {
            try { Directory.Delete(tempDir, recursive: true); } catch { }
        }
    }

    private static void TestExternalRefreshDispositionsAndStaleRetention()
    {
        var tempDir = Path.Combine(Path.GetTempPath(), $"doc-refresh-test-{Guid.NewGuid():N}");
        Directory.CreateDirectory(tempDir);
        try
        {
            var filePath = Path.Combine(tempDir, "refresh.md");
            var original = "# Refresh\n\nInitial body.\n";
            File.WriteAllText(filePath, original, new UTF8Encoding(false));

            using var loader = new DocumentLoader();
            loader.Open(DocumentCapability.FromPickerPath(filePath));

            var sw = Stopwatch.StartNew();
            while (sw.ElapsedMilliseconds < 5000 && (loader.Document is null || loader.IsLoading))
            {
                loader.Poll();
                Thread.Sleep(10);
            }
            Assert(loader.Document is not null && !loader.IsLoading, "initial document loaded");
            var gen1 = loader.Document!.Generation;

            // 1. Unchanged content -> disposition Unchanged, no new generation
            loader.RefreshExternal();
            sw.Restart();
            while (sw.ElapsedMilliseconds < 5000 && loader.IsLoading)
            {
                loader.Poll();
                Thread.Sleep(10);
            }
            Assert(loader.Document.Generation == gen1, "unchanged refresh preserves generation");

            // 2. Modified content -> disposition ReadRequired, generation advances
            File.WriteAllText(filePath, "# Refresh\n\nUpdated body content.\n", new UTF8Encoding(false));
            loader.RefreshExternal();
            sw.Restart();
            while (sw.ElapsedMilliseconds < 5000 && (loader.Document.Generation == gen1 || loader.IsLoading))
            {
                loader.Poll();
                Thread.Sleep(10);
            }
            Assert(loader.Document.Generation > gen1, "external edit advances generation");
            var gen2 = loader.Document.Generation;

            // 3. Deletion -> NotFound error, existing document preserved
            File.Delete(filePath);
            loader.RefreshExternal();
            sw.Restart();
            while (sw.ElapsedMilliseconds < 5000 && (loader.Error is null || loader.IsLoading))
            {
                loader.Poll();
                Thread.Sleep(10);
            }
            Assert(loader.Error == DocumentError.NotFound, "deletion reports NotFound");
            Assert(loader.Document.Generation == gen2, "stale document retained across deletion");
            Assert(loader.Availability() == CoreAvailability.Unavailable(CoreUnavailability.Deleted), "availability is Deleted");
        }
        finally
        {
            try { Directory.Delete(tempDir, recursive: true); } catch { }
        }
    }

    private static void TestFolderCapabilityContainmentGrantRevocationAndRejection()
    {
        var tempRoot = Path.Combine(Path.GetTempPath(), $"folder-cap-test-{Guid.NewGuid():N}");
        var workspaceDir = Path.Combine(tempRoot, "docs");
        var siblingDir = Path.Combine(tempRoot, "docs2");
        var outsideDir = Path.Combine(tempRoot, "outside");
        Directory.CreateDirectory(workspaceDir);
        Directory.CreateDirectory(siblingDir);
        Directory.CreateDirectory(outsideDir);
        try
        {
            var insideFile = Path.Combine(workspaceDir, "readme.md");
            var siblingFile = Path.Combine(siblingDir, "secret.md");
            var outsideFile = Path.Combine(outsideDir, "outside.md");
            File.WriteAllText(insideFile, "# Inside\n", new UTF8Encoding(false));
            File.WriteAllText(siblingFile, "# Sibling\n", new UTF8Encoding(false));
            File.WriteAllText(outsideFile, "# Outside\n", new UTF8Encoding(false));

            Assert(!FolderCapability.TryFromUserInput(@"\\server\share\docs", out _, out var uncErr) &&
                   uncErr == FolderError.NonLocalFolder, "UNC folder rejected as NonLocalFolder");
            Assert(!FolderCapability.TryFromUserInput("https://example.com/docs", out _, out var httpErr) &&
                   httpErr == FolderError.NonLocalFolder, "HTTP folder URI rejected as NonLocalFolder");

            var fileAsFolder = FolderCapability.FromPickerPath(insideFile);
            Assert(!fileAsFolder.ValidateAccess(out var notDirErr) &&
                   notDirErr == FolderError.NotDirectory, "regular file rejected as NotDirectory");

            var missingFolder = FolderCapability.FromPickerPath(Path.Combine(tempRoot, "missing"));
            Assert(!missingFolder.ValidateAccess(out var missingErr) &&
                   missingErr == FolderError.NotFound, "missing folder rejected as NotFound");

            var folder = FolderCapability.FromPickerPath(workspaceDir);
            Assert(folder.ToString() == "FolderCapability(<private>)", "FolderCapability.ToString is redacted");
            Assert(folder.ValidateAccess(out _), "valid folder validates");

            Assert(folder.ResolveDocument("readme.md", out var childCap, out _) && childCap is not null,
                "inside relative document resolves");
            var readOk = DocumentReader.Read(childCap!, 1024 * 1024);
            Assert(readOk.IsOk, "folder-bound document reads");

            Assert(!folder.ResolveDocument(@"..\outside\outside.md", out _, out var traversalErr) &&
                   traversalErr == FolderError.OutsideFolder, "parent traversal rejected as OutsideFolder");
            Assert(!folder.ResolveDocument(siblingFile, out _, out var prefixErr) &&
                   prefixErr == FolderError.OutsideFolder, "sibling prefix directory rejected as OutsideFolder");
            Assert(!folder.ResolveDocument(workspaceDir, out _, out var selfErr) &&
                   selfErr == FolderError.OutsideFolder, "folder root itself rejected as document");

            // Symlink/junction escape check and symlinked workspace root check (Developer Mode or junction)
            var junctionDir = Path.Combine(workspaceDir, "escape-link");
            var rootAliasDir = Path.Combine(tempRoot, "docs-alias");
            try
            {
                Directory.CreateSymbolicLink(junctionDir, outsideDir);
                Assert(!folder.ResolveDocument(Path.Combine("escape-link", "outside.md"), out _, out var linkErr) &&
                       linkErr == FolderError.OutsideFolder, "symlink escaping folder rejected as OutsideFolder");

                Directory.CreateSymbolicLink(rootAliasDir, workspaceDir);
                var aliasFolder = FolderCapability.FromPickerPath(rootAliasDir);
                Assert(aliasFolder.ResolveDocument(Path.Combine(rootAliasDir, "readme.md"), out var aliasChild, out _) &&
                       aliasChild is not null, "rooted child inside symlinked workspace root resolves");
            }
            catch (UnauthorizedAccessException)
            {
                // Symlink creation without Developer Mode / privilege is skipped
            }
            catch (IOException)
            {
            }

            folder.Revoke();
            Assert(folder.IsRevoked, "folder grant marks revoked");
            Assert(childCap!.IsRevoked, "folder revocation propagates to child document capability");
            var readAfterRevoke = DocumentReader.Read(childCap, 1024 * 1024);
            Assert(!readAfterRevoke.IsOk && readAfterRevoke.Error == DocumentError.AccessRevoked,
                "child read after folder revocation returns AccessRevoked");
        }
        finally
        {
            try { Directory.Delete(tempRoot, recursive: true); } catch { }
        }
    }

    private static void TestFolderAclDenialRevocationAndMoveRecovery()
    {
        var tempRoot = Path.Combine(Path.GetTempPath(), $"folder-acl-test-{Guid.NewGuid():N}");
        var folderPath = Path.Combine(tempRoot, "workspace");
        var movedPath = Path.Combine(tempRoot, "workspace-moved");
        Directory.CreateDirectory(folderPath);
        var currentUser = WindowsIdentity.GetCurrent().User!;
        FileSystemAccessRule? denyRule = null;
        try
        {
            // 1. Pre-existing ACL denial -> PermissionDenied
            var dirInfo = new DirectoryInfo(folderPath);
            var security = dirInfo.GetAccessControl();
            denyRule = new FileSystemAccessRule(
                currentUser,
                FileSystemRights.ListDirectory | FileSystemRights.ReadData,
                AccessControlType.Deny);
            security.AddAccessRule(denyRule);
            dirInfo.SetAccessControl(security);

            var deniedFolder = FolderCapability.FromPickerPath(folderPath);
            Assert(!deniedFolder.ValidateAccess(out var deniedErr) && deniedErr == FolderError.PermissionDenied,
                $"denied folder reported {deniedErr}");

            // Restore ACL
            security.RemoveAccessRule(denyRule);
            dirInfo.SetAccessControl(security);
            denyRule = null;

            // 2. Validated then ACL withdrawn -> AccessRevoked
            var activeFolder = FolderCapability.FromPickerPath(folderPath);
            Assert(activeFolder.ValidateAccess(out _), "folder validates after ACL restored");

            security = dirInfo.GetAccessControl();
            denyRule = new FileSystemAccessRule(
                currentUser,
                FileSystemRights.ListDirectory | FileSystemRights.ReadData,
                AccessControlType.Deny);
            security.AddAccessRule(denyRule);
            dirInfo.SetAccessControl(security);

            Assert(!activeFolder.ValidateAccess(out var revokedErr) && revokedErr == FolderError.AccessRevoked,
                $"post-validation ACL withdrawal reported {revokedErr}");

            security.RemoveAccessRule(denyRule);
            dirInfo.SetAccessControl(security);
            denyRule = null;

            // 3. Moved folder -> NotFound, then relocated folder validates
            Directory.Move(folderPath, movedPath);
            Assert(!activeFolder.ValidateAccess(out var movedErr) && movedErr == FolderError.NotFound,
                $"moved folder reported {movedErr}");

            var relocated = FolderCapability.FromPickerPath(movedPath);
            Assert(relocated.ValidateAccess(out _), "relocated folder validates");
        }
        finally
        {
            if (denyRule is not null && Directory.Exists(folderPath))
            {
                try
                {
                    var dirInfo = new DirectoryInfo(folderPath);
                    var sec = dirInfo.GetAccessControl();
                    sec.RemoveAccessRule(denyRule);
                    dirInfo.SetAccessControl(sec);
                }
                catch { }
            }
            try { Directory.Delete(tempRoot, recursive: true); } catch { }
        }
    }

    private static void TestRecentsStoreCapsDedupUntrustedJsonAndFutureVersion()
    {
        var tempDir = Path.Combine(Path.GetTempPath(), $"recents-test-{Guid.NewGuid():N}");
        Directory.CreateDirectory(tempDir);
        try
        {
            var storePath = Path.Combine(tempDir, "recents.json");
            var store = new RecentsStore(storePath);

            // Add 12 files -> capped at 10, newest first
            for (var i = 1; i <= 12; i++)
            {
                var file = Path.Combine(tempDir, $"doc{i:D2}.md");
                store.RecordFile(DocumentCapability.FromPickerPath(file));
            }
            Assert(store.Files.Count == 10, $"recent files capped at 10 (got {store.Files.Count})");
            Assert(store.Files[0].DisplayName == "doc12.md", "newest recent file is first");

            // Case-insensitive dedup moves existing entry to top
            store.RecordFile(DocumentCapability.FromPickerPath(Path.Combine(tempDir, "DOC05.MD")));
            Assert(store.Files.Count == 10, "dedup preserves 10-entry cap");
            Assert(store.Files[0].DisplayName.Equals("doc05.md", StringComparison.OrdinalIgnoreCase),
                "deduped entry moves to front");

            // Folders cap & relocate
            for (var i = 1; i <= 11; i++)
            {
                var dir = Path.Combine(tempDir, $"ws{i:D2}");
                store.RecordFolder(FolderCapability.FromPickerPath(dir));
            }
            Assert(store.Folders.Count == 10, "recent folders capped at 10");
            var oldFolder = Path.Combine(tempDir, "ws05");
            var newFolder = Path.Combine(tempDir, "ws05-moved");
            Assert(store.Relocate(RecentKind.Folder, oldFolder, newFolder), "Relocate succeeds");
            Assert(store.Folders[0].DisplayName == "ws05-moved", "relocated folder moves to front");

            // Reload from disk verifies persistence
            var reloaded = new RecentsStore(storePath);
            Assert(reloaded.Files.Count == 10 && reloaded.Folders.Count == 10, "reloaded recents match");

            // Untrusted JSON with mixed invalid/UNC/relative/valid entries
            var validFile = Path.Combine(tempDir, "valid.md");
            var validFolder = Path.Combine(tempDir, "valid-ws");
            var untrustedJson = $$"""
            {
              "version": 1,
              "files": [
                { "path": "\\\\server\\share\\unc.md", "display_name": "unc.md" },
                { "path": "relative.md", "display_name": "relative.md" },
                { "path": 12345 },
                { "path": "{{validFile.Replace("\\", "\\\\")}}", "display_name": "valid\u202E.md" }
              ],
              "folders": [
                { "path": "https://example.com/ws" },
                { "path": "{{validFolder.Replace("\\", "\\\\")}}" }
              ]
            }
            """;
            File.WriteAllText(storePath, untrustedJson, new UTF8Encoding(false));
            var sanitized = new RecentsStore(storePath);
            Assert(sanitized.Files.Count == 1 && !sanitized.Files[0].DisplayName.Contains('\u202E'),
                "untrusted JSON drops invalid/UNC/relative entries and sanitizes bidi display name");
            Assert(sanitized.Folders.Count == 1 && sanitized.Folders[0].DisplayName == "valid-ws",
                "untrusted JSON keeps valid folder");

            // Future schema version is never overwritten
            var futureJson = "{ \"version\": 99, \"files\": [], \"folders\": [] }";
            File.WriteAllText(storePath, futureJson, new UTF8Encoding(false));
            var futureStore = new RecentsStore(storePath);
            futureStore.RecordFile(DocumentCapability.FromPickerPath(validFile));
            Assert(File.ReadAllText(storePath).Contains("\"version\": 99"),
                "future recents schema version is preserved on disk");
        }
        finally
        {
            try { Directory.Delete(tempDir, recursive: true); } catch { }
        }
    }

    private static void TestRecentsAvailabilityChecks()
    {
        var tempDir = Path.Combine(Path.GetTempPath(), $"recents-avail-test-{Guid.NewGuid():N}");
        var folderPath = Path.Combine(tempDir, "ws");
        Directory.CreateDirectory(folderPath);
        var filePath = Path.Combine(folderPath, "note.md");
        File.WriteAllText(filePath, "# Note\n", new UTF8Encoding(false));

        var currentUser = WindowsIdentity.GetCurrent().User!;
        FileSystemAccessRule? denyRule = null;
        try
        {
            Assert(RecentsStore.CheckAvailability(new RecentEntry(RecentKind.File, filePath, "note.md", DateTimeOffset.UtcNow))
                   == RecentAvailability.Available, "existing file is Available");
            Assert(RecentsStore.CheckAvailability(new RecentEntry(RecentKind.Folder, folderPath, "ws", DateTimeOffset.UtcNow))
                   == RecentAvailability.Available, "existing folder is Available");

            // Wrong type
            Assert(RecentsStore.CheckAvailability(new RecentEntry(RecentKind.File, folderPath, "ws", DateTimeOffset.UtcNow))
                   == RecentAvailability.InvalidType, "directory as recent file is InvalidType");
            Assert(RecentsStore.CheckAvailability(new RecentEntry(RecentKind.Folder, filePath, "note.md", DateTimeOffset.UtcNow))
                   == RecentAvailability.InvalidType, "file as recent folder is InvalidType");

            // ACL denied
            var dirInfo = new DirectoryInfo(folderPath);
            var security = dirInfo.GetAccessControl();
            denyRule = new FileSystemAccessRule(
                currentUser,
                FileSystemRights.ListDirectory | FileSystemRights.ReadData,
                AccessControlType.Deny);
            security.AddAccessRule(denyRule);
            dirInfo.SetAccessControl(security);

            Assert(RecentsStore.CheckAvailability(new RecentEntry(RecentKind.Folder, folderPath, "ws", DateTimeOffset.UtcNow))
                   == RecentAvailability.PermissionDenied, "denied folder is PermissionDenied");

            security.RemoveAccessRule(denyRule);
            dirInfo.SetAccessControl(security);
            denyRule = null;

            // Missing
            File.Delete(filePath);
            Assert(RecentsStore.CheckAvailability(new RecentEntry(RecentKind.File, filePath, "note.md", DateTimeOffset.UtcNow))
                   == RecentAvailability.Missing, "deleted file is Missing");
        }
        finally
        {
            if (denyRule is not null && Directory.Exists(folderPath))
            {
                try
                {
                    var dirInfo = new DirectoryInfo(folderPath);
                    var sec = dirInfo.GetAccessControl();
                    sec.RemoveAccessRule(denyRule);
                    dirInfo.SetAccessControl(sec);
                }
                catch { }
            }
            try { Directory.Delete(tempDir, recursive: true); } catch { }
        }
    }

    private static void TestWorkspaceTreeEnumerationFilteringAndLargeFolder()
    {
        var tempDir = Path.Combine(Path.GetTempPath(), $"md-win-tree-{Guid.NewGuid():N}");
        Directory.CreateDirectory(tempDir);
        try
        {
            var docsDir = Path.Combine(tempDir, "docs");
            var subDir = Path.Combine(docsDir, "sub");
            var hiddenDir = Path.Combine(tempDir, ".hidden");
            Directory.CreateDirectory(subDir);
            Directory.CreateDirectory(hiddenDir);

            File.WriteAllText(Path.Combine(tempDir, "readme.md"), "# Root");
            File.WriteAllText(Path.Combine(tempDir, "notes.txt"), "text");
            File.WriteAllText(Path.Combine(tempDir, ".gitignore"), "*");
            File.WriteAllText(Path.Combine(docsDir, "guide.markdown"), "# Guide");
            File.WriteAllText(Path.Combine(docsDir, "inv\u200Bfile.md"), "# Invisible");
            File.WriteAllText(Path.Combine(subDir, "deep.mdx"), "# Deep");
            File.WriteAllText(Path.Combine(hiddenDir, "secret.md"), "# Secret");

            var folder = FolderCapability.FromPickerPath(tempDir);
            var tree = WorkspaceTree.Enumerate(folder);

            Assert(!tree.Truncated, "small workspace should not be truncated");
            Assert(tree.TotalMarkdownFiles == 5, $"expected 5 markdown files, got {tree.TotalMarkdownFiles}");

            var expanded = new HashSet<ulong>();
            var topMd = tree.BuildVisibleRows(expanded, showAllFiles: false, showHidden: false);
            // Directories first: "docs", then markdown file "readme.md"
            Assert(topMd.Count == 2, $"expected 2 top-level visible rows, got {topMd.Count}");
            Assert(topMd[0].Entry.Name == "docs" && topMd[0].Entry.IsDirectory, "docs dir should be first");
            Assert(topMd[1].Entry.Name == "readme.md" && topMd[1].Entry.IsMarkdown, "readme.md should be second");
            Assert(topMd[0].PositionInSet == 1 && topMd[0].SizeOfSet == 2, "docs PositionInSet/SizeOfSet");
            Assert(topMd[1].PositionInSet == 2 && topMd[1].SizeOfSet == 2, "readme.md PositionInSet/SizeOfSet");

            // Expand docs
            expanded.Add(topMd[0].Entry.Id);
            var expandedMd = tree.BuildVisibleRows(expanded, showAllFiles: false, showHidden: false);
            Assert(expandedMd.Count == 5, $"expected 5 visible rows when docs expanded, got {expandedMd.Count}");
            Assert(expandedMd[1].Entry.Name == "sub" && expandedMd[1].Entry.Depth == 1, "sub dir at depth 1");
            Assert(expandedMd[2].Entry.Name == "guide.markdown" && expandedMd[2].Entry.Depth == 1, "guide.markdown at depth 1");
            Assert(expandedMd[3].Entry.Name == "inv\u200Bfile.md" && expandedMd[3].Entry.HasInvisible, "invisible flag preserved");

            // Show all files (including notes.txt)
            var allFiles = tree.BuildVisibleRows(expanded, showAllFiles: true, showHidden: false);
            Assert(allFiles.Any(r => r.Entry.Name == "notes.txt"), "notes.txt visible with ShowAllFiles");
            Assert(!allFiles.Any(r => r.Entry.Name == ".gitignore" || r.Entry.Name == ".hidden"), "hidden excluded");

            // Show hidden
            var withHidden = tree.BuildVisibleRows(expanded, showAllFiles: true, showHidden: true);
            Assert(withHidden.Any(r => r.Entry.Name == ".hidden"), ".hidden visible with ShowHidden");
            Assert(withHidden.Any(r => r.Entry.Name == ".gitignore"), ".gitignore visible with ShowHidden");

            // ResolveDocument through FolderCapability
            Assert(folder.ResolveDocument(expandedMd[2].Entry.RelativePath, out var guideCap, out _) && guideCap is not null,
                "ResolveDocument succeeds for enumerated relative path");

            // Large folder check (2,500 files)
            var largeDir = Path.Combine(tempDir, "large");
            Directory.CreateDirectory(largeDir);
            for (var i = 0; i < 2500; i++)
            {
                File.WriteAllText(Path.Combine(largeDir, $"doc_{i:D4}.md"), "# Doc");
            }
            var largeTree = WorkspaceTree.Enumerate(folder);
            Assert(largeTree.Entries.Count >= 2500, $"expected >= 2500 entries, got {largeTree.Entries.Count}");
            var largeEntry = largeTree.Entries.First(e => e.Name == "large");
            var largeRows = largeTree.BuildVisibleRows(new HashSet<ulong> { largeEntry.Id }, showAllFiles: false, showHidden: false);
            Assert(largeRows.Count >= 2500, $"expected >= 2500 visible rows, got {largeRows.Count}");
        }
        finally
        {
            try { Directory.Delete(tempDir, recursive: true); } catch { }
        }
    }

    private static void TestWorkspaceSearchBoundedCancellationAndRevocation()
    {
        var tempDir = Path.Combine(Path.GetTempPath(), $"ws-search-test-{Guid.NewGuid():N}");
        Directory.CreateDirectory(tempDir);
        try
        {
            var docsDir = Path.Combine(tempDir, "docs");
            Directory.CreateDirectory(docsDir);
            File.WriteAllText(Path.Combine(tempDir, "alpha.md"), "# Alpha\n\nTarget keyword here.\n", new UTF8Encoding(false));
            File.WriteAllText(Path.Combine(docsDir, "beta-target.md"), "# Beta\n\nAnother target keyword.\n", new UTF8Encoding(false));
            File.WriteAllText(Path.Combine(docsDir, "notes.txt"), "target in txt should be skipped\n", new UTF8Encoding(false));
            File.WriteAllText(Path.Combine(tempDir, ".hidden.md"), "# Hidden\n\ntarget in hidden\n", new UTF8Encoding(false));

            Assert(FolderCapability.TryFromUserInput(tempDir, out var folder, out _) && folder is not null,
                "FolderCapability created");
            var tree = WorkspaceTree.Enumerate(folder!);

            // 1. Name + content search
            var snap = WorkspaceSearch.Run(folder!, tree, "target");
            Assert(!snap.IsRunning && !snap.IsCancelled, "search completed");
            Assert(snap.NameHits.Count == 1 && snap.NameHits[0].RelativePath == "docs/beta-target.md",
                $"expected 1 name hit, got {snap.NameHits.Count}");
            Assert(snap.ContentHits.Count == 2, $"expected 2 content hits, got {snap.ContentHits.Count}");
            Assert(snap.ContentHits.All(h => h.Anchor.Length > 0 && h.SourceSha256.Length == 64),
                "content hits carry anchors and hashes");

            // 2. Bounded MaxFiles stops cleanly
            var bounded = WorkspaceSearch.Run(
                folder!,
                tree,
                "target",
                new WorkspaceSearchOptions(MaxFiles: 1));
            Assert(bounded.IsIncomplete && bounded.StopReason == CoreFfi.MD_STOP_FILES && bounded.FilesRead == 1,
                $"expected stop at MaxFiles=1, got stop={bounded.StopReason}, read={bounded.FilesRead}");

            // 3. Cancellation before/during search
            using var cts = new CancellationTokenSource();
            cts.Cancel();
            var cancelled = WorkspaceSearch.Run(folder!, tree, "target", cancellationToken: cts.Token);
            Assert(cancelled.IsCancelled, "cancelled search reports IsCancelled");

            // 4. Folder revocation
            folder!.Revoke();
            var revoked = WorkspaceSearch.Run(folder!, tree, "target");
            Assert(!string.IsNullOrEmpty(revoked.ErrorMessage), "revoked folder reports error");
        }
        finally
        {
            try { Directory.Delete(tempDir, recursive: true); } catch { }
        }
    }

    private static void TestWorkspaceWatcherChangeStormAndInFlightGating()
    {
        var tempDir = Path.Combine(Path.GetTempPath(), $"markdown-win002e-storm-{Guid.NewGuid():N}");
        Directory.CreateDirectory(tempDir);
        try
        {
            var folder = FolderCapability.FromPickerPath(tempDir);
            var changeCount = 0;
            using var firstChanged = new ManualResetEventSlim(false);
            using var secondChanged = new ManualResetEventSlim(false);
            FolderError? lostError = null;

            using var watcher = new WorkspaceWatcher(
                folder,
                () =>
                {
                    var count = Interlocked.Increment(ref changeCount);
                    if (count == 1)
                    {
                        firstChanged.Set();
                    }
                    else if (count == 2)
                    {
                        secondChanged.Set();
                    }
                },
                err => lostError = err,
                debounceMs: 60,
                maxCoalesceMs: 300);

            Assert(watcher.IsArmed, "WorkspaceWatcher is armed");

            // Change storm: 150 rapid file writes should coalesce into a small bounded count (1 while in-flight)
            for (var i = 0; i < 150; i++)
            {
                File.WriteAllText(Path.Combine(tempDir, $"note-{i:D3}.md"), $"# Note {i}\n");
            }

            Assert(firstChanged.Wait(3000), "coalesced change callback fired after storm");
            // While refresh is still in-flight (NotifyRefreshCompleted not called yet),
            // additional changes must be gated and not fire a second overlapping callback.
            File.WriteAllText(Path.Combine(tempDir, "during-inflight.md"), "# Inflight\n");
            Thread.Sleep(250);
            Assert(Volatile.Read(ref changeCount) == 1,
                $"expected 1 callback while refresh in-flight, got {Volatile.Read(ref changeCount)}");

            watcher.NotifyRefreshCompleted();
            Assert(secondChanged.Wait(3000), "queued in-flight change triggered follow-up refresh");
            watcher.NotifyRefreshCompleted();

            Assert(lostError is null, $"unexpected access loss during storm: {lostError}");
        }
        finally
        {
            try { Directory.Delete(tempDir, recursive: true); } catch { }
        }
    }

    private static void TestWorkspaceWatcherAccessLossOnDeletionAndRevocation()
    {
        var tempDir = Path.Combine(Path.GetTempPath(), $"markdown-win002e-loss-{Guid.NewGuid():N}");
        Directory.CreateDirectory(tempDir);
        try
        {
            // 1. Deletion detection via parent watcher / validation
            var folder = FolderCapability.FromPickerPath(tempDir);
            using var lostSignal = new ManualResetEventSlim(false);
            FolderError? lostError = null;
            using (var watcher = new WorkspaceWatcher(
                folder,
                () => { },
                err =>
                {
                    lostError = err;
                    lostSignal.Set();
                },
                debounceMs: 40,
                maxCoalesceMs: 200))
            {
                Directory.Delete(tempDir, recursive: true);
                Assert(lostSignal.Wait(3000), "WorkspaceWatcher detected deleted root folder");
                Assert(lostError == FolderError.NotFound, $"expected NotFound, got {lostError}");
            }

            // 2. Grant revocation detection
            Directory.CreateDirectory(tempDir);
            var folder2 = FolderCapability.FromPickerPath(tempDir);
            using var revokedSignal = new ManualResetEventSlim(false);
            FolderError? revokedError = null;
            using (var watcher2 = new WorkspaceWatcher(
                folder2,
                () => { },
                err =>
                {
                    revokedError = err;
                    revokedSignal.Set();
                },
                debounceMs: 40,
                maxCoalesceMs: 200))
            {
                folder2.Revoke();
                watcher2.ScheduleChange();
                Assert(revokedSignal.Wait(3000), "WorkspaceWatcher detected revoked folder grant");
                Assert(revokedError == FolderError.AccessRevoked, $"expected AccessRevoked, got {revokedError}");
            }
        }
        finally
        {
            try { Directory.Delete(tempDir, recursive: true); } catch { }
        }
    }
}
