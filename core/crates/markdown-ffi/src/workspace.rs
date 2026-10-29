//! Workspace search over adapter-supplied entries and bytes (CORE-005B2):
//! ABI v1 additions under D-022 (new functions filling caller-owned structs
//! and record arrays; no new handle types). Semantics are
//! `markdown_core::workspace` unchanged; this module validates arguments,
//! maps zero limits to their defaults and flattens results. Content search
//! accounting lives in a caller-owned `md_content_progress`, read on entry
//! and written back only on success.
use crate::abi::*;
use crate::flat::small;
use crate::search::Records;
use crate::*;
use markdown_core::workspace::{
    Admission, ContentLimits, ContentOptions, ContentProgress, ContentQuery, DocumentOutcome,
    EntryKind, MAX_PATH_BYTES, NameRank, NameSearchOptions, PathError, SkipCounts, SkipReason,
    StopReason, WorkspaceEntry, WorkspaceError, WorkspacePath, find_names,
};

fn workspace_error(error: WorkspaceError) -> md_status {
    match error {
        WorkspaceError::EmptyQuery
        | WorkspaceError::QueryTooLong
        | WorkspaceError::InvalidOptions => MD_ERR_INVALID_ARGUMENT,
        // FFI calls are never cancelled.
        WorkspaceError::Cancelled => MD_ERR_INTERNAL,
    }
}

fn path_code(error: PathError) -> u32 {
    match error {
        PathError::Empty => MD_PATH_EMPTY,
        PathError::TooLong => MD_PATH_TOO_LONG,
        PathError::TooDeep => MD_PATH_TOO_DEEP,
        PathError::Absolute => MD_PATH_ABSOLUTE,
        PathError::EmptyComponent => MD_PATH_EMPTY_COMPONENT,
        PathError::DotComponent => MD_PATH_DOT_COMPONENT,
        PathError::Control => MD_PATH_CONTROL,
    }
}

/// Validate one display path from caller bytes. Over-long input is rejected
/// before it is decoded, so the cost per path is bounded.
fn workspace_path(path: &[u8]) -> Result<WorkspacePath, u32> {
    if path.is_empty() {
        return Err(MD_PATH_EMPTY);
    }
    if path.len() > MAX_PATH_BYTES {
        return Err(MD_PATH_TOO_LONG);
    }
    let text = std::str::from_utf8(path).map_err(|_| MD_PATH_INVALID_UTF8)?;
    WorkspacePath::new(text).map_err(path_code)
}

fn flag(set: bool, bit: u32) -> u32 {
    if set { bit } else { 0 }
}

/// Zero selects the documented default.
fn or_default<T: Default + PartialEq>(value: T, default: T) -> T {
    if value == T::default() {
        default
    } else {
        value
    }
}

/// Borrow a caller input array (null only with a zero count).
///
/// # Safety
/// A non-null `ptr` must be aligned and valid for reads of `count`
/// initialized values of `T`, unchanged until the call returns.
unsafe fn records<'a, T>(ptr: *const T, count: usize) -> Result<&'a [T], md_status> {
    if count == 0 {
        return Ok(&[]);
    }
    if ptr.is_null()
        || !ptr.is_aligned()
        || count
            .checked_mul(size_of::<T>())
            .is_none_or(|bytes| bytes > isize::MAX as usize)
    {
        return Err(MD_ERR_INVALID_ARGUMENT);
    }
    // SAFETY: non-null, aligned, size bounded by isize::MAX and readable per
    // the caller contract.
    Ok(unsafe { std::slice::from_raw_parts(ptr, count) })
}

/// A UTF-8 query of at most `MD_WORKSPACE_MAX_QUERY_BYTES`.
///
/// # Safety
/// As for [`bytes`].
unsafe fn query_text<'a>(query: *const u8, length: usize) -> Result<&'a str, md_status> {
    // SAFETY: forwarded caller contract.
    let query = unsafe { bytes(query, length) }?;
    if query.len() > MD_WORKSPACE_MAX_QUERY_BYTES as usize {
        return Err(MD_ERR_INVALID_ARGUMENT);
    }
    std::str::from_utf8(query).map_err(|_| MD_ERR_INVALID_ARGUMENT)
}

