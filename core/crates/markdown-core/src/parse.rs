//! Bounded basic parser. Extension conformance and session identity are separate phases.
use crate::*;
use pulldown_cmark::{CodeBlockKind, Event, LinkType, Options, Parser, Tag, TagEnd};
use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

/// Output ceilings do not interrupt the underlying parser's initial source scan.
#[derive(Clone, Copy, Debug)]
pub struct ParseLimits {
    pub model: ModelLimits,
    pub max_events: usize,
    /// Total copied source-derived string bytes, including repeated references.
    pub max_text_bytes: usize,
}
impl Default for ParseLimits {
    fn default() -> Self {
        Self {
            model: ModelLimits {
                max_source_bytes: 8 * 1024 * 1024,
                max_nodes: 250_000,
                max_depth: 128,
                ..ModelLimits::default()
            },
            max_events: 1_000_000,
            max_text_bytes: 32 * 1024 * 1024,
        }
    }
}

/// Structural errors never include user source, destinations or identifiers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParseError {
    SourceLimit,
    EventLimit,
    NodeLimit,
    DepthLimit,
    TextLimit,
    SourceMapping,
    Model(ModelError),
}
impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Model(e) => write!(f, "model: {e}"),
            other => f.write_str(match other {
                Self::SourceLimit => "source_limit",
                Self::EventLimit => "event_limit",
                Self::NodeLimit => "node_limit",
                Self::DepthLimit => "depth_limit",
                Self::TextLimit => "text_limit",
                Self::SourceMapping => "source_mapping",
                Self::Model(_) => unreachable!(),
            }),
        }
    }
}
impl std::error::Error for ParseError {}

struct Frame {
    node: usize,
    end: Option<TagEnd>, // None is an explicit paragraph synthesized for a tight item.
}
enum Capture {
    Code,
    Image { label_end: usize },
    Fallback,
}
struct Builder<'a> {
    source: &'a str,
    limits: ParseLimits,
    copied: usize,
    doc: DocumentInput,
    stack: Vec<Frame>,
    capture: Option<(usize, Capture, usize)>,
    depth: usize,
    anchors: BTreeSet<String>,
    suffixes: BTreeMap<String, usize>,
}

