//! Hand-written, versioned C ABI over `markdown-core` (D-009).
//!
//! The checked-in header `core/include/markdown_core.h` is the contract;
//! `specs/ffi/README.md` documents ownership, threading, versioning and error
//! rules. Every entry point catches panics, validates pointers it is given
//! (null, alignment, length), zeroes its outputs first and reports a stable
//! `md_status`. Handles are registry keys, never pointers, so a stale or
//! repeated release is detected instead of freeing memory twice.
//!
//! No Rust lifetime, `Arc` or parser pointer crosses the boundary: snapshot
//! views point into an immutable flat encoding owned by the snapshot handle.
#![deny(unsafe_op_in_unsafe_fn)]
#![warn(clippy::undocumented_unsafe_blocks)]

pub mod abi;
mod flat;
#[cfg(test)]
mod tests;

use abi::*;
use markdown_core::session::*;
use markdown_core::*;
use std::collections::BTreeMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};

// ---------------------------------------------------------------------------
// Handle registry

struct JobEntry {
    token: CancellationToken,
    job: Mutex<Option<Job>>,
}
struct SnapshotEntry {
    snapshot: Arc<Snapshot>,
    flat: OnceLock<Result<flat::Flat, md_status>>,
}
struct Registry {
    next: u64,
    sessions: BTreeMap<u64, Arc<DocumentSession>>,
    jobs: BTreeMap<u64, Arc<JobEntry>>,
    snapshots: BTreeMap<u64, Arc<SnapshotEntry>>,
}
static REGISTRY: Mutex<Registry> = Mutex::new(Registry {
    next: 1,
    sessions: BTreeMap::new(),
    jobs: BTreeMap::new(),
    snapshots: BTreeMap::new(),
});

/// Registry access survives a poisoned lock: every mutation is a single map
/// insert/remove, so a contained panic cannot leave it half-updated.
fn registry() -> MutexGuard<'static, Registry> {
    REGISTRY.lock().unwrap_or_else(PoisonError::into_inner)
}
impl Registry {
    fn allocate(&mut self) -> Result<u64, md_status> {
        let id = self.next;
        self.next = id.checked_add(1).ok_or(MD_ERR_LIMIT)?;
        Ok(id)
    }
}
fn session(handle: md_session) -> Result<Arc<DocumentSession>, md_status> {
    registry()
        .sessions
        .get(&handle)
        .cloned()
        .ok_or(MD_ERR_INVALID_HANDLE)
}
fn job(handle: md_job) -> Result<Arc<JobEntry>, md_status> {
    registry()
        .jobs
        .get(&handle)
        .cloned()
        .ok_or(MD_ERR_INVALID_HANDLE)
}
fn snapshot(handle: md_snapshot) -> Result<Arc<SnapshotEntry>, md_status> {
    registry()
        .snapshots
        .get(&handle)
        .cloned()
        .ok_or(MD_ERR_INVALID_HANDLE)
}
fn insert_snapshot(snapshot: Arc<Snapshot>) -> Result<md_snapshot, md_status> {
    let entry = Arc::new(SnapshotEntry {
        snapshot,
        flat: OnceLock::new(),
    });
    let mut registry = registry();
    let id = registry.allocate()?;
    registry.snapshots.insert(id, entry);
    Ok(id)
}

// ---------------------------------------------------------------------------
// Boundary helpers

#[cfg(test)]
thread_local! {
    /// Test-only panic injection at `checkpoint()`, per thread.
    static INJECT_PANIC: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}
/// Point inside each entry point (after outputs are zeroed) where tests can
/// inject a panic to prove containment. A no-op outside `cfg(test)`.
#[inline]
fn checkpoint() {
    #[cfg(test)]
    if INJECT_PANIC.with(|flag| flag.replace(false)) {
        panic!("injected test panic");
    }
}

/// Run an entry point body, mapping panics to `MD_ERR_PANIC`.
fn guard(body: impl FnOnce() -> Result<(), md_status>) -> md_status {
    match catch_unwind(AssertUnwindSafe(body)) {
        Ok(Ok(())) => MD_OK,
        Ok(Err(status)) => status,
        Err(_) => MD_ERR_PANIC,
    }
}

