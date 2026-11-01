use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock, mpsc};
use std::time::{Duration, Instant};

use super::access::{DocumentCapability, DocumentError, ReadFailure, read_document};
use markdown_linux_core::{
    CoreDocument, CoreError, CoreOutcome, CoreParseFailure, CoreResult, CoreSession, HostLimits,
    ParseRequest,
};

fn lock<T>(value: &Mutex<T>) -> MutexGuard<'_, T> {
    value.lock().unwrap_or_else(|p| p.into_inner())
}
#[derive(Default)]
pub(super) struct Cancellation {
    state: Mutex<CancelState>,
    wake: Condvar,
    #[cfg(test)]
    busy_attempts: AtomicU64,
}
#[derive(Default)]
struct CancelState {
    cancelled: bool,
    parse: Option<ParseRequest>,
    completed: bool,
}
impl Cancellation {
    pub fn is_cancelled(&self) -> bool {
        lock(&self.state).cancelled
    }
    fn cancel(&self) {
        let parse = {
            let mut state = lock(&self.state);
            state.cancelled = true;
            state.parse.take()
        };
        self.wake.notify_all();
        // Queued parse cancellation calls back synchronously: never hold our lock.
        if let Some(parse) = parse {
            parse.cancel();
        }
    }
    fn install(&self, parse: ParseRequest) {
        let cancel = {
            let mut state = lock(&self.state);
            if state.cancelled {
                Some(parse)
            } else if state.completed {
                None
            } else {
                state.parse = Some(parse);
                None
            }
        };
        if let Some(parse) = cancel {
            parse.cancel();
        }
    }
    fn complete(&self) {
        let mut state = lock(&self.state);
        state.completed = true;
        state.parse = None;
    }
    fn wait(&self, interval: Duration) {
        let guard = lock(&self.state);
        if !guard.cancelled {
            drop(
                self.wake
                    .wait_timeout(guard, interval)
                    .unwrap_or_else(|p| p.into_inner()),
            );
        }
    }
}

use crate::render::{RenderError, RenderPlan};