/// The single entry of a content call. Its path must be valid: the adapter
/// classifies names first, and a per-file call can report the bad one.
///
/// # Safety
/// As for [`bytes`].
unsafe fn content_entry(path: *const u8, length: usize) -> Result<WorkspaceEntry, md_status> {
    // SAFETY: forwarded caller contract.
    let path = unsafe { bytes(path, length) }?;
    let path = workspace_path(path).map_err(|_| MD_ERR_INVALID_ARGUMENT)?;
    // The core copies the ID into results only; this call returns no ID.
    Ok(WorkspaceEntry { id: 0, path })
}

// ---------------------------------------------------------------------------
// Path classification

/// Validate a display path and classify it with the shared entry filter.
///
/// # Safety
/// `path` must be readable for `path_length` bytes (null only with zero);
/// `out_info` must point to a writable, aligned `md_workspace_path_info`
/// with `struct_size` set. They may not overlap.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn md_workspace_classify_path(
    path: *const u8,
    path_length: usize,
    out_info: *mut md_workspace_path_info,
) -> md_status {
    guard(|| {
        // SAFETY: forwarded caller contract.
        let out = unsafe { Out::versioned(out_info) }?;
        // SAFETY: forwarded caller contract.
        let path = unsafe { bytes(path, path_length) }?;
        checkpoint();
        let mut info = md_workspace_path_info::sized();
        match workspace_path(path) {
            Ok(path) => {
                info.flags = flag(path.kind() == EntryKind::Markdown, MD_PATH_MARKDOWN)
                    | flag(path.is_hidden(), MD_PATH_HIDDEN)
                    | flag(path.has_invisible(), MD_PATH_INVISIBLE);
                info.name_start = small(path.name_start())?;
            }
            Err(code) => info.error = code,
        }
        out.set(info);
        Ok(())
    })
}

// ---------------------------------------------------------------------------
// File-name search

fn name_options(options: &md_name_options) -> Result<NameSearchOptions, md_status> {
    if options.entry_size != size::<md_workspace_entry>()
        || options.hit_size != size::<md_name_hit>()
    {
        return Err(MD_ERR_STRUCT_SIZE);
    }
    if options.flags & !(MD_NAMES_CASE_SENSITIVE | MD_NAMES_INCLUDE_HIDDEN | MD_NAMES_ALL_FILES)
        != 0
        || options.max_results > MD_NAMES_MAX_RESULTS
        || options.max_entries > MD_NAMES_MAX_ENTRIES
    {
        return Err(MD_ERR_INVALID_ARGUMENT);
    }
    Ok(NameSearchOptions {
        case_sensitive: options.flags & MD_NAMES_CASE_SENSITIVE != 0,
        include_hidden: options.flags & MD_NAMES_INCLUDE_HIDDEN != 0,
        markdown_only: options.flags & MD_NAMES_ALL_FILES == 0,
        max_results: or_default(options.max_results, MD_NAMES_DEFAULT_MAX_RESULTS) as usize,
        max_entries: or_default(options.max_entries, MD_NAMES_DEFAULT_MAX_ENTRIES) as usize,
    })
}

fn rank_code(rank: NameRank) -> u32 {
    match rank {
        NameRank::NamePrefix => MD_NAME_RANK_PREFIX,
        NameRank::Name => MD_NAME_RANK_NAME,
        NameRank::Path => MD_NAME_RANK_PATH,
    }
}

