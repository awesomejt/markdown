//! Document find and reading metadata over one immutable snapshot.
//!
//! Everything here is pure: no I/O, threads, clocks, locale data or new
//! dependencies. Both functions walk the validated semantic tree of a v1 or v2
//! document in reading order, using the same visible-text extraction, and map
//! results back to exact UTF-8 source byte ranges and node indices. See
//! `specs/search/README.md` for the full contract and its Unicode limits.
use crate::session::Snapshot;
use crate::*;
use std::collections::HashMap;

mod find;
mod read;
mod unicode;

pub use find::{
    DEFAULT_MAX_MATCHES, FindMatch, FindOptions, FindResults, MAX_MATCHES, MAX_QUERY_BYTES,
    MatchSegment, find, find_cancellable,
};
pub(crate) use find::{kmp_table, normalize_query};
pub use read::{
    DEFAULT_WORDS_PER_MINUTE, MAX_WORDS_PER_MINUTE, MIN_WORDS_PER_MINUTE, ReadOptions, ReadStats,
    SectionStats, estimated_seconds, read_stats, read_stats_cancellable,
};
pub use unicode::{fold_case, is_format, word_count};

/// Typed failures. None carries document contents or query text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SearchError {
    /// The supplied source is not the one the document was validated against.
    SourceMismatch,
    /// The raw query exceeds [`MAX_QUERY_BYTES`].
    QueryTooLong,
    /// An option is outside its documented range.
    InvalidOptions,
    /// The cancellation token was observed; no partial result is returned.
    Cancelled,
}
impl std::fmt::Display for SearchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::SourceMismatch => "source_mismatch",
            Self::QueryTooLong => "query_too_long",
            Self::InvalidOptions => "invalid_options",
            Self::Cancelled => "cancelled",
        })
    }
}
impl std::error::Error for SearchError {}
impl From<Cancelled> for SearchError {
    fn from(_: Cancelled) -> Self {
        Self::Cancelled
    }
}

/// Characters processed between cancellation polls.
pub(crate) const CANCEL_CHECK_CHARS: usize = 16 * 1024;

/// A borrowed, validated document plus the exact source its spans refer to.
/// Node indices in results index `nodes()` (the model's node array).
#[derive(Clone, Copy, Debug)]
pub struct SearchDocument<'a> {
    source: &'a str,
    roots: &'a [NodeId],
    nodes: &'a [Node],
    outline: &'a [OutlineEntry],
}
impl<'a> SearchDocument<'a> {
    /// A published session snapshot already owns its exact source.
    pub fn from_snapshot(snapshot: &'a Snapshot) -> Self {
        let (roots, nodes) = snapshot.structure();
        let outline = match snapshot.model() {
            crate::session::SnapshotModel::V1(d) => &d.input().outline[..],
            #[cfg(feature = "model-v2")]
            crate::session::SnapshotModel::V2(d) => &d.input().outline[..],
        };
        Self {
            source: snapshot.source(),
            roots,
            nodes,
            outline,
        }
    }
    /// Binds a validated v1 document to its source. The length and SHA-256
    /// fingerprint must match (an O(n) hash), otherwise `SourceMismatch`.
    pub fn from_v1(document: &'a Document, source: &'a str) -> Result<Self, SearchError> {
        let input = document.input();
        Self::bind(
            &input.source,
            &input.roots,
            &input.nodes,
            &input.outline,
            source,
        )
    }
    /// [`from_v1`](Self::from_v1) for a validated v2 document.
    #[cfg(feature = "model-v2")]
    pub fn from_v2(
        document: &'a crate::v2::Document,
        source: &'a str,
    ) -> Result<Self, SearchError> {
        let input = document.input();
        Self::bind(
            &input.source,
            &input.roots,
            &input.nodes,
            &input.outline,
            source,
        )
    }
    fn bind(
        info: &SourceInfo,
        roots: &'a [NodeId],
        nodes: &'a [Node],
        outline: &'a [OutlineEntry],
        source: &'a str,
    ) -> Result<Self, SearchError> {
        if info.bytes != source.len() as u64 || *info != SourceInfo::of(source) {
            return Err(SearchError::SourceMismatch);
        }
        Ok(Self {
            source,
            roots,
            nodes,
            outline,
        })
    }
    pub fn source(&self) -> &'a str {
        self.source
    }
    pub fn nodes(&self) -> &'a [Node] {
        self.nodes
    }
    pub fn outline(&self) -> &'a [OutlineEntry] {
        self.outline
    }
}

/// Leaf text containers ("units"). Matches and words never cross units.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum UnitKind {
    Paragraph,
    Heading,
    TableCell,
    CodeBlock,
    Unsupported,
}

/// How a piece of visible text maps back to source bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Mapping {
    /// `text` equals `source[start..start + text.len()]` byte for byte.
    Exact(usize),
    /// `text` is derived (entity, code-span/line normalization, alt with
    /// markup); any part of it maps to this whole span.
    Atomic(SourceSpan),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PieceKind {
    Text,
    CodeSpan,
    CodeBlock,
    Alt,
    /// Unsupported/raw source shown as literal text.
    Raw,
    /// Soft/hard break: visible whitespace with no text value.
    Break,
    /// Footnote reference: visible, but not searchable or countable text.
    Separator,
}

