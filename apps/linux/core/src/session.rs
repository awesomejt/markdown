use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::mpsc::channel;
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use crate::document::CoreDocument;
use crate::ffi::{self, md_job, md_session, size_of};
use crate::runtime::{self, ADMISSION, CoreError, HostLimits, checked};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoreOutcome {
    Published,
    Unchanged,
    Superseded,
    Cancelled,
    Failed,
    Closed,
}

impl CoreOutcome {
    fn from_raw(value: u32) -> Option<Self> {
        Some(match value {
            1 => CoreOutcome::Published,
            2 => CoreOutcome::Unchanged,
            3 => CoreOutcome::Superseded,
            4 => CoreOutcome::Cancelled,
            5 => CoreOutcome::Failed,
            6 => CoreOutcome::Closed,
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoreParseFailure {
    InvalidUtf8,
    SourceLimit,
    EventLimit,
    NodeLimit,
    DepthLimit,
    TextLimit,
    SourceMapping,
    Model,
    Other,
}

impl CoreParseFailure {
    fn from_raw(value: u32) -> Option<Self> {
        Some(match value {
            1 => CoreParseFailure::InvalidUtf8,
            2 => CoreParseFailure::SourceLimit,
            3 => CoreParseFailure::EventLimit,
            4 => CoreParseFailure::NodeLimit,
            5 => CoreParseFailure::DepthLimit,
            6 => CoreParseFailure::TextLimit,
            7 => CoreParseFailure::SourceMapping,
            8 => CoreParseFailure::Model,
            9 => CoreParseFailure::Other,
            _ => return None,
        })
    }
}

/// Adapter-observed read failures, reported to the session as external changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoreUnavailability {
    Deleted,
    PermissionDenied,
    AccessRevoked,
    Busy,
    Io,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoreAvailability {
    Available,
    Unavailable(CoreUnavailability),
}

#[derive(Clone, Copy, Debug, Default)]
pub struct CoreTimings {
    /// `md_job_run`: parse and model construction.
    pub parse: Duration,
    /// First `md_snapshot_get_view`: the flat-view encode.
    pub encode: Duration,
    /// Copying the view into owned values.
    pub copy: Duration,
}

#[derive(Clone, Debug)]
pub struct CoreResult {
    pub outcome: CoreOutcome,
    pub generation: u64,
    pub latest: u64,
    pub failure: Option<CoreParseFailure>,
    pub document: Option<CoreDocument>,
    pub timings: CoreTimings,
}

type Callback = Box<dyn FnOnce(Result<CoreResult, CoreError>) + Send>;

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// A one-shot result shared by concurrent waiters and callbacks.
struct Completion<T> {
    state: Mutex<CompletionState<T>>,
    ready: Condvar,
}

type Waiter<T> = Box<dyn FnOnce(T) + Send>;

struct CompletionState<T> {
    value: Option<T>,
    callbacks: Vec<Waiter<T>>,
}

impl<T: Clone> Completion<T> {
    fn new() -> Self {
        Completion {
            state: Mutex::new(CompletionState {
                value: None,
                callbacks: Vec::new(),
            }),
            ready: Condvar::new(),
        }
    }

    fn set(&self, value: T) {
        let callbacks = {
            let mut state = lock(&self.state);
            state.value = Some(value.clone());
            std::mem::take(&mut state.callbacks)
        };
        self.ready.notify_all();
        for callback in callbacks {
            callback(value.clone());
        }
    }

    fn wait(&self) -> T {
        let mut state = lock(&self.state);
        loop {
            if let Some(value) = state.value.as_ref() {
                return value.clone();
            }
            state = self
                .ready
                .wait(state)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
        }
    }

    /// Run `callback` with the value: now if it is set, otherwise from `set`.
    fn then(&self, callback: impl FnOnce(T) + Send + 'static) {
        let value = {
            let mut state = lock(&self.state);
            match state.value.clone() {
                Some(value) => value,
                None => return state.callbacks.push(Box::new(callback)),
            }
        };
        callback(value);
    }
}

struct SessionState {
    handle: md_session,
    closing: Option<Arc<Completion<Result<(), CoreError>>>>,
}

struct SessionInner {
    state: Mutex<SessionState>,
    limits: HostLimits,
}

/// One document lineage. Short calls lock the handle; parsing, encoding,
/// copying and destruction run on the shared core worker. Close and cancel may
/// race the worker through the thread-safe C ABI.
pub struct CoreSession {
    inner: Arc<SessionInner>,
}

impl CoreSession {
    /// A session over host limits that requests model v2 (D-025).
    pub fn new(document_id: &str) -> Result<Self, CoreError> {
        Self::with_limits(document_id, HostLimits::HOST)
    }

    pub fn with_limits(document_id: &str, limits: HostLimits) -> Result<Self, CoreError> {
        runtime::validate()?;
        let options = ffi::md_session_options {
            struct_size: size_of::<ffi::md_session_options>(),
            model_version: ffi::MD_MODEL_V2,
            max_source_bytes: limits.max_source_bytes,
            max_nodes: limits.max_nodes,
            max_depth: 0,
            max_events: limits.max_events,
            max_text_bytes: limits.max_text_bytes,
        };
        let mut handle = 0;
        // SAFETY: the ID bytes and options are read during the call; one u64 output.
        checked(unsafe {
            ffi::md_session_create(
                document_id.as_ptr(),
                document_id.len(),
                &options,
                &mut handle,
            )
        })?;
        Ok(CoreSession {
            inner: Arc::new(SessionInner {
                state: Mutex::new(SessionState {
                    handle,
                    closing: None,
                }),
                limits,
            }),
        })
    }

    pub fn limits(&self) -> HostLimits {
        self.inner.limits
    }

    /// Detach and cancel now, then wait for model destruction on the worker.
    /// Concurrent and repeated calls share one result.
    ///
    /// The release queues behind every call already on the shared worker,
    /// including other sessions' parses, so this can block for a full large
    /// parse. Never call it on the GTK main thread: use [`CoreSession::close_then`]
    /// or drop the session there.
    pub fn close(&self) -> Result<(), CoreError> {
        if runtime::on_worker() {
            return Err(CoreError::OnWorker);
        }
        self.begin_close().wait()
    }

    /// [`CoreSession::close`] without blocking: the session is detached and
    /// refuses new work at once, and `done` runs once the release has finished,
    /// on the worker (or at once, on this thread, if it already has).
    pub fn close_then(&self, done: impl FnOnce(Result<(), CoreError>) + Send + 'static) {
        self.begin_close().then(done);
    }

    fn begin_close(&self) -> Arc<Completion<Result<(), CoreError>>> {
        let mut state = lock(&self.inner.state);
        if let Some(closing) = &state.closing {
            return closing.clone();
        }
        let completion = Arc::new(Completion::new());
        let old = detach(&mut state);
        state.closing = Some(completion.clone());
        let done = completion.clone();
        // SAFETY: release takes a handle value; a stale one is rejected.
        runtime::submit(move || done.set(checked(unsafe { ffi::md_session_release(old) })));
        completion
    }

    /// Cancel the session's in-flight job, if any.
    pub fn cancel(&self) -> Result<(), CoreError> {
        let state = lock(&self.inner.state);
        if state.handle == 0 {
            return Err(CoreError::Closed);
        }
        let mut cancelled = 0;
        // SAFETY: a live session handle and one u32 output.
        checked(unsafe { ffi::md_session_cancel(state.handle, &mut cancelled) })
    }

    /// Record a failed adapter read (no I/O): supersedes in-flight work and
    /// returns the new or already-recorded generation.
    pub fn record_unavailable(&self, change: CoreUnavailability) -> Result<u64, CoreError> {
        let (kind, reason) = match change {
            CoreUnavailability::Deleted => (ffi::MD_CHANGE_DELETED, ffi::MD_REASON_NONE),
            CoreUnavailability::PermissionDenied => {
                (ffi::MD_CHANGE_UNREADABLE, ffi::MD_REASON_PERMISSION_DENIED)
            }
            CoreUnavailability::AccessRevoked => {
                (ffi::MD_CHANGE_UNREADABLE, ffi::MD_REASON_ACCESS_REVOKED)
            }
            CoreUnavailability::Busy => (ffi::MD_CHANGE_UNREADABLE, ffi::MD_REASON_BUSY),
            CoreUnavailability::Io => (ffi::MD_CHANGE_UNREADABLE, ffi::MD_REASON_IO),
        };
        let state = lock(&self.inner.state);
        if state.handle == 0 {
            return Err(CoreError::Closed);
        }
        let (mut disposition, mut generation) = (0, 0);
        // SAFETY: a live session, no token, two distinct outputs.
        checked(unsafe {
            ffi::md_session_external_change(
                state.handle,
                kind,
                reason,
                std::ptr::null(),
                0,
                &mut disposition,
                &mut generation,
            )
        })?;
        if disposition != ffi::MD_DISPOSITION_RECORDED {
            return Err(CoreError::InvalidSnapshot);
        }
        Ok(generation)
    }

    pub fn availability(&self) -> Result<CoreAvailability, CoreError> {
        let state = lock(&self.inner.state);
        if state.handle == 0 {
            return Err(CoreError::Closed);
        }
        let mut info = ffi::md_session_status_info {
            struct_size: size_of::<ffi::md_session_status_info>(),
            ..Default::default()
        };
        // SAFETY: a live session and an output of the negotiated size.
        checked(unsafe { ffi::md_session_get_status(state.handle, &mut info) })?;
        use CoreUnavailability::*;
        Ok(match (info.availability, info.unreadable_reason) {
            (ffi::MD_AVAILABLE, _) => CoreAvailability::Available,
            (ffi::MD_DELETED, _) => CoreAvailability::Unavailable(Deleted),
            (ffi::MD_UNREADABLE, ffi::MD_REASON_PERMISSION_DENIED) => {
                CoreAvailability::Unavailable(PermissionDenied)
            }
            (ffi::MD_UNREADABLE, ffi::MD_REASON_ACCESS_REVOKED) => {
                CoreAvailability::Unavailable(AccessRevoked)
            }
            (ffi::MD_UNREADABLE, ffi::MD_REASON_BUSY) => CoreAvailability::Unavailable(Busy),
            (ffi::MD_UNREADABLE, ffi::MD_REASON_IO) => CoreAvailability::Unavailable(Io),
            _ => return Err(CoreError::InvalidSnapshot),
        })
    }

    /// Queue a parse of `source` on the core worker. `done` runs exactly once:
    /// on the worker when the call finishes, or on the cancelling thread when a
    /// queued call is cancelled. With `copy_unchanged` false, an `Unchanged`
    /// outcome (the bytes equal the published source) returns no document.
    pub fn parse(
        &self,
        source: Vec<u8>,
        revision_token: Option<&[u8]>,
        copy_unchanged: bool,
        done: impl FnOnce(Result<CoreResult, CoreError>) + Send + 'static,
    ) -> Result<ParseRequest, CoreError> {
        if lock(&self.inner.state).handle == 0 {
            return Err(CoreError::Closed);
        }
        ADMISSION.reserve()?;
        let request = Arc::new(Request {
            state: Mutex::new(RequestState {
                phase: Phase::Queued,
                source: Some(source),
                token: revision_token.map(<[u8]>::to_vec),
                done: Some(Box::new(done)),
                cancelled: false,
                job: 0,
            }),
        });
        let (worker_request, session) = (request.clone(), self.inner.clone());
        runtime::submit(move || {
            let Some((source, token)) = worker_request.begin() else {
                return;
            };
            // A panic here is an adapter bug; still call back and release admission.
            let result = catch_unwind(AssertUnwindSafe(|| {
                session.run(source, token, copy_unchanged, &worker_request)
            }))
            .unwrap_or(Err(CoreError::Internal));
            worker_request.complete(result);
        });
        Ok(ParseRequest { request })
    }

    /// [`CoreSession::parse`], waiting for the result. Never call it on the worker.
    pub fn parse_blocking(
        &self,
        source: Vec<u8>,
        revision_token: Option<&[u8]>,
        copy_unchanged: bool,
    ) -> Result<CoreResult, CoreError> {
        if runtime::on_worker() {
            return Err(CoreError::OnWorker);
        }
        let (sender, receiver) = channel();
        self.parse(source, revision_token, copy_unchanged, move |result| {
            let _ = sender.send(result);
        })?;
        receiver.recv().unwrap_or(Err(CoreError::Closed))
    }
}

/// Take the handle and cancel its in-flight job; the caller releases it.
fn detach(state: &mut SessionState) -> md_session {
    let old = std::mem::replace(&mut state.handle, 0);
    if old != 0 {
        let mut cancelled = 0;
        // SAFETY: a live session handle and one u32 output.
        unsafe { ffi::md_session_cancel(old, &mut cancelled) };
    }
    old
}

impl Drop for CoreSession {
    /// Without an explicit close, release on the worker without waiting.
    fn drop(&mut self) {
        let old = detach(&mut lock(&self.inner.state));
        if old != 0 {
            // SAFETY: release takes a handle value; nothing else holds this one now.
            runtime::submit(move || unsafe {
                ffi::md_session_release(old);
            });
        }
    }
}

impl SessionInner {
    fn closed(&self) -> bool {
        lock(&self.state).handle == 0
    }

    fn run(
        &self,
        source: Vec<u8>,
        token: Option<Vec<u8>>,
        copy_unchanged: bool,
        request: &Request,
    ) -> Result<CoreResult, CoreError> {
        request.check_cancelled()?;
        let (mut job, mut generation) = (0, 0);
        {
            let state = lock(&self.state);
            if state.handle == 0 {
                return Err(CoreError::Closed);
            }
            let token = token.as_deref().unwrap_or_default();
            // SAFETY: the core copies source and token during the call; two outputs.
            checked(unsafe {
                ffi::md_session_submit(
                    state.handle,
                    source.as_ptr(),
                    source.len(),
                    token.as_ptr(),
                    token.len(),
                    &mut job,
                    &mut generation,
                )
            })?;
        }
        // The core holds its own copy (at most the limit plus one byte).
        drop(source);
        request.install_job(job);
        let _job = JobGuard(request);
        let mut result = ffi::md_job_result {
            struct_size: size_of::<ffi::md_job_result>(),
            ..Default::default()
        };
        let start = Instant::now();
        // SAFETY: an unrun job on this worker thread and an output of the negotiated size.
        checked(unsafe { ffi::md_job_run(job, &mut result) })?;
        let parsed = Instant::now();
        let snapshot = SnapshotGuard(result.snapshot);
        request.check_cancelled()?;
        if self.closed() {
            return Err(CoreError::Closed);
        }
        let outcome = CoreOutcome::from_raw(result.outcome).ok_or(CoreError::InvalidSnapshot)?;
        let failure = CoreParseFailure::from_raw(result.failure);
        if outcome == CoreOutcome::Failed && failure.is_none() {
            return Err(CoreError::InvalidSnapshot);
        }
        let (mut encoded, mut copied) = (parsed, parsed);
        let mut document = None;
        if outcome == CoreOutcome::Published
            || (outcome == CoreOutcome::Unchanged && copy_unchanged)
        {
            if snapshot.0 == 0 {
                return Err(CoreError::InvalidSnapshot);
            }
            let mut view = ffi::md_snapshot_view {
                struct_size: size_of::<ffi::md_snapshot_view>(),
                ..Default::default()
            };
            // SAFETY: a live snapshot handle and an output of the negotiated size.
            checked(unsafe { ffi::md_snapshot_get_view(snapshot.0, &mut view) })?;
            encoded = Instant::now();
            // The view's arrays stay valid until `snapshot` drops after this copy.
            document = Some(CoreDocument::copying(
                &view,
                self.limits.max_snapshot_bytes,
            )?);
            copied = Instant::now();
        }
        drop(snapshot);
        request.check_cancelled()?;
        if self.closed() {
            return Err(CoreError::Closed);
        }
        Ok(CoreResult {
            outcome,
            generation: result.generation,
            latest: result.latest,
            failure,
            document,
            timings: CoreTimings {
                parse: parsed - start,
                encode: encoded - parsed,
                copy: copied - encoded,
            },
        })
    }
}

struct SnapshotGuard(ffi::md_snapshot);

impl Drop for SnapshotGuard {
    fn drop(&mut self) {
        // SAFETY: releasing 0 is a no-op; the handle is owned here and released once.
        unsafe { ffi::md_snapshot_release(self.0) };
    }
}

struct JobGuard<'a>(&'a Request);

impl Drop for JobGuard<'_> {
    fn drop(&mut self) {
        self.0.release_job();
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Queued,
    Running,
    Finished,
}

struct RequestState {
    phase: Phase,
    source: Option<Vec<u8>>,
    token: Option<Vec<u8>>,
    done: Option<Callback>,
    cancelled: bool,
    job: md_job,
}

/// Queued cancellation drops this request's input and releases admission before
/// the worker reaches it. Running work keeps its slot until it finishes. Every
/// terminal path calls back and releases admission exactly once.
struct Request {
    state: Mutex<RequestState>,
}

impl Request {
    fn begin(&self) -> Option<(Vec<u8>, Option<Vec<u8>>)> {
        let mut state = lock(&self.state);
        if state.phase != Phase::Queued {
            return None;
        }
        state.phase = Phase::Running;
        Some((state.source.take()?, state.token.take()))
    }

    fn check_cancelled(&self) -> Result<(), CoreError> {
        match lock(&self.state).cancelled {
            true => Err(CoreError::Cancelled),
            false => Ok(()),
        }
    }

    fn install_job(&self, job: md_job) {
        let mut state = lock(&self.state);
        state.job = job;
        if state.cancelled {
            // SAFETY: a live job handle; cancel may precede the run.
            unsafe { ffi::md_job_cancel(job) };
        }
    }

    fn release_job(&self) {
        let mut state = lock(&self.state);
        // SAFETY: the job is owned by this request and released once, under the lock
        // that `cancel` also takes, so cancel never sees a released handle.
        unsafe { ffi::md_job_release(state.job) };
        state.job = 0;
    }

    fn cancel(&self) {
        let done = {
            let mut state = lock(&self.state);
            state.cancelled = true;
            match state.phase {
                Phase::Queued => {
                    state.phase = Phase::Finished;
                    state.source = None;
                    state.token = None;
                    ADMISSION.release();
                    state.done.take()
                }
                Phase::Running => {
                    if state.job != 0 {
                        // SAFETY: a live job handle, cancellable from any thread.
                        unsafe { ffi::md_job_cancel(state.job) };
                    }
                    None
                }
                Phase::Finished => None,
            }
        };
        if let Some(done) = done {
            done(Err(CoreError::Cancelled));
        }
    }

    fn complete(&self, result: Result<CoreResult, CoreError>) {
        let (done, result) = {
            let mut state = lock(&self.state);
            debug_assert!(state.phase == Phase::Running);
            state.phase = Phase::Finished;
            ADMISSION.release();
            let result = if state.cancelled {
                Err(CoreError::Cancelled)
            } else {
                result
            };
            (state.done.take(), result)
        };
        if let Some(done) = done {
            done(result);
        }
    }
}

/// A queued or running parse. Dropping it does not cancel the call.
pub struct ParseRequest {
    request: Arc<Request>,
}

impl ParseRequest {
    /// Cancel the call: a queued call finishes now with `Cancelled`; a running
    /// call is cancelled in the core and reports `Cancelled` when it returns.
    pub fn cancel(&self) {
        self.request.cancel();
    }

    /// Whether the worker has started this call (for tests and diagnostics).
    pub fn is_running(&self) -> bool {
        lock(&self.request.state).phase == Phase::Running
    }
}
