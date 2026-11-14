//! Package consumer tests, run against the debug and the release core package:
//! negotiation, typed errors, owned v2 values, failures, cancellation, close and
//! lifecycle. Live-handle counts are process-wide, so the tests run one at a time.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, channel};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use markdown_linux_core::ffi::{self, size_of};
use markdown_linux_core::runtime::run_on_worker;
use markdown_linux_core::*;

static SERIAL: Mutex<()> = Mutex::new(());

/// Serializes tests and checks that each leaves no native handle behind.
struct Isolated {
    _guard: MutexGuard<'static, ()>,
    baseline: HandleCounts,
}

fn isolated() -> Isolated {
    let guard = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    drain_worker();
    Isolated {
        _guard: guard,
        baseline: live_handles().unwrap(),
    }
}

impl Drop for Isolated {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            drain_worker();
            assert_eq!(
                live_handles().unwrap(),
                self.baseline,
                "native handles leaked"
            );
            assert_eq!(pending_parses(), 0, "admission leaked");
        }
    }
}

/// Wait until everything queued on the core worker so far has run.
fn drain_worker() {
    let (sender, receiver) = channel();
    run_on_worker(move || sender.send(()).unwrap());
    receiver.recv_timeout(Duration::from_secs(60)).unwrap();
}

/// Occupy the worker until the returned sender is dropped or sent to.
fn hold_worker() -> std::sync::mpsc::Sender<()> {
    let (release, held) = channel::<()>();
    let (started, running) = channel();
    run_on_worker(move || {
        started.send(()).unwrap();
        let _ = held.recv();
    });
    running.recv().unwrap();
    release
}

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap()
}

fn profile(name: &str) -> Vec<u8> {
    let path = repository().join("build/fixtures/performance").join(name);
    std::fs::read(&path).unwrap_or_else(|_| {
        panic!(
            "{} is missing: run tools/generate_performance_fixtures.py",
            path.display()
        )
    })
}

fn parse_async(
    session: &CoreSession,
    source: Vec<u8>,
) -> (ParseRequest, Receiver<Result<CoreResult, CoreError>>) {
    let (sender, receiver) = channel();
    let request = session
        .parse(source, None, true, move |result| {
            sender.send(result).unwrap()
        })
        .unwrap();
    (request, receiver)
}

const SAMPLE: &str = "---\ntitle: Sample title\n---\n# Heading one\n\nText with a \
    [link](https://example.com/a) and ![alt](images/pic.png).\n\n## Second\n\n- [x] done\n";

#[test]
fn package_matches_build_profile() {
    let expected = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    assert_eq!(PACKAGE_CONFIGURATION, expected);
    let identity = std::fs::read_to_string(
        repository()
            .join("build/linux/core-package")
            .join(expected)
            .join("identity"),
    )
    .unwrap();
    assert!(identity.contains(&format!("sha256={PACKAGE_FINGERPRINT}\n")));
    assert_eq!(PACKAGE_FINGERPRINT.len(), 64);
}