/// Borrow caller bytes for the duration of the call. A null pointer is only
/// accepted with a zero length.
///
/// # Safety
/// A non-null `ptr` must be valid for reads of `len` bytes, unchanged until
/// the call returns.
unsafe fn bytes<'a>(ptr: *const u8, len: usize) -> Result<&'a [u8], md_status> {
    if len == 0 {
        return Ok(&[]);
    }
    if ptr.is_null() || len > isize::MAX as usize {
        return Err(MD_ERR_INVALID_ARGUMENT);
    }
    // SAFETY: non-null, length bounded by isize::MAX and readable per the
    // caller contract; u8 has no alignment requirement.
    Ok(unsafe { std::slice::from_raw_parts(ptr, len) })
}

/// Validate a caller output pointer and zero it.
///
/// # Safety
/// A non-null, aligned `ptr` must be valid for writes of `T`.
unsafe fn output<'a, T: Default>(ptr: *mut T) -> Result<&'a mut T, md_status> {
    if ptr.is_null() || !ptr.is_aligned() {
        return Err(MD_ERR_INVALID_ARGUMENT);
    }
    // SAFETY: non-null, aligned and writable per the caller contract. The
    // value is overwritten without reading the caller's bytes.
    unsafe {
        ptr.write(T::default());
        Ok(&mut *ptr)
    }
}
/// Like [`output`] for optional out-parameters: null means "not wanted".
///
/// # Safety
/// As for [`output`] when non-null.
unsafe fn optional_output<'a, T: Default>(ptr: *mut T) -> Result<Option<&'a mut T>, md_status> {
    if ptr.is_null() {
        Ok(None)
    } else {
        // SAFETY: forwarded caller contract.
        unsafe { output(ptr) }.map(Some)
    }
}

/// Header-versioned structs: read `struct_size` first, require an exact
/// match, then zero everything after it.
trait Versioned: Default {
    fn set_struct_size(&mut self, size: u32);
}
macro_rules! sized {
    ($($t:ty),+) => {$(
        impl Versioned for $t {
            fn set_struct_size(&mut self, size: u32) { self.struct_size = size; }
        }
    )+};
}
sized!(
    md_abi_info,
    md_session_options,
    md_session_status_info,
    md_job_result,
    md_snapshot_view,
    md_anchor_resolution
);

/// Validate a versioned output struct and zero it, keeping `struct_size`.
///
/// # Safety
/// A non-null, aligned `ptr` must be valid for reads and writes of `T`.
unsafe fn versioned_output<'a, T: Versioned>(ptr: *mut T) -> Result<&'a mut T, md_status> {
    if ptr.is_null() || !ptr.is_aligned() {
        return Err(MD_ERR_INVALID_ARGUMENT);
    }
    // SAFETY: non-null and aligned; `struct_size` is the first u32 field of
    // every versioned struct (repr(C)), readable per the caller contract.
    let size = unsafe { ptr.cast::<u32>().read() };
    if size != size_of_u32::<T>() {
        return Err(MD_ERR_STRUCT_SIZE);
    }
    // SAFETY: the caller declared a buffer of exactly size_of::<T>() bytes.
    let value = unsafe { &mut *ptr };
    *value = T::default();
    value.set_struct_size(size);
    Ok(value)
}
const fn size_of_u32<T>() -> u32 {
    abi::size::<T>()
}

fn write_buffer(
    data: &[u8],
    buffer: *mut u8,
    capacity: usize,
    length: &mut usize,
) -> Result<(), md_status> {
    *length = data.len();
    if capacity < data.len() {
        return Err(MD_ERR_BUFFER_TOO_SMALL);
    }
    if buffer.is_null() {
        return Err(MD_ERR_INVALID_ARGUMENT);
    }
    // SAFETY: the caller promised `capacity >= data.len()` writable bytes at
    // a non-null `buffer`; the regions cannot overlap (core-owned source).
    unsafe { std::ptr::copy_nonoverlapping(data.as_ptr(), buffer, data.len()) };
    Ok(())
}