/// A run of one node's display text with its source mapping.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Piece<'a> {
    pub node: usize,
    pub kind: PieceKind,
    pub text: &'a str,
    /// Byte offset of `text` within the node's display text.
    pub text_offset: usize,
    pub mapping: Mapping,
}
impl Piece<'_> {
    /// Source range of `text[start..start + len]`; always UTF-8 boundaries.
    pub fn source(&self, start: usize, len: usize) -> SourceSpan {
        match self.mapping {
            Mapping::Exact(base) => SourceSpan {
                start: (base + start) as u64,
                end: (base + start + len) as u64,
            },
            Mapping::Atomic(span) => span,
        }
    }
}

/// Receives a unit's pieces in reading order, one at a time, so callers can
/// stream them without retaining the whole unit.
pub(crate) trait PieceSink<'a> {
    fn piece(&mut self, piece: Piece<'a>);
}
/// Adapts a closure to [`PieceSink`].
pub(crate) struct Each<F>(pub F);
impl<'a, F: FnMut(Piece<'a>)> PieceSink<'a> for Each<F> {
    fn piece(&mut self, piece: Piece<'a>) {
        (self.0)(piece)
    }
}

pub(crate) struct Walker<'a> {
    doc: SearchDocument<'a>,
    index: HashMap<&'a str, usize>,
}
impl<'a> Walker<'a> {
    pub fn new(doc: SearchDocument<'a>) -> Self {
        let index = doc
            .nodes
            .iter()
            .enumerate()
            .map(|(i, n)| (n.id.0.as_str(), i))
            .collect();
        Self { doc, index }
    }
    fn lookup(&self, id: &NodeId) -> Option<usize> {
        self.index.get(id.0.as_str()).copied()
    }
    /// Leaf units in reading order (iterative; bounded by the node count).
    pub fn units(&self) -> Vec<(usize, UnitKind)> {
        let mut out = Vec::new();
        let mut stack: Vec<&NodeId> = self.doc.roots.iter().rev().collect();
        while let Some(id) = stack.pop() {
            let Some(i) = self.lookup(id) else { continue };
            let kind = match &self.doc.nodes[i].content {
                NodeKind::Paragraph { .. } => UnitKind::Paragraph,
                NodeKind::Heading { .. } => UnitKind::Heading,
                NodeKind::TableCell { .. } => UnitKind::TableCell,
                NodeKind::CodeBlock { .. } => UnitKind::CodeBlock,
                NodeKind::UnsupportedBlock { .. } => UnitKind::Unsupported,
                NodeKind::BlockQuote { .. }
                | NodeKind::List { .. }
                | NodeKind::ListItem { .. }
                | NodeKind::Table { .. }
                | NodeKind::TableRow { .. }
                | NodeKind::FootnoteDefinition { .. }
                | NodeKind::Alert { .. } => {
                    stack.extend(self.doc.nodes[i].content.children().iter().rev());
                    continue;
                }
                _ => continue,
            };
            out.push((i, kind));
        }
        out
    }
    /// Append the visible-text pieces of one unit in reading order.
    pub fn pieces(&self, unit: usize, out: &mut impl PieceSink<'a>) {
        let source = self.doc.source;
        let node = &self.doc.nodes[unit];
        match &node.content {
            NodeKind::CodeBlock { info, text } => {
                code_block_pieces(source, unit, node.span, info.is_some(), text, out)
            }
            NodeKind::UnsupportedBlock { .. } => raw_piece(source, unit, node.span, out),
            content => {
                let mut stack: Vec<&NodeId> = content.children().iter().rev().collect();
                while let Some(id) = stack.pop() {
                    let Some(i) = self.lookup(id) else { continue };
                    let child = &self.doc.nodes[i];
                    match &child.content {
                        NodeKind::Text { text } => out.piece(Piece {
                            node: i,
                            kind: PieceKind::Text,
                            text,
                            text_offset: 0,
                            mapping: if child.span.slice(source) == Some(text.as_str()) {
                                Mapping::Exact(child.span.start as usize)
                            } else {
                                Mapping::Atomic(child.span)
                            },
                        }),
                        NodeKind::CodeSpan { text } => out.piece(Piece {
                            node: i,
                            kind: PieceKind::CodeSpan,
                            text,
                            text_offset: 0,
                            mapping: code_span_mapping(source, child.span, text),
                        }),
                        NodeKind::SoftBreak {} | NodeKind::HardBreak {} => {
                            out.piece(empty(i, PieceKind::Break, child.span))
                        }
                        NodeKind::FootnoteReference { .. } => {
                            out.piece(empty(i, PieceKind::Separator, child.span))
                        }
                        NodeKind::Image { alt, .. } => out.piece(Piece {
                            node: i,
                            kind: PieceKind::Alt,
                            text: alt,
                            text_offset: 0,
                            mapping: alt_mapping(source, child.span, alt),
                        }),
                        NodeKind::UnsupportedInline { .. } => raw_piece(source, i, child.span, out),
                        other => stack.extend(other.children().iter().rev()),
                    }
                }
            }
        }
    }
}

