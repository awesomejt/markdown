//! Literal, source-mapped find over a snapshot's visible text.
use super::unicode::{
    ZWJ, fold_case, is_extend, is_format, is_regional_indicator, joins_word, mid_joins,
};
use super::{CANCEL_CHECK_CHARS, Each, Piece, PieceKind, SearchDocument, SearchError, Walker};
use crate::{CancellationToken, SourceSpan};
use std::collections::VecDeque;

/// Raw query ceiling (UTF-8 bytes, before normalization).
pub const MAX_QUERY_BYTES: usize = 4096;
pub const DEFAULT_MAX_MATCHES: usize = 10_000;
/// Largest accepted `FindOptions::max_matches`.
pub const MAX_MATCHES: usize = 1_000_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FindOptions {
    /// Exact scalar comparison when true; otherwise [`super::fold_case`].
    pub case_sensitive: bool,
    /// Require word boundaries at query edges that are word characters.
    pub whole_word: bool,
    /// Result budget, 1..=[`MAX_MATCHES`]; more matches set `truncated`.
    pub max_matches: usize,
}
impl Default for FindOptions {
    fn default() -> Self {
        Self {
            case_sensitive: false,
            whole_word: false,
            max_matches: DEFAULT_MAX_MATCHES,
        }
    }
}

