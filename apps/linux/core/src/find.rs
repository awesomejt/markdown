//! Find over the session's current snapshot (LNX-001F, specs/search).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::ffi;
use crate::runtime::{CoreError, HostLimits, checked, on_worker, submit};
use crate::session::{CoreSession, SessionInner, lock};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CoreFindOptions {
    pub case_sensitive: bool,
    pub whole_word: bool,
    pub max_matches: u32,
}

impl Default for CoreFindOptions {
    fn default() -> Self {
        Self {
            case_sensitive: false,
            whole_word: false,
            max_matches: ffi::MD_FIND_DEFAULT_MAX_MATCHES,
        }
    }
}

/// The part of one match inside one node: a UTF-8 byte range in that node's
/// display text and its source span.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CoreFindSegment {
    pub node: u32,
    pub text_start: u32,
    pub text_end: u32,
    pub span_start: u32,
    pub span_end: u32,
    pub exact: bool,
}

/// One match in reading order; segments are in `CoreFindResult.segments`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreFindMatch {
    pub unit: u32,
    pub span_start: u32,
    pub span_end: u32,
    pub exact: bool,
    pub first_segment: usize,
    pub segment_count: usize,
}

/// Owned copies of one `md_snapshot_find` result for snapshot `generation`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreFindResult {
    pub generation: u64,
    pub matches: Vec<CoreFindMatch>,
    pub segments: Vec<CoreFindSegment>,
    pub truncated: bool,
}

#[derive(Clone, Default)]
pub struct FindGate {
    cancelled: Arc<AtomicBool>,
}

impl FindGate {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}

impl SessionInner {
    pub(crate) fn run_find(
        &self,
        query: &[u8],
        options: CoreFindOptions,
        generation: u64,
        cancelled: impl Fn() -> bool,
    ) -> Result<CoreFindResult, CoreError> {
        if cancelled() {
            return Err(CoreError::Cancelled);
        }
        let (snapshot, target_gen) = {
            let state = lock(&self.state);
            if state.closing.is_some() || state.handle == 0 {
                return Err(CoreError::Closed);
            }
            let mut info = ffi::md_session_status_info {
                struct_size: ffi::size_of::<ffi::md_session_status_info>(),
                ..Default::default()
            };
            checked(unsafe { ffi::md_session_get_status(state.handle, &mut info) })?;
            if generation != 0 && info.published != generation {
                return Err(CoreError::Superseded {
                    expected: generation,
                    actual: info.published,
                });
            }
            let target_gen = if generation == 0 {
                info.published
            } else {
                generation
            };
            let mut snap: ffi::md_snapshot = 0;
            checked(unsafe { ffi::md_session_current(state.handle, &mut snap) })?;
            if snap == 0 {
                return Err(CoreError::Superseded {
                    expected: generation,
                    actual: info.published,
                });
            }
            (snap, target_gen)
        };

        struct SnapshotGuard(ffi::md_snapshot);
        impl Drop for SnapshotGuard {
            fn drop(&mut self) {
                // SAFETY: valid live snapshot handle.
                unsafe { ffi::md_snapshot_release(self.0) };
            }
        }
        let _guard = SnapshotGuard(snapshot);

        if cancelled() {
            return Err(CoreError::Cancelled);
        }

        let mut flags = 0u32;
        if options.case_sensitive {
            flags |= ffi::MD_FIND_CASE_SENSITIVE;
        }
        if options.whole_word {
            flags |= ffi::MD_FIND_WHOLE_WORD;
        }

        let settings = ffi::md_find_options {
            struct_size: ffi::MD_FIND_OPTIONS_SIZE,
            flags,
            max_matches: options.max_matches,
            match_size: ffi::MD_FIND_MATCH_SIZE,
            segment_size: ffi::MD_MATCH_SEGMENT_SIZE,
            reserved: 0,
        };

        let limit = if options.max_matches == 0 {
            ffi::MD_FIND_DEFAULT_MAX_MATCHES
        } else {
            options.max_matches
        } as usize;

        let mut matches = vec![ffi::md_find_match::default(); limit.min(256)];
        let mut segments = vec![ffi::md_match_segment::default(); (limit * 2).min(512)];
        let mut result = ffi::md_find_result {
            struct_size: ffi::MD_FIND_RESULT_SIZE,
            ..Default::default()
        };

        for attempt in 0..2 {
            result = ffi::md_find_result {
                struct_size: ffi::MD_FIND_RESULT_SIZE,
                ..Default::default()
            };
            let status = unsafe {
                ffi::md_snapshot_find(
                    snapshot,
                    query.as_ptr(),
                    query.len(),
                    &settings,
                    matches.as_mut_ptr(),
                    matches.len(),
                    segments.as_mut_ptr(),
                    segments.len(),
                    &mut result,
                )
            };
            if status == ffi::MD_ERR_BUFFER_TOO_SMALL && attempt == 0 {
                let req_matches = result.match_count as usize;
                let req_segments = result.segment_count as usize;
                if req_matches > limit
                    || req_segments > limit * 4 + 2 * (HostLimits::HOST.max_nodes as usize)
                {
                    return Err(CoreError::InvalidSnapshot);
                }
                matches.resize(req_matches, ffi::md_find_match::default());
                segments.resize(req_segments, ffi::md_match_segment::default());
                continue;
            }
            checked(status)?;
            break;
        }

        let match_count = result.match_count as usize;
        let segment_count = result.segment_count as usize;
        if match_count > matches.len() || segment_count > segments.len() {
            return Err(CoreError::InvalidSnapshot);
        }

        let exact_flag = ffi::MD_MATCH_EXACT;
        let mut owned_segments = Vec::with_capacity(segment_count);
        for seg in &segments[..segment_count] {
            if seg.text_start > seg.text_end || seg.span_start > seg.span_end {
                return Err(CoreError::InvalidSnapshot);
            }
            owned_segments.push(CoreFindSegment {
                node: seg.node,
                text_start: seg.text_start,
                text_end: seg.text_end,
                span_start: seg.span_start,
                span_end: seg.span_end,
                exact: seg.flags & exact_flag != 0,
            });
        }

        let mut owned_matches = Vec::with_capacity(match_count);
        for m in &matches[..match_count] {
            let first = m.first_segment as usize;
            let count = m.segment_count as usize;
            let end = first.checked_add(count).ok_or(CoreError::InvalidSnapshot)?;
            if m.span_start > m.span_end || end > segment_count {
                return Err(CoreError::InvalidSnapshot);
            }
            owned_matches.push(CoreFindMatch {
                unit: m.unit,
                span_start: m.span_start,
                span_end: m.span_end,
                exact: m.flags & exact_flag != 0,
                first_segment: first,
                segment_count: count,
            });
        }

        Ok(CoreFindResult {
            generation: target_gen,
            matches: owned_matches,
            segments: owned_segments,
            truncated: result.flags & ffi::MD_FIND_TRUNCATED != 0,
        })
    }
}