/// Literal search of each entry's display path, ranked as the core does.
///
/// Entries reference `paths` by byte range. Only the first `max_entries` are
/// examined; an out-of-range reference among them is
/// `MD_ERR_INVALID_ARGUMENT`, while a path the core rejects is skipped and
/// counted in `skipped_invalid`, so one bad name cannot fail the search.
/// Examined paths totalling more than `MD_NAMES_MAX_PATH_BYTES` are
/// `MD_ERR_LIMIT`, checked before any copy.
///
/// # Safety
/// `paths` and `query` must be readable for their lengths (null only with
/// zero); `entries` must be null with zero count or an aligned, readable
/// array of `entry_count` records; `options` must point to a readable,
/// aligned `md_name_options`; `hits` must be null with zero capacity or a
/// writable, aligned array of `hit_capacity` records; `out_result` must point
/// to a writable, aligned `md_name_result` with `struct_size` set. Outputs
/// may not overlap each other or any input.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn md_workspace_find_names(
    paths: *const u8,
    paths_length: usize,
    entries: *const md_workspace_entry,
    entry_count: usize,
    query: *const u8,
    query_length: usize,
    options: *const md_name_options,
    hits: *mut md_name_hit,
    hit_capacity: usize,
    out_result: *mut md_name_result,
) -> md_status {
    guard(|| {
        // SAFETY: forwarded caller contract.
        let out = unsafe { Out::versioned(out_result) }?;
        // SAFETY: forwarded caller contract.
        let options = name_options(&unsafe { input(options) }?)?;
        // SAFETY: forwarded caller contract.
        let hit_out = unsafe { Records::new(hits, hit_capacity) }?;
        // SAFETY: forwarded caller contract.
        let (paths, entries, query) = unsafe {
            (
                bytes(paths, paths_length)?,
                records(entries, entry_count)?,
                query_text(query, query_length)?,
            )
        };
        checkpoint();
        // Validate the query against no entries first: an empty query is a
        // successful search with no results (as md_snapshot_find).
        match find_names(&[], query, &options) {
            Err(WorkspaceError::EmptyQuery) => return Ok(()),
            result => result.map_err(workspace_error)?,
        };
        let mut result = md_name_result::sized();
        let examined = &entries[..entries.len().min(options.max_entries)];
        // Ranges may alias, so the copies below are bounded by the total
        // length, not by the paths buffer (PR #11 review).
        let total: u64 = examined.iter().map(|e| u64::from(e.path_length)).sum();
        if total > MD_NAMES_MAX_PATH_BYTES {
            return Err(MD_ERR_LIMIT);
        }
        let mut valid = Vec::with_capacity(examined.len());
        let mut index = Vec::with_capacity(examined.len());
        for (i, entry) in examined.iter().enumerate() {
            if entry.reserved != 0 {
                return Err(MD_ERR_INVALID_ARGUMENT);
            }
            let path = usize::try_from(entry.path_offset)
                .ok()
                .and_then(|start| Some(start..start.checked_add(entry.path_length as usize)?))
                .and_then(|range| paths.get(range))
                .ok_or(MD_ERR_INVALID_ARGUMENT)?;
            match workspace_path(path) {
                Ok(path) => {
                    valid.push(WorkspaceEntry { id: entry.id, path });
                    index.push(i);
                }
                Err(_) => result.skipped_invalid += 1,
            }
        }
        let found = find_names(&valid, query, &options).map_err(workspace_error)?;
        let records = found
            .hits
            .iter()
            .map(|hit| {
                Ok(md_name_hit {
                    id: hit.id,
                    entry: small(index[hit.entry])?,
                    rank: rank_code(hit.rank),
                    start: small(hit.start)?,
                    end: small(hit.end)?,
                })
            })
            .collect::<Result<Vec<_>, md_status>>()?;
        result.flags = flag(found.truncated, MD_NAMES_TRUNCATED)
            | flag(entries.len() > examined.len(), MD_NAMES_ENTRIES_LIMITED);
        result.hit_count = records.len() as u64;
        result.total_hits = found.total_hits;
        result.entries_examined = found.entries_examined + result.skipped_invalid;
        result.skipped_hidden = found.skipped_hidden;
        result.skipped_not_markdown = found.skipped_not_markdown;
        if !hit_out.fits(records.len()) {
            out.set(result);
            return Err(MD_ERR_BUFFER_TOO_SMALL);
        }
        hit_out.write(&records);
        out.set(result);
        Ok(())
    })
}

// ---------------------------------------------------------------------------
// Content search