fn failure_code(error: &SessionError) -> u32 {
    match error {
        SessionError::InvalidUtf8 => MD_FAILURE_INVALID_UTF8,
        SessionError::Parse(error) => match error {
            ParseError::SourceLimit => MD_FAILURE_SOURCE_LIMIT,
            ParseError::EventLimit => MD_FAILURE_EVENT_LIMIT,
            ParseError::NodeLimit => MD_FAILURE_NODE_LIMIT,
            ParseError::DepthLimit => MD_FAILURE_DEPTH_LIMIT,
            ParseError::TextLimit => MD_FAILURE_TEXT_LIMIT,
            ParseError::SourceMapping => MD_FAILURE_SOURCE_MAPPING,
            ParseError::Model(_) => MD_FAILURE_MODEL,
        },
        _ => MD_FAILURE_OTHER,
    }
}
fn session_error(error: SessionError) -> md_status {
    match error {
        SessionError::InvalidDocumentId => MD_ERR_INVALID_DOCUMENT_ID,
        SessionError::RevisionToken => MD_ERR_INVALID_REVISION_TOKEN,
        SessionError::GenerationExhausted => MD_ERR_GENERATION_EXHAUSTED,
        _ => MD_ERR_INTERNAL,
    }
}
fn anchor_error(error: AnchorError) -> md_status {
    match error {
        AnchorError::NoBlocks => MD_ERR_NO_BLOCKS,
        AnchorError::UnknownNode => MD_ERR_UNKNOWN_NODE,
        AnchorError::UnsupportedVersion => MD_ERR_ANCHOR_VERSION,
    }
}
fn token(bytes: &[u8]) -> Result<Option<RevisionToken>, md_status> {
    if bytes.is_empty() {
        return Ok(None);
    }
    RevisionToken::new(bytes)
        .map(Some)
        .map_err(|_| MD_ERR_INVALID_REVISION_TOKEN)
}

/// Bit mask of model versions this build can produce.
const fn model_versions() -> u32 {
    let mut bits = 1 << MD_MODEL_V1;
    if cfg!(feature = "model-v2") {
        bits |= 1 << MD_MODEL_V2;
    }
    bits
}

/// ABI ceilings keep every span, index and string offset inside `u32`.
pub const MAX_SOURCE_BYTES: u64 = 64 * 1024 * 1024;
pub const MAX_NODES: u64 = 1_000_000;
pub const MAX_DEPTH: u64 = 256;
pub const MAX_EVENTS: u64 = 16_000_000;
pub const MAX_TEXT_BYTES: u64 = 256 * 1024 * 1024;

fn session_options(options: &md_session_options) -> Result<SessionOptions, md_status> {
    let model = match options.model_version {
        MD_MODEL_V1 => ModelVersion::V1,
        #[cfg(feature = "model-v2")]
        MD_MODEL_V2 => ModelVersion::V2,
        _ => return Err(MD_ERR_UNSUPPORTED_MODEL),
    };
    let mut limits = ParseLimits::default();
    let pick = |value: u64, ceiling: u64, default: usize| -> Result<usize, md_status> {
        match value {
            0 => Ok(default),
            v if v > ceiling => Err(MD_ERR_INVALID_ARGUMENT),
            v => usize::try_from(v).map_err(|_| MD_ERR_INVALID_ARGUMENT),
        }
    };
    limits.model.max_source_bytes = pick(
        options.max_source_bytes,
        MAX_SOURCE_BYTES,
        limits.model.max_source_bytes,
    )?;
    limits.model.max_nodes = pick(options.max_nodes, MAX_NODES, limits.model.max_nodes)?;
    limits.model.max_depth = pick(options.max_depth, MAX_DEPTH, limits.model.max_depth)?;
    limits.max_events = pick(options.max_events, MAX_EVENTS, limits.max_events)?;
    limits.max_text_bytes = pick(
        options.max_text_bytes,
        MAX_TEXT_BYTES,
        limits.max_text_bytes,
    )?;
    Ok(SessionOptions { model, limits })
}

// ---------------------------------------------------------------------------
// Version negotiation and diagnostics

