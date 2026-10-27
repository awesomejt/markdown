//! Literal, source-mapped find over a snapshot's visible text.
use super::unicode::{ZWJ, fold_case, is_extend, is_regional_indicator, joins_word};
use super::{CANCEL_CHECK_CHARS, Piece, PieceKind, SearchDocument, SearchError, Walker};
use crate::{CancellationToken, SourceSpan};

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
    if pattern.iter().all(|&c| c == SPACE) {
        return Ok(results);
    }
    let failure = kmp_table(&pattern);
    let walker = Walker::new(*doc);
    let mut pieces = Vec::new();
    let mut chars = Vec::new();
    let mut budget = 0usize;
    for (unit, _) in walker.units() {
        token.check()?;
        walker.pieces(unit, &mut pieces);
        build(
            &pieces,
            options.case_sensitive,
            &mut chars,
            &mut budget,
            token,
        )?;
        if search_unit(
            unit,
            &pieces,
            &chars,
            &pattern,
            &failure,
            options,
            &mut results,
            &mut budget,
            token,
        )? {
            break;
        }
    }
    Ok(results)
}

const SPACE: u32 = ' ' as u32;
/// Never equal to a pattern scalar: separators break matches.
const SEPARATOR: u32 = 0x11_0000;

#[derive(Clone, Copy)]
struct Char {
    key: u32,
    piece: u32,
    start: usize,
    len: u8,
    /// Consecutive regional indicators ending here (0 if not one).
    ri: u32,
}
impl Char {
    fn scalar(&self) -> Option<char> {
        char::from_u32(self.key)
    }
}

fn key(c: char, case_sensitive: bool) -> u32 {
    if case_sensitive {
        c as u32
    } else {
        fold_case(c) as u32
    }
}

fn normalize_query(query: &str, case_sensitive: bool) -> Vec<u32> {
    let mut out: Vec<u32> = Vec::with_capacity(query.len());
    for c in query.chars() {
        if c.is_whitespace() {
            if out.last() != Some(&SPACE) {
                out.push(SPACE);
            }
        } else {
            out.push(key(c, case_sensitive));
        }
    }
    out
}

fn poll(budget: &mut usize, token: &CancellationToken) -> Result<(), SearchError> {
    *budget += 1;
    if *budget >= CANCEL_CHECK_CHARS {
        *budget = 0;
        token.check()?;
    }
    Ok(())
}

/// Flatten pieces into comparison keys, collapsing whitespace runs (spaces,
/// line endings, breaks) to one space mapped to the run's first character.
fn build(
    pieces: &[Piece<'_>],
    case_sensitive: bool,
    out: &mut Vec<Char>,
    budget: &mut usize,
    token: &CancellationToken,
) -> Result<(), SearchError> {
    out.clear();
    let push = |out: &mut Vec<Char>, key: u32, piece: usize, start: usize, len: usize| {
        let ri = match (char::from_u32(key), out.last()) {
            (Some(c), Some(prev)) if is_regional_indicator(c) => prev.ri + 1,
            (Some(c), None) if is_regional_indicator(c) => 1,
            _ => 0,
        };
        out.push(Char {
            key,
            piece: piece as u32,
            start,
            len: len as u8,
            ri,
        });
    };
    for (p, piece) in pieces.iter().enumerate() {
        match piece.kind {
            PieceKind::Separator => push(out, SEPARATOR, p, 0, 0),
            PieceKind::Break => {
                if out.last().is_none_or(|c| c.key != SPACE) {
                    push(out, SPACE, p, 0, 0);
                }
            }
            _ => {
                for (start, c) in piece.text.char_indices() {
                    poll(budget, token)?;
                    if c.is_whitespace() {
                        if out.last().is_none_or(|c| c.key != SPACE) {
                            push(out, SPACE, p, start, c.len_utf8());
                        }
                    } else {
                        push(out, key(c, case_sensitive), p, start, c.len_utf8());
                    }
                }
            }
        }
    }
    Ok(())
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

/// True when `b` continues the grapheme cluster ending with `a`.
fn continues_cluster(a: &Char, b: &Char) -> bool {
    let (Some(x), Some(y)) = (a.scalar(), b.scalar()) else {
        return false;
    };
    if y.is_whitespace() || x.is_whitespace() {
        return false;
    }
    is_extend(y) || x == ZWJ || (b.ri > 0 && a.ri % 2 == 1)
}
fn joins(a: &Char, b: &Char) -> bool {
    match (a.scalar(), b.scalar()) {
        (Some(x), Some(y)) => joins_word(x, y),
        _ => false,
    }
}

/// Returns true when the result budget is exhausted (search stops).
#[allow(clippy::too_many_arguments)]
fn search_unit(
    unit: usize,
    pieces: &[Piece<'_>],
    chars: &[Char],
    pattern: &[u32],
    failure: &[usize],
    options: &FindOptions,
    results: &mut FindResults,
    budget: &mut usize,
    token: &CancellationToken,
) -> Result<bool, SearchError> {
    let m = pattern.len();
    let mut k = 0;
    let mut free = 0;
    for (j, c) in chars.iter().enumerate() {
        poll(budget, token)?;
        while k > 0 && c.key != pattern[k] {
            k = failure[k - 1];
        }
        if c.key == pattern[k] {
            k += 1;
        }
        if k < m {
            continue;
        }
        k = failure[k - 1];
        let i = j + 1 - m;
        if i < free {
            continue;
        }
        let (first, last) = (&chars[i], &chars[j]);
        let before = i.checked_sub(1).map(|b| &chars[b]);
        let after = chars.get(j + 1);
        if before.is_some_and(|b| continues_cluster(b, first))
            || after.is_some_and(|a| continues_cluster(last, a))
        {
            continue;
        }
        if options.whole_word
            && (before.is_some_and(|b| joins(b, first)) || after.is_some_and(|a| joins(last, a)))
        {
            continue;
        }
        if results.matches.len() == options.max_matches {
            results.truncated = true;
            return Ok(true);
        }
        results.matches.push(to_match(unit, pieces, &chars[i..=j]));
        free = j + 1;
    }
    Ok(false)
}

fn to_match(unit: usize, pieces: &[Piece<'_>], chars: &[Char]) -> FindMatch {
    let mut segments: Vec<MatchSegment> = Vec::new();
    let mut source = SourceSpan {
        start: u64::MAX,
        end: 0,
    };
    let mut exact = true;
    for c in chars {
        let piece = &pieces[c.piece as usize];
        let len = usize::from(c.len);
        let span = piece.source(c.start, len);
        let is_exact = matches!(piece.mapping, super::Mapping::Exact(_));
        let text_start = (piece.text_offset + c.start) as u64;
        let text_end = text_start + len as u64;
        // Whitespace normalization is documented; only non-space characters
        // must map to their own source bytes for an exact match.
        let is_exact = is_exact || c.key == SPACE;
        exact &= is_exact;
        source.start = source.start.min(span.start);
        source.end = source.end.max(span.end);
        match segments.last_mut() {
            Some(s) if s.node == piece.node => {
                s.text_end = s.text_end.max(text_end);
                s.source.start = s.source.start.min(span.start);
                s.source.end = s.source.end.max(span.end);
                s.exact &= is_exact;
            }
            _ => segments.push(MatchSegment {
                node: piece.node,
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
