//! Per-document content search with caller-held workspace budgets.
use super::WorkspaceError;
use super::path::{EntryFilter, WorkspaceEntry, filter};
use super::snippet::{Snippet, snippets};
use crate::search::{
    FindOptions, MAX_QUERY_BYTES, SearchDocument, SearchError, Walker, find_cancellable,
    normalize_query,
};
use crate::session::{Anchor, Snapshot, SnapshotModel};
use crate::*;
use std::sync::Arc;

pub const DEFAULT_MAX_FILES: u64 = 10_000;
pub const MAX_FILES: u64 = 1_000_000;
pub const DEFAULT_MAX_TOTAL_BYTES: u64 = 256 * 1024 * 1024;
pub const MAX_TOTAL_BYTES: u64 = 16 * 1024 * 1024 * 1024;
/// Default and largest per-file size: the reader's own parse ceiling.
pub const MAX_FILE_BYTES: usize = 8 * 1024 * 1024;
pub const DEFAULT_MAX_MATCHES_PER_FILE: usize = 100;
pub const MAX_MATCHES_PER_FILE: usize = 10_000;
pub const DEFAULT_MAX_TOTAL_MATCHES: u64 = 2_000;
pub const MAX_TOTAL_MATCHES: u64 = 100_000;
pub const DEFAULT_SNIPPET_CONTEXT: usize = 40;
/// Largest accepted `ContentLimits::snippet_context` (characters per side).
pub const MAX_SNIPPET_CONTEXT: usize = 200;

/// Workspace-wide resource budgets. Every field has a documented range;
/// [`ContentQuery::new`] rejects values outside it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ContentLimits {
    /// Files read and searched, 1..=[`MAX_FILES`].
    pub max_files: u64,
    /// Bytes read and searched, 1..=[`MAX_TOTAL_BYTES`].
    pub max_total_bytes: u64,
    /// Larger files are skipped, 1..=[`MAX_FILE_BYTES`].
    pub max_file_bytes: usize,
    /// Matches kept per file, 1..=[`MAX_MATCHES_PER_FILE`].
    pub max_matches_per_file: usize,
    /// Matches kept in total, 1..=[`MAX_TOTAL_MATCHES`].
    pub max_total_matches: u64,
    /// Visible characters on each side of a match, 0..=[`MAX_SNIPPET_CONTEXT`].
    pub snippet_context: usize,
}
impl Default for ContentLimits {
    fn default() -> Self {
        Self {
            max_files: DEFAULT_MAX_FILES,
            max_total_bytes: DEFAULT_MAX_TOTAL_BYTES,
            max_file_bytes: MAX_FILE_BYTES,
            max_matches_per_file: DEFAULT_MAX_MATCHES_PER_FILE,
            max_total_matches: DEFAULT_MAX_TOTAL_MATCHES,
            snippet_context: DEFAULT_SNIPPET_CONTEXT,
        }
    }
}
impl ContentLimits {
    fn valid(&self) -> bool {
        (1..=MAX_FILES).contains(&self.max_files)
            && (1..=MAX_TOTAL_BYTES).contains(&self.max_total_bytes)
            && (1..=MAX_FILE_BYTES).contains(&self.max_file_bytes)
            && (1..=MAX_MATCHES_PER_FILE).contains(&self.max_matches_per_file)
            && (1..=MAX_TOTAL_MATCHES).contains(&self.max_total_matches)
            && self.snippet_context <= MAX_SNIPPET_CONTEXT
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ContentOptions {
    /// Same meaning as [`FindOptions`].
    pub case_sensitive: bool,
    pub whole_word: bool,
    /// Search dot-file paths (FR-1.4); default false.
    pub include_hidden: bool,
    pub limits: ContentLimits,
}

/// Why one file was not searched. The search continues.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SkipReason {
    Hidden,
    NotMarkdown,
    /// Larger than `max_file_bytes`.
    TooLarge,
    InvalidUtf8,
    /// The bounded parser rejected it (nesting, node or event limits).
    ParseFailed,
}

/// Which workspace budget ended the search. Once set it is permanent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StopReason {
    Files,
    Bytes,
    Matches,
}

/// Per-reason skip counts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SkipCounts {
    pub hidden: u64,
    pub not_markdown: u64,
    pub too_large: u64,
    pub invalid_utf8: u64,
    pub parse_failed: u64,
}

