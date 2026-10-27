//! Owned, versioned semantic document values for native adapters.
//!
//! Basic Markdown parsing and validation perform no file access, UI or FFI calls.
//! Use [`Document::new`] or [`Document::from_json`] to obtain validated snapshots.
#![forbid(unsafe_code)]

mod model;
mod parse;
mod validate;
pub use model::*;
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
