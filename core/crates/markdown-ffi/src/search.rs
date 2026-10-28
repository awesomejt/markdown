//! Find, word count and section reading time over a snapshot handle: the
//! first ABI v1 additions under D-022 (new functions filling caller-owned
//! record arrays; no new handle types). Semantics are `markdown_core::search`
//! unchanged; this module only validates arguments and flattens results.
use crate::abi::*;
use crate::flat::small;
use crate::*;
use markdown_core::search::{
    self, FindMatch, FindOptions, FindResults, MatchSegment, ReadOptions, ReadStats,
    SearchDocument, SearchError,
};

/// A caller-owned record array (null only with zero capacity). Like [`Out`],
/// it is written only through raw stores, and only after all fallible work,
/// so failures leave it untouched.
struct Records<T> {
    ptr: *mut T,
    capacity: usize,
}
impl<T: Copy> Records<T> {
    /// # Safety
    /// A non-null, aligned `ptr` must be valid for writes of `capacity`
    /// values of `T`.
    unsafe fn new(ptr: *mut T, capacity: usize) -> Result<Self, md_status> {
        if (capacity != 0 && ptr.is_null())
            || (!ptr.is_null() && !ptr.is_aligned())
            || capacity
                .checked_mul(size_of::<T>())
                .is_none_or(|bytes| bytes > isize::MAX as usize)
        {
            return Err(MD_ERR_INVALID_ARGUMENT);
        }
        Ok(Self { ptr, capacity })
    }
    fn fits(&self, count: usize) -> bool {
        count <= self.capacity
    }
    fn set(&self, index: usize, value: T) {
        assert!(index < self.capacity, "record index checked by `fits`");
        // SAFETY: `new` validated the pointer for `capacity` writes and the
        // index is in range.
        unsafe { self.ptr.add(index).write(value) };
    }
    fn write(&self, values: &[T]) {
        assert!(self.fits(values.len()), "record count checked by `fits`");
        if values.is_empty() {
            return;
        }
        // SAFETY: `new` validated the pointer for `capacity` writes;
        // `values` is core-owned, so the ranges cannot overlap.
        unsafe { std::ptr::copy_nonoverlapping(values.as_ptr(), self.ptr, values.len()) };
    }
}

fn search_error(error: SearchError) -> md_status {
    match error {
        SearchError::QueryTooLong | SearchError::InvalidOptions => MD_ERR_INVALID_ARGUMENT,
        // A snapshot owns its source and FFI calls are never cancelled.
        SearchError::SourceMismatch | SearchError::Cancelled => MD_ERR_INTERNAL,
    }
}

fn find_options(options: &md_find_options) -> Result<FindOptions, md_status> {
    if options.match_size != size::<md_find_match>()
        || options.segment_size != size::<md_match_segment>()
    {
        return Err(MD_ERR_STRUCT_SIZE);
    }
    if options.flags & !(MD_FIND_CASE_SENSITIVE | MD_FIND_WHOLE_WORD) != 0
        || options.reserved != 0
        || options.max_matches > MD_FIND_MAX_MATCHES
    {
        return Err(MD_ERR_INVALID_ARGUMENT);
    }
    Ok(FindOptions {
        case_sensitive: options.flags & MD_FIND_CASE_SENSITIVE != 0,
        whole_word: options.flags & MD_FIND_WHOLE_WORD != 0,
        max_matches: match options.max_matches {
            0 => MD_FIND_DEFAULT_MAX_MATCHES,
            n => n,
        } as usize,
    })
}

fn read_options(options: &md_read_options) -> Result<ReadOptions, md_status> {
    if options.section_size != size::<md_read_section>() {
        return Err(MD_ERR_STRUCT_SIZE);
    }
    let words_per_minute = match options.words_per_minute {
        0 => MD_READ_DEFAULT_WPM,
        n if (MD_READ_MIN_WPM..=MD_READ_MAX_WPM).contains(&n) => n,
        _ => return Err(MD_ERR_INVALID_ARGUMENT),
    };
    if options.flags & !(MD_READ_INCLUDE_HEADINGS | MD_READ_INCLUDE_CODE) != 0 {
        return Err(MD_ERR_INVALID_ARGUMENT);
    }
    Ok(ReadOptions {
        words_per_minute,
        include_headings: options.flags & MD_READ_INCLUDE_HEADINGS != 0,
        include_code: options.flags & MD_READ_INCLUDE_CODE != 0,
    })
}

fn exact(exact: bool) -> u32 {
    if exact { MD_MATCH_EXACT } else { 0 }
}

fn match_record(found: &FindMatch, first_segment: usize) -> Result<md_find_match, md_status> {
    Ok(md_find_match {
        unit: small(found.unit)?,
        flags: exact(found.exact),
        span_start: small(found.source.start)?,
        span_end: small(found.source.end)?,
        first_segment: small(first_segment)?,
        segment_count: small(found.segments.len())?,
    })
}
fn segment_record(segment: &MatchSegment) -> Result<md_match_segment, md_status> {
    Ok(md_match_segment {
        node: small(segment.node)?,
        flags: exact(segment.exact),
        text_start: small(segment.text_start)?,
        text_end: small(segment.text_end)?,
        span_start: small(segment.source.start)?,
        span_end: small(segment.source.end)?,
    })
}