fn content_options(options: &md_content_options) -> Result<ContentOptions, md_status> {
    if options.hit_size != size::<md_content_hit>() {
        return Err(MD_ERR_STRUCT_SIZE);
    }
    if options.flags
        & !(MD_CONTENT_CASE_SENSITIVE | MD_CONTENT_WHOLE_WORD | MD_CONTENT_INCLUDE_HIDDEN)
        != 0
        || options.reserved != 0
    {
        return Err(MD_ERR_INVALID_ARGUMENT);
    }
    // Ranges are checked by `ContentQuery::new`.
    Ok(ContentOptions {
        case_sensitive: options.flags & MD_CONTENT_CASE_SENSITIVE != 0,
        whole_word: options.flags & MD_CONTENT_WHOLE_WORD != 0,
        include_hidden: options.flags & MD_CONTENT_INCLUDE_HIDDEN != 0,
        limits: ContentLimits {
            max_files: or_default(options.max_files, MD_CONTENT_DEFAULT_MAX_FILES),
            max_total_bytes: or_default(
                options.max_total_bytes,
                MD_CONTENT_DEFAULT_MAX_TOTAL_BYTES,
            ),
            max_file_bytes: or_default(options.max_file_bytes, MD_CONTENT_DEFAULT_MAX_FILE_BYTES)
                as usize,
            max_matches_per_file: or_default(
                options.max_matches_per_file,
                MD_CONTENT_DEFAULT_MAX_MATCHES_PER_FILE,
            ) as usize,
            max_total_matches: u64::from(or_default(
                options.max_total_matches,
                MD_CONTENT_DEFAULT_MAX_TOTAL_MATCHES,
            )),
            // Zero context is meaningful, so this one has no zero default.
            snippet_context: options.snippet_context as usize,
        },
    })
}

/// Validate the query and options together, as `ContentQuery::new` does.
/// Unlike name search, an empty content query is an error: a workspace pass
/// would otherwise charge budgets for files it cannot match.
///
/// # Safety
/// As for [`bytes`] and [`input`].
unsafe fn content_query(
    query: *const u8,
    query_length: usize,
    options: *const md_content_options,
) -> Result<ContentQuery, md_status> {
    // SAFETY: forwarded caller contract.
    let options = content_options(&unsafe { input(options) }?)?;
    // SAFETY: forwarded caller contract.
    let query = unsafe { query_text(query, query_length) }?;
    ContentQuery::new(query, options).map_err(workspace_error)
}

fn skip_code(reason: SkipReason) -> u32 {
    match reason {
        SkipReason::Hidden => MD_SKIP_HIDDEN,
        SkipReason::NotMarkdown => MD_SKIP_NOT_MARKDOWN,
        SkipReason::TooLarge => MD_SKIP_TOO_LARGE,
        SkipReason::InvalidUtf8 => MD_SKIP_INVALID_UTF8,
        SkipReason::ParseFailed => MD_SKIP_PARSE_FAILED,
    }
}

fn stop_code(reason: StopReason) -> u32 {
    match reason {
        StopReason::Files => MD_STOP_FILES,
        StopReason::Bytes => MD_STOP_BYTES,
        StopReason::Matches => MD_STOP_MATCHES,
    }
}

fn progress_from(progress: &md_content_progress) -> Result<ContentProgress, md_status> {
    let stopped = match progress.stopped {
        0 => None,
        MD_STOP_FILES => Some(StopReason::Files),
        MD_STOP_BYTES => Some(StopReason::Bytes),
        MD_STOP_MATCHES => Some(StopReason::Matches),
        _ => return Err(MD_ERR_INVALID_ARGUMENT),
    };
    Ok(ContentProgress {
        files_read: progress.files_read,
        files_matched: progress.files_matched,
        bytes_read: progress.bytes_read,
        matches: progress.matches,
        files_truncated: progress.files_truncated,
        skipped: SkipCounts {
            hidden: progress.skipped_hidden,
            not_markdown: progress.skipped_not_markdown,
            too_large: progress.skipped_too_large,
            invalid_utf8: progress.skipped_invalid_utf8,
            parse_failed: progress.skipped_parse_failed,
        },
        stopped,
    })
}