fn empty(node: usize, kind: PieceKind, span: SourceSpan) -> Piece<'static> {
    Piece {
        node,
        kind,
        text: "",
        text_offset: 0,
        mapping: Mapping::Atomic(span),
    }
}
fn raw_piece<'a>(source: &'a str, node: usize, span: SourceSpan, out: &mut impl PieceSink<'a>) {
    match span.slice(source) {
        Some(text) => out.piece(Piece {
            node,
            kind: PieceKind::Raw,
            text,
            text_offset: 0,
            mapping: Mapping::Exact(span.start as usize),
        }),
        None => out.piece(empty(node, PieceKind::Separator, span)),
    }
}
/// Code span text is the content between equal backtick runs, minus one
/// stripped space on each side; line endings become spaces (then atomic).
fn code_span_mapping(source: &str, span: SourceSpan, text: &str) -> Mapping {
    let Some(slice) = span.slice(source) else {
        return Mapping::Atomic(span);
    };
    let ticks = slice.len() - slice.trim_start_matches('`').len();
    if ticks > 0 && slice.len() >= 2 * ticks && slice.ends_with(&slice[..ticks]) {
        let inner = &slice[ticks..slice.len() - ticks];
        if inner == text {
            return Mapping::Exact(span.start as usize + ticks);
        }
        if inner.len() >= 2
            && inner.starts_with(' ')
            && inner.ends_with(' ')
            && &inner[1..inner.len() - 1] == text
        {
            return Mapping::Exact(span.start as usize + ticks + 1);
        }
    }
    Mapping::Atomic(span)
}
/// Plain alt text directly follows `![`; alt derived from markup is atomic.
fn alt_mapping(source: &str, span: SourceSpan, alt: &str) -> Mapping {
    match span.slice(source).and_then(|s| s.strip_prefix("![")) {
        Some(rest) if rest.starts_with(alt) && rest[alt.len()..].starts_with(']') => {
            Mapping::Exact(span.start as usize + 2)
        }
        _ => Mapping::Atomic(span),
    }
}
/// Map each code-block text line to the suffix of its source line (after any
/// fence, indentation or container prefix). Lines that are not an exact
/// suffix (for example expanded tabs) map atomically to that source line;
/// LF line endings are exact and CRLF (normalized to LF) map atomically to
/// the source line ending.
/// Fencing comes from the model (`info` is present only for fenced blocks),
/// never from the text: an indented block may itself start with ``` .
fn code_block_pieces<'a>(
    source: &'a str,
    node: usize,
    span: SourceSpan,
    fenced: bool,
    text: &'a str,
    out: &mut impl PieceSink<'a>,
) {
    let (Some(block), Ok(base), Ok(end)) = (
        span.slice(source),
        usize::try_from(span.start),
        usize::try_from(span.end),
    ) else {
        out.piece(Piece {
            node,
            kind: PieceKind::CodeBlock,
            text,
            text_offset: 0,
            mapping: Mapping::Atomic(span),
        });
        return;
    };
    let mut cursor = base;
    let opener = block.trim_start_matches([' ', '\t']);
    if fenced && (opener.starts_with("```") || opener.starts_with("~~~")) {
        cursor = block.find('\n').map_or(end, |p| base + p + 1);
    }
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        let content = line.strip_suffix('\n').unwrap_or(line);
        let line_end = source[cursor..end].find('\n').map_or(end, |p| cursor + p);
        let row = &source[cursor..line_end];
        let row = row.strip_suffix('\r').unwrap_or(row);
        let row_end = cursor + row.len();
        let mapping = if cursor < end && row.ends_with(content) {
            Mapping::Exact(row_end - content.len())
        } else if cursor < end {
            Mapping::Atomic(SourceSpan {
                start: cursor as u64,
                end: row_end as u64,
            })
        } else {
            Mapping::Atomic(span)
        };
        out.piece(Piece {
            node,
            kind: PieceKind::CodeBlock,
            text: content,
            text_offset: offset,
            mapping,
        });
        if content.len() < line.len() {
            let eol_end = if line_end < end { line_end + 1 } else { end };
            out.piece(Piece {
                node,
                kind: PieceKind::CodeBlock,
                text: &line[content.len()..],
                text_offset: offset + content.len(),
                mapping: if cursor < end && source.get(row_end..eol_end) == Some("\n") {
                    Mapping::Exact(row_end)
                } else if cursor < end {
                    Mapping::Atomic(SourceSpan {
                        start: row_end as u64,
                        end: eol_end as u64,
                    })
                } else {
                    Mapping::Atomic(span)
                },
            });
        }
        offset += line.len();
        cursor = if line_end < end { line_end + 1 } else { end };
    }
}
