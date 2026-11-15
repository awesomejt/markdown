//! Safe wrapper for workspace path classification via the C ABI (CORE-005B2/CORE-005D).

use crate::ffi;
use crate::runtime::{CoreError, checked};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorkspacePathInfo {
    pub is_markdown: bool,
    pub is_hidden: bool,
    pub has_invisible: bool,
    pub name_start: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkspacePathError {
    Empty,
    TooLong,
    TooDeep,
    Absolute,
    EmptyComponent,
    DotComponent,
    Control,
    InvalidUtf8,
}

/// Classify a relative `/`-separated display path without opening or resolving files.
pub fn classify_workspace_path(
    path: &[u8],
) -> Result<Result<WorkspacePathInfo, WorkspacePathError>, CoreError> {
    let mut info = ffi::md_workspace_path_info {
        struct_size: ffi::size_of::<ffi::md_workspace_path_info>(),
        ..Default::default()
    };
    let ptr = if path.is_empty() {
        std::ptr::null()
    } else {
        path.as_ptr()
    };
    // SAFETY: `ptr` is either null with length 0 or points to `path.len()` readable bytes,
    // and `info` is an exclusively borrowed `md_workspace_path_info` with its exact struct_size.
    checked(unsafe { ffi::md_workspace_classify_path(ptr, path.len(), &mut info) })?;
    let res = match info.error {
        ffi::MD_PATH_VALID => Ok(WorkspacePathInfo {
            is_markdown: (info.flags & ffi::MD_PATH_MARKDOWN) != 0,
            is_hidden: (info.flags & ffi::MD_PATH_HIDDEN) != 0,
            has_invisible: (info.flags & ffi::MD_PATH_INVISIBLE) != 0,
            name_start: info.name_start,
        }),
        ffi::MD_PATH_EMPTY => Err(WorkspacePathError::Empty),
        ffi::MD_PATH_TOO_LONG => Err(WorkspacePathError::TooLong),
        ffi::MD_PATH_TOO_DEEP => Err(WorkspacePathError::TooDeep),
        ffi::MD_PATH_ABSOLUTE => Err(WorkspacePathError::Absolute),
        ffi::MD_PATH_EMPTY_COMPONENT => Err(WorkspacePathError::EmptyComponent),
        ffi::MD_PATH_DOT_COMPONENT => Err(WorkspacePathError::DotComponent),
        ffi::MD_PATH_CONTROL => Err(WorkspacePathError::Control),
        ffi::MD_PATH_INVALID_UTF8 => Err(WorkspacePathError::InvalidUtf8),
        _ => return Err(CoreError::IncompatibleAbi),
    };
    Ok(res)
}

#[derive(Clone, Copy, Debug)]
pub struct WorkspaceNameEntry<'a> {
    pub id: u64,
    pub rel_path: &'a str,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NameSearchOptions {
    pub case_sensitive: bool,
    pub include_hidden: bool,
    pub all_files: bool,
    pub max_results: u32,
    pub max_entries: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum NameRank {
    Prefix,
    Name,
    Path,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NameHit {
    pub id: u64,
    pub entry_index: u32,
    pub rank: NameRank,
    pub match_start: u32,
    pub match_end: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NameSearchResult {
    pub hits: Vec<NameHit>,
    pub truncated: bool,
    pub entries_limited: bool,
    pub total_hits: u64,
    pub entries_examined: u64,
    pub skipped_hidden: u64,
    pub skipped_not_markdown: u64,
    pub skipped_invalid: u64,
}

pub fn find_workspace_names(
    entries: &[WorkspaceNameEntry<'_>],
    query: &str,
    options: &NameSearchOptions,
) -> Result<NameSearchResult, CoreError> {
    let total_bytes: usize = entries
        .iter()
        .map(|e| e.rel_path.len())
        .fold(0usize, |acc, n| acc.saturating_add(n));
    let mut paths = Vec::with_capacity(total_bytes);
    let mut ffi_entries = Vec::with_capacity(entries.len());
    for entry in entries {
        let offset = paths.len() as u64;
        let len = u32::try_from(entry.rel_path.len()).map_err(|_| CoreError::InvalidArgument)?;
        paths.extend_from_slice(entry.rel_path.as_bytes());
        ffi_entries.push(ffi::md_workspace_entry {
            id: entry.id,
            path_offset: offset,
            path_length: len,
            reserved: 0,
        });
    }

    let mut flags = 0u32;
    if options.case_sensitive {
        flags |= ffi::MD_NAMES_CASE_SENSITIVE;
    }
    if options.include_hidden {
        flags |= ffi::MD_NAMES_INCLUDE_HIDDEN;
    }
    if options.all_files {
        flags |= ffi::MD_NAMES_ALL_FILES;
    }

    let ffi_options = ffi::md_name_options {
        struct_size: ffi::size_of::<ffi::md_name_options>(),
        flags,
        max_results: options.max_results,
        max_entries: options.max_entries,
        entry_size: ffi::size_of::<ffi::md_workspace_entry>(),
        hit_size: ffi::size_of::<ffi::md_name_hit>(),
    };

    let initial_cap = if options.max_results == 0 {
        ffi::MD_NAMES_DEFAULT_MAX_RESULTS as usize
    } else {
        (options.max_results as usize).min(ffi::MD_NAMES_MAX_RESULTS as usize)
    };
    let mut hits = vec![ffi::md_name_hit::default(); initial_cap];
    let mut result = ffi::md_name_result {
        struct_size: ffi::size_of::<ffi::md_name_result>(),
        ..Default::default()
    };

    let paths_ptr = if paths.is_empty() {
        std::ptr::null()
    } else {
        paths.as_ptr()
    };
    let entries_ptr = if ffi_entries.is_empty() {
        std::ptr::null()
    } else {
        ffi_entries.as_ptr()
    };
    let query_bytes = query.as_bytes();
    let query_ptr = if query_bytes.is_empty() {
        std::ptr::null()
    } else {
        query_bytes.as_ptr()
    };

    // SAFETY: all pointers match their lengths/capacities and struct_size fields are set.
    let status = unsafe {
        ffi::md_workspace_find_names(
            paths_ptr,
            paths.len(),
            entries_ptr,
            ffi_entries.len(),
            query_ptr,
            query_bytes.len(),
            &ffi_options,
            if hits.is_empty() {
                std::ptr::null_mut()
            } else {
                hits.as_mut_ptr()
            },
            hits.len(),
            &mut result,
        )
    };

    if status == ffi::MD_ERR_BUFFER_TOO_SMALL {
        let needed = usize::try_from(result.hit_count).map_err(|_| CoreError::IncompatibleAbi)?;
        hits.resize(needed, ffi::md_name_hit::default());
        result = ffi::md_name_result {
            struct_size: ffi::size_of::<ffi::md_name_result>(),
            ..Default::default()
        };
        // SAFETY: resized buffer matches `needed` capacity.
        checked(unsafe {
            ffi::md_workspace_find_names(
                paths_ptr,
                paths.len(),
                entries_ptr,
                ffi_entries.len(),
                query_ptr,
                query_bytes.len(),
                &ffi_options,
                if hits.is_empty() {
                    std::ptr::null_mut()
                } else {
                    hits.as_mut_ptr()
                },
                hits.len(),
                &mut result,
            )
        })?;
    } else {
        checked(status)?;
    }

    let count = usize::try_from(result.hit_count).map_err(|_| CoreError::IncompatibleAbi)?;
    let slice = hits.get(..count).ok_or(CoreError::IncompatibleAbi)?;
    let mut out_hits = Vec::with_capacity(count);
    for h in slice {
        let rank = match h.rank {
            ffi::MD_NAME_RANK_PREFIX => NameRank::Prefix,
            ffi::MD_NAME_RANK_NAME => NameRank::Name,
            ffi::MD_NAME_RANK_PATH => NameRank::Path,
            _ => return Err(CoreError::IncompatibleAbi),
        };
        out_hits.push(NameHit {
            id: h.id,
            entry_index: h.entry,
            rank,
            match_start: h.start,
            match_end: h.end,
        });
    }

    Ok(NameSearchResult {
        hits: out_hits,
        truncated: (result.flags & ffi::MD_NAMES_TRUNCATED) != 0,
        entries_limited: (result.flags & ffi::MD_NAMES_ENTRIES_LIMITED) != 0,
        total_hits: result.total_hits,
        entries_examined: result.entries_examined,
        skipped_hidden: result.skipped_hidden,
        skipped_not_markdown: result.skipped_not_markdown,
        skipped_invalid: result.skipped_invalid,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContentSkipReason {
    Hidden,
    NotMarkdown,
    TooLarge,
    InvalidUtf8,
    ParseFailed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContentStopReason {
    Files,
    Bytes,
    Matches,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ContentSearchOptions {
    pub case_sensitive: bool,
    pub whole_word: bool,
    pub include_hidden: bool,
    pub max_files: u64,
    pub max_total_bytes: u64,
    pub max_file_bytes: u32,
    pub max_matches_per_file: u32,
    pub max_total_matches: u32,
    pub snippet_context: u32,
}

impl Default for ContentSearchOptions {
    fn default() -> Self {
        Self {
            case_sensitive: false,
            whole_word: false,
            include_hidden: false,
            max_files: 0,
            max_total_bytes: 0,
            max_file_bytes: 0,
            max_matches_per_file: 0,
            max_total_matches: 0,
            snippet_context: ffi::MD_CONTENT_DEFAULT_SNIPPET_CONTEXT,
        }
    }
}

impl ContentSearchOptions {
    fn to_ffi(self) -> ffi::md_content_options {
        let mut flags = 0u32;
        if self.case_sensitive {
            flags |= ffi::MD_CONTENT_CASE_SENSITIVE;
        }
        if self.whole_word {
            flags |= ffi::MD_CONTENT_WHOLE_WORD;
        }
        if self.include_hidden {
            flags |= ffi::MD_CONTENT_INCLUDE_HIDDEN;
        }
        ffi::md_content_options {
            struct_size: ffi::size_of::<ffi::md_content_options>(),
            flags,
            max_files: self.max_files,
            max_total_bytes: self.max_total_bytes,
            max_file_bytes: self.max_file_bytes,
            max_matches_per_file: self.max_matches_per_file,
            max_total_matches: self.max_total_matches,
            snippet_context: self.snippet_context,
            hit_size: ffi::size_of::<ffi::md_content_hit>(),
            reserved: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ContentProgress {
    raw: ffi::md_content_progress,
}

impl Default for ContentProgress {
    fn default() -> Self {
        Self::new()
    }
}

impl ContentProgress {
    pub fn new() -> Self {
        Self {
            raw: ffi::md_content_progress {
                struct_size: ffi::size_of::<ffi::md_content_progress>(),
                ..Default::default()
            },
        }
    }

    pub fn stopped(&self) -> Option<ContentStopReason> {
        match self.raw.stopped {
            0 => None,
            ffi::MD_STOP_FILES => Some(ContentStopReason::Files),
            ffi::MD_STOP_BYTES => Some(ContentStopReason::Bytes),
            ffi::MD_STOP_MATCHES => Some(ContentStopReason::Matches),
            _ => Some(ContentStopReason::Matches),
        }
    }

    pub fn files_read(&self) -> u64 {
        self.raw.files_read
    }

    pub fn files_matched(&self) -> u64 {
        self.raw.files_matched
    }

    pub fn bytes_read(&self) -> u64 {
        self.raw.bytes_read
    }

    pub fn matches(&self) -> u64 {
        self.raw.matches
    }

    pub fn files_truncated(&self) -> u64 {
        self.raw.files_truncated
    }

    pub fn skipped_hidden(&self) -> u64 {
        self.raw.skipped_hidden
    }

    pub fn skipped_not_markdown(&self) -> u64 {
        self.raw.skipped_not_markdown
    }

    pub fn skipped_too_large(&self) -> u64 {
        self.raw.skipped_too_large
    }

    pub fn skipped_invalid_utf8(&self) -> u64 {
        self.raw.skipped_invalid_utf8
    }

    pub fn skipped_parse_failed(&self) -> u64 {
        self.raw.skipped_parse_failed
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContentAdmitOutcome {
    Read,
    Skipped(ContentSkipReason),
    Stopped(ContentStopReason),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContentHit {
    pub unit: u32,
    pub exact: bool,
    pub leading_more: bool,
    pub trailing_more: bool,
    pub span_start: u32,
    pub span_end: u32,
    pub line: u64,
    pub snippet: String,
    pub match_start: u32,
    pub match_end: u32,
    pub anchor: Option<Vec<u8>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ContentFileOutcome {
    Searched {
        hits: Vec<ContentHit>,
        truncated: bool,
        source_length: u64,
        source_sha256: [u8; 64],
    },
    Skipped(ContentSkipReason),
    Stopped(ContentStopReason),
}

fn map_skip_reason(reason: u32) -> Result<ContentSkipReason, CoreError> {
    match reason {
        ffi::MD_SKIP_HIDDEN => Ok(ContentSkipReason::Hidden),
        ffi::MD_SKIP_NOT_MARKDOWN => Ok(ContentSkipReason::NotMarkdown),
        ffi::MD_SKIP_TOO_LARGE => Ok(ContentSkipReason::TooLarge),
        ffi::MD_SKIP_INVALID_UTF8 => Ok(ContentSkipReason::InvalidUtf8),
        ffi::MD_SKIP_PARSE_FAILED => Ok(ContentSkipReason::ParseFailed),
        _ => Err(CoreError::IncompatibleAbi),
    }
}

fn map_stop_reason(reason: u32) -> Result<ContentStopReason, CoreError> {
    match reason {
        ffi::MD_STOP_FILES => Ok(ContentStopReason::Files),
        ffi::MD_STOP_BYTES => Ok(ContentStopReason::Bytes),
        ffi::MD_STOP_MATCHES => Ok(ContentStopReason::Matches),
        _ => Err(CoreError::IncompatibleAbi),
    }
}

pub fn admit_workspace_content(
    query: &str,
    options: &ContentSearchOptions,
    rel_path: &str,
    byte_length: u64,
    progress: &mut ContentProgress,
) -> Result<ContentAdmitOutcome, CoreError> {
    let ffi_options = options.to_ffi();
    let query_bytes = query.as_bytes();
    let path_bytes = rel_path.as_bytes();
    let mut outcome = 0u32;
    let mut reason = 0u32;

    // SAFETY: pointers match their lengths, `ffi_options` and `progress.raw` have valid struct_size.
    checked(unsafe {
        ffi::md_workspace_content_admit(
            if query_bytes.is_empty() {
                std::ptr::null()
            } else {
                query_bytes.as_ptr()
            },
            query_bytes.len(),
            &ffi_options,
            if path_bytes.is_empty() {
                std::ptr::null()
            } else {
                path_bytes.as_ptr()
            },
            path_bytes.len(),
            byte_length,
            &mut progress.raw,
            &mut outcome,
            &mut reason,
        )
    })?;

    match outcome {
        ffi::MD_CONTENT_READ => Ok(ContentAdmitOutcome::Read),
        ffi::MD_CONTENT_SKIPPED => Ok(ContentAdmitOutcome::Skipped(map_skip_reason(reason)?)),
        ffi::MD_CONTENT_STOPPED => Ok(ContentAdmitOutcome::Stopped(map_stop_reason(reason)?)),
        _ => Err(CoreError::IncompatibleAbi),
    }
}

pub fn search_workspace_content(
    query: &str,
    options: &ContentSearchOptions,
    rel_path: &str,
    content: &[u8],
    progress: &mut ContentProgress,
) -> Result<ContentFileOutcome, CoreError> {
    let ffi_options = options.to_ffi();
    let query_bytes = query.as_bytes();
    let path_bytes = rel_path.as_bytes();
    let query_ptr = if query_bytes.is_empty() {
        std::ptr::null()
    } else {
        query_bytes.as_ptr()
    };
    let path_ptr = if path_bytes.is_empty() {
        std::ptr::null()
    } else {
        path_bytes.as_ptr()
    };
    let content_ptr = if content.is_empty() {
        std::ptr::null()
    } else {
        content.as_ptr()
    };

    let default_cap = if options.max_matches_per_file == 0 {
        ffi::MD_CONTENT_DEFAULT_MAX_MATCHES_PER_FILE as usize
    } else {
        (options.max_matches_per_file as usize).min(256)
    };
    let mut hits = vec![ffi::md_content_hit::default(); default_cap];
    let mut text = vec![0u8; default_cap * ffi::MD_CONTENT_MAX_SNIPPET_BYTES as usize];
    let mut anchors = vec![0u8; default_cap * ffi::MD_ANCHOR_MAX_BYTES as usize];
    let mut result = ffi::md_content_result {
        struct_size: ffi::size_of::<ffi::md_content_result>(),
        ..Default::default()
    };

    // SAFETY: all buffers and struct_size values match their capacities and ABI definitions.
    let status = unsafe {
        ffi::md_workspace_content_search(
            query_ptr,
            query_bytes.len(),
            &ffi_options,
            path_ptr,
            path_bytes.len(),
            content_ptr,
            content.len(),
            &mut progress.raw,
            if hits.is_empty() {
                std::ptr::null_mut()
            } else {
                hits.as_mut_ptr()
            },
            hits.len(),
            if text.is_empty() {
                std::ptr::null_mut()
            } else {
                text.as_mut_ptr()
            },
            text.len(),
            if anchors.is_empty() {
                std::ptr::null_mut()
            } else {
                anchors.as_mut_ptr()
            },
            anchors.len(),
            &mut result,
        )
    };

    if status == ffi::MD_ERR_BUFFER_TOO_SMALL {
        let hit_cap = usize::try_from(result.hit_count).map_err(|_| CoreError::IncompatibleAbi)?;
        let text_cap =
            usize::try_from(result.text_length).map_err(|_| CoreError::IncompatibleAbi)?;
        let anchor_cap =
            usize::try_from(result.anchor_length).map_err(|_| CoreError::IncompatibleAbi)?;
        hits.resize(hit_cap, ffi::md_content_hit::default());
        text.resize(text_cap, 0);
        anchors.resize(anchor_cap, 0);
        result = ffi::md_content_result {
            struct_size: ffi::size_of::<ffi::md_content_result>(),
            ..Default::default()
        };
        // SAFETY: resized buffers match required capacities reported by the core.
        checked(unsafe {
            ffi::md_workspace_content_search(
                query_ptr,
                query_bytes.len(),
                &ffi_options,
                path_ptr,
                path_bytes.len(),
                content_ptr,
                content.len(),
                &mut progress.raw,
                if hits.is_empty() {
                    std::ptr::null_mut()
                } else {
                    hits.as_mut_ptr()
                },
                hits.len(),
                if text.is_empty() {
                    std::ptr::null_mut()
                } else {
                    text.as_mut_ptr()
                },
                text.len(),
                if anchors.is_empty() {
                    std::ptr::null_mut()
                } else {
                    anchors.as_mut_ptr()
                },
                anchors.len(),
                &mut result,
            )
        })?;
    } else {
        checked(status)?;
    }

    match result.outcome {
        ffi::MD_CONTENT_SKIPPED => Ok(ContentFileOutcome::Skipped(map_skip_reason(result.reason)?)),
        ffi::MD_CONTENT_STOPPED => Ok(ContentFileOutcome::Stopped(map_stop_reason(result.reason)?)),
        ffi::MD_CONTENT_SEARCHED => {
            let count =
                usize::try_from(result.hit_count).map_err(|_| CoreError::IncompatibleAbi)?;
            let slice = hits.get(..count).ok_or(CoreError::IncompatibleAbi)?;
            let mut out_hits = Vec::with_capacity(count);
            for h in slice {
                let text_start = h.text_offset as usize;
                let text_end = text_start
                    .checked_add(h.text_length as usize)
                    .ok_or(CoreError::IncompatibleAbi)?;
                let snippet_bytes = text
                    .get(text_start..text_end)
                    .ok_or(CoreError::IncompatibleAbi)?;
                let snippet = std::str::from_utf8(snippet_bytes)
                    .map_err(|_| CoreError::IncompatibleAbi)?
                    .to_string();
                if h.match_start > h.match_end || h.match_end > h.text_length {
                    return Err(CoreError::IncompatibleAbi);
                }
                let anchor = if (h.flags & ffi::MD_HIT_ANCHOR) != 0 && h.anchor_length > 0 {
                    let a_start = h.anchor_offset as usize;
                    let a_end = a_start
                        .checked_add(h.anchor_length as usize)
                        .ok_or(CoreError::IncompatibleAbi)?;
                    let a_bytes = anchors
                        .get(a_start..a_end)
                        .ok_or(CoreError::IncompatibleAbi)?;
                    Some(a_bytes.to_vec())
                } else {
                    None
                };
                out_hits.push(ContentHit {
                    unit: h.unit,
                    exact: (h.flags & ffi::MD_HIT_EXACT) != 0,
                    leading_more: (h.flags & ffi::MD_HIT_LEADING_MORE) != 0,
                    trailing_more: (h.flags & ffi::MD_HIT_TRAILING_MORE) != 0,
                    span_start: h.span_start,
                    span_end: h.span_end,
                    line: h.line,
                    snippet,
                    match_start: h.match_start,
                    match_end: h.match_end,
                    anchor,
                });
            }
            Ok(ContentFileOutcome::Searched {
                hits: out_hits,
                truncated: (result.flags & ffi::MD_CONTENT_TRUNCATED) != 0,
                source_length: result.source_length,
                source_sha256: result.source_sha256,
            })
        }
        _ => Err(CoreError::IncompatibleAbi),
    }
}
