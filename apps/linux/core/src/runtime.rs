use std::cell::Cell;
use std::ffi::CStr;
use std::fmt;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::mpsc::{Sender, channel};
use std::sync::{Mutex, OnceLock};

use crate::ffi::{self, md_status};

/// Configuration and archive fingerprint of the linked core package (build.rs).
pub const PACKAGE_CONFIGURATION: &str = env!("MD_PACKAGE_CONFIGURATION");
pub const PACKAGE_FINGERPRINT: &str = env!("MD_PACKAGE_FINGERPRINT");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoreError {
    /// A C ABI status other than `MD_OK`.
    Status(md_status),
    IncompatibleAbi,
    InvalidSnapshot,
    /// The flat view is larger than the host's copy budget.
    SnapshotBudget,
    Closed,
    /// One parse is running and another is queued.
    Busy,
    Cancelled,
    /// A blocking call made on the core worker, which would deadlock.
    OnWorker,
    /// An adapter invariant failed (a host bug).
    Internal,
    Superseded {
        expected: u64,
        actual: u64,
    },
    QueryTooLong,
    InvalidArgument,
    NoBlocks,
}

impl fmt::Display for CoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            // SAFETY: md_status_name returns a static NUL-terminated ASCII string.
            CoreError::Status(code) => {
                let name = unsafe { CStr::from_ptr(ffi::md_status_name(*code)) };
                f.write_str(&name.to_string_lossy())
            }
            CoreError::IncompatibleAbi => f.write_str("Incompatible core ABI/model/layout"),
            CoreError::InvalidSnapshot => f.write_str("Invalid core snapshot"),
            CoreError::SnapshotBudget => f.write_str("Core snapshot exceeds the host copy budget"),
            CoreError::Closed => f.write_str("Core session closed"),
            CoreError::Busy => f.write_str("Core worker capacity reached"),
            CoreError::Cancelled => f.write_str("Core call cancelled"),
            CoreError::OnWorker => f.write_str("Blocking core call on the core worker"),
            CoreError::Internal => f.write_str("Internal core adapter failure"),
            CoreError::Superseded { expected, actual } => {
                write!(
                    f,
                    "Snapshot superseded: expected {expected}, actual {actual}"
                )
            }
            CoreError::QueryTooLong => f.write_str("Find query exceeds maximum 4096 bytes"),
            CoreError::InvalidArgument => f.write_str("Invalid argument"),
            CoreError::NoBlocks => f.write_str("Document has no blocks to anchor"),
        }
    }
}

impl std::error::Error for CoreError {}

pub(crate) fn checked(status: md_status) -> Result<(), CoreError> {
    match status {
        ffi::MD_OK => Ok(()),
        code => Err(CoreError::Status(code)),
    }
}

/// Per-session limits. Zero core limits select the core defaults.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HostLimits {
    pub max_source_bytes: u64,
    pub max_nodes: u64,
    pub max_events: u64,
    pub max_text_bytes: u64,
    /// Bytes copied out of one snapshot view, checked before allocating.
    pub max_snapshot_bytes: u64,
}

impl HostLimits {
    /// Host policy: the accepted 5 MiB profiles, not the core's 8 MiB default.
    /// The frozen 5 MiB prose profile uses 461,177 nodes and about 932,000 events.
    pub const HOST: HostLimits = HostLimits {
        max_source_bytes: 5 * 1024 * 1024,
        max_nodes: 500_000,
        max_events: 1_000_000,
        max_text_bytes: 16 * 1024 * 1024,
        max_snapshot_bytes: 128 * 1024 * 1024,
    };
}

