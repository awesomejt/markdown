//! Owned, versioned semantic document values for native adapters.
//!
//! Basic Markdown parsing and validation perform no file access, UI or FFI calls.
//! Use [`Document::new`] or [`Document::from_json`] to obtain validated snapshots.
#![forbid(unsafe_code)]

mod metadata;
mod model;
mod parse;
#[cfg(feature = "model-v2")]
pub mod v2;
mod validate;
pub use model::*;
#[cfg(feature = "model-v2")]
pub use parse::parse_markdown_v2;
pub use parse::{ParseError, ParseLimits, parse_markdown};
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
