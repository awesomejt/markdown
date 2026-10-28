//! Bounded one-line visible-text excerpts around find matches.
use crate::CancellationToken;
use crate::search::{CANCEL_CHECK_CHARS, Each, FindMatch, Piece, PieceKind, Walker, is_format};
use std::collections::VecDeque;

/// Matched characters kept in a snippet; a longer match is clipped and the
/// snippet ends there with `trailing_more` set.
pub const MAX_SNIPPET_MATCH_CHARS: usize = 256;

/// Visible text around one match: whitespace runs, breaks and footnote
/// references collapse to one space and format characters are dropped, as in
/// find. `text[match_start..match_end]` is the (possibly clipped) match.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snippet {
    pub text: String,
    pub match_start: u64,
    pub match_end: u64,
    /// Visible text of the same block precedes `text`.
    pub leading_more: bool,
    /// Visible text of the same block follows `text`, or the match was clipped.
    pub trailing_more: bool,
}

/// Display-text position of one character: node index and byte offset.
type CharId = (usize, usize);

struct Pending {
    text: String,
    match_start: usize,
    match_end: Option<usize>,
    match_chars: usize,
    clipped: bool,
    after: usize,
    leading_more: bool,
    trailing_more: bool,
    done: bool,
}
impl Pending {
    fn finish(mut self) -> Snippet {
        let match_end = self.match_end.unwrap_or(self.text.len());
        let kept = self.text[match_end..].trim_end().len();
        self.text.truncate(match_end + kept);
        Snippet {
            text: self.text,
            match_start: self.match_start as u64,
            match_end: match_end as u64,
            leading_more: self.leading_more,
            trailing_more: self.trailing_more,
        }
    }
}