pub enum LoadEvent {
    Loaded {
        document: Arc<CoreDocument>,
        plan: Arc<RenderPlan>,
        generation: u64,
        unchanged: bool,
    },
    Failed(DocumentError),
}
type PreparedResult = Result<(CoreResult, Option<Arc<RenderPlan>>), DocumentError>;
// Called only from successful core completion, outside cancellation/core locks.
// The injected pure builder is also the deterministic post-parse test seam.
fn prepare_result(
    result: Result<CoreResult, DocumentError>,
    cancel: &Cancellation,
    build: impl FnOnce(&CoreDocument) -> Result<RenderPlan, RenderError>,
) -> Option<PreparedResult> {
    if cancel.is_cancelled() {
        return None;
    }
    let prepared = result.and_then(|result| {
        let plan = if matches!(
            result.outcome,
            CoreOutcome::Published | CoreOutcome::Unchanged
        ) {
            let document = result.document.as_ref().ok_or(DocumentError::ParseFailed)?;
            Some(Arc::new(build(document).map_err(|error| match error {
                RenderError::Budget => DocumentError::TooComplex,
                _ => DocumentError::ParseFailed,
            })?))
        } else {
            None
        };
        Ok((result, plan))
    });
    if cancel.is_cancelled() {
        None
    } else {
        Some(prepared)
    }
}
struct Completion {
    serial: u64,
    result: PreparedResult,
}
struct Job {
    owner: u64,
    serial: u64,
    capability: DocumentCapability,
    cancel: Arc<Cancellation>,
    session: Arc<CoreSession>,
    sender: mpsc::Sender<Completion>,
}
struct Dispatcher {
    queue: Mutex<VecDeque<Job>>,
    wake: Condvar,
}
fn dispatcher() -> &'static Arc<Dispatcher> {
    static INSTANCE: OnceLock<Arc<Dispatcher>> = OnceLock::new();
    INSTANCE.get_or_init(|| {
        let dispatcher = Arc::new(Dispatcher {
            queue: Mutex::new(VecDeque::new()),
            wake: Condvar::new(),
        });
        let worker = dispatcher.clone();
        std::thread::Builder::new()
            .name("markdown-files".into())
            .spawn(move || {
                loop {
                    let job = {
                        let mut queue = lock(&worker.queue);
                        while queue.is_empty() {
                            queue = worker.wake.wait(queue).unwrap_or_else(|p| p.into_inner());
                        }
                        queue.pop_front().unwrap()
                    };
                    process(job);
                }
            })
            .expect("file worker creation");
        dispatcher
    })
}
fn send(job: &Job, result: PreparedResult) {
    if !job.cancel.is_cancelled() {
        let _ = job.sender.send(Completion {
            serial: job.serial,
            result,
        });
    }
}
fn process(job: Job) {
    process_with(job, read_document, Duration::from_secs(30));
}
fn process_with(
    job: Job,
    reader: impl FnOnce(
        &DocumentCapability,
        &Cancellation,
        usize,
    ) -> Result<super::access::ReadDocument, ReadFailure>,
    timeout: Duration,
) {
    let read = match reader(
        &job.capability,
        &job.cancel,
        HostLimits::HOST.max_source_bytes as usize,
    ) {
        Ok(read) => read,
        Err(ReadFailure::Cancelled) => return,
        Err(ReadFailure::Error(error)) => {
            send(&job, Err(error));
            return;
        }
    };
    let deadline = Instant::now() + timeout;
    loop {
        if job.cancel.is_cancelled() {
            return;
        }
        if job.capability.is_revoked() {
            send(&job, Err(DocumentError::AccessRevoked));
            return;
        }
        if Instant::now() >= deadline {
            send(&job, Err(DocumentError::Busy));
            return;
        }
        let cancel = job.cancel.clone();
        let sender = job.sender.clone();
        let serial = job.serial;
        // parse consumes its input even on Busy. Only this dispatcher clones a
        // bounded original for submission; no prepared-buffer queue exists.
        match job.session.parse(
            read.bytes.clone(),
            Some(&read.revision),
            true,
            move |result| {
                cancel.complete();
                if !cancel.is_cancelled() {
                    let mapped = result.map_err(|e| {
                        if e == CoreError::SnapshotBudget {
                            DocumentError::TooComplex
                        } else {
                            DocumentError::ParseFailed
                        }
                    });
                    let Some(prepared) = prepare_result(mapped, &cancel, |document| {
                        RenderPlan::build(document, || cancel.is_cancelled())
                    }) else {
                        return;
                    };
                    let _ = sender.send(Completion {
                        serial,
                        result: prepared,
                    });
                }
            },
        ) {
            Ok(parse) => {
                job.cancel.install(parse);
                return;
            }
            Err(CoreError::Busy) => {
                #[cfg(test)]
                job.cancel.busy_attempts.fetch_add(1, Ordering::Relaxed);
                job.cancel.wait(Duration::from_millis(50));
            }
            Err(_) => {
                send(&job, Err(DocumentError::ParseFailed));
                return;
            }
        }
    }
}