/// Report the library ABI and every struct size.
///
/// # Safety
/// `info` must be null or point to a writable, aligned `md_abi_info` whose
/// `struct_size` the caller has set.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn md_abi_negotiate(abi_version: u32, info: *mut md_abi_info) -> md_status {
    guard(|| {
        // SAFETY: forwarded caller contract.
        let info = unsafe { versioned_output(info) }?;
        checkpoint();
        *info = md_abi_info {
            struct_size: info.struct_size,
            abi_version: MD_ABI_VERSION,
            model_versions: model_versions(),
            anchor_version: ANCHOR_VERSION,
            pointer_size: size::<usize>(),
            max_anchor_bytes: MD_ANCHOR_MAX_BYTES,
            session_options_size: size::<md_session_options>(),
            session_status_size: size::<md_session_status_info>(),
            job_result_size: size::<md_job_result>(),
            snapshot_view_size: size::<md_snapshot_view>(),
            node_size: size::<md_node>(),
            intent_size: size::<md_intent>(),
            outline_entry_size: size::<md_outline_entry>(),
            diagnostic_size: size::<md_diagnostic>(),
            property_size: size::<md_property>(),
            anchor_resolution_size: size::<md_anchor_resolution>(),
        };
        if abi_version != MD_ABI_VERSION {
            return Err(MD_ERR_VERSION_MISMATCH);
        }
        Ok(())
    })
}

/// Static, NUL-terminated ASCII name of a status; never null, never freed.
#[unsafe(no_mangle)]
pub extern "C" fn md_status_name(status: md_status) -> *const std::ffi::c_char {
    let name: &'static std::ffi::CStr = match status {
        MD_OK => c"ok",
        MD_ERR_INVALID_ARGUMENT => c"invalid_argument",
        MD_ERR_VERSION_MISMATCH => c"version_mismatch",
        MD_ERR_STRUCT_SIZE => c"struct_size",
        MD_ERR_INVALID_HANDLE => c"invalid_handle",
        MD_ERR_UNSUPPORTED_MODEL => c"unsupported_model",
        MD_ERR_BUFFER_TOO_SMALL => c"buffer_too_small",
        MD_ERR_LIMIT => c"limit",
        MD_ERR_INVALID_STATE => c"invalid_state",
        MD_ERR_INVALID_DOCUMENT_ID => c"invalid_document_id",
        MD_ERR_INVALID_REVISION_TOKEN => c"invalid_revision_token",
        MD_ERR_GENERATION_EXHAUSTED => c"generation_exhausted",
        MD_ERR_ANCHOR_INVALID => c"anchor_invalid",
        MD_ERR_ANCHOR_VERSION => c"anchor_version",
        MD_ERR_NO_BLOCKS => c"no_blocks",
        MD_ERR_UNKNOWN_NODE => c"unknown_node",
        MD_ERR_PANIC => c"panic",
        MD_ERR_INTERNAL => c"internal",
        _ => c"unknown",
    };
    name.as_ptr()
}

/// Counts of live handles of each type (process-wide), for leak checks.
///
/// # Safety
/// Each pointer must be null or point to a writable, aligned `uint64_t`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn md_live_handles(
    sessions: *mut u64,
    jobs: *mut u64,
    snapshots: *mut u64,
) -> md_status {
    guard(|| {
        // SAFETY: forwarded caller contract for each optional pointer.
        let (sessions, jobs, snapshots) = unsafe {
            (
                optional_output(sessions)?,
                optional_output(jobs)?,
                optional_output(snapshots)?,
            )
        };
        let registry = registry();
        checkpoint();
        for (out, count) in [
            (sessions, registry.sessions.len()),
            (jobs, registry.jobs.len()),
            (snapshots, registry.snapshots.len()),
        ] {
            if let Some(out) = out {
                *out = count as u64;
            }
        }
        Ok(())
    })
}

// ---------------------------------------------------------------------------
// Sessions