#[test]
fn negotiation_and_typed_errors() {
    let _isolated = isolated();
    validate().unwrap();
    let mut info = ffi::md_abi_info {
        struct_size: size_of::<ffi::md_abi_info>(),
        ..Default::default()
    };
    // SAFETY: a valid output of the declared size.
    let status = unsafe { ffi::md_abi_negotiate(2, &mut info) };
    assert_eq!(status, ffi::MD_ERR_VERSION_MISMATCH);
    assert_eq!(
        info.abi_version, 1,
        "a mismatch still reports the library's values"
    );
    assert_eq!(
        info.model_versions,
        (1 << ffi::MD_MODEL_V1) | (1 << ffi::MD_MODEL_V2)
    );
    info.struct_size -= 4;
    // SAFETY: as above; the core rejects the size without writing.
    let status = unsafe { ffi::md_abi_negotiate(1, &mut info) };
    assert_eq!(status, ffi::MD_ERR_STRUCT_SIZE);
    assert_eq!(
        CoreError::Status(ffi::MD_ERR_STRUCT_SIZE).to_string(),
        "struct_size"
    );

    let error = CoreSession::new("").err().unwrap();
    assert_eq!(error, CoreError::Status(ffi::MD_ERR_INVALID_DOCUMENT_ID));
    let over_ceiling = HostLimits {
        max_source_bytes: 65 * 1024 * 1024,
        ..HostLimits::HOST
    };
    let error = CoreSession::with_limits("doc", over_ceiling).err().unwrap();
    assert_eq!(error, CoreError::Status(ffi::MD_ERR_INVALID_ARGUMENT));

    let session = CoreSession::new("doc").unwrap();
    let error = session.parse_blocking(b"# x".to_vec(), Some(&[b'a'; 257]), true);
    assert_eq!(
        error.err(),
        Some(CoreError::Status(ffi::MD_ERR_INVALID_REVISION_TOKEN))
    );
    // SAFETY: a released-handle check; handles are never reused.
    assert_eq!(
        unsafe { ffi::md_snapshot_release(u64::MAX) },
        ffi::MD_ERR_INVALID_HANDLE
    );
    session.close().unwrap();
}

#[test]
fn owned_v2_values_and_generations() {
    let _isolated = isolated();
    let session = CoreSession::new("sample").unwrap();
    let first = session
        .parse_blocking(SAMPLE.as_bytes().to_vec(), Some(b"rev-1"), true)
        .unwrap();
    assert_eq!(first.outcome, CoreOutcome::Published);
    assert_eq!((first.generation, first.failure), (1, None));
    let document = first.document.unwrap();
    assert_eq!(document.generation, 1);
    assert_eq!(document.source, SAMPLE.as_bytes());
    assert_eq!(document.document_id, "sample");
    assert_eq!(document.title.as_deref(), Some("Sample title"));
    assert!(document.front_matter.is_some());
    assert_eq!(document.source_sha256.len(), 64);
    let headings: Vec<_> = document
        .outline
        .iter()
        .map(|entry| (entry.level, document.text(entry.title).unwrap()))
        .collect();
    assert_eq!(headings, [(1, "Heading one"), (2, "Second")]);
    let heading = document.nodes[document.outline[0].node as usize];
    assert_eq!((heading.kind, heading.value), (ffi::MD_NODE_HEADING, 1));
    assert_eq!((document.links.len(), document.resources.len()), (1, 1));
    assert_eq!(
        document.text(document.links[0].target).unwrap(),
        "https://example.com/a"
    );
    assert!(document.copied_bytes >= SAMPLE.len() as u64);
    assert!(
        document
            .text(ffi::md_str {
                offset: u32::MAX,
                length: 1
            })
            .is_err()
    );

    // The same bytes are unchanged; skipping the copy returns no document.
    let same = session
        .parse_blocking(SAMPLE.as_bytes().to_vec(), None, false)
        .unwrap();
    assert_eq!((same.outcome, same.generation), (CoreOutcome::Unchanged, 2));
    assert!(same.document.is_none());
    let copied = session
        .parse_blocking(SAMPLE.as_bytes().to_vec(), None, true)
        .unwrap();
    assert_eq!(copied.outcome, CoreOutcome::Unchanged);
    assert!(copied.document.is_some());
    let next = session
        .parse_blocking(b"# Changed\n".to_vec(), None, true)
        .unwrap();
    assert_eq!((next.outcome, next.generation), (CoreOutcome::Published, 4));

    // Owned values outlive the session and every native handle.
    session.close().unwrap();
    drain_worker();
    assert_eq!(document.text(document.outline[1].title).unwrap(), "Second");
}