fn progress_record(progress: &ContentProgress) -> md_content_progress {
    md_content_progress {
        struct_size: size::<md_content_progress>(),
        stopped: progress.stopped.map_or(0, stop_code),
        files_read: progress.files_read,
        files_matched: progress.files_matched,
        bytes_read: progress.bytes_read,
        matches: progress.matches,
        files_truncated: progress.files_truncated,
        skipped_hidden: progress.skipped.hidden,
        skipped_not_markdown: progress.skipped.not_markdown,
        skipped_too_large: progress.skipped.too_large,
        skipped_invalid_utf8: progress.skipped.invalid_utf8,
        skipped_parse_failed: progress.skipped.parse_failed,
    }
}

/// Read the caller's in/out progress (exact `struct_size`, known stop code).
/// It is written back only on success, so every failure leaves it unchanged.
///
/// # Safety
/// A non-null, aligned `ptr` must be valid for reads and writes of
/// `md_content_progress`.
unsafe fn progress(
    ptr: *mut md_content_progress,
) -> Result<(Out<md_content_progress>, ContentProgress), md_status> {
    // SAFETY: forwarded caller contract.
    let value = unsafe { input(ptr.cast_const()) }?;
    Ok((Out(ptr), progress_from(&value)?))
}

/// Decide whether to read one file, before reading it. Records skips and a
/// stop in `progress`.
///
/// # Safety
/// `query` and `path` must be readable for their lengths (null only with
/// zero); `options` must point to a readable, aligned `md_content_options`;
/// `progress` must point to a readable and writable, aligned
/// `md_content_progress` with `struct_size` set; `out_outcome` and
/// `out_reason` must be writable, aligned `uint32_t`s. Outputs may not
/// overlap each other or any input.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn md_workspace_content_admit(
    query: *const u8,
    query_length: usize,
    options: *const md_content_options,
    path: *const u8,
    path_length: usize,
    byte_length: u64,
    progress: *mut md_content_progress,
    out_outcome: *mut u32,
    out_reason: *mut u32,
) -> md_status {
    guard(|| {
        // SAFETY: forwarded caller contract.
        let (outcome_out, reason_out) = unsafe { (Out::new(out_outcome)?, Out::new(out_reason)?) };
        // SAFETY: forwarded caller contract.
        let query = unsafe { content_query(query, query_length, options) }?;
        // SAFETY: forwarded caller contract.
        let (progress_out, mut accounting) = unsafe { self::progress(progress) }?;
        // SAFETY: forwarded caller contract.
        let entry = unsafe { content_entry(path, path_length) }?;
        checkpoint();
        let (outcome, reason) = match query.admit(&mut accounting, &entry, byte_length) {
            Admission::Read => (MD_CONTENT_READ, 0),
            Admission::Skip(reason) => (MD_CONTENT_SKIPPED, skip_code(reason)),
            Admission::Stop(reason) => (MD_CONTENT_STOPPED, stop_code(reason)),
        };
        progress_out.set(progress_record(&accounting));
        outcome_out.set(outcome);
        reason_out.set(reason);
        Ok(())
    })
}