impl CoreSession {
    /// Literal find over the session's current snapshot.
    ///
    /// The call cannot be interrupted while executing in the core, so it runs
    /// on the shared core worker. If cancelled before execution starts, it returns
    /// `CoreError::Cancelled`. It fails with `CoreError::Superseded` unless the current
    /// snapshot matches `generation`.
    pub fn find(
        &self,
        query: &str,
        options: CoreFindOptions,
        generation: u64,
        gate: Option<FindGate>,
        done: impl FnOnce(Result<CoreFindResult, CoreError>) + Send + 'static,
    ) {
        let query_bytes = query.as_bytes().to_vec();
        if query_bytes.len() > ffi::MD_FIND_MAX_QUERY_BYTES as usize {
            done(Err(CoreError::QueryTooLong));
            return;
        }
        if options.max_matches > ffi::MD_FIND_MAX_MATCHES {
            done(Err(CoreError::InvalidArgument));
            return;
        }

        let inner = self.inner().clone();
        submit(move || {
            if let Some(g) = &gate
                && g.is_cancelled()
            {
                done(Err(CoreError::Cancelled));
                return;
            }
            let is_cancelled = move || gate.as_ref().is_some_and(|g| g.is_cancelled());
            let result = inner.run_find(&query_bytes, options, generation, is_cancelled);
            done(result);
        });
    }

    pub fn find_blocking(
        &self,
        query: &str,
        options: CoreFindOptions,
        generation: u64,
    ) -> Result<CoreFindResult, CoreError> {
        if on_worker() {
            return Err(CoreError::OnWorker);
        }
        let (sender, receiver) = std::sync::mpsc::channel();
        self.find(query, options, generation, None, move |res| {
            let _ = sender.send(res);
        });
        receiver.recv().map_err(|_| CoreError::Internal)?
    }
}