#[test]
fn parse_failures_and_recovery() {
    let _isolated = isolated();
    let session = CoreSession::new("failures").unwrap();
    let invalid = session
        .parse_blocking(vec![b'#', b' ', 0xFF], None, true)
        .unwrap();
    assert_eq!(invalid.outcome, CoreOutcome::Failed);
    assert_eq!(invalid.failure, Some(CoreParseFailure::InvalidUtf8));
    assert!(invalid.document.is_none());
    let over = vec![b'a'; HostLimits::HOST.max_source_bytes as usize + 1];
    let over = session.parse_blocking(over, None, true).unwrap();
    assert_eq!(over.failure, Some(CoreParseFailure::SourceLimit));
    let dense = "*a"
        .repeat(HostLimits::HOST.max_source_bytes as usize / 2)
        .into_bytes();
    let dense = session.parse_blocking(dense, None, true).unwrap();
    assert_eq!(dense.failure, Some(CoreParseFailure::NodeLimit));
    let good = session
        .parse_blocking(b"# Fine\n".to_vec(), None, true)
        .unwrap();
    assert_eq!((good.outcome, good.generation), (CoreOutcome::Published, 4));
    session.close().unwrap();
}

#[test]
fn copy_budget_is_checked_before_copying() {
    let _isolated = isolated();
    let tiny = HostLimits {
        max_snapshot_bytes: 64,
        ..HostLimits::HOST
    };
    let session = CoreSession::with_limits("budget", tiny).unwrap();
    assert_eq!(session.limits(), tiny);
    let result = session.parse_blocking(SAMPLE.as_bytes().to_vec(), None, true);
    assert_eq!(result.err(), Some(CoreError::SnapshotBudget));
    session.close().unwrap();
}

#[test]
fn queued_cancellation_admits_a_replacement_and_the_queue_is_bounded() {
    let _isolated = isolated();
    let session = CoreSession::new("queue").unwrap();
    let release = hold_worker();
    let (first, first_result) = parse_async(&session, b"# One\n".to_vec());
    let (_second, second_result) = parse_async(&session, b"# Two\n".to_vec());
    assert_eq!(pending_parses(), 2);
    let busy = session.parse(b"# Three\n".to_vec(), None, true, |_| {});
    assert_eq!(busy.err(), Some(CoreError::Busy));

    // Cancelling a queued call answers now and frees its slot before the worker runs.
    assert!(!first.is_running());
    first.cancel();
    assert_eq!(
        first_result.try_recv().unwrap().err(),
        Some(CoreError::Cancelled)
    );
    assert_eq!(pending_parses(), 1);
    let (_third, third_result) = parse_async(&session, b"# Three\n".to_vec());
    first.cancel();
    drop(release);

    let second = second_result.recv().unwrap().unwrap();
    let third = third_result.recv().unwrap().unwrap();
    // The cancelled call never reached the core, so it took no generation.
    assert_eq!((second.generation, third.generation), (1, 2));
    session.close().unwrap();
}

#[test]
fn cancellation_during_a_large_call() {
    let _isolated = isolated();
    let source = profile("prose-5mib.md");
    let session = CoreSession::new("large").unwrap();
    let (request, result) = parse_async(&session, source.clone());
    let deadline = Instant::now() + Duration::from_secs(30);
    while !request.is_running() {
        assert!(
            Instant::now() < deadline,
            "the worker never started the call"
        );
        std::thread::sleep(Duration::from_micros(200));
    }
    request.cancel();
    assert_eq!(result.recv().unwrap().err(), Some(CoreError::Cancelled));

    // A session cancel stops running work too, and the session stays usable.
    let (_request, result) = parse_async(&session, source);
    session.cancel().unwrap();
    match result.recv().unwrap() {
        Ok(result) => assert!(
            matches!(
                result.outcome,
                CoreOutcome::Cancelled | CoreOutcome::Published
            ),
            "{:?}",
            result.outcome
        ),
        Err(error) => panic!("{error}"),
    }
    let after = session
        .parse_blocking(b"# After\n".to_vec(), None, true)
        .unwrap();
    assert_eq!(after.outcome, CoreOutcome::Published);
    session.close().unwrap();
}