/// Search one file's bytes, re-checking admission with their real length.
///
/// Hits go into `hits`; each hit's snippet text is
/// `text[text_offset .. text_offset + text_length)` and its serialized anchor
/// is `anchors[anchor_offset .. anchor_offset + anchor_length)`. If any of
/// the three arrays is too small, the call returns `MD_ERR_BUFFER_TOO_SMALL`
/// with the required counts in `out_result`, writes nothing else and leaves
/// `progress` unchanged, so the retry is identical.
///
/// # Safety
/// `query`, `path` and `content` must be readable for their lengths (null
/// only with zero); `options` and `progress` as for
/// [`md_workspace_content_admit`]; `hits`, `text` and `anchors` must be null
/// with zero capacity or writable, aligned arrays of their capacities;
/// `out_result` must point to a writable, aligned `md_content_result` with
/// `struct_size` set. Outputs may not overlap each other or any input.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn md_workspace_content_search(
    query: *const u8,
    query_length: usize,
    options: *const md_content_options,
    path: *const u8,
    path_length: usize,
    content: *const u8,
    content_length: usize,
    progress: *mut md_content_progress,
    hits: *mut md_content_hit,
    hit_capacity: usize,
    text: *mut u8,
    text_capacity: usize,
    anchors: *mut u8,
    anchor_capacity: usize,
    out_result: *mut md_content_result,
) -> md_status {
    guard(|| {
        // SAFETY: forwarded caller contract.
        let out = unsafe { Out::versioned(out_result) }?;
        // SAFETY: forwarded caller contract.
        let query = unsafe { content_query(query, query_length, options) }?;
        // SAFETY: forwarded caller contract.
        let (progress_out, mut accounting) = unsafe { self::progress(progress) }?;
        // SAFETY: forwarded caller contract.
        let (hit_out, text_out, anchor_out) = unsafe {
            (
                Records::new(hits, hit_capacity)?,
                Records::new(text, text_capacity)?,
                Records::new(anchors, anchor_capacity)?,
            )
        };
        // SAFETY: forwarded caller contract.
        let (entry, content) = unsafe {
            (
                content_entry(path, path_length)?,
                bytes(content, content_length)?,
            )
        };
        checkpoint();
        let outcome = query
            .search(&mut accounting, &entry, content, &CancellationToken::new())
            .map_err(workspace_error)?;
        let mut result = md_content_result::sized();
        match outcome {
            DocumentOutcome::Skipped(reason) => {
                result.outcome = MD_CONTENT_SKIPPED;
                result.reason = skip_code(reason);
            }
            DocumentOutcome::Stopped(reason) => {
                result.outcome = MD_CONTENT_STOPPED;
                result.reason = stop_code(reason);
            }
            DocumentOutcome::Searched(document) => {
                result.outcome = MD_CONTENT_SEARCHED;
                result.flags = flag(document.truncated, MD_CONTENT_TRUNCATED);
                result.source_length = document.source.bytes;
                result.source_sha256 = document
                    .source
                    .sha256
                    .as_bytes()
                    .try_into()
                    .map_err(|_| MD_ERR_INTERNAL)?;
                // Hits are bounded by max_matches_per_file, so building the
                // records and the anchor bytes before writing is bounded too.
                let mut records = Vec::with_capacity(document.hits.len());
                let mut anchor_bytes = Vec::new();
                let mut text_length = 0usize;
                for hit in &document.hits {
                    let anchor = hit.anchor.as_ref().map(crate::anchor_bytes).transpose()?;
                    let snippet = &hit.snippet;
                    records.push(md_content_hit {
                        unit: small(hit.unit)?,
                        flags: flag(hit.exact, MD_HIT_EXACT)
                            | flag(snippet.leading_more, MD_HIT_LEADING_MORE)
                            | flag(snippet.trailing_more, MD_HIT_TRAILING_MORE)
                            | flag(anchor.is_some(), MD_HIT_ANCHOR),
                        span_start: small(hit.source.start)?,
                        span_end: small(hit.source.end)?,
                        line: hit.line,
                        text_offset: small(text_length)?,
                        text_length: small(snippet.text.len())?,
                        match_start: small(snippet.match_start)?,
                        match_end: small(snippet.match_end)?,
                        anchor_offset: small(anchor_bytes.len())?,
                        anchor_length: small(anchor.as_ref().map_or(0, Vec::len))?,
                    });
                    text_length += snippet.text.len();
                    anchor_bytes.extend(anchor.unwrap_or_default());
                }
                result.hit_count = records.len() as u64;
                result.text_length = text_length as u64;
                result.anchor_length = anchor_bytes.len() as u64;
                if !hit_out.fits(records.len())
                    || !text_out.fits(text_length)
                    || !anchor_out.fits(anchor_bytes.len())
                {
                    out.set(result);
                    return Err(MD_ERR_BUFFER_TOO_SMALL);
                }
                hit_out.write(&records);
                for (hit, record) in document.hits.iter().zip(&records) {
                    text_out.write_at(record.text_offset as usize, hit.snippet.text.as_bytes());
                }
                anchor_out.write(&anchor_bytes);
            }
        }
        progress_out.set(progress_record(&accounting));
        out.set(result);
        Ok(())
    })
}
