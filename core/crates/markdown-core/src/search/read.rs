//! Word count, estimated reading time and per-section timing.
use super::unicode::WordCounter;
use super::{CANCEL_CHECK_CHARS, PieceKind, SearchDocument, SearchError, UnitKind, Walker};
use crate::{CancellationToken, NodeKind};

/// markdown-viewer parity (its D-32: the owner's measured reading speed).
pub const DEFAULT_WORDS_PER_MINUTE: u32 = 160;
pub const MIN_WORDS_PER_MINUTE: u32 = 60;
pub const MAX_WORDS_PER_MINUTE: u32 = 600;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReadOptions {
    /// Integer rate in `MIN_WORDS_PER_MINUTE..=MAX_WORDS_PER_MINUTE`.
    pub words_per_minute: u32,
    /// Count heading text (in the section the heading starts).
    pub include_headings: bool,
    /// Count code spans and code blocks (advanced-engine fences never count).
    pub include_code: bool,
}
impl Default for ReadOptions {
    fn default() -> Self {
        Self {
            words_per_minute: DEFAULT_WORDS_PER_MINUTE,
            include_headings: false,
            include_code: false,
        }
    }
}

/// One reading section: the intro before the first heading, then one per
/// heading in reading order (every level, including nested containers).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SectionStats {
    /// Heading node index; `None` for the intro.
    pub heading: Option<usize>,
    /// Matching outline entry index; `None` for the intro.
    pub outline_index: Option<usize>,
    /// Heading level 1–6; 0 for the intro.
    pub level: u8,
    /// Words up to the next heading of any level.
    pub words: u64,
    pub seconds: u64,
    /// Words including deeper subsections, up to the next heading of the
    /// same or a higher level. Equals `words` for the intro.
    pub cumulative_words: u64,
    pub cumulative_seconds: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReadStats {
    pub words: u64,
    /// Estimated seconds for `words` (not a sum of rounded sections).
    pub seconds: u64,
    pub words_per_minute: u32,
    /// Intro first (always present, possibly zero words), then headings.
    pub sections: Vec<SectionStats>,
}

/// `round(words * 60 / wpm)` with halves rounded up; 0 when `wpm` is 0.
pub fn estimated_seconds(words: u64, words_per_minute: u32) -> u64 {
    if words_per_minute == 0 {
        return 0;
    }
    let wpm = u128::from(words_per_minute);
    ((u128::from(words) * 120 + wpm) / (2 * wpm)) as u64
}

/// [`read_stats_cancellable`] without cancellation.
pub fn read_stats(
    doc: &SearchDocument<'_>,
    options: &ReadOptions,
) -> Result<ReadStats, SearchError> {
    read_stats_cancellable(doc, options, &CancellationToken::new())
}

/// Count words per section. Cancellation returns `Err(Cancelled)` only.
pub fn read_stats_cancellable(
    doc: &SearchDocument<'_>,
    options: &ReadOptions,
    token: &CancellationToken,
) -> Result<ReadStats, SearchError> {
    token.check()?;
    let wpm = options.words_per_minute;
    if !(MIN_WORDS_PER_MINUTE..=MAX_WORDS_PER_MINUTE).contains(&wpm) {
        return Err(SearchError::InvalidOptions);
    }
    let walker = Walker::new(*doc);
    let nodes = doc.nodes();
    let mut sections = vec![section(None, None, 0)];
    let mut headings = 0usize;
    let mut pieces = Vec::new();
    let mut budget = 0usize;
    for (unit, kind) in walker.units() {
        token.check()?;
        let counted = match kind {
            UnitKind::Heading => {
                let level = match nodes[unit].content {
                    NodeKind::Heading { level, .. } => level,
                    _ => 1,
                };
                sections.push(section(Some(unit), Some(headings), level));
                headings += 1;
                options.include_headings
            }
            UnitKind::CodeBlock => options.include_code,
            UnitKind::Unsupported => false,
            UnitKind::Paragraph | UnitKind::TableCell => true,
        };
        if !counted {
            continue;
        }
        walker.pieces(unit, &mut pieces);
        let mut counter = WordCounter::default();
        for piece in &pieces {
            let include = match piece.kind {
                PieceKind::Text | PieceKind::CodeBlock => true,
                PieceKind::CodeSpan => options.include_code,
                PieceKind::Break => {
                    counter.separate();
                    continue;
                }
                PieceKind::Alt | PieceKind::Raw | PieceKind::Separator => false,
            };
            if !include {
                counter.separate();
                continue;
            }
            for c in piece.text.chars() {
                budget += 1;
                if budget >= CANCEL_CHECK_CHARS {
                    budget = 0;
                    token.check()?;
                }
                counter.push(c);
            }
        }
        if let Some(current) = sections.last_mut() {
            current.words += counter.words;
        }
    }
    // The validator guarantees one outline entry per heading in reading
    // order; drop the link defensively if a document disagrees.
    let outline = doc.outline();
    for s in &mut sections {
        if let (Some(node), Some(i)) = (s.heading, s.outline_index)
            && outline.get(i).map(|e| &e.node) != Some(&nodes[node].id)
        {
            s.outline_index = None;
        }
    }
    cumulate(&mut sections);
    let words = sections.iter().map(|s| s.words).sum();
    for s in &mut sections {
        s.seconds = estimated_seconds(s.words, wpm);
        s.cumulative_seconds = estimated_seconds(s.cumulative_words, wpm);
    }
    Ok(ReadStats {
        words,
        seconds: estimated_seconds(words, wpm),
        words_per_minute: wpm,
        sections,
    })
}

fn section(heading: Option<usize>, outline_index: Option<usize>, level: u8) -> SectionStats {
    SectionStats {
        heading,
        outline_index,
        level,
        words: 0,
        seconds: 0,
        cumulative_words: 0,
        cumulative_seconds: 0,
    }
}

/// Each heading accumulates the sections after it until a heading of the
/// same or higher level; at most six levels are open at once.
fn cumulate(sections: &mut [SectionStats]) {
    let mut open: Vec<usize> = Vec::new();
    for i in 0..sections.len() {
        let level = sections[i].level;
        sections[i].cumulative_words = sections[i].words;
        if level == 0 {
            continue;
        }
        open.retain(|&o| sections[o].level < level);
        let words = sections[i].words;
        for &o in &open {
            sections[o].cumulative_words += words;
        }
        open.push(i);
    }
}
