//! Owned, versioned semantic document values for native adapters.
//!
//! Basic Markdown parsing and validation perform no file access, UI or FFI calls.
//! Use [`Document::new`] or [`Document::from_json`] to obtain validated snapshots.
//! [`session`] adds generations, cancellation, external-change state and anchors.
//! [`search`] adds source-mapped find, word count and section reading time.
//! [`workspace`] adds capability-fed file-name and content search with budgets.
#![forbid(unsafe_code)]

mod autolink;
mod cancel;
mod metadata;
mod model;
mod parse;
pub mod search;
pub mod session;
#[cfg(feature = "model-v2")]
pub mod v2;
mod validate;
pub mod workspace;
pub use cancel::{CancellableParseError, CancellationToken, Cancelled};
pub use model::*;
pub use parse::{
    CANCEL_CHECK_EVENTS, ParseError, ParseLimits, parse_markdown, parse_markdown_cancellable,
};
#[cfg(feature = "model-v2")]
pub use parse::{parse_markdown_v2, parse_markdown_v2_cancellable};
pub use validate::{Document, ModelError, ModelLimits};

/// Generated schema for tooling; no schema dependency is enabled by default.
#[cfg(feature = "schema")]
pub fn document_schema() -> schemars::Schema {
    let mut schema = schemars::schema_for!(DocumentInput);
    schema.insert(
        "$id".into(),
        serde_json::Value::String("urn:markdown:semantic-document:1".into()),
    );
    schema
}