/// Produce a validated immutable snapshot from exact UTF-8 bytes, with no I/O.
/// IDs are deterministic snapshot-local ordinals, not cross-revision identity.
pub fn parse_markdown(
    source: &str,
    document_id: DocumentId,
    revision_id: RevisionId,
    limits: ParseLimits,
) -> Result<Document, ParseError> {
    if source.len() > limits.model.max_source_bytes {
        return Err(ParseError::SourceLimit);
    }
    let mut b = Builder {
        source,
        limits,
        copied: 0,
        stack: Vec::new(),
        capture: None,
        depth: 0,
        anchors: BTreeSet::new(),
        suffixes: BTreeMap::new(),
        doc: DocumentInput {
            model_version: MODEL_VERSION,
            document_id,
            revision_id,
            source: SourceInfo::of(source),
            capabilities: BTreeSet::new(),
            roots: Vec::new(),
            nodes: Vec::new(),
            outline: Vec::new(),
            metadata: Metadata::default(),
            links: Vec::new(),
            resources: Vec::new(),
            diagnostics: Vec::new(),
        },
    };
    // Reject invalid caller identities before the parser allocates its internal tree.
    Document::new(b.doc.clone(), source, limits.model).map_err(ParseError::Model)?;
    let offset = front_matter_end(source).unwrap_or(0);
    if offset > 0 {
        b.doc.metadata.front_matter_span = Some(span(0..offset));
        b.doc.capabilities.insert(Capability::FrontMatter);
        b.diagnostic("front_matter_deferred", span(0..offset), None);
    }
    let options =
        Options::ENABLE_TABLES | Options::ENABLE_TASKLISTS | Options::ENABLE_STRIKETHROUGH;
    // Deliberately no YAML metadata mode (D-015), HTML renderer, smart punctuation,
    // heading attributes, math, footnotes, alerts or broken-link callback.
    for (count, (event, range)) in Parser::new_ext(&source[offset..], options)
        .into_offset_iter()
        .enumerate()
    {
        if count >= limits.max_events {
            return Err(ParseError::EventLimit);
        }
        let range = range.start + offset..range.end + offset;
        b.event(event, range)?;
    }
    if !b.stack.is_empty() || b.capture.is_some() {
        return Err(ParseError::SourceMapping);
    }
    // Some upstream tab-expanded container offsets cannot satisfy the frozen
    // containment/order contract. Keep the original body readable without
    // inventing offsets or publishing any of the invalid graph's intents.
    let identity = (b.doc.document_id.clone(), b.doc.revision_id.clone());
    let source_info = b.doc.source.clone();
    let metadata = b.doc.metadata.clone();
    match Document::new(b.doc, source, limits.model) {
        Err(error) if matches!(error.code, "child_span" | "sibling_order") => {
            let body_span = span(offset..source.len());
            let id = NodeId("n0".into());
            let mut capabilities = BTreeSet::new();
            let mut diagnostics = Vec::new();
            if let Some(front_matter) = metadata.front_matter_span {
                capabilities.insert(Capability::FrontMatter);
                diagnostics.push(Diagnostic {
                    code: "front_matter_deferred".into(),
                    severity: Severity::Info,
                    message: "Front matter is preserved as source; property parsing is deferred."
                        .into(),
                    span: Some(front_matter),
                    node: None,
                });
            }
            diagnostics.push(Diagnostic {
                code: "source_mapping_fallback".into(), severity: Severity::Warning,
                message: "Container source offsets cannot be represented; the body is preserved as inert source.".into(),
                span: Some(body_span), node: Some(id.clone()),
            });
            Document::new(
                DocumentInput {
                    model_version: MODEL_VERSION,
                    document_id: identity.0,
                    revision_id: identity.1,
                    source: source_info,
                    metadata,
                    capabilities,
                    roots: vec![id.clone()],
                    nodes: vec![Node {
                        id,
                        span: body_span,
                        content: NodeKind::UnsupportedBlock {
                            feature: "unrepresentable_source_mapping".into(),
                            reason: UnsupportedReason::InvalidInput,
                        },
                    }],
                    outline: Vec::new(),
                    links: Vec::new(),
                    resources: Vec::new(),
                    diagnostics,
                },
                source,
                limits.model,
            )
            .map_err(ParseError::Model)
        }
        result => result.map_err(ParseError::Model),
    }
}

