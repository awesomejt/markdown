//! Comrak adapter: walks the arena AST into `Raw`, converting line/column
//! source positions to byte offsets.

use crate::common::{AvColumn, Block, BlockKind, Inl, MathForm, Raw, Span};
use comrak::nodes::{AlertType, AstNode, NodeValue};
use comrak::{Arena, Options, parse_document};

pub fn options() -> Options<'static> {
    let mut o = Options::default();
    o.extension.strikethrough = true;
    o.extension.table = true;
    o.extension.autolink = true;
    o.extension.tasklist = true;
    o.extension.footnotes = true;
    o.extension.math_dollars = true;
    o.extension.math_code = true;
    o.extension.alerts = true;
    o.extension.front_matter_delimiter = Some("---".into());
    o
}

pub fn parse_only(source: &str) -> usize {
    let arena = Arena::new();
    parse_document(&arena, source, &options())
        .descendants()
        .count()
}

struct Walk<'s> {
    source: &'s str,
    lines: Vec<usize>,
    raw: Raw,
}

impl Walk<'_> {
    /// Byte range of a node; comrak columns are 1-based UTF-8 byte offsets
    /// and the end position is inclusive.
    fn span(&self, node: &AstNode<'_>) -> Option<Span> {
        let p = node.data.borrow().sourcepos;
        if p.start.line == 0 || p.end.line == 0 {
            return None;
        }
        let at =
            |line: usize, col: usize| self.lines.get(line - 1).map(|s| s + col.saturating_sub(1));
        let start = at(p.start.line, p.start.column)?;
        let end = (at(p.end.line, p.end.column)? + 1).min(self.source.len());
        (start <= end).then_some(start..end)
    }

    fn plain<'a>(node: &'a AstNode<'a>, out: &mut String) {
        for d in node.descendants() {
            match &d.data.borrow().value {
                NodeValue::Text(t) => out.push_str(t),
                NodeValue::Code(c) => out.push_str(&c.literal),
                NodeValue::Math(m) => out.push_str(&format!("${}$", m.literal)),
                _ => {}
            }
        }
    }

    fn block<'a>(&mut self, node: &'a AstNode<'a>) {
        let value = node.data.borrow().value.clone();
        let span = self.span(node).unwrap_or(0..0);
        match value {
            NodeValue::FrontMatter(_) => self.raw.front_matter = Some(span),
            NodeValue::Paragraph => {
                let mut b = Block::new(BlockKind::Paragraph, span);
                self.inlines(node, &mut b, false);
                self.raw.blocks.push(b);
            }
            NodeValue::Heading(h) => {
                let mut text = String::new();
                Self::plain(node, &mut text);
                let mut b = Block::new(BlockKind::Heading, span.clone());
                self.inlines(node, &mut b, false);
                self.raw.blocks.push(b);
                self.raw.headings.push((h.level, text, span));
            }
            NodeValue::CodeBlock(c) => {
                if c.fenced {
                    self.raw
                        .fences
                        .push((c.info.split_whitespace().next().map(String::from), span));
                }
            }
            NodeValue::BlockQuote => {
                self.raw.blockquotes.push(span);
                self.children(node);
            }
            NodeValue::Alert(a) => {
                let kind = match a.alert_type {
                    AlertType::Note => "NOTE",
                    AlertType::Tip => "TIP",
                    AlertType::Important => "IMPORTANT",
                    AlertType::Warning => "WARNING",
                    AlertType::Caution => "CAUTION",
                };
                self.raw.alerts.push((kind.into(), span));
                self.children(node);
            }
            NodeValue::TaskItem(_) => {
                self.raw.task_items += 1;
                self.children(node);
            }
            NodeValue::Table(_) => {
                self.raw.tables += 1;
                self.table(node);
            }
            NodeValue::FootnoteDefinition(_) => {
                self.raw.footnote_definitions.push(span);
                self.children(node);
            }
            NodeValue::HtmlBlock(_) => self.raw.html_constructs += 1,
            _ => self.children(node),
        }
    }

    fn children<'a>(&mut self, node: &'a AstNode<'a>) {
        for child in node.children() {
            self.block(child);
        }
    }

    fn table<'a>(&mut self, node: &'a AstNode<'a>) {
        let mut av = None;
        for row in node.children() {
            let header = matches!(row.data.borrow().value, NodeValue::TableRow(true));
            if header {
                let names: Vec<String> = row
                    .children()
                    .map(|c| {
                        let mut s = String::new();
                        Self::plain(c, &mut s);
                        s.trim().to_ascii_lowercase()
                    })
                    .collect();
                let find = |n: &str| names.iter().position(|h| h == n);
                if let (Some(v), Some(a)) = (find("visual"), find("audio")) {
                    av = Some((v, a));
                    self.raw.av_tables += 1;
                }
                continue;
            }
            for (col, cell) in row.children().enumerate() {
                let kind = av.and_then(|(v, a)| {
                    (col == v)
                        .then_some(AvColumn::Visual)
                        .or((col == a).then_some(AvColumn::Audio))
                });
                let mut b = Block::new(
                    BlockKind::Cell { av: kind },
                    self.span(cell).unwrap_or(0..0),
                );
                self.inlines(cell, &mut b, false);
                self.raw.blocks.push(b);
            }
        }
    }

    fn inlines<'a>(&mut self, node: &'a AstNode<'a>, b: &mut Block, opaque: bool) {
        for child in node.children() {
            let value = child.data.borrow().value.clone();
            let span = self.span(child);
            match value {
                NodeValue::Text(_) if !opaque => {
                    if let Some(s) = span {
                        b.push_text(self.source, s);
                    }
                }
                NodeValue::Strong if !opaque => {
                    let mut text = String::new();
                    Self::plain(child, &mut text);
                    b.inl.push(Inl::Strong {
                        span: span.unwrap_or(0..0),
                        text,
                    });
                }
                NodeValue::Link(l) => {
                    if let Some(s) = span.clone() {
                        self.raw.links.push((l.url.clone(), s));
                    }
                    b.inl.push(Inl::Opaque);
                    self.inlines(child, b, true);
                }
                NodeValue::Image(_) | NodeValue::Code(_) | NodeValue::FootnoteReference(_) => {
                    b.inl.push(Inl::Opaque)
                }
                NodeValue::Math(m) => {
                    if let Some(s) = span {
                        let form = if m.display_math {
                            MathForm::Display
                        } else {
                            MathForm::Inline
                        };
                        self.raw.math.push((form, s));
                    }
                    b.inl.push(Inl::Opaque);
                }
                NodeValue::HtmlInline(_) => {
                    self.raw.html_constructs += 1;
                    b.inl.push(Inl::Opaque);
                }
                NodeValue::SoftBreak | NodeValue::LineBreak => b.inl.push(Inl::Break),
                _ => self.inlines(child, b, opaque),
            }
        }
    }
}

pub fn extract(source: &str) -> Raw {
    let arena = Arena::new();
    let root = parse_document(&arena, source, &options());
    let mut lines = vec![0];
    lines.extend(source.match_indices('\n').map(|(i, _)| i + 1));
    let mut w = Walk {
        source,
        lines,
        raw: Raw::default(),
    };
    w.children(root);
    w.raw
}