/// Create a session for one document lineage.
///
/// # Safety
/// `document_id` must be readable for `document_id_length` bytes; `options`
/// must point to a readable, aligned `md_session_options`; `out_session` to a
/// writable, aligned `md_session`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn md_session_create(
    document_id: *const u8,
    document_id_length: usize,
    options: *const md_session_options,
    out_session: *mut md_session,
) -> md_status {
    guard(|| {
        // SAFETY: forwarded caller contract.
        let out = unsafe { output(out_session) }?;
        if options.is_null() || !options.is_aligned() {
            return Err(MD_ERR_INVALID_ARGUMENT);
        }
        // SAFETY: non-null, aligned and readable per the caller contract;
        // struct_size is the leading u32.
        if unsafe { options.cast::<u32>().read() } != size::<md_session_options>() {
            return Err(MD_ERR_STRUCT_SIZE);
        }
        // SAFETY: the declared size matches md_session_options exactly.
        let options = session_options(unsafe { &*options })?;
        // SAFETY: forwarded caller contract.
        let id = unsafe { bytes(document_id, document_id_length) }?;
        let id = std::str::from_utf8(id).map_err(|_| MD_ERR_INVALID_DOCUMENT_ID)?;
        let session =
            DocumentSession::new(DocumentId(id.to_owned()), options).map_err(session_error)?;
        checkpoint();
        let mut registry = registry();
        let handle = registry.allocate()?;
        registry.sessions.insert(handle, Arc::new(session));
        *out = handle;
        Ok(())
    })
}

/// Release a session. In-flight work is cancelled; snapshots stay valid.
#[unsafe(no_mangle)]
pub extern "C" fn md_session_release(session: md_session) -> md_status {
    guard(|| {
        if session == 0 {
            return Ok(());
        }
        checkpoint();
        let removed = registry().sessions.remove(&session);
        // Drop outside the registry lock.
        removed.map(drop).ok_or(MD_ERR_INVALID_HANDLE)
    })
}

/// Copy `bytes` into a new submission and return its job.
///
/// # Safety
/// `bytes`/`token` must be readable for their lengths (null only with zero
/// length); `out_job` must be writable and aligned; `out_generation` null or
/// writable and aligned.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn md_session_submit(
    session: md_session,
    bytes: *const u8,
    length: usize,
    token: *const u8,
    token_length: usize,
    out_job: *mut md_job,
    out_generation: *mut u64,
) -> md_status {
    guard(|| {
        // SAFETY: forwarded caller contract.
        let (out, generation) = unsafe { (output(out_job)?, optional_output(out_generation)?) };
        // SAFETY: forwarded caller contract.
        let (source, token_bytes) = unsafe {
            (
                self::bytes(bytes, length)?,
                self::bytes(token, token_length)?,
            )
        };
        let token = self::token(token_bytes)?;
        let session = self::session(session)?;
        checkpoint();
        let job = session
            .submit(source.to_vec(), token)
            .map_err(session_error)?;
        let job_generation = job.generation().get();
        let entry = Arc::new(JobEntry {
            token: job.cancellation_token().clone(),
            job: Mutex::new(Some(job)),
        });
        let mut registry = registry();
        let handle = registry.allocate()?;
        registry.jobs.insert(handle, entry);
        *out = handle;
        if let Some(generation) = generation {
            *generation = job_generation;
        }
        Ok(())
    })
}

