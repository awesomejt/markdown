//! File-name search over adapter-supplied entries.
use super::path::{EntryFilter, WorkspaceEntry, filter};
use super::{WorkspaceError, normalize_name_query};
use crate::CancellationToken;
use crate::search::{MAX_QUERY_BYTES, fold_case, kmp_table};
use std::collections::BinaryHeap;

pub const DEFAULT_MAX_NAME_RESULTS: usize = 200;
/// Largest accepted `NameSearchOptions::max_results`.
pub const MAX_NAME_RESULTS: usize = 10_000;
pub const DEFAULT_MAX_NAME_ENTRIES: usize = 100_000;
/// Largest accepted `NameSearchOptions::max_entries`.
pub const MAX_NAME_ENTRIES: usize = 1_000_000;
/// Entries examined between cancellation polls.
const CANCEL_CHECK_ENTRIES: usize = 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NameSearchOptions {
    /// Exact scalar comparison when true; otherwise simple case folding.
    pub case_sensitive: bool,
    /// Consider dot-file paths (FR-1.4); default false.
    pub include_hidden: bool,
    /// Consider only Markdown names; default true.
    pub markdown_only: bool,
    /// Result budget, 1..=[`MAX_NAME_RESULTS`]; more hits set `truncated`.
    pub max_results: usize,
    /// Entries examined, 1..=[`MAX_NAME_ENTRIES`]; later entries are ignored
    /// and set `entries_limited`.
    pub max_entries: usize,
}
impl Default for NameSearchOptions {
    fn default() -> Self {
        Self {
            case_sensitive: false,
            include_hidden: false,
            markdown_only: true,
            max_results: DEFAULT_MAX_NAME_RESULTS,
            max_entries: DEFAULT_MAX_NAME_ENTRIES,
        }
    }
}

/// How well an entry matched; results sort by rank first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum NameRank {
    /// The file name starts with the query.
    NamePrefix,
    /// The query occurs inside the file name.
    Name,
    /// The query occurs only in the folder part or across a `/`.
    Path,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NameHit {
    /// Index into the supplied entry slice.
    pub entry: usize,
    /// The entry's adapter ID.
    pub id: u64,
    pub rank: NameRank,
    /// Byte range of the first match in `path.as_str()` (UTF-8 boundaries).
    pub start: u64,
    pub end: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NameResults {
    /// Best hits by (rank, path bytes, entry index), at most `max_results`.
    pub hits: Vec<NameHit>,
    /// All matching entries examined, including those beyond `max_results`.
    pub total_hits: u64,
    /// More hits exist than `hits` holds.
    pub truncated: bool,
    /// Entries examined (before filters), at most `max_entries`.
    pub entries_examined: u64,
    pub skipped_hidden: u64,
    pub skipped_not_markdown: u64,
    /// The slice had more than `max_entries` entries; the rest were ignored.
    pub entries_limited: bool,
}

/// [`find_names_cancellable`] without cancellation.
pub fn find_names(
    entries: &[WorkspaceEntry],
    query: &str,
    options: &NameSearchOptions,
) -> Result<NameResults, WorkspaceError> {
    find_names_cancellable(entries, query, options, &CancellationToken::new())
}

/// Literal search of each entry's display path. The query is trimmed; it may
/// contain `/` to match across folders. Work is O(total path bytes) plus
/// O(hits × log max_results); memory is O(max_results + query). Cancellation
/// returns `Err(Cancelled)` and no partial results.
pub fn find_names_cancellable(
    entries: &[WorkspaceEntry],
    query: &str,
    options: &NameSearchOptions,
    token: &CancellationToken,
) -> Result<NameResults, WorkspaceError> {
    token.check()?;
    if query.len() > MAX_QUERY_BYTES {
        return Err(WorkspaceError::QueryTooLong);
    }
    if !(1..=MAX_NAME_RESULTS).contains(&options.max_results)
        || !(1..=MAX_NAME_ENTRIES).contains(&options.max_entries)
    {
        return Err(WorkspaceError::InvalidOptions);
    }
    let pattern = normalize_name_query(query, options.case_sensitive);
    if pattern.is_empty() {
        return Err(WorkspaceError::EmptyQuery);
    }
    let failure = kmp_table(&pattern);
    let mut results = NameResults {
        entries_limited: entries.len() > options.max_entries,
        ..NameResults::default()
    };
    // Max-heap of the best `max_results` hits; the worst is evicted first.
    let mut best: BinaryHeap<(NameRank, &str, usize, u64, u64)> =
        BinaryHeap::with_capacity(options.max_results + 1);
    let mut keys: Vec<(u32, usize)> = Vec::new();
    for (index, entry) in entries.iter().take(options.max_entries).enumerate() {
        if index % CANCEL_CHECK_ENTRIES == 0 {
            token.check()?;
        }
        results.entries_examined += 1;
        match filter(&entry.path, options.include_hidden, options.markdown_only) {
            Some(EntryFilter::Hidden) => {
                results.skipped_hidden += 1;
                continue;
            }
            Some(EntryFilter::NotMarkdown) => {
                results.skipped_not_markdown += 1;
                continue;
            }
            None => {}
        }
        let path = entry.path.as_str();
        keys.clear();
        keys.extend(path.char_indices().map(|(i, c)| {
            let key = if options.case_sensitive {
                c
            } else {
                fold_case(c)
            };
            (key as u32, i)
        }));
        let name_from = keys.partition_point(|&(_, i)| i < entry.path.name_start());
        let hit = match first_match(&keys[name_from..], &pattern, &failure) {
            Some(0) => Some((NameRank::NamePrefix, name_from)),
            Some(at) => Some((NameRank::Name, name_from + at)),
            None => first_match(&keys, &pattern, &failure).map(|at| (NameRank::Path, at)),
        };
        let Some((rank, at)) = hit else { continue };
        results.total_hits += 1;
        let start = keys[at].1 as u64;
        let end = keys.get(at + pattern.len()).map_or(path.len(), |k| k.1) as u64;
        best.push((rank, path, index, start, end));
        if best.len() > options.max_results {
            best.pop();
        }
    }
    results.truncated = results.total_hits > best.len() as u64;
    results.hits = best
        .into_sorted_vec()
        .into_iter()
        .map(|(rank, _, entry, start, end)| NameHit {
            entry,
            id: entries[entry].id,
            rank,
            start,
            end,
        })
        .collect();
    Ok(results)
}

/// Index of the first KMP match of `pattern` in `keys`.
fn first_match(keys: &[(u32, usize)], pattern: &[u32], failure: &[usize]) -> Option<usize> {
    let mut k = 0;
    for (i, &(key, _)) in keys.iter().enumerate() {
        while k > 0 && key != pattern[k] {
            k = failure[k - 1];
        }
        if key == pattern[k] {
            k += 1;
        }
        if k == pattern.len() {
            return Some(i + 1 - k);
        }
    }
    None
}