/// Caller-held accounting for one workspace search. Start from `default()`
/// and pass the same value to every call; the core updates it only when a
/// call returns `Ok`, so a cancelled call leaves it unchanged.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ContentProgress {
    /// Files admitted and read, including those then skipped as
    /// `InvalidUtf8` or `ParseFailed`; the `max_files` budget counts these.
    pub files_read: u64,
    pub files_matched: u64,
    /// Bytes of those files; the `max_total_bytes` budget counts these.
    pub bytes_read: u64,
    pub matches: u64,
    /// Files whose hits were cut by the per-file or total match budget.
    pub files_truncated: u64,
    pub skipped: SkipCounts,
    /// Set when a budget is exhausted; every later call returns `Stop`.
    pub stopped: Option<StopReason>,
}
impl ContentProgress {
    fn skip(&mut self, reason: SkipReason) {
        let count = match reason {
            SkipReason::Hidden => &mut self.skipped.hidden,
            SkipReason::NotMarkdown => &mut self.skipped.not_markdown,
            SkipReason::TooLarge => &mut self.skipped.too_large,
            SkipReason::InvalidUtf8 => &mut self.skipped.invalid_utf8,
            SkipReason::ParseFailed => &mut self.skipped.parse_failed,
        };
        // Saturating: progress is caller data (the C ABI passes it back in),
        // and these counts are not bounded by a budget check.
        *count = count.saturating_add(1);
    }
}

/// Pre-read decision for one entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Admission {
    /// Read the file and pass its bytes to [`ContentQuery::search`].
    Read,
    /// Do not read this file; continue with the next.
    Skip(SkipReason),
    /// Stop reading files.
    Stop(StopReason),
}

/// One match in one file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContentHit {
    /// Leaf block node index in this parse (snapshot-local, D-021).
    pub unit: usize,
    /// Source bytes covering the match, as [`crate::search::FindMatch`].
    pub source: SourceSpan,
    pub exact: bool,
    /// 1-based line of `source.start`, counting LF line endings.
    pub line: u64,
    pub snippet: Snippet,
    /// Position of `source.start`, resolvable against a later snapshot of
    /// the same file when its bytes have changed. Every block that can hold a
    /// match is anchorable, so this is `None` only if that invariant breaks.
    pub anchor: Option<Anchor>,
}

/// Hits for one searched file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocumentHits {
    pub id: u64,
    /// Length and SHA-256 of the searched bytes; equal to a later snapshot's
    /// `source_info()` when the file is unchanged.
    pub source: SourceInfo,
    /// In reading order.
    pub hits: Vec<ContentHit>,
    /// More matches exist in this file than `hits` holds.
    pub truncated: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DocumentOutcome {
    /// Searched; `hits` may be empty.
    Searched(DocumentHits),
    Skipped(SkipReason),
    Stopped(StopReason),
}

/// A validated content query with its options. Immutable and `Sync`; one
/// query serves a whole workspace search through a [`ContentProgress`].
#[derive(Clone, Debug)]
pub struct ContentQuery {
    query: String,
    options: ContentOptions,
}
impl ContentQuery {
    /// Fails with `QueryTooLong`, `InvalidOptions` or `EmptyQuery` (nothing
    /// visible remains after trimming).
    pub fn new(query: &str, options: ContentOptions) -> Result<Self, WorkspaceError> {
        if query.len() > MAX_QUERY_BYTES {
            return Err(WorkspaceError::QueryTooLong);
        }
        if !options.limits.valid() {
            return Err(WorkspaceError::InvalidOptions);
        }
        if normalize_query(query, true).is_empty() {
            return Err(WorkspaceError::EmptyQuery);
        }
        Ok(Self {
            query: query.to_owned(),
            options,
        })
    }
    pub fn options(&self) -> &ContentOptions {
        &self.options
    }

    /// Decide before reading. `byte_len` is the adapter's size estimate.
    /// Records skips and a stop in `progress`; `Read` changes nothing.
    pub fn admit(
        &self,
        progress: &mut ContentProgress,
        entry: &WorkspaceEntry,
        byte_len: u64,
    ) -> Admission {
        if let Some(reason) = progress.stopped {
            return Admission::Stop(reason);
        }
        let limits = &self.options.limits;
        let skip = match filter(&entry.path, self.options.include_hidden, true) {
            Some(EntryFilter::Hidden) => Some(SkipReason::Hidden),
            Some(EntryFilter::NotMarkdown) => Some(SkipReason::NotMarkdown),
            None if byte_len > limits.max_file_bytes as u64 => Some(SkipReason::TooLarge),
            None => None,
        };
        if let Some(reason) = skip {
            progress.skip(reason);
            return Admission::Skip(reason);
        }
        let stop = if progress.files_read >= limits.max_files {
            Some(StopReason::Files)
        } else if progress.matches >= limits.max_total_matches {
            Some(StopReason::Matches)
        } else if progress.bytes_read.saturating_add(byte_len) > limits.max_total_bytes {
            Some(StopReason::Bytes)
        } else {
            None
        };
        match stop {
            Some(reason) => {
                progress.stopped = Some(reason);
                Admission::Stop(reason)
            }
            None => Admission::Read,
        }
    }