/// Report a platform change notification; performs no I/O.
///
/// # Safety
/// `token` must be readable for `token_length` bytes (null only with zero);
/// `out_disposition` writable and aligned; `out_generation` null or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn md_session_external_change(
    session: md_session,
    change: u32,
    reason: u32,
    token: *const u8,
    token_length: usize,
    out_disposition: *mut u32,
    out_generation: *mut u64,
) -> md_status {
    guard(|| {
        // SAFETY: forwarded caller contract.
        let (disposition, generation) =
            unsafe { (output(out_disposition)?, optional_output(out_generation)?) };
        // SAFETY: forwarded caller contract.
        let token = self::token(unsafe { bytes(token, token_length) }?)?;
        let change = match (change, reason) {
            (MD_CHANGE_MODIFIED, MD_REASON_NONE) => ExternalChange::Modified { token },
            (MD_CHANGE_DELETED, MD_REASON_NONE) if token.is_none() => ExternalChange::Deleted,
            (MD_CHANGE_UNREADABLE, reason) if token.is_none() => {
                ExternalChange::Unreadable(match reason {
                    MD_REASON_PERMISSION_DENIED => UnreadableReason::PermissionDenied,
                    MD_REASON_ACCESS_REVOKED => UnreadableReason::AccessRevoked,
                    MD_REASON_BUSY => UnreadableReason::Busy,
                    MD_REASON_IO => UnreadableReason::Io,
                    _ => return Err(MD_ERR_INVALID_ARGUMENT),
                })
            }
            _ => return Err(MD_ERR_INVALID_ARGUMENT),
        };
        let session = self::session(session)?;
        checkpoint();
        let (code, value) = match session.external_change(change).map_err(session_error)? {
            ChangeDisposition::Unchanged => (MD_DISPOSITION_UNCHANGED, 0),
            ChangeDisposition::ReadRequired => (MD_DISPOSITION_READ_REQUIRED, 0),
            ChangeDisposition::Pending(g) => (MD_DISPOSITION_PENDING, g.get()),
            ChangeDisposition::Recorded(g) => (MD_DISPOSITION_RECORDED, g.get()),
        };
        *disposition = code;
        if let Some(generation) = generation {
            *generation = value;
        }
        Ok(())
    })
}

/// Cancel the in-flight submission; `*out_cancelled` is 1 if one was live.
///
/// # Safety
/// `out_cancelled` must be null or writable and aligned.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn md_session_cancel(
    session: md_session,
    out_cancelled: *mut u32,
) -> md_status {
    guard(|| {
        // SAFETY: forwarded caller contract.
        let out = unsafe { optional_output(out_cancelled) }?;
        let session = self::session(session)?;
        checkpoint();
        let cancelled = session.cancel();
        if let Some(out) = out {
            *out = u32::from(cancelled);
        }
        Ok(())
    })
}

/// Copy the session status.
///
/// # Safety
/// `out_status` must point to a writable, aligned `md_session_status_info`
/// with `struct_size` set.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn md_session_get_status(
    session: md_session,
    out_status: *mut md_session_status_info,
) -> md_status {
    guard(|| {
        // SAFETY: forwarded caller contract.
        let out = unsafe { versioned_output(out_status) }?;
        let session = self::session(session)?;
        checkpoint();
        let status = session.status();
        let generation = |g: Option<Generation>| g.map_or(0, Generation::get);
        out.flags = if status.is_current() {
            MD_STATUS_IS_CURRENT
        } else {
            0
        };
        out.latest = generation(status.latest);
        out.confirmed = generation(status.confirmed);
        out.pending = generation(status.pending);
        out.published = status
            .published
            .as_ref()
            .map_or(0, |s| s.generation().get());
        (out.availability, out.unreadable_reason) = match status.availability {
            Availability::Available => (MD_AVAILABLE, MD_REASON_NONE),
            Availability::Deleted => (MD_DELETED, MD_REASON_NONE),
            Availability::Unreadable(reason) => (
                MD_UNREADABLE,
                match reason {
                    UnreadableReason::PermissionDenied => MD_REASON_PERMISSION_DENIED,
                    UnreadableReason::AccessRevoked => MD_REASON_ACCESS_REVOKED,
                    UnreadableReason::Busy => MD_REASON_BUSY,
                    UnreadableReason::Io => MD_REASON_IO,
                },
            ),
        };
        if let Some((g, error)) = &status.failure {
            out.failure_generation = g.get();
            out.failure = failure_code(error);
        }
        Ok(())
    })
}

/// Acquire a new handle to the published snapshot (0 when none).
///
/// # Safety
/// `out_snapshot` must be writable and aligned.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn md_session_current(
    session: md_session,
    out_snapshot: *mut md_snapshot,
) -> md_status {
    guard(|| {
        // SAFETY: forwarded caller contract.
        let out = unsafe { output(out_snapshot) }?;
        let session = self::session(session)?;
        checkpoint();
        if let Some(snapshot) = session.current() {
            *out = insert_snapshot(snapshot)?;
        }
        Ok(())
    })
}

// ---------------------------------------------------------------------------
// Jobs