#[test]
fn close_during_work_and_reject_further_use() {
    let _isolated = isolated();
    let session = CoreSession::new("closing").unwrap();
    let release = hold_worker();
    let (_queued, queued_result) = parse_async(&session, b"# Queued\n".to_vec());
    let closer = std::thread::scope(|scope| {
        let closing = scope.spawn(|| session.close());
        // Close detaches at once: new work is refused while the worker is held.
        let deadline = Instant::now() + Duration::from_secs(10);
        while session.cancel() != Err(CoreError::Closed) {
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        }
        assert_eq!(
            session.parse(b"# x".to_vec(), None, true, |_| {}).err(),
            Some(CoreError::Closed)
        );
        assert_eq!(session.availability().err(), Some(CoreError::Closed));
        assert_eq!(
            session.record_unavailable(CoreUnavailability::Io).err(),
            Some(CoreError::Closed)
        );
        drop(release);
        closing.join().unwrap()
    });
    closer.unwrap();
    assert_eq!(queued_result.recv().unwrap().err(), Some(CoreError::Closed));
    session.close().unwrap();
}

#[test]
fn concurrent_close_waits_for_cleanup() {
    let _isolated = isolated();
    let before = live_handles().unwrap().sessions;
    let session = CoreSession::new("concurrent").unwrap();
    session
        .parse_blocking(b"# Ready\n".to_vec(), None, true)
        .unwrap();
    let release = hold_worker();
    std::thread::scope(|scope| {
        let closers: Vec<_> = (0..4).map(|_| scope.spawn(|| session.close())).collect();
        std::thread::sleep(Duration::from_millis(20));
        assert!(
            closers.iter().all(|closer| !closer.is_finished()),
            "close waits for the worker"
        );
        drop(release);
        for closer in closers {
            closer.join().unwrap().unwrap();
            assert_eq!(live_handles().unwrap().sessions, before);
        }
    });
}

#[test]
fn dropping_a_session_releases_it_on_the_worker() {
    let _isolated = isolated();
    let session = CoreSession::new("dropped").unwrap();
    let release = hold_worker();
    let (_request, result) = parse_async(&session, b"# Dropped\n".to_vec());
    drop(session);
    drop(release);
    assert_eq!(result.recv().unwrap().err(), Some(CoreError::Closed));
}

#[test]
fn availability_follows_recorded_changes() {
    let _isolated = isolated();
    let session = CoreSession::new("availability").unwrap();
    assert_eq!(session.availability().unwrap(), CoreAvailability::Available);
    let generation = session
        .record_unavailable(CoreUnavailability::PermissionDenied)
        .unwrap();
    assert_eq!(generation, 1);
    assert_eq!(
        session.availability().unwrap(),
        CoreAvailability::Unavailable(CoreUnavailability::PermissionDenied)
    );
    session
        .record_unavailable(CoreUnavailability::Deleted)
        .unwrap();
    assert_eq!(
        session.availability().unwrap(),
        CoreAvailability::Unavailable(CoreUnavailability::Deleted)
    );
    let back = session
        .parse_blocking(b"# Back\n".to_vec(), None, true)
        .unwrap();
    assert_eq!(back.outcome, CoreOutcome::Published);
    assert_eq!(session.availability().unwrap(), CoreAvailability::Available);
    session.close().unwrap();
}

#[test]
fn repeated_lifecycle() {
    let _isolated = isolated();
    for iteration in 0..200 {
        let session = CoreSession::new(&format!("doc-{iteration}")).unwrap();
        let source = format!("# Document {iteration}\n\nBody text.\n").into_bytes();
        let result = session.parse_blocking(source, None, true).unwrap();
        assert_eq!(result.outcome, CoreOutcome::Published);
        if iteration % 2 == 0 {
            session.close().unwrap();
        }
    }
}