impl Default for HostLimits {
    fn default() -> Self {
        HostLimits::HOST
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HandleCounts {
    pub sessions: u64,
    pub jobs: u64,
    pub snapshots: u64,
}

impl HandleCounts {
    pub fn total(&self) -> u64 {
        self.sessions + self.jobs + self.snapshots
    }
}

/// Negotiate ABI v1 and require model v2 and identical record layouts.
pub fn validate() -> Result<(), CoreError> {
    use ffi::size_of;
    let mut info = ffi::md_abi_info {
        struct_size: size_of::<ffi::md_abi_info>(),
        ..Default::default()
    };
    // SAFETY: info is a valid, exclusively borrowed md_abi_info of the declared size.
    checked(unsafe { ffi::md_abi_negotiate(ffi::MD_ABI_VERSION, &mut info) })?;
    let sizes = [
        (
            info.session_options_size,
            size_of::<ffi::md_session_options>(),
        ),
        (
            info.session_status_size,
            size_of::<ffi::md_session_status_info>(),
        ),
        (info.job_result_size, size_of::<ffi::md_job_result>()),
        (info.snapshot_view_size, size_of::<ffi::md_snapshot_view>()),
        (info.node_size, size_of::<ffi::md_node>()),
        (info.intent_size, size_of::<ffi::md_intent>()),
        (info.outline_entry_size, size_of::<ffi::md_outline_entry>()),
        (info.diagnostic_size, size_of::<ffi::md_diagnostic>()),
        (info.property_size, size_of::<ffi::md_property>()),
        (
            info.anchor_resolution_size,
            size_of::<ffi::md_anchor_resolution>(),
        ),
    ];
    let compatible = info.abi_version == ffi::MD_ABI_VERSION
        && info.model_versions & (1 << ffi::MD_MODEL_V2) != 0
        && info.pointer_size == size_of::<*const u8>()
        && info.anchor_version == 1
        && info.max_anchor_bytes == ffi::MD_ANCHOR_MAX_BYTES
        && sizes.iter().all(|(library, host)| library == host)
        && size_of::<ffi::md_find_options>() == ffi::MD_FIND_OPTIONS_SIZE
        && size_of::<ffi::md_find_result>() == ffi::MD_FIND_RESULT_SIZE
        && size_of::<ffi::md_find_match>() == ffi::MD_FIND_MATCH_SIZE
        && size_of::<ffi::md_match_segment>() == ffi::MD_MATCH_SEGMENT_SIZE;
    if compatible {
        Ok(())
    } else {
        Err(CoreError::IncompatibleAbi)
    }
}

/// Process-wide live core handles, for leak checks.
pub fn live_handles() -> Result<HandleCounts, CoreError> {
    let mut counts = HandleCounts::default();
    // SAFETY: three distinct, valid u64 outputs.
    checked(unsafe {
        ffi::md_live_handles(
            &mut counts.sessions,
            &mut counts.jobs,
            &mut counts.snapshots,
        )
    })?;
    Ok(counts)
}

type Work = Box<dyn FnOnce() + Send>;

thread_local! {
    static ON_WORKER: Cell<bool> = const { Cell::new(false) };
}

/// All parse, flat-view, copy and destruction work across windows shares one
/// serial worker thread, never the GTK main thread.
pub(crate) fn submit(work: impl FnOnce() + Send + 'static) {
    static WORKER: OnceLock<Mutex<Sender<Work>>> = OnceLock::new();
    let sender = WORKER.get_or_init(|| {
        let (sender, receiver) = channel::<Work>();
        std::thread::Builder::new()
            .name("markdown-core".into())
            .spawn(move || {
                ON_WORKER.with(|flag| flag.set(true));
                for work in receiver {
                    // Work closures report their own failures; a panic must not stop the worker.
                    let _ = catch_unwind(AssertUnwindSafe(work));
                }
            })
            .expect("spawn the core worker thread");
        Mutex::new(sender)
    });
    let sender = sender
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    sender
        .send(Box::new(work))
        .expect("the core worker thread outlives the process");
}

pub(crate) fn on_worker() -> bool {
    ON_WORKER.with(Cell::get)
}

/// Runs `work` on the core worker after everything queued before it. Tests
/// use it to hold the worker, so queued requests are deterministic.
#[doc(hidden)]
pub fn run_on_worker(work: impl FnOnce() + Send + 'static) {
    submit(work);
}

/// One running parse plus at most one queued call **across every session in the
/// process**, not per session; reject rather than retaining an unbounded series
/// of caller byte buffers behind the serial worker. Two windows reparsing at once
/// make a third window's parse return `Busy`, so callers coalesce or retry.
pub(crate) struct Admission {
    count: Mutex<usize>,
}

pub(crate) static ADMISSION: Admission = Admission {
    count: Mutex::new(0),
};

const MAX_ADMITTED: usize = 2;

impl Admission {
    fn lock(&self) -> std::sync::MutexGuard<'_, usize> {
        self.count
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub(crate) fn reserve(&self) -> Result<(), CoreError> {
        let mut count = self.lock();
        if *count >= MAX_ADMITTED {
            return Err(CoreError::Busy);
        }
        *count += 1;
        Ok(())
    }

    pub(crate) fn release(&self) {
        *self.lock() -= 1;
    }
}

/// Admitted parse calls, running and queued.
pub fn pending_parses() -> usize {
    *ADMISSION.lock()
}
