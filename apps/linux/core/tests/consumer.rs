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
