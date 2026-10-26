//! Disposable FEAS-008 semantic block model.
//!
//! The model is plain owned values: blocks of rendered text with styled runs,
//! block source spans, an outline, link intents and image requests. Hosts never
//! see parser lifetimes. It is shaped like data that could cross the FEAS-006 C
//! ABI, but is linked in-process here because this spike tests presentation,
//! not interop. CORE-002 replaces it.

use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use std::collections::HashMap;
use std::ops::Range;

pub const STYLE_BOLD: u8 = 1;
pub const STYLE_ITALIC: u8 = 2;
pub const STYLE_CODE: u8 = 4;
pub const STYLE_STRIKE: u8 = 8;
pub const STYLE_MARKER: u8 = 16;

/// Largest rendered-text fragment a host should lay out in one row. GTK
/// realizes about 200 list rows regardless of height, so this bounds the text
/// laid out per jump. Runs without a break opportunity use half of it.
pub const FRAGMENT_BYTES: usize = 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockKind {
    Heading(u8),
    Paragraph,
    ListItem,
    Code,
    TableRow,
    Rule,
    Html,
    Math,
    Image,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Run {
    pub range: Range<u32>,
    pub style: u8,
    pub link: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LinkIntent {
    External(String),
    Fragment(String),
    MissingFragment(String),
    LocalDocument(String),
    Blocked(&'static str),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImageRequest {
    /// Normalized relative path to resolve against the document-folder
    /// capability. The host still checks containment after resolving links.
    Local(String),
    Blocked(&'static str),
}

#[derive(Clone, Debug)]
pub struct Block {
    pub kind: BlockKind,
    pub text: String,
    pub runs: Vec<Run>,
    pub source: Range<usize>,
    pub indent: u8,
    pub quote: bool,
    /// Separator placed before this block when copying rendered text.
    pub sep_before: &'static str,
    pub slug: Option<String>,
    pub image: Option<ImageRequest>,
}

#[derive(Clone, Debug)]
pub struct OutlineEntry {
    pub block: u32,
    pub level: u8,
    pub title: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Item {
    pub block: u32,
    pub start: u32,
    pub end: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Pos {
    pub block: u32,
    pub offset: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Match {
    pub block: u32,
    pub start: u32,
    pub end: u32,
}

pub struct Document {
    pub source: String,
    pub blocks: Vec<Block>,
    pub outline: Vec<OutlineEntry>,
    pub links: Vec<String>,
    pub items: Vec<Item>,
    pub first_item: Vec<u32>,
}

pub fn parser_options() -> Options {
    Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_MATH
        | Options::ENABLE_GFM
}

struct Builder<'a> {
    source: &'a str,
    blocks: Vec<Block>,
    links: Vec<String>,
    current: Option<Block>,
    style: u8,
    link: Option<u32>,
    lists: Vec<Option<u64>>,
    quote_depth: u8,
    in_list_item: bool,
    in_table_head: bool,
    cell_count: u32,
    image_depth: u32,
    image_dest: Option<String>,
    only_image: bool,
}

impl<'a> Builder<'a> {
    fn indent(&self) -> u8 {
        (self.lists.len() as u8).saturating_add(self.quote_depth)
    }

    fn open(&mut self, kind: BlockKind, source: Range<usize>, sep: &'static str) {
        self.flush();
        self.current = Some(Block {
            kind,
            text: String::new(),
            runs: Vec::new(),
            source,
            indent: self.indent(),
            quote: self.quote_depth > 0,
            sep_before: sep,
            slug: None,
            image: None,
        });
        self.only_image = true;
    }

    fn flush(&mut self) {
        if let Some(mut block) = self.current.take() {
            if block.kind == BlockKind::Paragraph && self.only_image && block.image.is_some() {
                block.kind = BlockKind::Image;
            } else {
                block.image = None;
            }
            if block.kind == BlockKind::Code && block.text.ends_with('\n') {
                block.text.pop();
            }
            if !block.text.is_empty() || block.kind == BlockKind::Image {
                if self.blocks.is_empty() {
                    block.sep_before = "";
                }
                self.blocks.push(block);
            }
        }
    }

    fn push(&mut self, text: &str, extra: u8) {
        if text.is_empty() {
            return;
        }
        if self.current.is_none() {
            // Text outside a block (tight list item without nested paragraph).
            let kind = if self.in_list_item {
                BlockKind::ListItem
            } else {
                BlockKind::Paragraph
            };
            self.open(kind, 0..0, "\n\n");
        }
        if self.image_depth == 0 && extra & STYLE_MARKER == 0 {
            self.only_image = false;
        }
        let style = self.style | extra;
        let link = self.link;
        let block = self.current.as_mut().unwrap();
        let start = block.text.len() as u32;
        block.text.push_str(text);
        let end = block.text.len() as u32;
        match block.runs.last_mut() {
            Some(last) if last.range.end == start && last.style == style && last.link == link => {
                last.range.end = end;
            }
            _ => block.runs.push(Run {
                range: start..end,
                style,
                link,
            }),
        }
    }
}

fn list_marker(lists: &mut [Option<u64>]) -> String {
    match lists.last_mut() {
        Some(Some(n)) => {
            let marker = format!("{n}. ");
            *n += 1;
            marker
        }
        _ => "• ".to_string(),
    }
}

pub fn slugify(title: &str) -> String {
    let mut slug = String::new();
    for c in title.chars() {
        if c.is_alphanumeric() || c == '_' || c == '-' {
            slug.extend(c.to_lowercase());
        } else if c == ' ' {
            slug.push('-');
        }
    }
    slug
}

/// Classify a link destination without touching the filesystem or network.
pub fn classify_link(dest: &str, slugs: &HashMap<String, u32>) -> LinkIntent {
    let lower = dest.trim().to_ascii_lowercase();
    if let Some(fragment) = dest.strip_prefix('#') {
        return if slugs.contains_key(fragment) {
            LinkIntent::Fragment(fragment.to_string())
        } else {
            LinkIntent::MissingFragment(fragment.to_string())
        };
    }
    if lower.starts_with("https://") || lower.starts_with("http://") || lower.starts_with("mailto:")
    {
        return LinkIntent::External(dest.to_string());
    }
    if lower.contains(':') {
        // javascript:, data:, file:, vbscript: and every other scheme.
        return LinkIntent::Blocked("scheme not allowed");
    }
    match normalize_relative(dest.split('#').next().unwrap_or("")) {
        Some(path) => LinkIntent::LocalDocument(path),
        None => LinkIntent::Blocked("path escapes document folder"),
    }
}

/// Decide whether an image may be requested from the document folder.
pub fn classify_image(dest: &str) -> ImageRequest {
    let lower = dest.trim().to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") || lower.starts_with("//") {
        return ImageRequest::Blocked("remote images disabled");
    }
    if lower.contains(':') {
        return ImageRequest::Blocked("scheme not allowed");
    }
    let Some(path) = normalize_relative(dest) else {
        return ImageRequest::Blocked("path escapes document folder");
    };
    let ext = path.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    if !matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "gif" | "webp") {
        return ImageRequest::Blocked("format not allowed");
    }
    ImageRequest::Local(path)
}

/// Lexically normalize a relative path; reject absolute or escaping paths.
fn normalize_relative(path: &str) -> Option<String> {
    if path.is_empty() || path.starts_with('/') || path.starts_with('\\') {
        return None;
    }
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split(['/', '\\']) {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            other => parts.push(other),
        }
    }
    (!parts.is_empty()).then(|| parts.join("/"))
}

impl Document {
    pub fn parse(source: String) -> Document {
        Document::parse_with(source, FRAGMENT_BYTES)
    }

    pub fn parse_with(source: String, fragment_bytes: usize) -> Document {
        let mut b = Builder {
            source: &source,
            blocks: Vec::new(),
            links: Vec::new(),
            current: None,
            style: 0,
            link: None,
            lists: Vec::new(),
            quote_depth: 0,
            in_list_item: false,
            in_table_head: false,
            cell_count: 0,
            image_depth: 0,
            image_dest: None,
            only_image: true,
        };
        let parser = Parser::new_ext(b.source, parser_options()).into_offset_iter();
        for (event, range) in parser {
            match event {
                Event::Start(tag) => match tag {
                    Tag::Paragraph => {
                        let continuing = b.in_list_item
                            && b.current.as_ref().is_some_and(|c| {
                                c.kind == BlockKind::ListItem
                                    && c.runs.iter().all(|r| r.style & STYLE_MARKER != 0)
                            });
                        if !continuing {
                            b.open(BlockKind::Paragraph, range, "\n\n");
                        }
                    }
                    Tag::Heading { level, .. } => {
                        let n = match level {
                            HeadingLevel::H1 => 1,
                            HeadingLevel::H2 => 2,
                            HeadingLevel::H3 => 3,
                            HeadingLevel::H4 => 4,
                            HeadingLevel::H5 => 5,
                            HeadingLevel::H6 => 6,
                        };
                        b.open(BlockKind::Heading(n), range, "\n\n");
                    }
                    Tag::CodeBlock(_) => b.open(BlockKind::Code, range, "\n\n"),
                    Tag::HtmlBlock => b.open(BlockKind::Html, range, "\n\n"),
                    Tag::BlockQuote(_) => {
                        b.flush();
                        b.quote_depth += 1;
                    }
                    Tag::List(start) => {
                        b.flush();
                        b.lists.push(start);
                    }
                    Tag::Item => {
                        let sep = if b
                            .blocks
                            .last()
                            .is_some_and(|l| l.kind == BlockKind::ListItem)
                        {
                            "\n"
                        } else {
                            "\n\n"
                        };
                        b.open(BlockKind::ListItem, range, sep);
                        b.in_list_item = true;
                        let marker = list_marker(&mut b.lists);
                        b.push(&marker, STYLE_MARKER);
                    }
                    Tag::Table(_) => b.flush(),
                    Tag::TableHead => {
                        b.in_table_head = true;
                        b.open(BlockKind::TableRow, range, "\n\n");
                        b.cell_count = 0;
                    }
                    Tag::TableRow => {
                        b.open(BlockKind::TableRow, range, "\n");
                        b.cell_count = 0;
                    }
                    Tag::TableCell => {
                        if b.cell_count > 0 {
                            b.push("\t", 0);
                        }
                        b.cell_count += 1;
                        if b.in_table_head {
                            b.style |= STYLE_BOLD;
                        }
                    }
                    Tag::Emphasis => b.style |= STYLE_ITALIC,
                    Tag::Strong => b.style |= STYLE_BOLD,
                    Tag::Strikethrough => b.style |= STYLE_STRIKE,
                    Tag::Link { dest_url, .. } => {
                        b.links.push(dest_url.to_string());
                        b.link = Some(b.links.len() as u32 - 1);
                    }
                    Tag::Image { dest_url, .. } => {
                        b.image_depth += 1;
                        b.image_dest = Some(dest_url.to_string());
                    }
                    _ => {}
                },
                Event::End(end) => match end {
                    TagEnd::Paragraph => {
                        if !b.in_list_item {
                            b.flush();
                        }
                    }
                    TagEnd::Heading(_) | TagEnd::CodeBlock | TagEnd::HtmlBlock => b.flush(),
                    TagEnd::BlockQuote(_) => {
                        b.flush();
                        b.quote_depth -= 1;
                    }
                    TagEnd::List(_) => {
                        b.flush();
                        b.lists.pop();
                        b.in_list_item = !b.lists.is_empty();
                    }
                    TagEnd::Item => {
                        b.flush();
                        b.in_list_item = false;
                    }
                    TagEnd::TableHead => {
                        b.flush();
                        b.in_table_head = false;
                    }
                    TagEnd::TableRow | TagEnd::Table => b.flush(),
                    TagEnd::TableCell => {
                        if b.in_table_head {
                            b.style &= !STYLE_BOLD;
                        }
                    }
                    TagEnd::Emphasis => b.style &= !STYLE_ITALIC,
                    TagEnd::Strong => b.style &= !STYLE_BOLD,
                    TagEnd::Strikethrough => b.style &= !STYLE_STRIKE,
                    TagEnd::Link => b.link = None,
                    TagEnd::Image => {
                        b.image_depth -= 1;
                        if let (Some(dest), Some(block)) = (b.image_dest.take(), b.current.as_mut())
                        {
                            if block.image.is_none() {
                                block.image = Some(classify_image(&dest));
                            } else {
                                b.only_image = false;
                            }
                        }
                    }
                    _ => {}
                },
                Event::Text(text) => b.push(&text, 0),
                Event::Code(text) => {
                    b.only_image = false;
                    b.push(&text, STYLE_CODE);
                }
                Event::InlineMath(text) => b.push(&text, STYLE_CODE),
                Event::DisplayMath(text) => {
                    b.open(BlockKind::Math, range, "\n\n");
                    b.push(&text, STYLE_CODE);
                    b.flush();
                }
                // Raw HTML is inert text.
                Event::Html(text) => b.push(&text, STYLE_CODE),
                Event::InlineHtml(text) => b.push(&text, STYLE_CODE),
                Event::SoftBreak => b.push(" ", 0),
                Event::HardBreak => b.push("\n", 0),
                Event::Rule => {
                    b.open(BlockKind::Rule, range, "\n\n");
                    b.push("———", 0);
                    b.flush();
                }
                Event::TaskListMarker(done) => b.push(if done { "☑ " } else { "☐ " }, STYLE_MARKER),
                Event::FootnoteReference(name) => b.push(&format!("[{name}]"), 0),
            }
        }
        b.flush();
        let blocks = b.blocks;
        let links = b.links;
        Document::from_blocks(source, blocks, links, fragment_bytes)
    }

    fn from_blocks(
        source: String,
        mut blocks: Vec<Block>,
        links: Vec<String>,
        fragment_bytes: usize,
    ) -> Document {
        let mut outline = Vec::new();
        let mut seen: HashMap<String, u32> = HashMap::new();
        for (index, block) in blocks.iter_mut().enumerate() {
            if let BlockKind::Heading(level) = block.kind {
                let base = slugify(&block.text);
                let count = seen.entry(base.clone()).or_insert(0);
                let slug = if *count == 0 {
                    base.clone()
                } else {
                    format!("{base}-{count}")
                };
                *count += 1;
                block.slug = Some(slug);
                outline.push(OutlineEntry {
                    block: index as u32,
                    level,
                    title: block.text.clone(),
                });
            }
        }
        let mut items = Vec::new();
        let mut first_item = Vec::with_capacity(blocks.len());
        for (index, block) in blocks.iter().enumerate() {
            first_item.push(items.len() as u32);
            for range in fragments(&block.text, fragment_bytes) {
                items.push(Item {
                    block: index as u32,
                    start: range.start as u32,
                    end: range.end as u32,
                });
            }
        }
        Document {
            source,
            blocks,
            outline,
            links,
            items,
            first_item,
        }
    }

    pub fn slugs(&self) -> HashMap<String, u32> {
        self.blocks
            .iter()
            .enumerate()
            .filter_map(|(i, b)| b.slug.clone().map(|s| (s, i as u32)))
            .collect()
    }

    pub fn start(&self) -> Pos {
        Pos {
            block: 0,
            offset: 0,
        }
    }

    pub fn end(&self) -> Pos {
        match self.blocks.last() {
            Some(last) => Pos {
                block: self.blocks.len() as u32 - 1,
                offset: last.text.len() as u32,
            },
            None => self.start(),
        }
    }

    /// Item that displays `pos`; an offset at a fragment boundary belongs to
    /// the earlier fragment so a selection end stays on the visible row.
    pub fn item_for(&self, pos: Pos) -> u32 {
        let Some(&first) = self.first_item.get(pos.block as usize) else {
            return self.items.len().saturating_sub(1) as u32;
        };
        let mut index = first;
        while let Some(next) = self.items.get(index as usize + 1) {
            if next.block != pos.block || next.start >= pos.offset {
                break;
            }
            index += 1;
        }
        index
    }

    /// Rendered text between two positions, with block separators.
    pub fn text_between(&self, a: Pos, b: Pos) -> String {
        let (start, end) = if a <= b { (a, b) } else { (b, a) };
        let mut out = String::new();
        for index in start.block..=end.block.min(self.blocks.len() as u32 - 1) {
            let block = &self.blocks[index as usize];
            let from = if index == start.block {
                start.offset as usize
            } else {
                0
            };
            let to = if index == end.block {
                end.offset as usize
            } else {
                block.text.len()
            };
            if index != start.block {
                out.push_str(block.sep_before);
            }
            out.push_str(&block.text[from.min(to)..to]);
        }
        out
    }

    /// Markdown source covering the selected blocks. A whole-document
    /// selection returns the source exactly.
    pub fn source_between(&self, a: Pos, b: Pos) -> &str {
        let (start, end) = if a <= b { (a, b) } else { (b, a) };
        if start == self.start() && end == self.end() {
            return &self.source;
        }
        let blocks = &self.blocks[start.block as usize..=end.block as usize];
        let from = blocks.iter().map(|b| b.source.start).min().unwrap_or(0);
        let to = blocks.iter().map(|b| b.source.end).max().unwrap_or(0);
        &self.source[from..to.max(from)]
    }

    /// Byte offset of `pos` inside `text_between(start, end)`.
    pub fn plain_offset(&self, pos: Pos) -> usize {
        let mut offset = 0;
        for (index, block) in self.blocks.iter().enumerate().take(pos.block as usize) {
            if index > 0 {
                offset += block.sep_before.len();
            }
            offset += block.text.len();
        }
        if pos.block > 0 {
            offset += self.blocks[pos.block as usize].sep_before.len();
        }
        offset + pos.offset as usize
    }

    /// Case-insensitive (ASCII folding) search over rendered text.
    pub fn find(&self, query: &str) -> Vec<Match> {
        let needle = query.to_ascii_lowercase();
        let mut matches = Vec::new();
        if needle.is_empty() {
            return matches;
        }
        for (index, block) in self.blocks.iter().enumerate() {
            let hay = block.text.to_ascii_lowercase();
            let mut from = 0;
            while let Some(found) = hay[from..].find(&needle) {
                let start = from + found;
                matches.push(Match {
                    block: index as u32,
                    start: start as u32,
                    end: (start + needle.len()) as u32,
                });
                from = start + needle.len();
            }
        }
        matches
    }
}

/// Split text into display fragments of at most `max` bytes, preferring a
/// newline, then a space; an unbreakable run is cut at a character boundary
/// within `max / 2` bytes.
pub fn fragments(text: &str, max: usize) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let mut start = 0;
    loop {
        let rest = &text[start..];
        if rest.len() <= max / 2 || (rest.len() <= max && rest.contains([' ', '\n'])) {
            break;
        }
        let mut cut = (start + max).min(text.len());
        while !text.is_char_boundary(cut) {
            cut -= 1;
        }
        let window = &text[start..cut];
        let lower = window.len() / 2;
        if let Some(pos) = window.rfind('\n').filter(|&p| p >= lower) {
            cut = start + pos + 1;
        } else if let Some(pos) = window.rfind(' ').filter(|&p| p >= lower) {
            cut = start + pos + 1;
        } else {
            cut = start + (max / 2).max(1);
            while !text.is_char_boundary(cut) {
                cut += 1;
            }
        }
        out.push(start..cut);
        start = cut;
    }
    out.push(start..text.len());
    out
}

#[cfg(feature = "html")]
pub mod html {
    //! HTML for the WebView baseline, with the same inert-HTML, link and
    //! image policy as the native model.
    use super::*;
    use pulldown_cmark::CowStr;

    pub fn render(source: &str) -> String {
        let doc = Document::parse(source.to_string());
        let slugs = doc.slugs();
        let mut headings = doc.blocks.iter().filter_map(|b| b.slug.clone());
        let events = Parser::new_ext(source, parser_options()).map(|event| match event {
            Event::Html(text) | Event::InlineHtml(text) => Event::Code(text),
            Event::Start(Tag::Heading {
                level,
                classes,
                attrs,
                ..
            }) => Event::Start(Tag::Heading {
                level,
                id: headings.next().map(CowStr::from),
                classes,
                attrs,
            }),
            Event::Start(Tag::Link {
                link_type,
                dest_url,
                title,
                id,
            }) => {
                let dest = match classify_link(&dest_url, &slugs) {
                    LinkIntent::External(_)
                    | LinkIntent::Fragment(_)
                    | LinkIntent::LocalDocument(_) => dest_url,
                    _ => CowStr::from(""),
                };
                Event::Start(Tag::Link {
                    link_type,
                    dest_url: dest,
                    title,
                    id,
                })
            }
            Event::Start(Tag::Image {
                link_type,
                dest_url,
                title,
                id,
            }) => {
                let dest = match classify_image(&dest_url) {
                    ImageRequest::Local(path) => CowStr::from(path),
                    ImageRequest::Blocked(_) => CowStr::from(""),
                };
                Event::Start(Tag::Image {
                    link_type,
                    dest_url: dest,
                    title,
                    id,
                })
            }
            other => other,
        });
        let mut out = String::with_capacity(source.len() * 3 / 2);
        pulldown_cmark::html::push_html(&mut out, events);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(text: &str) -> Document {
        Document::parse(text.to_string())
    }

    #[test]
    fn blocks_runs_and_separators() {
        let d = doc("# Title\n\nSome *em* and **strong** [link](#title).\n\n- one\n- two\n");
        let kinds: Vec<_> = d.blocks.iter().map(|b| b.kind).collect();
        assert_eq!(
            kinds,
            [
                BlockKind::Heading(1),
                BlockKind::Paragraph,
                BlockKind::ListItem,
                BlockKind::ListItem
            ]
        );
        assert_eq!(d.blocks[1].text, "Some em and strong link.");
        assert!(d.blocks[1].runs.iter().any(|r| r.style == STYLE_ITALIC));
        assert_eq!(
            d.text_between(d.start(), d.end()),
            "Title\n\nSome em and strong link.\n\n• one\n• two"
        );
        assert_eq!(d.source_between(d.start(), d.end()), d.source);
        assert_eq!(d.outline[0].title, "Title");
        assert_eq!(
            classify_link(&d.links[0], &d.slugs()),
            LinkIntent::Fragment("title".into())
        );
    }

    #[test]
    fn plain_offset_matches_join() {
        let d = doc("# A\n\npara one\n\n1. x\n2. y\n\n| a | b |\n|---|---|\n| 1 | 2 |\n");
        let all = d.text_between(d.start(), d.end());
        for (i, block) in d.blocks.iter().enumerate() {
            let at = d.plain_offset(Pos {
                block: i as u32,
                offset: 0,
            });
            assert_eq!(&all[at..at + block.text.len()], block.text);
        }
        assert!(all.contains("1. x\n2. y"));
        assert!(all.contains("a\tb\n1\t2"));
    }

    #[test]
    fn fragments_bound_long_lines() {
        let line = "x".repeat(131_072);
        let parts = fragments(&line, FRAGMENT_BYTES);
        assert!(parts.iter().all(|r| r.len() <= FRAGMENT_BYTES / 2));
        assert_eq!(parts.last().unwrap().end, line.len());
        let words = "word ".repeat(2000);
        assert!(
            fragments(&words, 100)
                .iter()
                .all(|r| words[r.clone()].ends_with(' ') || r.end == words.len())
        );
        let wide = "é".repeat(5000);
        assert!(
            fragments(&wide, 101)
                .iter()
                .all(|r| wide.is_char_boundary(r.start))
        );
    }

    #[test]
    fn item_lookup_and_selection_text() {
        let d = doc(&format!("{}\n\nnext", "y".repeat(10_000)));
        let half = (FRAGMENT_BYTES / 2) as u32;
        assert_eq!(d.items.len(), 10_000usize.div_ceil(FRAGMENT_BYTES / 2) + 1);
        assert_eq!(
            d.item_for(Pos {
                block: 0,
                offset: 0
            }),
            0
        );
        assert_eq!(
            d.item_for(Pos {
                block: 0,
                offset: half
            }),
            0
        );
        assert_eq!(
            d.item_for(Pos {
                block: 0,
                offset: half + 1
            }),
            1
        );
        assert_eq!(
            d.item_for(Pos {
                block: 1,
                offset: 2
            }),
            d.items.len() as u32 - 1
        );
        assert_eq!(
            d.text_between(
                Pos {
                    block: 1,
                    offset: 4
                },
                Pos {
                    block: 0,
                    offset: 9998
                }
            ),
            "yy\n\nnext"
        );
    }

    #[test]
    fn find_is_case_insensitive() {
        let d = doc("Alpha beta\n\nBETA gamma beta");
        let m = d.find("beta");
        assert_eq!(m.len(), 3);
        assert_eq!(
            m[1],
            Match {
                block: 1,
                start: 0,
                end: 4
            }
        );
    }

    #[test]
    fn link_and_image_policy() {
        let slugs = HashMap::from([("a".to_string(), 0)]);
        assert_eq!(
            classify_link("https://x.test", &slugs),
            LinkIntent::External("https://x.test".into())
        );
        assert_eq!(
            classify_link("#b", &slugs),
            LinkIntent::MissingFragment("b".into())
        );
        assert_eq!(
            classify_link("javascript:alert(1)", &slugs),
            LinkIntent::Blocked("scheme not allowed")
        );
        assert_eq!(
            classify_link("file:///etc/passwd", &slugs),
            LinkIntent::Blocked("scheme not allowed")
        );
        assert_eq!(
            classify_link("../../x.md", &slugs),
            LinkIntent::Blocked("path escapes document folder")
        );
        assert_eq!(
            classify_link("docs/./b.md#s", &slugs),
            LinkIntent::LocalDocument("docs/b.md".into())
        );
        assert_eq!(
            classify_image("img/a.png"),
            ImageRequest::Local("img/a.png".into())
        );
        assert_eq!(
            classify_image("https://x.test/a.png"),
            ImageRequest::Blocked("remote images disabled")
        );
        assert_eq!(
            classify_image("/etc/a.png"),
            ImageRequest::Blocked("path escapes document folder")
        );
        assert_eq!(
            classify_image("a.svg"),
            ImageRequest::Blocked("format not allowed")
        );
        assert_eq!(
            classify_image("data:image/png;base64,AA"),
            ImageRequest::Blocked("scheme not allowed")
        );
    }

    #[test]
    fn standalone_image_becomes_image_block() {
        let d = doc("![alt text](img/a.png)\n\nInline ![x](b.png) image");
        assert_eq!(d.blocks[0].kind, BlockKind::Image);
        assert_eq!(d.blocks[0].text, "alt text");
        assert_eq!(
            d.blocks[0].image,
            Some(ImageRequest::Local("img/a.png".into()))
        );
        assert_eq!(d.blocks[1].kind, BlockKind::Paragraph);
        assert_eq!(d.blocks[1].image, None);
    }

    #[test]
    fn loose_list_items_and_quotes() {
        let d = doc("- one\n\n  more\n\n- two\n\n> quoted\n");
        assert_eq!(d.blocks[0].text, "• one");
        assert_eq!(d.blocks[1].text, "more");
        assert_eq!(d.blocks[2].text, "• two");
        assert!(d.blocks[3].quote);
    }

    #[test]
    fn raw_html_is_inert_text() {
        let d = doc("<script>alert(1)</script>\n\nok <b>x</b>");
        assert_eq!(d.blocks[0].kind, BlockKind::Html);
        assert!(d.blocks[0].text.contains("<script>"));
        assert!(d.blocks[1].text.contains("<b>"));
    }
}
