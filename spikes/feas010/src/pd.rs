//! pulldown-cmark adapter: walks the offset event stream into `Raw`.

use crate::common::{AvColumn, Block, BlockKind, Inl, MathForm, Raw, Span};
use pulldown_cmark::{BlockQuoteKind, CodeBlockKind, Event, Options, Parser, Tag, TagEnd};

pub fn options() -> Options {
    Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_GFM
        | Options::ENABLE_MATH
}

/// Parse and walk only, as in FEAS-002.
pub fn parse_only(source: &str) -> usize {
    let base = crate::common::front_matter_span(source).map_or(0, |s| s.end);
    Parser::new_ext(&source[base..], options())
        .into_offset_iter()
        .count()
}

fn quote_kind(k: BlockQuoteKind) -> &'static str {
    match k {
        BlockQuoteKind::Note => "NOTE",
        BlockQuoteKind::Tip => "TIP",
        BlockQuoteKind::Important => "IMPORTANT",
        BlockQuoteKind::Warning => "WARNING",
        BlockQuoteKind::Caution => "CAUTION",
    }
}

#[derive(Default)]
struct Walk {
    raw: Raw,
    block: Option<Block>,
    heading: Option<(u8, String, Span)>,
    in_item: usize,
    opaque: usize,
    strong: Option<(Span, String, usize)>,
    in_head: bool,
    header: Vec<String>,
    av: Option<(usize, usize)>,
    col: usize,
}

impl Walk {
    fn close(&mut self) {
        if let Some(b) = self.block.take() {
            self.raw.blocks.push(b);
        }
    }

    /// Tight list items have no paragraph events: open an implicit block.
    fn ensure_block(&mut self, range: &Span) {
        if self.block.is_none() && self.in_item > 0 {
            self.block = Some(Block::new(BlockKind::Paragraph, range.clone()));
        }
    }

    fn inline(&mut self, item: Inl, range: &Span) {
        if self.opaque > 0 || self.strong.is_some() || self.in_head {
            return;
        }
        self.ensure_block(range);
        if let Some(b) = &mut self.block {
            b.span.end = b.span.end.max(range.end);
            b.inl.push(item);
        }
    }

    fn text(&mut self, source: &str, text: &str, range: Span) {
        if let Some((_, h, _)) = &mut self.heading {
            h.push_str(text);
        }
        if self.in_head {
            if let Some(last) = self.header.last_mut() {
                last.push_str(text);
            }
            return;
        }
        if let Some((_, s, _)) = &mut self.strong {
            s.push_str(text);
            return;
        }
        if self.opaque > 0 {
            return;
        }
        self.ensure_block(&range);
        if let Some(b) = &mut self.block {
            b.span.end = b.span.end.max(range.end);
            b.push_text(source, range);
        }
    }
}