#[test]
fn blocking_calls_are_refused_on_the_worker() {
    let _isolated = isolated();
    let session = std::sync::Arc::new(CoreSession::new("worker").unwrap());
    let (sender, receiver) = channel();
    let on_worker = session.clone();
    run_on_worker(move || {
        let parse = on_worker.parse_blocking(b"# x".to_vec(), None, true).err();
        sender.send((parse, on_worker.close().err())).unwrap();
    });
    let refused = Some(CoreError::OnWorker);
    assert_eq!(receiver.recv().unwrap(), (refused, refused));
    session.close().unwrap();
}

#[test]
fn corpus_and_performance_profiles() {
    let _isolated = isolated();
    let mut corpus: Vec<PathBuf> = std::fs::read_dir(repository().join("fixtures/markdown"))
        .unwrap()
        .flat_map(|group| std::fs::read_dir(group.unwrap().path()).unwrap())
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "md"))
        .collect();
    corpus.sort();
    assert_eq!(corpus.len(), 8);
    let profiles = [
        "prose-1mib.md",
        "prose-5mib.md",
        "delimiter-storm-1mib.md",
        "long-lines-1mib.md",
    ];
    let sources = corpus
        .iter()
        .map(|path| std::fs::read(path).unwrap())
        .chain(profiles.iter().map(|name| profile(name)));
    for (index, source) in sources.enumerate() {
        let session = CoreSession::new(&format!("corpus-{index}")).unwrap();
        let result = session.parse_blocking(source, None, true).unwrap();
        assert_eq!(
            result.outcome,
            CoreOutcome::Published,
            "input {index}: {:?}",
            result.failure
        );
        let document = result.document.unwrap();
        assert!(!document.nodes.is_empty());
        assert!(
            document
                .outline
                .iter()
                .all(|entry| document.text(entry.title).is_ok())
        );
        session.close().unwrap();
    }
}

#[test]
fn close_then_does_not_block_and_reports_the_release() {
    let _isolated = isolated();
    let session = CoreSession::new("close-then").unwrap();
    session
        .parse_blocking(b"# Ready\n".to_vec(), None, true)
        .unwrap();
    let release = hold_worker();
    let (sender, closed) = channel();
    session.close_then(move |result| sender.send(result).unwrap());
    // Detached at once, while the release still waits behind the held worker.
    assert_eq!(session.cancel(), Err(CoreError::Closed));
    assert!(closed.try_recv().is_err());
    drop(release);
    closed
        .recv_timeout(Duration::from_secs(30))
        .unwrap()
        .unwrap();
    // A later caller sees the shared result immediately.
    let (sender, again) = channel();
    session.close_then(move |result| sender.send(result).unwrap());
    again.try_recv().unwrap().unwrap();
    session.close().unwrap();
}

