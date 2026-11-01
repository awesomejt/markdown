//! UI-free Rust adapter over the core's hand-written C ABI v1 (LNX-001B, D-009).
//!
//! The Linux host links the staged model-v2 static package (see build.rs and
//! `tools/package_core.py`), never the core's Rust crates, so it depends only on
//! the public C facade. Sessions request model v2 (D-025); results are owned
//! copies, so no handle, pointer or borrowed core memory reaches the UI.

pub mod document;
pub mod ffi;
pub mod runtime;
pub mod session;

pub use document::CoreDocument;
pub use runtime::{
    CoreError, HandleCounts, HostLimits, PACKAGE_CONFIGURATION, PACKAGE_FINGERPRINT, live_handles,
    pending_parses, validate,
};
pub use session::{
    CoreAvailability, CoreOutcome, CoreParseFailure, CoreResult, CoreSession, CoreTimings,
    CoreUnavailability, ParseRequest,
};