fn span(range: Range<usize>) -> SourceSpan {
    SourceSpan {
        start: range.start as u64,
        end: range.end as u64,
    }
}
fn front_matter_end(source: &str) -> Option<usize> {
    let mut lines = source.split_inclusive('\n');
    if lines.next()?.trim_end_matches(['\r', '\n']) != "---" {
        return None;
    }
    let mut offset = source.find('\n')? + 1;
    for line in lines {
        offset += line.len();
        if matches!(line.trim_end_matches(['\r', '\n']), "---" | "...") {
            return Some(offset);
        }
    }
    None
}
impl Builder<'_> {
    fn text(&mut self, s: &str) -> Result<String, ParseError> {
        self.copied = self
            .copied
            .checked_add(s.len())
            .ok_or(ParseError::TextLimit)?;
        if self.copied > self.limits.max_text_bytes {
            return Err(ParseError::TextLimit);
        }
        Ok(s.to_owned())
    }
    fn diagnostic(&mut self, code: &str, span: SourceSpan, node: Option<NodeId>) {
        self.doc.diagnostics.push(Diagnostic {
            code: code.into(),
            severity: Severity::Info,
            message: match code {
                "front_matter_deferred" => {
                    "Front matter is preserved as source; property parsing is deferred."
                }
                "reference_link_deferred" => {
                    "Reference link is preserved as source; resolution is deferred."
                }
                _ => "Unsupported content is preserved as source and cannot execute.",
            }
            .into(),
            span: Some(span),
            node,
        });
    }
    fn add(&mut self, range: Range<usize>, content: NodeKind) -> Result<usize, ParseError> {
        if self.doc.nodes.len() >= self.limits.model.max_nodes {
            return Err(ParseError::NodeLimit);
        }
        // Include synthetic paragraphs in the published model depth limit.
        if self.stack.len() >= self.limits.model.max_depth {
            return Err(ParseError::DepthLimit);
        }
        let i = self.doc.nodes.len();
        let id = NodeId(format!("n{i}"));
        if let Some(parent) = self.stack.last() {
            let children = match &mut self.doc.nodes[parent.node].content {
                NodeKind::Paragraph { children }
                | NodeKind::Heading { children, .. }
                | NodeKind::BlockQuote { children }
                | NodeKind::ListItem { children, .. }
                | NodeKind::TableCell { children }
                | NodeKind::Emphasis { children }
                | NodeKind::Strong { children }
                | NodeKind::Strikethrough { children }
                | NodeKind::Link { children, .. } => children,
                NodeKind::List { items, .. } => items,
                NodeKind::Table { rows, .. } => rows,
                NodeKind::TableRow { cells, .. } => cells,
                _ => return Err(ParseError::SourceMapping),
            };
            children.push(id.clone());
        } else {
            self.doc.roots.push(id.clone());
        }
        self.doc.nodes.push(Node {
            id,
            span: span(range),
            content,
        });
        Ok(i)
    }
    fn inline_parent(&mut self, range: &Range<usize>) -> Result<(), ParseError> {
        if self
            .stack
            .last()
            .is_some_and(|f| matches!(self.doc.nodes[f.node].content, NodeKind::ListItem { .. }))
        {
            let node = self.add(
                range.clone(),
                NodeKind::Paragraph {
                    children: Vec::new(),
                },
            )?;
            self.stack.push(Frame { node, end: None });
        }
        if let Some(f) = self.stack.iter().rev().find(|f| f.end.is_none()) {
            self.doc.nodes[f.node].span.end = self.doc.nodes[f.node].span.end.max(range.end as u64);
        }
        Ok(())
    }
    fn event(&mut self, event: Event<'_>, range: Range<usize>) -> Result<(), ParseError> {
        if matches!(event, Event::Start(_)) {
            self.depth += 1;
            if self.depth > self.limits.model.max_depth {
                return Err(ParseError::DepthLimit);
            }
        }
        if let Some((node, capture, depth)) = &mut self.capture {
            let done = matches!(event, Event::End(_)) && self.depth == *depth;
            if !done {
                let piece = match &event {
                    Event::Text(t) | Event::Code(t) => Some(t.as_ref()),
                    Event::SoftBreak | Event::HardBreak => Some("\n"),
                    _ => None,
                };
                if let Some(piece) = piece {
                    let target = *node;
                    let is_code = matches!(capture, Capture::Code);
                    let is_image = matches!(capture, Capture::Image { .. });
                    if is_code || is_image {
                        let piece = self.text(piece)?;
                        match &mut self.doc.nodes[target].content {
                            NodeKind::CodeBlock { text, .. } => text.push_str(&piece),
                            NodeKind::Image { alt, .. } => alt.push_str(&piece),
                            _ => return Err(ParseError::SourceMapping),
                        }
                    }
                }
                if let Some((_, Capture::Image { label_end }, _)) = &mut self.capture {
                    *label_end = (*label_end).max(range.end);
                }
            }
            if matches!(event, Event::End(_)) {
                self.depth -= 1;
            }
            if done {
                let (node, capture, _) = self.capture.take().ok_or(ParseError::SourceMapping)?;
                if let Capture::Image { label_end } = capture {
                    self.finish_image_span(node, label_end)?;
                }
            }
            return Ok(());
        }
        match event {
            Event::Start(tag) => self.start(tag, range)?,
            Event::End(end) => {
                // Inline end events are part of a synthetic paragraph; block ends close it.
                if self.stack.last().is_some_and(|f| f.end.is_none()) {
                    self.stack.pop();
                }
                let frame = self.stack.pop().ok_or(ParseError::SourceMapping)?;
                if frame.end != Some(end) {
                    return Err(ParseError::SourceMapping);
                }
                self.finish(frame.node)?;
                self.depth -= 1;
            }
            Event::TaskListMarker(checked) => {
                let item = self
                    .stack
                    .iter()
                    .rev()
                    .find_map(|f| match &mut self.doc.nodes[f.node].content {
                        NodeKind::ListItem { .. } => Some(f.node),
                        _ => None,
                    })
                    .ok_or(ParseError::SourceMapping)?;
                if let NodeKind::ListItem { checked: value, .. } = &mut self.doc.nodes[item].content
                {
                    *value = Some(checked);
                }
                self.doc.capabilities.insert(Capability::TaskLists);
            }
            event => {
                let content = match event {
                    Event::Text(t) => NodeKind::Text {
                        text: self.text(&t)?,
                    },
                    Event::Code(t) => NodeKind::CodeSpan {
                        text: self.text(&t)?,
                    },
                    Event::SoftBreak => NodeKind::SoftBreak {},
                    Event::HardBreak => NodeKind::HardBreak {},
                    Event::Rule => NodeKind::ThematicBreak {},
                    Event::InlineHtml(_) => NodeKind::UnsupportedInline {
                        feature: "raw_html".into(),
                        reason: UnsupportedReason::Disabled,
                    },
                    _ => return Err(ParseError::SourceMapping),
                };
                if content.is_inline() {
                    self.inline_parent(&range)?;
                } else if self.stack.last().is_some_and(|f| f.end.is_none()) {
                    self.stack.pop();
                }
                let fallback = matches!(content, NodeKind::UnsupportedInline { .. });
                let i = self.add(range, content)?;
                if fallback {
                    self.diagnostic(
                        "raw_html_disabled",
                        self.doc.nodes[i].span,
                        Some(self.doc.nodes[i].id.clone()),
                    );
                }
            }
        }
        Ok(())
    }
    fn start(&mut self, tag: Tag<'_>, range: Range<usize>) -> Result<(), ParseError> {
        let inline = matches!(
            tag,
            Tag::Emphasis | Tag::Strong | Tag::Strikethrough | Tag::Link { .. } | Tag::Image { .. }
        );
        if inline {
            self.inline_parent(&range)?;
        } else if self.stack.last().is_some_and(|f| f.end.is_none()) {
            self.stack.pop();
        }
        let end = tag.to_end();
        let mut capture = None;
        let mut diagnostic = None;
        let content = match tag {
            Tag::Paragraph => {
                // Explicit paragraphs imply a loose list; tight-list paragraphs are synthesized.
                if self.stack.last().is_some_and(|f| {
                    matches!(self.doc.nodes[f.node].content, NodeKind::ListItem { .. })
                }) && let Some(list) = self
                    .stack
                    .iter()
                    .rev()
                    .find(|f| matches!(self.doc.nodes[f.node].content, NodeKind::List { .. }))
                    && let NodeKind::List { tight, .. } = &mut self.doc.nodes[list.node].content
                {
                    *tight = false;
                }
                NodeKind::Paragraph {
                    children: Vec::new(),
                }
            }
            Tag::Heading { level, .. } => NodeKind::Heading {
                level: level as u8,
                anchor: String::new(),
                children: Vec::new(),
            },
            Tag::BlockQuote(_) => NodeKind::BlockQuote {
                children: Vec::new(),
            },
            Tag::List(start) => NodeKind::List {
                ordered: start.is_some(),
                start,
                tight: true,
                items: Vec::new(),
            },
            Tag::Item => NodeKind::ListItem {
                checked: None,
                children: Vec::new(),
            },
            Tag::Emphasis => NodeKind::Emphasis {
                children: Vec::new(),
            },
            Tag::Strong => NodeKind::Strong {
                children: Vec::new(),
            },
            Tag::Strikethrough => {
                self.doc.capabilities.insert(Capability::Strikethrough);
                NodeKind::Strikethrough {
                    children: Vec::new(),
                }
            }
            Tag::Table(alignments) => {
                self.doc.capabilities.insert(Capability::Tables);
                NodeKind::Table {
                    alignments: alignments
                        .into_iter()
                        .map(|a| match a {
                            pulldown_cmark::Alignment::None => Alignment::None,
                            pulldown_cmark::Alignment::Left => Alignment::Left,
                            pulldown_cmark::Alignment::Center => Alignment::Center,
                            pulldown_cmark::Alignment::Right => Alignment::Right,
                        })
                        .collect(),
                    rows: Vec::new(),
                }
            }
            Tag::TableHead => NodeKind::TableRow {
                header: true,
                cells: Vec::new(),
            },
            Tag::TableRow => NodeKind::TableRow {
                header: false,
                cells: Vec::new(),
            },
            Tag::TableCell => NodeKind::TableCell {
                children: Vec::new(),
            },
            Tag::CodeBlock(kind) => {
                let info = match kind {
                    CodeBlockKind::Indented => None,
                    CodeBlockKind::Fenced(info) => Some(self.text(&info)?),
                };
                let language = info
                    .as_deref()
                    .unwrap_or("")
                    .split_whitespace()
                    .next()
                    .unwrap_or("");
                if matches!(language, "mermaid" | "d2" | "math" | "latex") {
                    capture = Some(Capture::Fallback);
                    diagnostic = Some("advanced_fence_disabled");
                    NodeKind::UnsupportedBlock {
                        feature: self.text(language)?,
                        reason: UnsupportedReason::Unavailable,
                    }
                } else {
                    capture = Some(Capture::Code);
                    NodeKind::CodeBlock {
                        info,
                        text: String::new(),
                    }
                }
            }
            Tag::HtmlBlock => {
                capture = Some(Capture::Fallback);
                diagnostic = Some("raw_html_disabled");
                NodeKind::UnsupportedBlock {
                    feature: "raw_html".into(),
                    reason: UnsupportedReason::Disabled,
                }
            }
            Tag::Link {
                link_type,
                dest_url,
                title,
                ..
            } => {
                if self
                    .stack
                    .iter()
                    .any(|f| matches!(self.doc.nodes[f.node].content, NodeKind::Link { .. }))
                {
                    capture = Some(Capture::Fallback);
                    diagnostic = Some("nested_link_disabled");
                    NodeKind::UnsupportedInline {
                        feature: "nested_link".into(),
                        reason: UnsupportedReason::InvalidInput,
                    }
                } else if !matches!(
                    link_type,
                    LinkType::Inline | LinkType::Autolink | LinkType::Email
                ) {
                    capture = Some(Capture::Fallback);
                    diagnostic = Some("reference_link_deferred");
                    NodeKind::UnsupportedInline {
                        feature: "reference_link".into(),
                        reason: UnsupportedReason::Unavailable,
                    }
                } else {
                    let destination = if link_type == LinkType::Email {
                        self.text(&format!("mailto:{dest_url}"))?
                    } else {
                        self.text(&dest_url)?
                    };
                    let target = LinkId(format!("l{}", self.doc.links.len()));
                    let disposition = classify_link(&destination);
                    self.doc.links.push(LinkIntent {
                        id: target.clone(),
                        span: span(range.clone()),
                        destination,
                        disposition,
                    });
                    NodeKind::Link {
                        target,
                        title: if title.is_empty() {
                            None
                        } else {
                            Some(self.text(&title)?)
                        },
                        children: Vec::new(),
                    }
                }
            }
            Tag::Image {
                link_type,
                dest_url,
                title,
                ..
            } => {
                if !matches!(
                    link_type,
                    LinkType::Inline | LinkType::Autolink | LinkType::Email
                ) {
                    capture = Some(Capture::Fallback);
                    diagnostic = Some("reference_link_deferred");
                    NodeKind::UnsupportedInline {
                        feature: "reference_image".into(),
                        reason: UnsupportedReason::Unavailable,
                    }
                } else {
                    let destination = self.text(&dest_url)?;
                    let access = if local(&destination) {
                        ResourceAccess::RequiresCapability {}
                    } else {
                        ResourceAccess::Denied {
                            reason: "nonlocal_or_unsafe_resource".into(),
                        }
                    };
                    let resource = ResourceId(format!("r{}", self.doc.resources.len()));
                    self.doc.resources.push(ResourceIntent {
                        id: resource.clone(),
                        span: span(range.clone()),
                        destination,
                        access,
                    });
                    capture = Some(Capture::Image {
                        label_end: range.start + 2,
                    });
                    NodeKind::Image {
                        resource,
                        alt: String::new(),
                        title: if title.is_empty() {
                            None
                        } else {
                            Some(self.text(&title)?)
                        },
                    }
                }
            }
            _ => return Err(ParseError::SourceMapping),
        };
        let node = self.add(range, content)?;
        if let Some(code) = diagnostic {
            self.diagnostic(
                code,
                self.doc.nodes[node].span,
                Some(self.doc.nodes[node].id.clone()),
            );
        }
        if let Some(capture) = capture {
            self.capture = Some((node, capture, self.depth));
        } else {
            self.stack.push(Frame {
                node,
                end: Some(end),
            });
        }
        Ok(())
    }
    fn finish(&mut self, node: usize) -> Result<(), ParseError> {
        let n = &self.doc.nodes[node];
        if matches!(n.content, NodeKind::Heading { .. }) {
            let mut title = String::new();
            let mut todo = n.content.children().iter().rev().collect::<Vec<_>>();
            while let Some(id) = todo.pop() {
                // Snapshot IDs are allocated as node ordinals, never parsed from caller values.
                let i = id.0[1..]
                    .parse::<usize>()
                    .map_err(|_| ParseError::SourceMapping)?;
                match &self.doc.nodes[i].content {
                    NodeKind::Text { text } | NodeKind::CodeSpan { text } => title.push_str(text),
                    NodeKind::Image { alt, .. } => title.push_str(alt),
                    NodeKind::SoftBreak {} | NodeKind::HardBreak {} => title.push(' '),
                    NodeKind::UnsupportedInline { feature, .. } if feature == "raw_html" => {}
                    NodeKind::UnsupportedInline { .. } => title.push_str(
                        self.doc.nodes[i]
                            .span
                            .slice(self.source)
                            .ok_or(ParseError::SourceMapping)?,
                    ),
                    content => todo.extend(content.children().iter().rev()),
                }
            }
            let title = self.text(&title)?;
            let base: String = title
                .to_lowercase()
                .chars()
                .filter_map(|c| {
                    if c.is_whitespace() {
                        Some('-')
                    } else if c.is_alphanumeric() || matches!(c, '-' | '_') {
                        Some(c)
                    } else {
                        None
                    }
                })
                .collect();
            let base = if base.is_empty() {
                "section".into()
            } else {
                base
            };
            let mut anchor = base.clone();
            let suffix = self.suffixes.entry(base.clone()).or_insert(1);
            while !self.anchors.insert(anchor.clone()) {
                anchor = format!("{base}-{suffix}");
                *suffix += 1;
            }
            let anchor = self.text(&anchor)?;
            let heading_id = self.doc.nodes[node].id.clone();
            if let NodeKind::Heading {
                level,
                anchor: value,
                ..
            } = &mut self.doc.nodes[node].content
            {
                *value = anchor.clone();
                self.doc.outline.push(OutlineEntry {
                    node: heading_id,
                    level: *level,
                    title,
                    anchor,
                });
            }
        }
        if let NodeKind::Link {
            target, children, ..
        } = &self.doc.nodes[node].content
        {
            let label_end = children
                .last()
                .map(|id| {
                    let i = id.0[1..]
                        .parse::<usize>()
                        .map_err(|_| ParseError::SourceMapping)?;
                    usize::try_from(self.doc.nodes[i].span.end)
                        .map_err(|_| ParseError::SourceMapping)
                })
                .transpose()?
                .unwrap_or(self.doc.nodes[node].span.start as usize + 1);
            let pos = target.0[1..]
                .parse::<usize>()
                .map_err(|_| ParseError::SourceMapping)?;
            self.doc.links[pos].span =
                destination_span(self.source, self.doc.nodes[node].span, label_end)?;
        }
        Ok(())
    }
    fn finish_image_span(&mut self, node: usize, label_end: usize) -> Result<(), ParseError> {
        if let NodeKind::Image { resource, .. } = &self.doc.nodes[node].content {
            let pos = resource.0[1..]
                .parse::<usize>()
                .map_err(|_| ParseError::SourceMapping)?;
            self.doc.resources[pos].span =
                destination_span(self.source, self.doc.nodes[node].span, label_end)?;
        }
        Ok(())
    }
}