/// pulldown-cmark's YAML metadata option also accepts `---` blocks mid-document,
/// which turns a thematic break plus setext heading into hidden metadata. So
/// front matter is detected here and only the remainder is parsed, with every
/// range shifted back to source offsets.
pub fn extract(source: &str) -> Raw {
    let mut w = Walk::default();
    w.raw.front_matter = crate::common::front_matter_span(source);
    let base = w.raw.front_matter.as_ref().map_or(0, |s| s.end);
    for (event, range) in Parser::new_ext(&source[base..], options()).into_offset_iter() {
        let range = range.start + base..range.end + base;
        match event {
            Event::Start(tag) => match tag {
                Tag::Paragraph => {
                    w.close();
                    w.block = Some(Block::new(BlockKind::Paragraph, range));
                }
                Tag::Heading { level, .. } => {
                    w.close();
                    w.heading = Some((level as u8, String::new(), range.clone()));
                    w.block = Some(Block::new(BlockKind::Heading, range));
                }
                Tag::CodeBlock(kind) => {
                    w.close();
                    if let CodeBlockKind::Fenced(info) = kind {
                        let lang = info.split_whitespace().next().map(String::from);
                        w.raw.fences.push((lang, range));
                    }
                    w.opaque += 1;
                }
                Tag::Link { dest_url, .. } => {
                    w.inline(Inl::Opaque, &range);
                    w.raw.links.push((dest_url.to_string(), range));
                    w.opaque += 1;
                }
                Tag::Image { .. } => {
                    w.inline(Inl::Opaque, &range);
                    w.opaque += 1;
                }
                Tag::BlockQuote(kind) => {
                    w.close();
                    match kind {
                        Some(k) => w.raw.alerts.push((quote_kind(k).into(), range)),
                        None => w.raw.blockquotes.push(range),
                    }
                }
                Tag::Item => {
                    w.close();
                    w.in_item += 1;
                }
                Tag::List(_) => w.close(),
                Tag::Table(_) => {
                    w.close();
                    w.raw.tables += 1;
                    w.av = None;
                }
                Tag::TableHead => {
                    w.in_head = true;
                    w.header.clear();
                }
                Tag::TableRow => w.col = 0,
                Tag::TableCell => {
                    if w.in_head {
                        w.header.push(String::new());
                    } else {
                        let av = w.av.and_then(|(v, a)| {
                            (w.col == v)
                                .then_some(AvColumn::Visual)
                                .or((w.col == a).then_some(AvColumn::Audio))
                        });
                        w.block = Some(Block::new(BlockKind::Cell { av }, range));
                    }
                }
                Tag::FootnoteDefinition(_) => {
                    w.close();
                    w.raw.footnote_definitions.push(range);
                }
                Tag::HtmlBlock => {
                    w.close();
                    w.raw.html_constructs += 1;
                }
                Tag::Strong => {
                    if w.strong.is_none() && w.opaque == 0 && !w.in_head {
                        w.ensure_block(&range);
                        w.strong = Some((range, String::new(), 0));
                    } else if let Some((_, _, depth)) = &mut w.strong {
                        *depth += 1;
                    }
                }
                _ => {}
            },
            Event::End(tag) => match tag {
                TagEnd::Paragraph | TagEnd::TableCell => {
                    w.close();
                    if tag == TagEnd::TableCell {
                        w.col += 1;
                    }
                }
                TagEnd::Heading(_) => {
                    if let Some(h) = w.heading.take() {
                        w.raw.headings.push(h);
                    }
                    w.close();
                }
                TagEnd::CodeBlock | TagEnd::Link | TagEnd::Image => w.opaque -= 1,
                TagEnd::Item => {
                    w.close();
                    w.in_item -= 1;
                }
                TagEnd::TableHead => {
                    w.in_head = false;
                    let find = |name: &str| {
                        w.header
                            .iter()
                            .position(|h| h.trim().eq_ignore_ascii_case(name))
                    };
                    if let (Some(v), Some(a)) = (find("visual"), find("audio")) {
                        w.av = Some((v, a));
                        w.raw.av_tables += 1;
                    }
                }
                TagEnd::Strong => match &mut w.strong {
                    Some((_, _, depth)) if *depth > 0 => *depth -= 1,
                    Some(_) => {
                        let (span, text, _) = w.strong.take().unwrap();
                        w.inline(Inl::Strong { span, text }, &range);
                    }
                    None => {}
                },
                _ => {}
            },
            Event::Text(t) => w.text(source, &t, range),
            Event::Code(t) => {
                if let Some((_, h, _)) = &mut w.heading {
                    h.push_str(&t);
                }
                w.inline(Inl::Opaque, &range);
            }
            Event::InlineMath(ref t) | Event::DisplayMath(ref t) => {
                let display = matches!(event, Event::DisplayMath(_));
                if let Some((_, h, _)) = &mut w.heading {
                    h.push_str(&format!("${t}$"));
                }
                w.raw.math.push((
                    if display {
                        MathForm::Display
                    } else {
                        MathForm::Inline
                    },
                    range.clone(),
                ));
                w.inline(Inl::Opaque, &range);
            }
            Event::InlineHtml(_) => {
                w.raw.html_constructs += 1;
                w.inline(Inl::Opaque, &range);
            }
            Event::SoftBreak | Event::HardBreak => w.inline(Inl::Break, &range),
            Event::TaskListMarker(_) => w.raw.task_items += 1,
            Event::FootnoteReference(_) => w.inline(Inl::Opaque, &range),
            _ => {}
        }
    }
    w.close();
    w.raw
}