/// Owned by one GTK window. Its serial covers reads, admission and callbacks,
/// including results already queued before replacement. No GTK objects cross threads.
pub struct DocumentLoader {
    owner: u64,
    serial: u64,
    closed: bool,
    capability: Option<DocumentCapability>,
    session: Option<Arc<CoreSession>>,
    cancel: Option<Arc<Cancellation>>,
    sender: mpsc::Sender<Completion>,
    receiver: mpsc::Receiver<Completion>,
    pub document: Option<Arc<CoreDocument>>,
}
impl DocumentLoader {
    pub fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let (sender, receiver) = mpsc::channel();
        Self {
            owner: NEXT.fetch_add(1, Ordering::Relaxed),
            serial: 0,
            closed: false,
            capability: None,
            session: None,
            cancel: None,
            sender,
            receiver,
            document: None,
        }
    }
    pub fn has_file(&self) -> bool {
        self.capability.is_some()
    }
    pub fn display_name(&self) -> Option<String> {
        self.capability
            .as_ref()
            .map(DocumentCapability::display_name)
    }
    fn invalidate(&mut self) {
        self.serial = self
            .serial
            .checked_add(1)
            .expect("request serial exhausted");
        if let Some(cancel) = self.cancel.take() {
            cancel.cancel();
        }
        lock(&dispatcher().queue).retain(|job| job.owner != self.owner);
        // Dispose any already queued large results away from GTK.
        let stale: Vec<_> = self.receiver.try_iter().collect();
        if !stale.is_empty() {
            std::thread::spawn(move || drop(stale));
        }
    }
    pub fn open(&mut self, capability: DocumentCapability) -> Result<(), DocumentError> {
        self.open_with(capability, CoreSession::new)
    }
    fn open_with(
        &mut self,
        capability: DocumentCapability,
        create: impl FnOnce(&str) -> Result<CoreSession, CoreError>,
    ) -> Result<(), DocumentError> {
        if self.closed {
            return Err(DocumentError::ParseFailed);
        }
        // A failed session allocation must leave the prior selection, request
        // and Reload lineage intact. Prepare before mutating any owned state.
        let next = self
            .serial
            .checked_add(1)
            .expect("request serial exhausted");
        let session = Arc::new(
            create(&format!("linux-document-{}-{next}", self.owner))
                .map_err(|_| DocumentError::ParseFailed)?,
        );
        self.invalidate();
        if let Some(old) = self.session.take() {
            old.close_then(|_| {});
        }
        self.discard_document();
        self.capability = Some(capability);
        self.session = Some(session);
        self.enqueue();
        Ok(())
    }
    /// Invalidate the previous selection when delivery cannot grant a local file.
    pub fn reject_selection(&mut self) {
        self.invalidate();
        self.discard_document();
        self.capability = None;
        if let Some(session) = self.session.take() {
            session.close_then(|_| {});
        }
    }
    pub fn reload(&mut self) {
        if self.closed || self.capability.is_none() {
            return;
        }
        self.invalidate();
        self.enqueue();
    }
    fn enqueue(&mut self) {
        let (Some(capability), Some(session)) = (&self.capability, &self.session) else {
            return;
        };
        let cancel = Arc::new(Cancellation::default());
        self.cancel = Some(cancel.clone());
        let worker = dispatcher();
        lock(&worker.queue).push_back(Job {
            owner: self.owner,
            serial: self.serial,
            capability: capability.clone(),
            cancel,
            session: session.clone(),
            sender: self.sender.clone(),
        });
        worker.wake.notify_one();
    }
    pub fn poll(&mut self) -> Option<LoadEvent> {
        while let Ok(completion) = self.receiver.try_recv() {
            if self.closed || completion.serial != self.serial {
                std::thread::spawn(move || drop(completion));
                continue;
            }
            let result = if self
                .capability
                .as_ref()
                .is_some_and(DocumentCapability::is_revoked)
            {
                std::thread::spawn(move || drop(completion.result));
                Err(DocumentError::AccessRevoked)
            } else {
                completion.result
            };
            let result = result.and_then(|(result, plan)| match result.outcome {
                CoreOutcome::Published | CoreOutcome::Unchanged => Ok((result, plan)),
                CoreOutcome::Cancelled | CoreOutcome::Closed | CoreOutcome::Superseded => {
                    Err(DocumentError::ParseFailed)
                }
                CoreOutcome::Failed => Err(match result.failure {
                    Some(CoreParseFailure::InvalidUtf8) => DocumentError::InvalidText,
                    Some(CoreParseFailure::SourceLimit) => DocumentError::TooLarge,
                    Some(
                        CoreParseFailure::EventLimit
                        | CoreParseFailure::NodeLimit
                        | CoreParseFailure::DepthLimit
                        | CoreParseFailure::TextLimit,
                    ) => DocumentError::TooComplex,
                    _ => DocumentError::ParseFailed,
                }),
            });
            return Some(match result {
                Ok((mut result, plan)) => {
                    let Some(plan) = plan else {
                        return Some(LoadEvent::Failed(DocumentError::ParseFailed));
                    };
                    let Some(document) = result.document.take() else {
                        return Some(LoadEvent::Failed(DocumentError::ParseFailed));
                    };
                    let document = Arc::new(document);
                    self.discard_document();
                    self.document = Some(document.clone());
                    LoadEvent::Loaded {
                        plan,
                        document,
                        generation: result.generation,
                        unchanged: result.outcome == CoreOutcome::Unchanged,
                    }
                }
                Err(error) => {
                    // Only the still-current owner may change availability.
                    if let Some(reason) = error.unavailable()
                        && let Some(session) = &self.session
                    {
                        let _ = session.record_unavailable(reason);
                    }
                    self.discard_document();
                    LoadEvent::Failed(error)
                }
            });
        }
        None
    }
    pub fn revoke(&mut self) {
        if let Some(capability) = &self.capability {
            capability.revoke();
        }
        self.reload();
    }
    fn discard_document(&mut self) {
        if let Some(document) = self.document.take() {
            std::thread::spawn(move || drop(document));
        }
    }
    pub fn close(&mut self) {
        if self.closed {
            return;
        }
        self.closed = true;
        self.invalidate();
        self.discard_document();
        if let Some(session) = self.session.take() {
            session.close_then(|_| {});
        }
    }
}
impl Default for DocumentLoader {
    fn default() -> Self {
        Self::new()
    }
}
impl Drop for DocumentLoader {
    fn drop(&mut self) {
        self.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use markdown_linux_core::{CoreAvailability, CoreUnavailability};
    use std::path::PathBuf;
    static TEST_LOCK: Mutex<()> = Mutex::new(());
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let dir = std::env::temp_dir().join(format!(
                "markdown-loader-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir(&dir).unwrap();
            Self(dir)
        }
        fn file(&self, name: &str, bytes: &[u8]) -> DocumentCapability {
            let path = self.0.join(name);
            std::fs::write(&path, bytes).unwrap();
            DocumentCapability::from_local_file(path)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }
    fn event(loader: &mut DocumentLoader) -> LoadEvent {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            if let Some(event) = loader.poll() {
                return event;
            }
            assert!(Instant::now() < deadline, "load deadline");
            std::thread::yield_now();
        }
    }
    fn loaded(loader: &mut DocumentLoader) -> (u64, bool) {
        match event(loader) {
            LoadEvent::Loaded {
                generation,
                unchanged,
                ..
            } => (generation, unchanged),
            LoadEvent::Failed(e) => panic!("{e:?}"),
        }
    }
    #[test]
    fn failed_open_preserves_selection_and_reload_lineage() {
        let _guard = lock(&TEST_LOCK);
        let fixture = Fixture::new();
        let mut loader = DocumentLoader::new();
        let selected = fixture.file("selected", b"# Selected");
        assert_eq!(
            loader.open_with(selected.clone(), |_| Err(CoreError::Internal)),
            Err(DocumentError::ParseFailed)
        );
        assert!(!loader.has_file());
        assert!(loader.session.is_none());
        loader.open(selected).unwrap();
        loaded(&mut loader);
        let serial = loader.serial;
        let session = loader.session.as_ref().unwrap().clone();
        let document = loader.document.as_ref().unwrap().clone();
        assert_eq!(
            loader.open_with(fixture.file("replacement", b"# Replacement"), |_| Err(
                CoreError::Internal
            )),
            Err(DocumentError::ParseFailed)
        );
        assert_eq!(loader.serial, serial);
        assert_eq!(loader.display_name().as_deref(), Some("selected"));
        assert!(Arc::ptr_eq(loader.session.as_ref().unwrap(), &session));
        assert!(Arc::ptr_eq(loader.document.as_ref().unwrap(), &document));
        loader.reload();
        assert!(loaded(&mut loader).1);
        loader.close();
    }
    #[test]
    fn reload_lineage_and_error_recovery() {
        let _guard = lock(&TEST_LOCK);
        let fixture = Fixture::new();
        let mut loader = DocumentLoader::new();
        loader
            .open(fixture.file("文 blank.any", b"# First\n"))
            .unwrap();
        let (first, _) = loaded(&mut loader);
        loader.reload();
        let (same, unchanged) = loaded(&mut loader);
        assert!(same > first);
        assert!(unchanged);
        std::fs::write(fixture.0.join("文 blank.any"), b"# Second\n").unwrap();
        loader.reload();
        let (changed, unchanged) = loaded(&mut loader);
        assert!(changed > first);
        assert!(!unchanged);
        std::fs::remove_file(fixture.0.join("文 blank.any")).unwrap();
        loader.reload();
        assert!(matches!(
            event(&mut loader),
            LoadEvent::Failed(DocumentError::NotFound)
        ));
        assert_eq!(
            loader.session.as_ref().unwrap().availability().unwrap(),
            CoreAvailability::Unavailable(CoreUnavailability::Deleted)
        );
        std::fs::write(fixture.0.join("文 blank.any"), b"").unwrap();
        loader.reload();
        loaded(&mut loader);
        assert_eq!(
            loader.session.as_ref().unwrap().availability().unwrap(),
            CoreAvailability::Available
        );
        loader.revoke();
        assert!(matches!(
            event(&mut loader),
            LoadEvent::Failed(DocumentError::AccessRevoked)
        ));
        loader.close();
    }
    #[test]
    fn stale_errors_and_already_queued_results_are_silent() {
        let _guard = lock(&TEST_LOCK);
        let fixture = Fixture::new();
        let mut loader = DocumentLoader::new();
        loader.open(fixture.file("one", b"# One")).unwrap();
        loaded(&mut loader);
        let old = loader.serial;
        loader.reload();
        loader
            .sender
            .send(Completion {
                serial: old,
                result: Err(DocumentError::PermissionDenied),
            })
            .unwrap();
        loaded(&mut loader);
        assert_eq!(
            loader.session.as_ref().unwrap().availability().unwrap(),
            CoreAvailability::Available
        );
        assert!(loader.poll().is_none());
        loader
            .sender
            .send(Completion {
                serial: loader.serial,
                result: Err(DocumentError::ReadFailed),
            })
            .unwrap();
        loader.open(fixture.file("two", b"# Two")).unwrap();
        loaded(&mut loader);
        assert_eq!(loader.document.as_ref().unwrap().source, b"# Two");
        loader.close();
        assert!(loader.poll().is_none());
    }
    #[test]
    fn read_barrier_replacement_and_close_suppress_publication() {
        let _guard = lock(&TEST_LOCK);
        let fixture = Fixture::new();
        for close in [false, true] {
            let mut loader = DocumentLoader::new();
            let capability = fixture.file("a", b"# A");
            loader.capability = Some(capability.clone());
            loader.session = Some(Arc::new(CoreSession::new("read-barrier").unwrap()));
            let cancel = Arc::new(Cancellation::default());
            loader.cancel = Some(cancel.clone());
            let job = Job {
                owner: loader.owner,
                serial: loader.serial,
                capability,
                cancel: cancel.clone(),
                session: loader.session.as_ref().unwrap().clone(),
                sender: loader.sender.clone(),
            };
            let (started, entered) = mpsc::channel();
            let (resume, barrier) = mpsc::channel();
            let worker = std::thread::spawn(move || {
                process_with(
                    job,
                    |cap, token, limit| {
                        started.send(()).unwrap();
                        barrier.recv().unwrap();
                        read_document(cap, token, limit)
                    },
                    Duration::from_secs(30),
                )
            });
            entered.recv_timeout(Duration::from_secs(5)).unwrap();
            if close {
                loader.close();
            } else {
                loader.open(fixture.file("b", b"# B")).unwrap();
            }
            assert!(cancel.is_cancelled());
            resume.send(()).unwrap();
            worker.join().unwrap();
            if !close {
                loaded(&mut loader);
                assert_eq!(loader.document.as_ref().unwrap().source, b"# B");
            }
            assert!(loader.poll().is_none());
            loader.close();
        }
    }
    #[test]
    fn completed_plan_barrier_replacement_reload_and_close_are_silent() {
        let _guard = lock(&TEST_LOCK);
        let fixture = Fixture::new();
        for action in 0..3 {
            let mut loader = DocumentLoader::new();
            loader.open(fixture.file("plan-a", b"# Current")).unwrap();
            loaded(&mut loader);
            let session = CoreSession::new("held-plan").unwrap();
            let parsed = session
                .parse_blocking(b"# Obsolete".to_vec(), None, true)
                .unwrap();
            session.close().unwrap();
            let cancel = loader.cancel.as_ref().unwrap().clone();
            let serial = loader.serial;
            let sender = loader.sender.clone();
            let (started, entered) = mpsc::channel();
            let (resume, held) = mpsc::channel();
            let worker = std::thread::spawn(move || {
                let prepared = prepare_result(Ok(parsed), &cancel, |document| {
                    let plan = RenderPlan::build(document, || cancel.is_cancelled())?;
                    started.send(()).unwrap();
                    held.recv().unwrap();
                    Ok(plan)
                });
                assert!(
                    prepared.is_none(),
                    "cancelled completed plan escaped suppression"
                );
                if let Some(result) = prepared {
                    sender.send(Completion { serial, result }).unwrap();
                }
            });
            entered.recv_timeout(Duration::from_secs(5)).unwrap();
            match action {
                0 => loader
                    .open(fixture.file("plan-b", b"# Replacement"))
                    .unwrap(),
                1 => loader.reload(),
                _ => loader.close(),
            };
            resume.send(()).unwrap();
            worker.join().unwrap();
            if action != 2 {
                loaded(&mut loader);
                assert_eq!(
                    loader.document.as_ref().unwrap().source,
                    if action == 0 {
                        b"# Replacement".as_slice()
                    } else {
                        b"# Current".as_slice()
                    }
                );
            }
            assert!(loader.poll().is_none());
            loader.close();
        }
    }
    #[test]
    fn invalid_utf8_and_complexity_are_not_access_failures() {
        let _guard = lock(&TEST_LOCK);
        let fixture = Fixture::new();
        let mut loader = DocumentLoader::new();
        loader.open(fixture.file("binary", &[0xff])).unwrap();
        assert!(matches!(
            event(&mut loader),
            LoadEvent::Failed(DocumentError::InvalidText)
        ));
        assert_eq!(
            loader.session.as_ref().unwrap().availability().unwrap(),
            CoreAvailability::Available
        );
        loader
            .open(fixture.file("dense", &b"*a".repeat(2_000_000)))
            .unwrap();
        assert!(matches!(
            event(&mut loader),
            LoadEvent::Failed(DocumentError::TooComplex)
        ));
        assert_eq!(
            loader.session.as_ref().unwrap().availability().unwrap(),
            CoreAvailability::Available
        );
        loader.close();
    }
    #[test]
    fn callback_before_install_and_synchronous_cancel_do_not_deadlock() {
        let _guard = lock(&TEST_LOCK);
        let session = CoreSession::new("install-race").unwrap();
        let cancel = Arc::new(Cancellation::default());
        // A barrier explicitly delays installation until callback completion.
        let (tx, rx) = mpsc::channel();
        let callback = cancel.clone();
        let parse = session
            .parse(b"# Test".to_vec(), None, true, move |_| {
                callback.complete();
                tx.send(()).unwrap();
            })
            .unwrap();
        rx.recv_timeout(Duration::from_secs(5)).unwrap();
        cancel.install(parse);
        assert!(lock(&cancel.state).parse.is_none());
        cancel.cancel();
        let blocker = CoreSession::new("blocker").unwrap();
        let (start, started) = mpsc::channel();
        let (release, wait) = mpsc::channel();
        let running = blocker
            .parse(b"# First".to_vec(), None, true, move |_| {
                start.send(()).unwrap();
                wait.recv().unwrap();
            })
            .unwrap();
        started.recv_timeout(Duration::from_secs(5)).unwrap();
        let token = Arc::new(Cancellation::default());
        let callback = token.clone();
        let (done, finished) = mpsc::channel();
        let parse = session
            .parse(b"# queued".to_vec(), None, true, move |_| {
                callback.complete();
                done.send(()).unwrap();
            })
            .unwrap();
        token.cancel();
        token.install(parse); // cancellation before installation
        finished.recv_timeout(Duration::from_secs(5)).unwrap();
        release.send(()).unwrap();
        drop(running);
        blocker.close().unwrap();
        session.close().unwrap();
    }
    #[test]
    fn third_window_retries_observed_busy() {
        let _guard = lock(&TEST_LOCK);
        let blocker = CoreSession::new("busy-blocker").unwrap();
        let (start, started) = mpsc::channel();
        let (release, wait) = mpsc::channel();
        blocker
            .parse(b"# First".to_vec(), None, true, move |_| {
                start.send(()).unwrap();
                wait.recv().unwrap();
            })
            .unwrap();
        started.recv_timeout(Duration::from_secs(5)).unwrap();
        let a = CoreSession::new("busy-a").unwrap();
        let b = CoreSession::new("busy-b").unwrap();
        let first = a.parse(b"# A".to_vec(), None, true, |_| {}).unwrap();
        let second = b.parse(b"# B".to_vec(), None, true, |_| {}).unwrap();
        let fixture = Fixture::new();
        let mut loader = DocumentLoader::new();
        loader.open(fixture.file("third", b"# Third")).unwrap();
        // Both admission slots are reserved until the worker barrier releases.
        assert_eq!(markdown_linux_core::pending_parses(), 2);
        let deadline = Instant::now() + Duration::from_secs(5);
        while loader
            .cancel
            .as_ref()
            .unwrap()
            .busy_attempts
            .load(Ordering::Relaxed)
            == 0
        {
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        }
        release.send(()).unwrap();
        loaded(&mut loader);
        loader.close();
        first.cancel();
        second.cancel();
        blocker.close().unwrap();
        a.close().unwrap();
        b.close().unwrap();
    }
}
