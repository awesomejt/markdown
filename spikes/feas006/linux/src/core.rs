//! Linux host side of the experimental FEAS-006 C ABI.
//! Unsafe operations stay in this module; only copied, owned values escape it.

use libloading::Library;
use std::fmt;
use std::mem::size_of;
use std::num::NonZeroU64;
use std::path::{Path, PathBuf};
use std::ptr;
use std::sync::{Arc, Mutex};
use std::time::Instant;

const ABI_VERSION: u32 = 1;
const MAX_INPUT: u64 = 16 * 1024 * 1024;
const MAX_OUTPUT: u64 = 64 * 1024 * 1024;
const MAX_BLOCKS: u64 = 1_000_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Status {
    Ok,
    BadArgument,
    VersionMismatch,
    InvalidUtf8,
    InputLimit,
    Cancelled,
    Panic,
    InvalidHandle,
    OutputLimit,
}

impl TryFrom<u32> for Status {
    type Error = CoreError;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        Ok(match value {
            0 => Self::Ok,
            1 => Self::BadArgument,
            2 => Self::VersionMismatch,
            3 => Self::InvalidUtf8,
            4 => Self::InputLimit,
            5 => Self::Cancelled,
            6 => Self::Panic,
            7 => Self::InvalidHandle,
            8 => Self::OutputLimit,
            other => return Err(CoreError::UnknownStatus(other)),
        })
    }
}

#[derive(Debug)]
pub enum CoreError {
    Load(String),
    MissingSymbol(&'static str),
    UnknownStatus(u32),
    Status(Status),
    InvalidSnapshot(&'static str),
    HostConversionFailure,
    Io(String),
}

impl fmt::Display for CoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Load(message) => write!(f, "core load failed: {message}"),
            Self::MissingSymbol(name) => write!(f, "core symbol missing: {name}"),
            Self::UnknownStatus(value) => write!(f, "unknown core status: {value}"),
            Self::Status(status) => write!(f, "core status: {status:?}"),
            Self::InvalidSnapshot(reason) => write!(f, "invalid core snapshot: {reason}"),
            Self::HostConversionFailure => write!(f, "injected host conversion failure"),
            Self::Io(message) => write!(f, "I/O failed: {message}"),
        }
    }
}