fn destination_span(
    source: &str,
    whole: SourceSpan,
    label_end: usize,
) -> Result<SourceSpan, ParseError> {
    let start = whole.start as usize;
    let end = whole.end as usize;
    let raw = source.get(start..end).ok_or(ParseError::SourceMapping)?;
    if raw.starts_with('<') {
        return Ok(span(start + 1..end - 1));
    }
    let tail = source
        .get(label_end..end)
        .ok_or(ParseError::SourceMapping)?;
    let relative = tail.find("](").ok_or(ParseError::SourceMapping)? + 2;
    let mut pos = label_end + relative;
    while source
        .as_bytes()
        .get(pos)
        .is_some_and(u8::is_ascii_whitespace)
    {
        pos += 1;
    }
    let angled = source.as_bytes().get(pos) == Some(&b'<');
    if angled {
        pos += 1;
    }
    let begin = pos;
    let mut depth = 0usize;
    while pos < end {
        let c = source.as_bytes()[pos];
        if c == b'\\'
            && source
                .as_bytes()
                .get(pos + 1)
                .is_some_and(u8::is_ascii_punctuation)
        {
            pos += 2;
            continue;
        }
        if angled && c == b'>' {
            return Ok(span(begin..pos));
        }
        if !angled {
            if depth == 0 && (c == b')' || c.is_ascii_whitespace()) {
                return Ok(span(begin..pos));
            }
            if c == b'(' {
                depth += 1;
            }
            if c == b')' {
                depth = depth.checked_sub(1).ok_or(ParseError::SourceMapping)?;
            }
        }
        pos += 1;
    }
    Err(ParseError::SourceMapping)
}