#[test]
fn find_over_current_snapshot() {
    let _isolated = isolated();
    let session = CoreSession::new("find-test").unwrap();
    let source = b"# Caf\xc3\xa9 and CAF\xc3\x89\n\nInline caf\xc3\xa9 and caf**\xc3\xa9**.\n\n\
        Decomposed cafe\xcc\x81 and German Stra\xc3\x9fe.\n\n\
        Astral \xf0\x9d\x92\xb3\xf0\x9d\x92\xb4 and emoji \xf0\x9f\x91\xa8\xe2\x80\x8d\xf0\x9f\x91\xa9\xe2\x80\x8d\xf0\x9f\x91\xa7 family.\n\n\
        CJK \xe4\xb8\xad\xe6\x96\x87 and RTL \xd7\xa2\xd7\x91\xd7\xa8\xd7\x99\xd7\xaa.\n";
    let first = session.parse_blocking(source.to_vec(), None, true).unwrap();
    let generation = first.generation;

    let find = |q: &str, opt: CoreFindOptions| {
        let res = session.find_blocking(q, opt, generation).unwrap();
        let mut matched_strings = Vec::new();
        for m in &res.matches {
            let slice =
                std::str::from_utf8(&source[m.span_start as usize..m.span_end as usize]).unwrap();
            matched_strings.push(slice.to_string());
        }
        (res, matched_strings)
    };

    let (_, cafe) = find("café", CoreFindOptions::default());
    assert_eq!(cafe, vec!["Café", "CAFÉ", "café", "caf**é"]);

    let (_, sensitive) = find(
        "café",
        CoreFindOptions {
            case_sensitive: true,
            ..Default::default()
        },
    );
    assert_eq!(sensitive, vec!["café", "caf**é"]);

    let (_, nfd) = find("cafe\u{0301}", CoreFindOptions::default());
    assert_eq!(nfd, vec!["cafe\u{0301}"]);

    let (_, no_combining) = find("cafe", CoreFindOptions::default());
    assert!(no_combining.is_empty());

    let (_, strasse) = find("STRAßE", CoreFindOptions::default());
    assert_eq!(strasse, vec!["Straße"]);

    let (_, strasse_sub) = find("STRASSE", CoreFindOptions::default());
    assert!(strasse_sub.is_empty());

    let (_, astral) = find("𝒳𝒴", CoreFindOptions::default());
    assert_eq!(astral, vec!["𝒳𝒴"]);

    let (_, no_emoji_partial) = find("👨", CoreFindOptions::default());
    assert!(no_emoji_partial.is_empty());

    let (_, emoji_full) = find("👨‍👩‍👧", CoreFindOptions::default());
    assert_eq!(emoji_full, vec!["👨‍👩‍👧"]);

    let (_, whole_word) = find(
        "family",
        CoreFindOptions {
            whole_word: true,
            ..Default::default()
        },
    );
    assert_eq!(whole_word, vec!["family"]);

    let (_, subword) = find(
        "fam",
        CoreFindOptions {
            whole_word: true,
            ..Default::default()
        },
    );
    assert!(subword.is_empty());

    let (_, cjk) = find("中文", CoreFindOptions::default());
    assert_eq!(cjk, vec!["中文"]);

    let (_, rtl) = find("עברית", CoreFindOptions::default());
    assert_eq!(rtl, vec!["עברית"]);

    let (empty_res, _) = find("   ", CoreFindOptions::default());
    assert!(empty_res.matches.is_empty());

    let limited = session
        .find_blocking(
            "caf",
            CoreFindOptions {
                max_matches: 2,
                ..Default::default()
            },
            generation,
        )
        .unwrap();
    assert_eq!(limited.matches.len(), 2);
    assert!(limited.truncated);

    // Over-long query (> 4096 bytes) rejected
    let long_query = "a".repeat(4097);
    let err = session
        .find_blocking(&long_query, CoreFindOptions::default(), generation)
        .unwrap_err();
    assert_eq!(err, CoreError::QueryTooLong);

    // Unpublished generation returns Superseded
    let err = session
        .find_blocking("café", CoreFindOptions::default(), generation + 10)
        .unwrap_err();
    assert!(matches!(err, CoreError::Superseded { .. }));

    // Reparse advances generation
    let second = session
        .parse_blocking(b"# caf\xc3\xa9 again\n".to_vec(), None, true)
        .unwrap();
    assert!(second.generation > generation);

    // Superseded generation returns Superseded
    let err = session
        .find_blocking("café", CoreFindOptions::default(), generation)
        .unwrap_err();
    assert!(matches!(err, CoreError::Superseded { .. }));

    // Find over current generation succeeds
    let current = session
        .find_blocking("café", CoreFindOptions::default(), second.generation)
        .unwrap();
    assert_eq!(current.matches.len(), 1);

    // Cancellation before worker runs returns Cancelled
    let hold = hold_worker();
    let gate = FindGate::new();
    let (tx, rx) = channel();
    session.find(
        "café",
        CoreFindOptions::default(),
        second.generation,
        Some(gate.clone()),
        move |res| {
            let _ = tx.send(res);
        },
    );
    gate.cancel();
    drop(hold);
    let cancelled_res = rx.recv_timeout(Duration::from_secs(10)).unwrap();
    assert_eq!(cancelled_res.unwrap_err(), CoreError::Cancelled);

    session.close().unwrap();
    let err = session
        .find_blocking("café", CoreFindOptions::default(), second.generation)
        .unwrap_err();
    assert_eq!(err, CoreError::Closed);
}