/// The part of one match inside one node's display text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MatchSegment {
    /// Index into the document's node array.
    pub node: usize,
    /// Byte range within the node's display text: `text` (text, code span,
    /// code block), `alt` (image) or the source slice (unsupported nodes).
    /// Breaks have an empty range at 0.
    pub text_start: u64,
    pub text_end: u64,
    /// Source bytes of this segment; widened to the node (or code line) when
    /// the display text is derived rather than a verbatim source copy.
    pub source: SourceSpan,
    pub exact: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FindMatch {
    /// Leaf block (paragraph, heading, table cell, code or unsupported block).
    pub unit: usize,
    /// Smallest source range covering every matched character. When `exact`,
    /// its slice equals the matched text plus any interleaved markup.
    pub source: SourceSpan,
    pub exact: bool,
    pub segments: Vec<MatchSegment>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FindResults {
    /// Non-overlapping matches in reading order; the index is stable for a
    /// snapshot, query and options.
    pub matches: Vec<FindMatch>,
    /// More matches exist beyond `max_matches`.
    pub truncated: bool,
}
impl FindResults {
    /// First match starting at or after `source_offset` (no wrap-around).
    pub fn next_from(&self, source_offset: u64) -> Option<usize> {
        self.matches
            .iter()
            .position(|m| m.source.start >= source_offset)
    }
    /// Last match starting before `source_offset` (no wrap-around).
    pub fn previous_before(&self, source_offset: u64) -> Option<usize> {
        self.matches
            .iter()
            .rposition(|m| m.source.start < source_offset)
    }
}

/// [`find_cancellable`] without cancellation.
pub fn find(
    doc: &SearchDocument<'_>,
    query: &str,
    options: &FindOptions,
) -> Result<FindResults, SearchError> {
    find_cancellable(doc, query, options, &CancellationToken::new())
}

/// Search visible text for a literal query. The token is polled before work,
/// per block and every few thousand characters; cancellation returns
/// `Err(Cancelled)` and no partial results.
pub fn find_cancellable(
    doc: &SearchDocument<'_>,
    query: &str,
    options: &FindOptions,
    token: &CancellationToken,
) -> Result<FindResults, SearchError> {
    token.check()?;
    if query.len() > MAX_QUERY_BYTES {
        return Err(SearchError::QueryTooLong);
    }
    if options.max_matches == 0 || options.max_matches > MAX_MATCHES {
        return Err(SearchError::InvalidOptions);
    }
    let pattern = normalize_query(query, options.case_sensitive);
    let mut results = FindResults::default();
    if pattern.is_empty() {
        return Ok(results);
    }
    let failure = kmp_table(&pattern);
    let walker = Walker::new(*doc);
    let mut scanner = Scanner::new(&pattern, &failure, options, token);
    for (unit, _) in walker.units() {
        token.check()?;
        scanner.start_unit(unit);
        walker.pieces(
            unit,
            &mut Each(|piece: Piece<'_>| scanner.piece(&piece, &mut results)),
        );
        scanner.finish_unit(&mut results);
        if scanner.cancelled {
            return Err(SearchError::Cancelled);
        }
        if scanner.stopped {
            break;
        }
    }
    Ok(results)
}

const SPACE: u32 = ' ' as u32;
/// Never equal to a pattern scalar: separators break matches.
const SEPARATOR: u32 = 0x11_0000;
/// Characters kept before a candidate start (two-character mid context).
const BEFORE: usize = 2;
/// Characters needed after a candidate end before it can be decided.
const AFTER: usize = 2;

/// One normalized character with everything a match needs, so the scanner
/// keeps no reference to its piece.
#[derive(Clone, Copy)]
struct Char {
    /// Comparison key (folded scalar, space or separator).
    key: u32,
    /// Original scalar for boundary checks; `None` for separators.
    ch: Option<char>,
    /// A regional indicator at an odd position in its run: it pairs with a
    /// following indicator into one flag.
    ri_odd: bool,
    /// Maps to its own source bytes (exact piece or a break).
    exact: bool,
    len: u8,
    node: usize,
    /// Byte offset in the node's display text.
    text_start: usize,
    source: SourceSpan,
}

fn key(c: char, case_sensitive: bool) -> u32 {
    if case_sensitive {
        c as u32
    } else {
        fold_case(c) as u32
    }
}

/// Collapse whitespace runs to one space, skip format characters, fold case
/// and trim edge whitespace (a block edge is not whitespace in the text).
fn normalize_query(query: &str, case_sensitive: bool) -> Vec<u32> {
    let mut out: Vec<u32> = Vec::with_capacity(query.len());
    for c in query.trim().chars() {
        if c.is_whitespace() {
            if out.last() != Some(&SPACE) {
                out.push(SPACE);
            }
        } else if !is_format(c) {
            out.push(key(c, case_sensitive));
        }
    }
    while out.last() == Some(&SPACE) {
        out.pop();
    }
    if out.first() == Some(&SPACE) {
        out.remove(0);
    }
    out
}

fn kmp_table(pattern: &[u32]) -> Vec<usize> {
    let mut table = vec![0; pattern.len()];
    let mut k = 0;
    for i in 1..pattern.len() {
        while k > 0 && pattern[i] != pattern[k] {
            k = table[k - 1];
        }
        if pattern[i] == pattern[k] {
            k += 1;
        }
        table[i] = k;
    }
    table
}

/// Streaming KMP over one unit at a time. Working memory is a ring of the
/// last `m + BEFORE + AFTER + 1` characters plus at most `AFTER + 1` pending
/// candidates: O(query length), independent of the unit's text length.
struct Scanner<'a> {
    pattern: &'a [u32],
    failure: &'a [usize],
    options: &'a FindOptions,
    token: &'a CancellationToken,
    case_sensitive: bool,
    ring: VecDeque<Char>,
    capacity: usize,
    /// Unit-local index of `ring[0]`, and of the next character.
    base: usize,
    count: usize,
    k: usize,
    /// First index a new match may start at (non-overlapping).
    free: usize,
    /// End indices of complete pattern hits awaiting lookahead.
    pending: VecDeque<usize>,
    ri_run: u32,
    unit: usize,
    budget: usize,
    cancelled: bool,
    /// The result budget is exhausted; the search stops.
    stopped: bool,
}
impl<'a> Scanner<'a> {
    fn new(
        pattern: &'a [u32],
        failure: &'a [usize],
        options: &'a FindOptions,
        token: &'a CancellationToken,
    ) -> Self {
        let capacity = pattern.len() + BEFORE + AFTER + 1;
        Self {
            pattern,
            failure,
            options,
            token,
            case_sensitive: options.case_sensitive,
            ring: VecDeque::with_capacity(capacity),
            capacity,
            base: 0,
            count: 0,
            k: 0,
            free: 0,
            pending: VecDeque::with_capacity(AFTER + 2),
            ri_run: 0,
            unit: 0,
            budget: 0,
            cancelled: false,
            stopped: false,
        }
    }
    fn start_unit(&mut self, unit: usize) {
        self.ring.clear();
        self.pending.clear();
        (self.base, self.count, self.k, self.free, self.ri_run) = (0, 0, 0, 0, 0);
        self.unit = unit;
    }
    fn halted(&self) -> bool {
        self.cancelled || self.stopped
    }
    fn last_is_space(&self) -> bool {
        self.ring.back().is_some_and(|c| c.key == SPACE)
    }
    fn piece(&mut self, piece: &Piece<'_>, results: &mut FindResults) {
        if self.halted() {
            return;
        }
        let exact_piece = matches!(piece.mapping, super::Mapping::Exact(_));
        let make = |key, ch, start: usize, len: usize, exact| Char {
            key,
            ch,
            ri_odd: false,
            exact,
            len: len as u8,
            node: piece.node,
            text_start: piece.text_offset + start,
            source: piece.source(start, len),
        };
        match piece.kind {
            PieceKind::Separator => self.push(make(SEPARATOR, None, 0, 0, false), results),
            PieceKind::Break => {
                if !self.last_is_space() {
                    self.push(make(SPACE, Some(' '), 0, 0, true), results);
                }
            }
            _ => {
                for (start, c) in piece.text.char_indices() {
                    self.budget += 1;
                    if self.budget >= CANCEL_CHECK_CHARS {
                        self.budget = 0;
                        if self.token.is_cancelled() {
                            self.cancelled = true;
                        }
                    }
                    if self.halted() {
                        return;
                    }
                    if c.is_whitespace() {
                        if !self.last_is_space() {
                            let ch = make(SPACE, Some(c), start, c.len_utf8(), exact_piece);
                            self.push(ch, results);
                        }
                    } else if !is_format(c) {
                        // Invisible format characters (soft hyphen, word
                        // joiner, BOM, bidi marks) are skipped, so `foobar`
                        // matches `foo\u{AD}bar`; the span still covers them.
                        let k = key(c, self.case_sensitive);
                        self.push(make(k, Some(c), start, c.len_utf8(), exact_piece), results);
                    }
                }
            }
        }
    }
    fn push(&mut self, mut c: Char, results: &mut FindResults) {
        if c.ch.is_some_and(is_regional_indicator) {
            self.ri_run += 1;
            c.ri_odd = self.ri_run % 2 == 1;
        } else {
            self.ri_run = 0;
        }
        if self.ring.len() == self.capacity {
            self.ring.pop_front();
            self.base += 1;
        }
        self.ring.push_back(c);
        self.count += 1;
        let (pattern, m) = (self.pattern, self.pattern.len());
        while self.k > 0 && c.key != pattern[self.k] {
            self.k = self.failure[self.k - 1];
        }
        if c.key == pattern[self.k] {
            self.k += 1;
        }
        if self.k == m {
            self.k = self.failure[m - 1];
            self.pending.push_back(self.count - 1);
        }
        while let Some(&j) = self.pending.front() {
            if self.count <= j + AFTER || self.stopped {
                break;
            }
            self.pending.pop_front();
            self.decide(j, results);
        }
    }
    fn finish_unit(&mut self, results: &mut FindResults) {
        while let Some(j) = self.pending.pop_front() {
            if self.halted() {
                break;
            }
            self.decide(j, results);
        }
    }
    fn at(&self, index: usize) -> Option<&Char> {
        index
            .checked_sub(self.base)
            .and_then(|i| self.ring.get(i))
            .filter(|_| index < self.count)
    }
    fn decide(&mut self, j: usize, results: &mut FindResults) {
        let i = j + 1 - self.pattern.len();
        if i < self.free {
            return;
        }
        let (Some(&first), Some(&last)) = (self.at(i), self.at(j)) else {
            return;
        };
        let before = i.checked_sub(1).and_then(|b| self.at(b));
        let after = self.at(j + 1);
        if before.is_some_and(|b| continues_cluster(b, &first))
            || after.is_some_and(|a| continues_cluster(&last, a))
        {
            return;
        }
        if self.options.whole_word {
            let before2 = i.checked_sub(2).and_then(|b| self.at(b));
            let after2 = self.at(j + 2);
            let joined_before = before.is_some_and(|b| {
                joins(b, &first) || before2.is_some_and(|b2| joins_across(b2, b, &first))
            });
            let joined_after = after.is_some_and(|a| {
                joins(&last, a) || after2.is_some_and(|a2| joins_across(&last, a, a2))
            });
            if joined_before || joined_after {
                return;
            }
        }
        if results.matches.len() == self.options.max_matches {
            results.truncated = true;
            self.stopped = true;
            return;
        }
        let chars = self.ring.range(i - self.base..=j - self.base);
        results.matches.push(to_match(self.unit, chars));
        self.free = j + 1;
    }
}

/// True when `b` continues the grapheme cluster ending with `a`.
fn continues_cluster(a: &Char, b: &Char) -> bool {
    let (Some(x), Some(y)) = (a.ch, b.ch) else {
        return false;
    };
    if y.is_whitespace() || x.is_whitespace() {
        return false;
    }
    is_extend(y) || x == ZWJ || (is_regional_indicator(y) && a.ri_odd)
}
fn joins(a: &Char, b: &Char) -> bool {
    match (a.ch, b.ch) {
        (Some(x), Some(y)) => joins_word(x, y),
        _ => false,
    }
}
/// `a mid c` joins one word as in the word counter (`cat's`, `3.14`).
fn joins_across(a: &Char, mid: &Char, c: &Char) -> bool {
    match (a.ch, mid.ch, c.ch) {
        (Some(x), Some(y), Some(z)) => mid_joins(x, y, z),
        _ => false,
    }
}

fn to_match<'c>(unit: usize, chars: impl Iterator<Item = &'c Char>) -> FindMatch {
    let mut segments: Vec<MatchSegment> = Vec::new();
    let mut source = SourceSpan {
        start: u64::MAX,
        end: 0,
    };
    let mut exact = true;
    for c in chars {
        let span = c.source;
        let is_exact = c.exact;
        let text_start = c.text_start as u64;
        let text_end = text_start + u64::from(c.len);
        exact &= is_exact;
        source.start = source.start.min(span.start);
        source.end = source.end.max(span.end);
        match segments.last_mut() {
            // Merge within one node unless that would make an exact segment
            // cover source bytes that are not its display text (for example a
            // `>` prefix between code lines). Skipped whitespace or format
            // characters inside one exact piece have equal text/source gaps.
            Some(s)
                if s.node == c.node
                    && (!(s.exact && is_exact)
                        || text_start.checked_sub(s.text_end)
                            == span.start.checked_sub(s.source.end)) =>
            {
                s.text_end = s.text_end.max(text_end);
                s.source.start = s.source.start.min(span.start);
                s.source.end = s.source.end.max(span.end);
                s.exact &= is_exact;
            }
            _ => segments.push(MatchSegment {
                node: c.node,
                text_start,
                text_end,
                source: span,
                exact: is_exact,
            }),
        }
    }
    FindMatch {
        unit,
        source,
        exact,
        segments,
    }
}