/// Run a job once on the calling thread. The handle stays live until
/// `md_job_release`; a second run returns `MD_ERR_INVALID_STATE`.
///
/// # Safety
/// `out_result` must point to a writable, aligned `md_job_result` with
/// `struct_size` set.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn md_job_run(job: md_job, out_result: *mut md_job_result) -> md_status {
    guard(|| {
        // SAFETY: forwarded caller contract.
        let out = unsafe { versioned_output(out_result) }?;
        let entry = self::job(job)?;
        let work = entry
            .job
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
            .ok_or(MD_ERR_INVALID_STATE)?;
        checkpoint();
        // Every outcome reports the job's own generation; an `Unchanged`
        // snapshot keeps the generation it was published at.
        out.generation = work.generation().get();
        let snapshot = match work.run() {
            Outcome::Published(s) => {
                out.outcome = MD_OUTCOME_PUBLISHED;
                Some(s)
            }
            Outcome::Unchanged(s) => {
                out.outcome = MD_OUTCOME_UNCHANGED;
                Some(s)
            }
            Outcome::Superseded { latest, .. } => {
                out.outcome = MD_OUTCOME_SUPERSEDED;
                out.latest = latest.get();
                None
            }
            Outcome::Cancelled { .. } => {
                out.outcome = MD_OUTCOME_CANCELLED;
                None
            }
            Outcome::Failed { error, .. } => {
                out.outcome = MD_OUTCOME_FAILED;
                out.failure = failure_code(&error);
                None
            }
            Outcome::Closed { .. } => {
                out.outcome = MD_OUTCOME_CLOSED;
                None
            }
        };
        if let Some(snapshot) = snapshot {
            out.snapshot = insert_snapshot(snapshot)?;
        }
        Ok(())
    })
}

/// Request cooperative cancellation of a job, running or not.
#[unsafe(no_mangle)]
pub extern "C" fn md_job_cancel(job: md_job) -> md_status {
    guard(|| {
        let entry = self::job(job)?;
        checkpoint();
        entry.token.cancel();
        Ok(())
    })
}

/// Release a job handle. An unrun job is dropped (clearing its pending
/// state); a job running on another thread finishes normally.
#[unsafe(no_mangle)]
pub extern "C" fn md_job_release(job: md_job) -> md_status {
    guard(|| {
        if job == 0 {
            return Ok(());
        }
        checkpoint();
        let removed = registry().jobs.remove(&job);
        removed.map(drop).ok_or(MD_ERR_INVALID_HANDLE)
    })
}

// ---------------------------------------------------------------------------
// Snapshots

/// Fill a bulk read-only view; pointers stay valid until the handle is
/// released.
///
/// # Safety
/// `out_view` must point to a writable, aligned `md_snapshot_view` with
/// `struct_size` set.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn md_snapshot_get_view(
    snapshot: md_snapshot,
    out_view: *mut md_snapshot_view,
) -> md_status {
    guard(|| {
        // SAFETY: forwarded caller contract.
        let out = unsafe { versioned_output(out_view) }?;
        let entry = self::snapshot(snapshot)?;
        checkpoint();
        let flat = entry
            .flat
            .get_or_init(|| flat::Flat::encode(&entry.snapshot))
            .as_ref()
            .map_err(|e| *e)?;
        // The registry keeps `entry` (and so `flat`) alive after this clone
        // drops, until md_snapshot_release.
        flat.view(
            entry.snapshot.source(),
            entry.snapshot.generation().get(),
            out,
        );
        Ok(())
    })
}

fn anchor_bytes(anchor: &Anchor) -> Result<Vec<u8>, md_status> {
    let bytes = serde_json::to_vec(anchor).map_err(|_| MD_ERR_INTERNAL)?;
    if bytes.len() > MD_ANCHOR_MAX_BYTES as usize {
        return Err(MD_ERR_INTERNAL);
    }
    Ok(bytes)
}