#[test]
fn anchors_and_external_changes() {
    let _isolated = isolated();
    let session = CoreSession::new("anchors").unwrap();
    let old_text = "# One\n\nFirst paragraph.\n\n## Two\n\nSecond paragraph.\n";
    let old_bytes = old_text.as_bytes().to_vec();
    let first = session
        .parse_blocking(old_bytes.clone(), Some(b"a"), true)
        .unwrap();
    let first_doc = first.document.unwrap();

    assert_eq!(
        session.external_change(Some("a")).unwrap(),
        markdown_linux_core::CoreChangeDisposition::Unchanged
    );
    assert_eq!(
        session.external_change(Some("b")).unwrap(),
        markdown_linux_core::CoreChangeDisposition::ReadRequired
    );
    assert_eq!(
        session.external_change(None).unwrap(),
        markdown_linux_core::CoreChangeDisposition::ReadRequired
    );

    let offset = (old_bytes.len() - "Second paragraph.\n".len() + 7) as u64;
    let anchor = first_doc.anchor_at(offset).unwrap();
    assert!(!anchor.is_empty() && anchor.len() <= 1024);

    let first_res = first_doc.resolve_anchor(&anchor).unwrap();
    assert!(first_res.is_some());
    assert_eq!(first_res.unwrap().source_offset, offset);

    let new_text = "# One\n\nInserted.\n\nFirst paragraph.\n\n## Two\n\nSecond paragraph.\n";
    let new_bytes = new_text.as_bytes().to_vec();
    let second = session
        .parse_blocking(new_bytes.clone(), Some(b"b"), true)
        .unwrap();
    let second_doc = second.document.unwrap();

    let moved = second_doc.resolve_anchor(&anchor).unwrap().unwrap();
    assert_eq!(moved.quality, markdown_linux_core::AnchorQuality::Exact);
    let inserted_bytes = "Inserted.\n\n".len() as u64;
    assert_eq!(moved.source_offset, offset + inserted_bytes);

    let old_check = first_doc.resolve_anchor(&anchor).unwrap().unwrap();
    assert_eq!(old_check.source_offset, offset);

    let unchanged = session
        .parse_blocking(new_bytes.clone(), Some(b"c"), false)
        .unwrap();
    assert_eq!(unchanged.outcome, CoreOutcome::Unchanged);
    assert!(unchanged.document.is_none());
    assert_eq!(
        session.external_change(Some("c")).unwrap(),
        markdown_linux_core::CoreChangeDisposition::Unchanged
    );

    assert!(second_doc.resolve_anchor(b"{}").is_err());
    assert!(second_doc.resolve_anchor(&vec![0u8; 2048]).is_err());

    let empty = session
        .parse_blocking(Vec::new(), Some(b"d"), true)
        .unwrap();
    let empty_doc = empty.document.unwrap();
    assert!(empty_doc.resolve_anchor(&anchor).unwrap().is_none());
    assert_eq!(empty_doc.anchor_at(0).unwrap_err(), CoreError::NoBlocks);
    session.close().unwrap();
}

#[test]
fn documents_release_their_snapshots() {
    let _isolated = isolated();
    {
        let session = CoreSession::new("release").unwrap();
        let res = session
            .parse_blocking(b"# Kept\n".to_vec(), None, true)
            .unwrap();
        let doc = res.document.unwrap();
        session.close().unwrap();
        drain_worker();
        let handles = live_handles().unwrap();
        assert_eq!(handles.sessions, 0);
        assert_eq!(handles.snapshots, 1);
        drop(doc);
    }
    drain_worker();
    let final_handles = live_handles().unwrap();
    assert_eq!(final_handles.sessions, 0);
    assert_eq!(final_handles.snapshots, 0);
}
