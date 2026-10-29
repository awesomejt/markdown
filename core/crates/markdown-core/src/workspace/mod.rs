//! Capability-fed workspace search primitives.
//!
//! The platform adapter owns folder access, enumeration, file reads, watching
//! and scheduling. It feeds the core [`WorkspaceEntry`] values (an opaque ID
//! plus a validated relative display path) and, for content search, the bytes
//! it read under its own permissions. The core never receives or resolves a
//! platform path and performs no I/O, threads or clocks. See
//! `specs/workspace-search/README.md` for the contract and budgets.
use crate::Cancelled;
use crate::search::fold_case;

mod content;
mod names;
mod path;
mod snippet;

pub use content::{
    Admission, ContentHit, ContentLimits, ContentOptions, ContentProgress, ContentQuery,
    DEFAULT_MAX_FILE_BYTES, DEFAULT_MAX_FILES, DEFAULT_MAX_MATCHES_PER_FILE,
    DEFAULT_MAX_TOTAL_BYTES, DEFAULT_MAX_TOTAL_MATCHES, DEFAULT_SNIPPET_CONTEXT, DocumentHits,
    DocumentOutcome, MAX_FILE_BYTES, MAX_FILES, MAX_MATCHES_PER_FILE, MAX_SNIPPET_CONTEXT,
    MAX_TOTAL_BYTES, MAX_TOTAL_MATCHES, SkipCounts, SkipReason, StopReason,
};
pub use names::{
    DEFAULT_MAX_NAME_ENTRIES, DEFAULT_MAX_NAME_RESULTS, MAX_NAME_ENTRIES, MAX_NAME_RESULTS,
    NameHit, NameRank, NameResults, NameSearchOptions, find_names, find_names_cancellable,
};
pub use path::{
    EntryFilter, EntryKind, MARKDOWN_EXTENSIONS, MAX_PATH_BYTES, MAX_PATH_DEPTH, PathError,
    WorkspaceEntry, WorkspacePath, classify_name,
};
pub use snippet::{MAX_SNIPPET_MATCH_CHARS, Snippet};

/// Typed failures. None carries a path, document contents or query text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkspaceError {
    /// Nothing visible remains after trimming the query.
    EmptyQuery,
    /// The raw query exceeds [`crate::search::MAX_QUERY_BYTES`].
    QueryTooLong,
    /// An option or limit is outside its documented range.
    InvalidOptions,
    /// The cancellation token was observed; no partial result is returned.
    Cancelled,
}
impl std::fmt::Display for WorkspaceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::EmptyQuery => "empty_query",
            Self::QueryTooLong => "query_too_long",
            Self::InvalidOptions => "invalid_options",
            Self::Cancelled => "cancelled",
        })
    }
}
impl std::error::Error for WorkspaceError {}
impl From<Cancelled> for WorkspaceError {
    fn from(_: Cancelled) -> Self {
        Self::Cancelled
    }
}

/// Name queries are trimmed and folded but otherwise literal: file names may
/// legitimately contain repeated spaces or format characters.
fn normalize_name_query(query: &str, case_sensitive: bool) -> Vec<u32> {
    query
        .trim()
        .chars()
        .map(|c| if case_sensitive { c } else { fold_case(c) } as u32)
        .collect()
}