/// Serialize an anchor for an absolute source offset into `buffer`.
///
/// # Safety
/// `buffer` must be writable for `capacity` bytes (may be null with zero);
/// `out_length` must be writable and aligned.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn md_snapshot_anchor_at(
    snapshot: md_snapshot,
    source_offset: u64,
    buffer: *mut u8,
    capacity: usize,
    out_length: *mut usize,
) -> md_status {
    guard(|| {
        // SAFETY: forwarded caller contract.
        let length = unsafe { output(out_length) }?;
        let entry = self::snapshot(snapshot)?;
        checkpoint();
        let anchor = entry
            .snapshot
            .anchor_at(source_offset)
            .map_err(anchor_error)?;
        write_buffer(&anchor_bytes(&anchor)?, buffer, capacity, length)
    })
}

/// Serialize an anchor for a byte offset within node `node` (a view index).
///
/// # Safety
/// As for [`md_snapshot_anchor_at`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn md_snapshot_anchor_for_node(
    snapshot: md_snapshot,
    node: u32,
    offset: u64,
    buffer: *mut u8,
    capacity: usize,
    out_length: *mut usize,
) -> md_status {
    guard(|| {
        // SAFETY: forwarded caller contract.
        let length = unsafe { output(out_length) }?;
        let entry = self::snapshot(snapshot)?;
        checkpoint();
        let id = nodes(&entry.snapshot)
            .get(node as usize)
            .map(|n| n.id.clone())
            .ok_or(MD_ERR_UNKNOWN_NODE)?;
        let anchor = entry
            .snapshot
            .anchor_for(&id, offset)
            .map_err(anchor_error)?;
        write_buffer(&anchor_bytes(&anchor)?, buffer, capacity, length)
    })
}

fn nodes(snapshot: &Snapshot) -> &[Node] {
    match snapshot.model() {
        SnapshotModel::V1(d) => &d.input().nodes,
        #[cfg(feature = "model-v2")]
        SnapshotModel::V2(d) => &d.input().nodes,
    }
}

/// Resolve serialized anchor bytes (untrusted) against this snapshot.
///
/// # Safety
/// `anchor` must be readable for `anchor_length` bytes; `out_resolution`
/// must point to a writable, aligned `md_anchor_resolution` with
/// `struct_size` set.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn md_snapshot_resolve_anchor(
    snapshot: md_snapshot,
    anchor: *const u8,
    anchor_length: usize,
    out_resolution: *mut md_anchor_resolution,
) -> md_status {
    guard(|| {
        // SAFETY: forwarded caller contract.
        let out = unsafe { versioned_output(out_resolution) }?;
        out.node = MD_NONE;
        // SAFETY: forwarded caller contract.
        let data = unsafe { bytes(anchor, anchor_length) }?;
        let entry = self::snapshot(snapshot)?;
        if data.len() > MD_ANCHOR_MAX_BYTES as usize {
            return Err(MD_ERR_ANCHOR_INVALID);
        }
        let anchor: Anchor = serde_json::from_slice(data).map_err(|_| MD_ERR_ANCHOR_INVALID)?;
        checkpoint();
        let Some(resolved) = entry.snapshot.resolve(&anchor).map_err(anchor_error)? else {
            out.quality = MD_ANCHOR_LOST;
            return Ok(());
        };
        out.node = nodes(&entry.snapshot)
            .iter()
            .position(|n| n.id == resolved.node)
            .and_then(|i| u32::try_from(i).ok())
            .ok_or(MD_ERR_INTERNAL)?;
        out.quality = match resolved.quality {
            AnchorMatch::Exact => MD_ANCHOR_EXACT,
            AnchorMatch::Relocated => MD_ANCHOR_RELOCATED,
            AnchorMatch::Nearest => MD_ANCHOR_NEAREST,
        };
        out.offset = resolved.offset;
        out.source_offset = resolved.source_offset;
        Ok(())
    })
}

/// Release a snapshot handle; its view memory is freed when the last handle
/// to the same encoding is gone. Other handles and sessions are unaffected.
#[unsafe(no_mangle)]
pub extern "C" fn md_snapshot_release(snapshot: md_snapshot) -> md_status {
    guard(|| {
        if snapshot == 0 {
            return Ok(());
        }
        checkpoint();
        let removed = registry().snapshots.remove(&snapshot);
        removed.map(drop).ok_or(MD_ERR_INVALID_HANDLE)
    })
}