/// Streams one unit's pieces, opening a snippet at each match start.
struct Builder<'m> {
    starts: Vec<CharId>,
    ends: Vec<CharId>,
    matches: &'m [FindMatch],
    context: usize,
    ring: VecDeque<char>,
    emitted: usize,
    last_space: bool,
    next: usize,
    /// Open snippets with their match index.
    active: Vec<(usize, Pending)>,
    out: Vec<Option<Snippet>>,
    budget: usize,
    token: &'m CancellationToken,
    cancelled: bool,
}
impl<'m> Builder<'m> {
    fn new(matches: &'m [FindMatch], context: usize, token: &'m CancellationToken) -> Self {
        let first = |m: &FindMatch| m.segments.first().map(|s| (s.node, s.text_start as usize));
        let last = |m: &FindMatch| m.segments.last().map(|s| (s.node, s.text_end as usize));
        Self {
            starts: matches
                .iter()
                .map(|m| first(m).unwrap_or_default())
                .collect(),
            ends: matches
                .iter()
                .map(|m| last(m).unwrap_or_default())
                .collect(),
            matches,
            context,
            ring: VecDeque::with_capacity(context + 1),
            emitted: 0,
            last_space: true,
            next: 0,
            active: Vec::new(),
            out: vec![None; matches.len()],
            budget: 0,
            token,
            cancelled: false,
        }
    }
    fn finished(&self) -> bool {
        self.cancelled || (self.next == self.matches.len() && self.active.is_empty())
    }
    fn piece(&mut self, piece: &Piece<'_>) {
        if self.finished() {
            return;
        }
        match piece.kind {
            PieceKind::Separator | PieceKind::Break => self.emit(' ', None),
            _ => {
                for (start, c) in piece.text.char_indices() {
                    self.budget += 1;
                    if self.budget >= CANCEL_CHECK_CHARS {
                        self.budget = 0;
                        self.cancelled |= self.token.is_cancelled();
                    }
                    if self.finished() {
                        return;
                    }
                    let id = (piece.node, piece.text_offset + start);
                    if c.is_whitespace() {
                        self.emit(' ', None);
                    } else if c.is_control() {
                        // Matchable in find, so it keeps its identity; shown
                        // as a replacement character to keep one clean line.
                        self.emit('\u{FFFD}', Some((id, c.len_utf8())));
                    } else if !is_format(c) {
                        self.emit(c, Some((id, c.len_utf8())));
                    }
                }
            }
        }
    }
    /// `id` is the character's position and UTF-8 length; `None` for spaces.
    fn emit(&mut self, c: char, id: Option<(CharId, usize)>) {
        if c == ' ' {
            if self.last_space {
                return;
            }
            self.last_space = true;
        } else {
            self.last_space = false;
        }
        let ends = &self.ends;
        let context = self.context;
        for (index, pending) in &mut self.active {
            if pending.match_end.is_none() {
                if pending.match_chars < MAX_SNIPPET_MATCH_CHARS {
                    pending.text.push(c);
                    pending.match_chars += 1;
                } else {
                    pending.clipped = true;
                }
                if id.is_some_and(|((n, at), len)| (n, at + len) == ends[*index]) {
                    pending.match_end = Some(pending.text.len());
                    if pending.clipped {
                        pending.trailing_more = true;
                        pending.done = true;
                    }
                }
            } else if pending.after < context {
                pending.text.push(c);
                pending.after += 1;
            } else {
                pending.trailing_more = true;
                pending.done = true;
            }
        }
        self.close_done();
        if let Some((char_id, len)) = id
            && self.next < self.matches.len()
            && char_id == self.starts[self.next]
        {
            let mut text: String = self.ring.iter().collect();
            let leading_more = self.emitted > self.ring.len();
            if text.starts_with(' ') {
                text.remove(0);
            }
            let match_start = text.len();
            text.push(c);
            let (n, at) = char_id;
            let single = (n, at + len) == self.ends[self.next];
            let pending = Pending {
                match_end: single.then_some(text.len()),
                text,
                match_start,
                match_chars: 1,
                clipped: false,
                after: 0,
                leading_more,
                trailing_more: false,
                done: false,
            };
            self.active.push((self.next, pending));
            self.next += 1;
        }
        if self.context > 0 {
            if self.ring.len() == self.context {
                self.ring.pop_front();
            }
            self.ring.push_back(c);
        }
        self.emitted += 1;
    }
    fn close_done(&mut self) {
        let mut i = 0;
        while i < self.active.len() {
            if self.active[i].1.done {
                let (index, pending) = self.active.swap_remove(i);
                self.out[index] = Some(pending.finish());
            } else {
                i += 1;
            }
        }
    }
    fn finish(mut self) -> Vec<Snippet> {
        for (index, pending) in std::mem::take(&mut self.active) {
            self.out[index] = Some(pending.finish());
        }
        self.out
            .into_iter()
            .map(|s| {
                s.unwrap_or(Snippet {
                    text: String::new(),
                    match_start: 0,
                    match_end: 0,
                    leading_more: false,
                    trailing_more: false,
                })
            })
            .collect()
    }
}

/// Snippets for `matches` (reading order, as returned by find), one per
/// match. Each unit with matches is walked once; units without are skipped.
/// Cost is O(visible text of matched units + matches × (context + match)).
pub(crate) fn snippets(
    walker: &Walker<'_>,
    matches: &[FindMatch],
    context: usize,
    token: &CancellationToken,
) -> Result<Vec<Snippet>, crate::Cancelled> {
    let mut out = Vec::with_capacity(matches.len());
    let mut rest = matches;
    while let Some(first) = rest.first() {
        token.check()?;
        let len = rest.iter().take_while(|m| m.unit == first.unit).count();
        let (group, tail) = rest.split_at(len);
        let mut builder = Builder::new(group, context, token);
        walker.pieces(
            first.unit,
            &mut Each(|piece: Piece<'_>| builder.piece(&piece)),
        );
        if builder.cancelled {
            return Err(crate::Cancelled);
        }
        out.extend(builder.finish());
        rest = tail;
    }
    Ok(out)
}