impl std::error::Error for CoreError {}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct AbiInfo {
    pub version: u32,
    pub pointer_bytes: u32,
    pub options_bytes: u32,
    pub block_bytes: u32,
    pub snapshot_bytes: u32,
    pub probe_hooks: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ParseOptions {
    pub version: u32,
    pub struct_bytes: u32,
    pub max_input_bytes: u64,
    pub max_output_bytes: u64,
    pub max_blocks: u64,
    pub cancel_handle: u64,
}

impl Default for ParseOptions {
    fn default() -> Self {
        Self {
            version: ABI_VERSION,
            struct_bytes: size_of::<Self>() as u32,
            max_input_bytes: MAX_INPUT,
            max_output_bytes: MAX_OUTPUT,
            max_blocks: MAX_BLOCKS,
            cancel_handle: 0,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct NativeBlock {
    id: u64,
    kind: u32,
    heading_level: u32,
    source_start: u64,
    source_end: u64,
    text_offset: u64,
    text_len: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct NativeSnapshot {
    version: u32,
    struct_bytes: u32,
    handle: u64,
    text: *const u8,
    text_len: u64,
    blocks: *const NativeBlock,
    block_count: u64,
    core_ns: u64,
}

impl Default for NativeSnapshot {
    fn default() -> Self {
        Self {
            version: ABI_VERSION,
            struct_bytes: size_of::<Self>() as u32,
            handle: 0,
            text: ptr::null(),
            text_len: 0,
            blocks: ptr::null(),
            block_count: 0,
            core_ns: 0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct CoreBlock {
    pub id: u64,
    pub kind: u32,
    pub heading_level: u32,
    pub source_start: u64,
    pub source_end: u64,
    pub text: String,
}

#[derive(Clone, Debug)]
pub struct CoreDocument {
    pub blocks: Vec<CoreBlock>,
    pub core_ms: f64,
    pub native_call_ms: f64,
    pub copy_decode_ms: f64,
    pub text_bytes: u64,
    pub table_bytes: u64,
}

type GetAbi = unsafe extern "C" fn(u32, *mut AbiInfo) -> u32;
type Parse = unsafe extern "C" fn(*const u8, u64, *const ParseOptions, *mut NativeSnapshot) -> u32;
type Handle = unsafe extern "C" fn(u64) -> u32;
type CancelCreate = unsafe extern "C" fn(*mut u64) -> u32;
type ProbeLive = unsafe extern "C" fn(*mut u64, *mut u64) -> u32;
type ProbePanic = unsafe extern "C" fn() -> u32;
type ProbeActive = unsafe extern "C" fn() -> u64;

struct Functions {
    get_abi: GetAbi,
    parse: Parse,
    snapshot_free: Handle,
    cancel_create: CancelCreate,
    cancel_request: Handle,
    cancel_free: Handle,
    probe_live: Option<ProbeLive>,
    probe_panic: Option<ProbePanic>,
    probe_active: Option<ProbeActive>,
}

pub struct CoreLibrary {
    // Kept before copied function pointers are ever used and dropped after them.
    _library: Library,
    functions: Functions,
    path: PathBuf,
    abi: AbiInfo,
}

// libloading::Library is Send+Sync on this Unix target. Functions are immutable,
// and the core explicitly supports concurrent calls.

impl CoreLibrary {
    pub fn adjacent_path() -> Result<PathBuf, CoreError> {
        let executable = std::env::current_exe().map_err(|e| CoreError::Io(e.to_string()))?;
        let parent = executable
            .parent()
            .ok_or(CoreError::Io("executable has no parent".into()))?;
        Ok(parent.join("libfeas006_core.so"))
    }

    pub fn load(path: &Path) -> Result<Arc<Self>, CoreError> {
        let canonical = path
            .canonicalize()
            .map_err(|e| CoreError::Load(format!("{}: {e}", path.display())))?;
        // SAFETY: The selected file is held by `CoreLibrary` for longer than all
        // copied symbols and handles. Required symbol signatures match md_core.h.
        let library =
            unsafe { Library::new(&canonical) }.map_err(|e| CoreError::Load(e.to_string()))?;
        unsafe fn required<T: Copy>(
            library: &Library,
            name: &'static [u8],
        ) -> Result<T, CoreError> {
            // SAFETY: Caller supplies the exact function pointer type from md_core.h.
            unsafe { library.get::<T>(name) }
                .map(|symbol| *symbol)
                .map_err(|_| {
                    CoreError::MissingSymbol(
                        std::str::from_utf8(&name[..name.len() - 1]).unwrap_or("symbol"),
                    )
                })
        }
        unsafe fn optional<T: Copy>(library: &Library, name: &'static [u8]) -> Option<T> {
            // SAFETY: Same signature condition as `required`; absence is allowed.
            unsafe { library.get::<T>(name) }.ok().map(|symbol| *symbol)
        }
        // SAFETY: Signatures are copied from the authoritative header.
        let functions = unsafe {
            Functions {
                get_abi: required(&library, b"md_get_abi_v1\0")?,
                parse: required(&library, b"md_parse_v1\0")?,
                snapshot_free: required(&library, b"md_snapshot_free_v1\0")?,
                cancel_create: required(&library, b"md_cancel_create_v1\0")?,
                cancel_request: required(&library, b"md_cancel_request_v1\0")?,
                cancel_free: required(&library, b"md_cancel_free_v1\0")?,
                probe_live: optional(&library, b"md_probe_live_v1\0"),
                probe_panic: optional(&library, b"md_probe_panic_v1\0"),
                probe_active: optional(&library, b"md_probe_active_v1\0"),
            }
        };
        let mut abi = AbiInfo::default();
        // SAFETY: `abi` is writable and aligned for the call duration.
        check_ok(unsafe { (functions.get_abi)(ABI_VERSION, &mut abi) })?;
        if abi.version != ABI_VERSION
            || abi.pointer_bytes != size_of::<usize>() as u32
            || abi.options_bytes != size_of::<ParseOptions>() as u32
            || abi.block_bytes != size_of::<NativeBlock>() as u32
            || abi.snapshot_bytes != size_of::<NativeSnapshot>() as u32
        {
            return Err(CoreError::InvalidSnapshot(
                "ABI layout negotiation mismatch",
            ));
        }
        let probes_complete = functions.probe_live.is_some()
            && functions.probe_panic.is_some()
            && functions.probe_active.is_some();
        if (abi.probe_hooks == 1) != probes_complete {
            return Err(CoreError::InvalidSnapshot(
                "probe export negotiation mismatch",
            ));
        }
        Ok(Arc::new(Self {
            _library: library,
            functions,
            path: canonical,
            abi,
        }))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn abi(&self) -> AbiInfo {
        self.abi
    }

    pub fn parse_and_copy(
        self: &Arc<Self>,
        input: &[u8],
        cancellation: Option<&Cancellation>,
        inject_host_failure: bool,
    ) -> Result<CoreDocument, CoreError> {
        let mut options = ParseOptions::default();
        if let Some(cancel) = cancellation {
            options.cancel_handle = cancel.handle()?;
        }
        let mut snapshot = NativeSnapshot::default();
        let started = Instant::now();
        // SAFETY: Input remains immutable for the call; options/output are valid,
        // aligned, distinct values. The returned pointers are consumed before free.
        let raw = unsafe {
            (self.functions.parse)(input.as_ptr(), input.len() as u64, &options, &mut snapshot)
        };
        let native_call_ms = started.elapsed().as_secs_f64() * 1000.0;
        check_ok(raw)?;
        let mut owner = SnapshotOwner::new(self.clone(), snapshot.handle)?;
        let converted = (|| {
            validate_snapshot(&snapshot, &options)?;
            let text_len = usize::try_from(snapshot.text_len)
                .map_err(|_| CoreError::InvalidSnapshot("text length overflow"))?;
            let block_count = usize::try_from(snapshot.block_count)
                .map_err(|_| CoreError::InvalidSnapshot("block count overflow"))?;
            let copy_started = Instant::now();
            // SAFETY: Null/length and negotiated bounds were validated; the owner
            // retains the creating library and handle through both bulk copies.
            let payload = if text_len == 0 {
                Vec::new()
            } else {
                // SAFETY: The validated non-null pointer names `text_len` bytes.
                unsafe { std::slice::from_raw_parts(snapshot.text, text_len) }.to_vec()
            };
            // SAFETY: Same ownership and range reasoning as the payload copy.
            let table = if block_count == 0 {
                Vec::new()
            } else {
                // SAFETY: The validated non-null pointer names `block_count` values.
                unsafe { std::slice::from_raw_parts(snapshot.blocks, block_count) }.to_vec()
            };
            if inject_host_failure {
                return Err(CoreError::HostConversionFailure);
            }
            let mut blocks = Vec::with_capacity(table.len());
            for (index, block) in table.into_iter().enumerate() {
                if block.id != index as u64 + 1 || block.kind > 5 || block.heading_level > 6 {
                    return Err(CoreError::InvalidSnapshot("block identity or type"));
                }
                if block.source_start > block.source_end || block.source_end > input.len() as u64 {
                    return Err(CoreError::InvalidSnapshot("source range"));
                }
                let start = usize::try_from(block.text_offset)
                    .map_err(|_| CoreError::InvalidSnapshot("text offset overflow"))?;
                let length = usize::try_from(block.text_len)
                    .map_err(|_| CoreError::InvalidSnapshot("text length overflow"))?;
                let end = start
                    .checked_add(length)
                    .ok_or(CoreError::InvalidSnapshot("text range overflow"))?;
                let bytes = payload
                    .get(start..end)
                    .ok_or(CoreError::InvalidSnapshot("text range"))?;
                let text = std::str::from_utf8(bytes)
                    .map_err(|_| CoreError::InvalidSnapshot("block UTF-8"))?
                    .to_owned();
                blocks.push(CoreBlock {
                    id: block.id,
                    kind: block.kind,
                    heading_level: block.heading_level,
                    source_start: block.source_start,
                    source_end: block.source_end,
                    text,
                });
            }
            Ok(CoreDocument {
                blocks,
                core_ms: snapshot.core_ns as f64 / 1_000_000.0,
                native_call_ms,
                copy_decode_ms: copy_started.elapsed().as_secs_f64() * 1000.0,
                text_bytes: snapshot.text_len,
                table_bytes: snapshot
                    .block_count
                    .checked_mul(size_of::<NativeBlock>() as u64)
                    .ok_or(CoreError::InvalidSnapshot("table size overflow"))?,
            })
        })();
        let release = owner.release();
        release?;
        converted
    }

    pub fn raw_status(
        self: &Arc<Self>,
        input: &[u8],
        options: ParseOptions,
    ) -> Result<Status, CoreError> {
        let mut snapshot = NativeSnapshot::default();
        // SAFETY: Valid borrowed input and control structs for the call duration.
        let raw = unsafe {
            (self.functions.parse)(input.as_ptr(), input.len() as u64, &options, &mut snapshot)
        };
        let status = Status::try_from(raw)?;
        if status == Status::Ok {
            let mut owner = SnapshotOwner::new(self.clone(), snapshot.handle)?;
            owner.release()?;
        }
        Ok(status)
    }

    pub fn null_control_status(&self) -> Result<Status, CoreError> {
        // SAFETY: Null options/output are explicitly rejected before dereference.
        Status::try_from(unsafe {
            (self.functions.parse)(ptr::null(), 0, ptr::null(), ptr::null_mut())
        })
    }

    pub fn cancellation(self: &Arc<Self>) -> Result<Cancellation, CoreError> {
        let mut handle = 0_u64;
        // SAFETY: `handle` is writable and aligned for the call duration.
        check_ok(unsafe { (self.functions.cancel_create)(&mut handle) })?;
        let handle = NonZeroU64::new(handle)
            .ok_or(CoreError::InvalidSnapshot("zero cancellation handle"))?;
        Ok(Cancellation {
            core: self.clone(),
            handle: Mutex::new(Some(handle)),
        })
    }

    pub fn probe_live(&self) -> Result<(u64, u64), CoreError> {
        let function = self
            .functions
            .probe_live
            .ok_or(CoreError::MissingSymbol("md_probe_live_v1"))?;
        let mut snapshots = 0;
        let mut cancellations = 0;
        // SAFETY: Both outputs are writable and aligned.
        check_ok(unsafe { function(&mut snapshots, &mut cancellations) })?;
        Ok((snapshots, cancellations))
    }

    pub fn probe_active(&self) -> Result<u64, CoreError> {
        let function = self
            .functions
            .probe_active
            .ok_or(CoreError::MissingSymbol("md_probe_active_v1"))?;
        // SAFETY: Function has no arguments or borrowed result.
        Ok(unsafe { function() })
    }

    pub fn probe_panic(&self) -> Result<Status, CoreError> {
        let function = self
            .functions
            .probe_panic
            .ok_or(CoreError::MissingSymbol("md_probe_panic_v1"))?;
        // SAFETY: Function has no arguments or borrowed result.
        Status::try_from(unsafe { function() })
    }

    pub fn test_snapshot_free(&self, handle: u64) -> Result<Status, CoreError> {
        // SAFETY: Handles are scalar identities; the core validates them.
        Status::try_from(unsafe { (self.functions.snapshot_free)(handle) })
    }

    pub fn test_double_snapshot_free(&self, input: &[u8]) -> Result<(Status, Status), CoreError> {
        let options = ParseOptions::default();
        let mut snapshot = NativeSnapshot::default();
        // SAFETY: Valid borrowed input and distinct aligned control structs.
        check_ok(unsafe {
            (self.functions.parse)(input.as_ptr(), input.len() as u64, &options, &mut snapshot)
        })?;
        let handle = NonZeroU64::new(snapshot.handle)
            .ok_or(CoreError::InvalidSnapshot("zero test snapshot handle"))?;
        // SAFETY: First call owns the live handle; second deliberately verifies
        // the documented stale-handle rejection without dereferencing memory.
        let first = Status::try_from(unsafe { (self.functions.snapshot_free)(handle.get()) })?;
        let second = Status::try_from(unsafe { (self.functions.snapshot_free)(handle.get()) })?;
        Ok((first, second))
    }

    pub fn test_cancel_request(&self, handle: u64) -> Result<Status, CoreError> {
        // SAFETY: Handles are scalar identities; the core validates them.
        Status::try_from(unsafe { (self.functions.cancel_request)(handle) })
    }
}

fn validate_snapshot(snapshot: &NativeSnapshot, options: &ParseOptions) -> Result<(), CoreError> {
    if snapshot.version != ABI_VERSION
        || snapshot.struct_bytes != size_of::<NativeSnapshot>() as u32
    {
        return Err(CoreError::InvalidSnapshot("snapshot header"));
    }
    if snapshot.handle == 0
        || snapshot.text_len > options.max_output_bytes
        || snapshot.block_count > options.max_blocks
    {
        return Err(CoreError::InvalidSnapshot("snapshot limits or handle"));
    }
    if (snapshot.text_len > 0 && snapshot.text.is_null())
        || (snapshot.block_count > 0 && snapshot.blocks.is_null())
    {
        return Err(CoreError::InvalidSnapshot("null data pointer"));
    }
    Ok(())
}

fn check_ok(raw: u32) -> Result<(), CoreError> {
    match Status::try_from(raw)? {
        Status::Ok => Ok(()),
        status => Err(CoreError::Status(status)),
    }
}

struct SnapshotOwner {
    core: Arc<CoreLibrary>,
    handle: Option<NonZeroU64>,
}

impl SnapshotOwner {
    fn new(core: Arc<CoreLibrary>, handle: u64) -> Result<Self, CoreError> {
        Ok(Self {
            core,
            handle: Some(NonZeroU64::new(handle).ok_or(CoreError::InvalidSnapshot("zero handle"))?),
        })
    }

    fn release(&mut self) -> Result<(), CoreError> {
        if let Some(handle) = self.handle {
            // SAFETY: The live nonzero handle belongs to this same library instance.
            check_ok(unsafe { (self.core.functions.snapshot_free)(handle.get()) })?;
            self.handle = None;
        }
        Ok(())
    }
}

impl Drop for SnapshotOwner {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            // SAFETY: Best-effort same-library fallback; errors cannot be surfaced in Drop.
            let _ = unsafe { (self.core.functions.snapshot_free)(handle.get()) };
        }
    }
}

pub struct Cancellation {
    core: Arc<CoreLibrary>,
    handle: Mutex<Option<NonZeroU64>>,
}

impl Cancellation {
    fn handle(&self) -> Result<u64, CoreError> {
        self.handle
            .lock()
            .map_err(|_| CoreError::InvalidSnapshot("cancellation lock"))?
            .map(NonZeroU64::get)
            .ok_or(CoreError::Status(Status::InvalidHandle))
    }

    pub fn request(&self) -> Result<Status, CoreError> {
        let handle = self.handle()?;
        // SAFETY: The live handle belongs to this same library instance.
        Status::try_from(unsafe { (self.core.functions.cancel_request)(handle) })
    }

    pub fn close(&self) -> Result<(), CoreError> {
        let mut slot = self
            .handle
            .lock()
            .map_err(|_| CoreError::InvalidSnapshot("cancellation lock"))?;
        if let Some(handle) = *slot {
            // SAFETY: Caller closes only after parse completion; handle belongs here.
            check_ok(unsafe { (self.core.functions.cancel_free)(handle.get()) })?;
            *slot = None;
        }
        Ok(())
    }

    pub fn id_for_test(&self) -> Result<u64, CoreError> {
        self.handle()
    }
}

impl Drop for Cancellation {
    fn drop(&mut self) {
        if let Ok(slot) = self.handle.get_mut()
            && let Some(handle) = slot.take()
        {
            // SAFETY: Best-effort same-library fallback; no parse may outlive its Arc.
            let _ = unsafe { (self.core.functions.cancel_free)(handle.get()) };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_decode_rejects_unknown_values() {
        assert!(matches!(
            Status::try_from(99),
            Err(CoreError::UnknownStatus(99))
        ));
    }

    #[test]
    fn host_layouts_match_x86_64_c_layout() {
        assert_eq!(size_of::<ParseOptions>(), 40);
        assert_eq!(size_of::<NativeBlock>(), 48);
        assert_eq!(size_of::<NativeSnapshot>(), 56);
    }
}