// Deliberately conservative URI policy. The platform must still decode/resolve
// local paths against an actual permission capability; model values grant no I/O.
fn safe_uri(value: &str) -> bool {
    !value.is_empty()
        && value.is_ascii()
        && !value.chars().any(|c| c.is_control() || c.is_whitespace())
        && !value.contains(['\\', '%'])
}
fn local(value: &str) -> bool {
    safe_uri(value)
        && !value.starts_with(['/', '#'])
        && !value.contains(':')
        && !value
            .split(['/', '?', '#'])
            .any(|part| matches!(part, "." | ".."))
}
fn classify_link(value: &str) -> LinkDisposition {
    if safe_uri(value) {
        if let Some(anchor) = value.strip_prefix('#').filter(|a| !a.is_empty()) {
            return LinkDisposition::Fragment {
                anchor: anchor.into(),
            };
        }
        let lower = value.to_ascii_lowercase();
        if ["https://", "http://", "mailto:"]
            .iter()
            .any(|p| lower.strip_prefix(p).is_some_and(|tail| !tail.is_empty()))
        {
            return LinkDisposition::External {};
        }
        if local(value) {
            return LinkDisposition::RequiresCapability {};
        }
    }
    LinkDisposition::Denied {
        reason: "unsafe_or_unsupported_destination".into(),
    }
}