/// Convert every match and segment record, writing each only when `out` is
/// given, and return the segment count. Records are built from the core
/// results one at a time, never as a second copy of the whole result.
fn find_records(
    results: &FindResults,
    out: Option<(&Records<md_find_match>, &Records<md_match_segment>)>,
) -> Result<usize, md_status> {
    let mut next = 0;
    for (index, found) in results.matches.iter().enumerate() {
        let record = match_record(found, next)?;
        if let Some((matches, _)) = out {
            matches.set(index, record);
        }
        for segment in &found.segments {
            let record = segment_record(segment)?;
            if let Some((_, segments)) = out {
                segments.set(next, record);
            }
            next += 1;
        }
    }
    Ok(next)
}

/// Flatten reading sections into the ABI records.
fn section_records(stats: &ReadStats) -> Result<Vec<md_read_section>, md_status> {
    let index = |value: Option<usize>| value.map_or(Ok(MD_NONE), small);
    stats
        .sections
        .iter()
        .map(|s| {
            Ok(md_read_section {
                heading: index(s.heading)?,
                outline_index: index(s.outline_index)?,
                level: u32::from(s.level),
                reserved: 0,
                words: s.words,
                seconds: s.seconds,
                cumulative_words: s.cumulative_words,
                cumulative_seconds: s.cumulative_seconds,
            })
        })
        .collect()
}

/// Literal find over the snapshot's visible text.
///
/// # Safety
/// `query` must be readable for `query_length` bytes (null only with zero);
/// `options` must point to a readable, aligned `md_find_options`; `matches`
/// and `segments` must be null with zero capacity or writable, aligned arrays
/// of their capacities; `out_result` must point to a writable, aligned
/// `md_find_result` with `struct_size` set. None may overlap.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn md_snapshot_find(
    snapshot: md_snapshot,
    query: *const u8,
    query_length: usize,
    options: *const md_find_options,
    matches: *mut md_find_match,
    match_capacity: usize,
    segments: *mut md_match_segment,
    segment_capacity: usize,
    out_result: *mut md_find_result,
) -> md_status {
    guard(|| {
        // SAFETY: forwarded caller contract.
        let out = unsafe { Out::versioned(out_result) }?;
        // SAFETY: forwarded caller contract.
        let options = find_options(&unsafe { input(options) }?)?;
        // SAFETY: forwarded caller contract.
        let (match_out, segment_out) = unsafe {
            (
                Records::new(matches, match_capacity)?,
                Records::new(segments, segment_capacity)?,
            )
        };
        // SAFETY: forwarded caller contract.
        let query = unsafe { bytes(query, query_length) }?;
        if query.len() > MD_FIND_MAX_QUERY_BYTES as usize {
            return Err(MD_ERR_INVALID_ARGUMENT);
        }
        let query = std::str::from_utf8(query).map_err(|_| MD_ERR_INVALID_ARGUMENT)?;
        let entry = self::snapshot(snapshot)?;
        checkpoint();
        let document = SearchDocument::from_snapshot(&entry.snapshot);
        let results = search::find(&document, query, &options).map_err(search_error)?;
        // Validate every record (the only fallible step) before writing any,
        // then write them straight into the caller's arrays.
        let segment_count = find_records(&results, None)?;
        let mut result = md_find_result::sized();
        result.flags = if results.truncated {
            MD_FIND_TRUNCATED
        } else {
            0
        };
        result.match_count = results.matches.len() as u64;
        result.segment_count = segment_count as u64;
        if !match_out.fits(results.matches.len()) || !segment_out.fits(segment_count) {
            out.set(result);
            return Err(MD_ERR_BUFFER_TOO_SMALL);
        }
        find_records(&results, Some((&match_out, &segment_out)))?;
        out.set(result);
        Ok(())
    })
}

/// Word count and per-section reading time.
///
/// # Safety
/// `options` must point to a readable, aligned `md_read_options`; `sections`
/// must be null with zero capacity or a writable, aligned array of
/// `section_capacity` records; `out_stats` must point to a writable, aligned
/// `md_read_stats` with `struct_size` set. None may overlap.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn md_snapshot_read_stats(
    snapshot: md_snapshot,
    options: *const md_read_options,
    sections: *mut md_read_section,
    section_capacity: usize,
    out_stats: *mut md_read_stats,
) -> md_status {
    guard(|| {
        // SAFETY: forwarded caller contract.
        let out = unsafe { Out::versioned(out_stats) }?;
        // SAFETY: forwarded caller contract.
        let options = read_options(&unsafe { input(options) }?)?;
        // SAFETY: forwarded caller contract.
        let section_out = unsafe { Records::new(sections, section_capacity) }?;
        let entry = self::snapshot(snapshot)?;
        checkpoint();
        let document = SearchDocument::from_snapshot(&entry.snapshot);
        let stats = search::read_stats(&document, &options).map_err(search_error)?;
        let records = section_records(&stats)?;
        let mut result = md_read_stats::sized();
        result.words_per_minute = stats.words_per_minute;
        result.words = stats.words;
        result.seconds = stats.seconds;
        result.section_count = records.len() as u64;
        if !section_out.fits(records.len()) {
            out.set(result);
            return Err(MD_ERR_BUFFER_TOO_SMALL);
        }
        section_out.write(&records);
        out.set(result);
        Ok(())
    })
}