    /// Search one file's bytes. Re-checks admission with the real length, so
    /// calling [`admit`](Self::admit) first is an optimization, not a
    /// requirement. Only cancellation is an error; it returns no partial
    /// result and leaves `progress` unchanged.
    pub fn search(
        &self,
        progress: &mut ContentProgress,
        entry: &WorkspaceEntry,
        bytes: &[u8],
        token: &CancellationToken,
    ) -> Result<DocumentOutcome, WorkspaceError> {
        token.check()?;
        let mut next = *progress;
        match self.admit(&mut next, entry, bytes.len() as u64) {
            Admission::Read => {}
            Admission::Skip(reason) => {
                *progress = next;
                return Ok(DocumentOutcome::Skipped(reason));
            }
            Admission::Stop(reason) => {
                *progress = next;
                return Ok(DocumentOutcome::Stopped(reason));
            }
        }
        let outcome = self.search_admitted(&mut next, entry, bytes, token)?;
        *progress = next;
        Ok(outcome)
    }

    fn search_admitted(
        &self,
        progress: &mut ContentProgress,
        entry: &WorkspaceEntry,
        bytes: &[u8],
        token: &CancellationToken,
    ) -> Result<DocumentOutcome, WorkspaceError> {
        // Every admitted read is charged, so post-read skips cannot evade the
        // file and byte budgets (PR #10 review).
        progress.files_read += 1;
        progress.bytes_read += bytes.len() as u64;
        let Ok(text) = std::str::from_utf8(bytes) else {
            progress.skip(SkipReason::InvalidUtf8);
            return Ok(DocumentOutcome::Skipped(SkipReason::InvalidUtf8));
        };
        let limits = &self.options.limits;
        let mut parse_limits = ParseLimits::default();
        parse_limits.model.max_source_bytes = limits.max_file_bytes;
        let source: Arc<str> = Arc::from(text);
        let document = match parse_markdown_cancellable(
            &source,
            DocumentId("workspace".into()),
            RevisionId("g1".into()),
            parse_limits,
            token,
        ) {
            Ok(document) => document,
            Err(CancellableParseError::Cancelled) => return Err(WorkspaceError::Cancelled),
            Err(CancellableParseError::Parse(_)) => {
                progress.skip(SkipReason::ParseFailed);
                return Ok(DocumentOutcome::Skipped(SkipReason::ParseFailed));
            }
        };
        let snapshot = Snapshot::detached(Arc::clone(&source), SnapshotModel::V1(document));
        let view = SearchDocument::from_snapshot(&snapshot);
        let remaining = limits.max_total_matches - progress.matches;
        let per_file = limits.max_matches_per_file as u64;
        let find_options = FindOptions {
            case_sensitive: self.options.case_sensitive,
            whole_word: self.options.whole_word,
            max_matches: per_file.min(remaining) as usize,
        };
        let found = find_cancellable(&view, &self.query, &find_options, token).map_err(|e| {
            match e {
                SearchError::Cancelled => WorkspaceError::Cancelled,
                // The query and options were validated in `new`.
                _ => WorkspaceError::InvalidOptions,
            }
        })?;
        let walker = Walker::new(view);
        let snippets = snippets(&walker, &found.matches, limits.snippet_context, token)?;
        let mut lines = LineCounter::new(&source);
        let mut hits = Vec::with_capacity(found.matches.len());
        for (m, snippet) in found.matches.into_iter().zip(snippets) {
            let anchor = snapshot.anchor_at(m.source.start).ok();
            hits.push(ContentHit {
                unit: m.unit,
                line: lines.line_of(m.source.start as usize),
                source: m.source,
                exact: m.exact,
                snippet,
                anchor,
            });
        }
        progress.matches += hits.len() as u64;
        if !hits.is_empty() {
            progress.files_matched = progress.files_matched.saturating_add(1);
        }
        if found.truncated {
            progress.files_truncated = progress.files_truncated.saturating_add(1);
        }
        if progress.matches >= limits.max_total_matches {
            progress.stopped = Some(StopReason::Matches);
        }
        Ok(DocumentOutcome::Searched(DocumentHits {
            id: entry.id,
            source: snapshot.source_info().clone(),
            hits,
            truncated: found.truncated,
        }))
    }
}

/// Incremental 1-based line numbers for mostly increasing offsets.
struct LineCounter<'a> {
    source: &'a [u8],
    offset: usize,
    line: u64,
}
impl<'a> LineCounter<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            source: source.as_bytes(),
            offset: 0,
            line: 1,
        }
    }
    fn line_of(&mut self, offset: usize) -> u64 {
        let offset = offset.min(self.source.len());
        if offset < self.offset {
            (self.offset, self.line) = (0, 1);
        }
        let newlines = self.source[self.offset..offset]
            .iter()
            .filter(|&&b| b == b'\n')
            .count();
        self.line += newlines as u64;
        self.offset = offset;
        self.line
    }
}
