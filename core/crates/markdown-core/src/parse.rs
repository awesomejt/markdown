//! Bounded parser with the MVP extensions (alerts, footnotes) and explicit
//! math/diagram/voice source fallbacks. Conformance, hostile-input coverage and
//! budgets are in docs/Parser-Budgets.md; sessions and anchors live in `session`.
use crate::*;
use pulldown_cmark::{
    BlockQuoteKind, CodeBlockKind, Event, LinkType, Options, Parser, Tag, TagEnd,
};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::ops::Range;
use unicase::UniCase;

/// Output ceilings do not interrupt the underlying parser's initial source scan.
/// Defaults admit representative prose at the source ceiling (CORE-003B4).
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
                max_nodes: 1_000_000,
                max_depth: 128,
                ..ModelLimits::default()
            },
            max_events: 2_000_000,
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
    /// End offset of the last child added, for the source-order conflict check.
    last_end: u64,
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
    references: bool,
    link_definitions: BTreeMap<LinkId, SourceSpan>,
    resource_definitions: BTreeMap<ResourceId, SourceSpan>,
    /// First definition label per upstream case-folded key; later ones are fallbacks.
    footnote_labels: BTreeMap<UniCase<String>, String>,
    /// Reference node indexes with the label as written, resolved after parsing.
    footnote_references: Vec<usize>,
    /// End offset of the last root added, for the source-order conflict check.
    root_end: u64,
    /// An upstream offset escaped its parent or preceded a sibling.
    offset_conflict: bool,
}

/// Produce a validated immutable snapshot from exact UTF-8 bytes, with no I/O.
/// IDs are deterministic snapshot-local ordinals, not cross-revision identity.
pub fn parse_markdown(
    source: &str,
    document_id: DocumentId,
    revision_id: RevisionId,
    limits: ParseLimits,
) -> Result<Document, ParseError> {
    let b = build(
        source,
        document_id,
        revision_id,
        limits,
        false,
        &Uninterrupted,
    )
    .map_err(Stop::never)?;
    finish_v1(b.doc, source, limits)
}
#[cfg(feature = "model-v2")]
pub fn parse_markdown_v2(
    source: &str,
    document_id: DocumentId,
    revision_id: RevisionId,
    limits: ParseLimits,
) -> Result<crate::v2::Document, ParseError> {
    let b = build(
        source,
        document_id,
        revision_id,
        limits,
        true,
        &Uninterrupted,
    )
    .map_err(Stop::never)?;
    finish_v2(b, source, limits)
}

/// [`parse_markdown`] with cooperative cancellation. The token is polled before
/// work starts, every [`CANCEL_CHECK_EVENTS`] parser events and between phases.
/// Output is identical to [`parse_markdown`] when the token is never cancelled.
/// The upstream parser's initial block scan cannot be interrupted; it is bounded
/// by `limits.model.max_source_bytes`.
pub fn parse_markdown_cancellable(
    source: &str,
    document_id: DocumentId,
    revision_id: RevisionId,
    limits: ParseLimits,
    token: &CancellationToken,
) -> Result<Document, CancellableParseError> {
    let b = build(source, document_id, revision_id, limits, false, token)?;
    token.check()?;
    Ok(finish_v1(b.doc, source, limits)?)
}
/// [`parse_markdown_v2`] with the same cancellation contract as
/// [`parse_markdown_cancellable`].
#[cfg(feature = "model-v2")]
pub fn parse_markdown_v2_cancellable(
    source: &str,
    document_id: DocumentId,
    revision_id: RevisionId,
    limits: ParseLimits,
    token: &CancellationToken,
) -> Result<crate::v2::Document, CancellableParseError> {
    let b = build(source, document_id, revision_id, limits, true, token)?;
    token.check()?;
    Ok(finish_v2(b, source, limits)?)
}

/// Parser events between cancellation polls; a bounded constant, not a budget.
pub const CANCEL_CHECK_EVENTS: usize = 256;

/// The one upstream option set; v2 provenance reparsing must see identical events.
/// Deliberately no YAML metadata mode (D-015), HTML renderer, smart punctuation,
/// heading attributes, old footnotes, definition lists, super/subscript,
/// wikilinks or broken-link callback. `ENABLE_GFM` adds only alert tags.
const PARSER_OPTIONS: Options = Options::ENABLE_TABLES
    .union(Options::ENABLE_TASKLISTS)
    .union(Options::ENABLE_STRIKETHROUGH)
    .union(Options::ENABLE_FOOTNOTES)
    .union(Options::ENABLE_MATH)
    .union(Options::ENABLE_GFM);

/// Interruption source polled by the builder. The uninterrupted form has an
/// uninhabited error so the default entry points cannot observe cancellation.
trait Checkpoint {
    type Interrupt;
    fn poll(&self) -> Result<(), Self::Interrupt>;
}
struct Uninterrupted;
impl Checkpoint for Uninterrupted {
    type Interrupt = std::convert::Infallible;
    fn poll(&self) -> Result<(), Self::Interrupt> {
        Ok(())
    }
}
impl Checkpoint for CancellationToken {
    type Interrupt = Cancelled;
    fn poll(&self) -> Result<(), Cancelled> {
        self.check()
    }
}
enum Stop<I> {
    Parse(ParseError),
    Interrupted(I),
}
impl<I> From<ParseError> for Stop<I> {
    fn from(error: ParseError) -> Self {
        Self::Parse(error)
    }
}
impl Stop<std::convert::Infallible> {
    fn never(self) -> ParseError {
        match self {
            Self::Parse(error) => error,
            Self::Interrupted(never) => match never {},
        }
    }
}
impl From<Stop<Cancelled>> for CancellableParseError {
    fn from(stop: Stop<Cancelled>) -> Self {
        match stop {
            Stop::Parse(error) => Self::Parse(error),
            Stop::Interrupted(Cancelled) => Self::Cancelled,
        }
    }
}
fn build<'a, C: Checkpoint>(
    source: &'a str,
    document_id: DocumentId,
    revision_id: RevisionId,
    limits: ParseLimits,
    references: bool,
    checkpoint: &C,
) -> Result<Builder<'a>, Stop<C::Interrupt>> {
    if source.len() > limits.model.max_source_bytes {
        return Err(ParseError::SourceLimit.into());
    }
    checkpoint.poll().map_err(Stop::Interrupted)?;
    let mut b = Builder {
        source,
        limits,
        copied: 0,
        stack: Vec::new(),
        capture: None,
        depth: 0,
        anchors: BTreeSet::new(),
        suffixes: BTreeMap::new(),
        references,
        link_definitions: BTreeMap::new(),
        resource_definitions: BTreeMap::new(),
        footnote_labels: BTreeMap::new(),
        footnote_references: Vec::new(),
        root_end: 0,
        offset_conflict: false,
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
        match crate::metadata::parse(&source[..offset], &mut b.copied, limits.max_text_bytes) {
            Ok(properties) => {
                if let Some(MetadataValue::String { value }) = properties.get("title")
                    && !value.trim().is_empty()
                {
                    b.doc.metadata.title = Some(b.text(value)?);
                }
                if matches!(properties.get("mdv"), Some(MetadataValue::String { value }) if value == "script")
                {
                    b.diagnostic("voice_script_unavailable", span(0..offset), None);
                }
                b.doc.metadata.properties = properties;
            }
            Err(crate::metadata::Error::TextLimit) => return Err(ParseError::TextLimit.into()),
            Err(error) => {
                let code = match error {
                    crate::metadata::Error::Duplicate => "front_matter_duplicate",
                    crate::metadata::Error::Limit => "front_matter_limit",
                    _ => "front_matter_unsupported",
                };
                b.diagnostic(code, span(0..offset), None);
                // Opt-in line scan when typed properties are omitted (FEAS-010 E1).
                if source[..offset]
                    .lines()
                    .any(|l| l.trim_end() == "mdv: script" || l.starts_with("mdv: script "))
                {
                    b.diagnostic("voice_script_unavailable", span(0..offset), None);
                }
            }
        }
    }
    checkpoint.poll().map_err(Stop::Interrupted)?;
    let options = PARSER_OPTIONS;
    let mut events = Parser::new_ext(&source[offset..], options).into_offset_iter();
    let mut count = 0usize;
    // V2 only: source ranges that produced events, for shadowed-definition notes.
    let mut covered: Vec<Range<usize>> = Vec::new();
    while let Some((event, range)) = events.next() {
        if count >= limits.max_events {
            return Err(ParseError::EventLimit.into());
        }
        if count.is_multiple_of(CANCEL_CHECK_EVENTS) {
            checkpoint.poll().map_err(Stop::Interrupted)?;
        }
        count += 1;
        if references
            && !matches!(
                event,
                Event::End(_)
                    | Event::Start(
                        Tag::BlockQuote(_) | Tag::List(_) | Tag::Item | Tag::FootnoteDefinition(_)
                    )
            )
        {
            match covered.last_mut() {
                Some(last) if (last.start..=last.end).contains(&range.start) => {
                    last.end = last.end.max(range.end);
                }
                _ => covered.push(range.clone()),
            }
        }
        let definition = if references {
            match &event {
                Event::Start(
                    Tag::Link { link_type, id, .. } | Tag::Image { link_type, id, .. },
                ) if matches!(
                    link_type,
                    LinkType::Reference | LinkType::Collapsed | LinkType::Shortcut
                ) =>
                {
                    events
                        .reference_definitions()
                        .get(id)
                        .map(|d| span(d.span.start + offset..d.span.end + offset))
                }
                _ => None,
            }
        } else {
            None
        };
        b.event(event, range.start + offset..range.end + offset, definition)?;
    }
    if !b.stack.is_empty() || b.capture.is_some() {
        return Err(ParseError::SourceMapping.into());
    }
    b.resolve_footnotes()?;
    b.localize_source_mapping();
    if references {
        checkpoint.poll().map_err(Stop::Interrupted)?;
        let body = &source[offset..];
        for r in shadowed_definitions(body, events.reference_definitions(), covered, options) {
            b.diagnostic(
                "reference_definition_shadowed",
                span(r.start + offset..r.end + offset),
                None,
            );
        }
    }
    Ok(b)
}

/// Bounded upper limit on shadowed-definition notes for one document.
const MAX_SHADOWED_DEFINITIONS: usize = 1024;

/// Find later duplicate reference definitions that CommonMark ignores. The
/// pinned parser emits no event for any definition and only reports the kept
/// first one, so ignored definitions are exactly the body source that neither
/// produced an event nor is a kept definition (beyond whitespace and container
/// markers). Each such gap is flattened (container prefixes removed) and
/// reparsed standalone; it is reported only if it yields no events at all
/// (purely definitions) and each found label is already kept at another span.
/// Anything else is a silent false negative. Reparsing is capped at twice the
/// body length and the result at `MAX_SHADOWED_DEFINITIONS`.
fn shadowed_definitions(
    body: &str,
    kept: &pulldown_cmark::RefDefs<'_>,
    mut covered: Vec<Range<usize>>,
    options: Options,
) -> Vec<Range<usize>> {
    let mut found = Vec::new();
    let mut kept_spans: Vec<Range<usize>> = kept.iter().map(|(_, d)| d.span.clone()).collect();
    if kept_spans.is_empty() {
        return found;
    }
    covered.append(&mut kept_spans);
    covered.sort_unstable_by_key(|r| r.start);
    let mut budget = body.len().saturating_mul(2);
    let mut at = 0usize;
    let mut gaps = Vec::new();
    for r in covered
        .iter()
        .chain(std::iter::once(&(body.len()..body.len())))
    {
        if r.start > at {
            gaps.push(at..r.start);
        }
        at = at.max(r.end);
    }
    for gap in gaps {
        let text = &body[gap.clone()];
        if !text.contains('[') {
            continue;
        }
        // Flatten: fragment bytes plus (fragment offset, body offset) per line.
        // Definitions cannot span a blank line, so each blank line splits.
        let mut fragment = String::new();
        let mut map: Vec<(usize, usize)> = Vec::new();
        let mut line_start = gap.start;
        for line in text.split_inclusive('\n') {
            let content = line_start + container_prefix(line);
            let rest = &body[content..line_start + line.len()];
            line_start += line.len();
            if rest.trim().is_empty() {
                if !scan_fragment(&fragment, &map, kept, options, &mut budget, &mut found) {
                    return finish_shadowed(found);
                }
                fragment.clear();
                map.clear();
            } else {
                map.push((fragment.len(), content));
                fragment.push_str(rest);
            }
        }
        if !scan_fragment(&fragment, &map, kept, options, &mut budget, &mut found) {
            break;
        }
    }
    finish_shadowed(found)
}
/// Returns false once the reparse budget or result cap is exhausted.
fn scan_fragment(
    fragment: &str,
    map: &[(usize, usize)],
    kept: &pulldown_cmark::RefDefs<'_>,
    options: Options,
    budget: &mut usize,
    found: &mut Vec<Range<usize>>,
) -> bool {
    let to_body = |p: usize, end: bool| {
        let i = map.partition_point(|&(f, _)| if end { f < p } else { f <= p });
        let (f, s) = map[i.saturating_sub(1)];
        s + (p - f)
    };
    let mut work = Vec::new();
    if fragment.contains('[') {
        work.push(0..fragment.len());
    }
    while let Some(r) = work.pop() {
        let part = &fragment[r.clone()];
        let Some(left) = budget.checked_sub(part.len()) else {
            return false;
        };
        *budget = left;
        let mut parser = Parser::new_ext(part, options);
        if parser.next().is_some() {
            continue; // Not purely definitions: fail closed for this region.
        }
        let mut local: Vec<Range<usize>> = Vec::new();
        for (label, def) in parser.reference_definitions().iter() {
            let source =
                to_body(r.start + def.span.start, false)..to_body(r.start + def.span.end, true);
            if kept.get(label).is_some_and(|k| k.span != source) {
                found.push(source);
                if found.len() >= MAX_SHADOWED_DEFINITIONS {
                    return false;
                }
            }
            local.push(def.span.clone());
        }
        // Same-label duplicates inside this fragment were dropped again.
        local.sort_unstable_by_key(|s| s.start);
        let mut at = 0usize;
        for s in local
            .iter()
            .chain(std::iter::once(&(part.len()..part.len())))
        {
            if s.start > at && part[at..s.start].contains('[') {
                work.push(r.start + at..r.start + s.start);
            }
            at = at.max(s.end);
        }
    }
    true
}
fn finish_shadowed(mut found: Vec<Range<usize>>) -> Vec<Range<usize>> {
    found.sort_unstable_by_key(|r| (r.start, r.end));
    found.dedup();
    found
}
/// Length of leading whitespace, block-quote and list-item markers on a line.
fn container_prefix(line: &str) -> usize {
    let bytes = line.as_bytes();
    let mut i = 0;
    loop {
        while matches!(bytes.get(i), Some(b' ' | b'\t')) {
            i += 1;
        }
        let marker_end = match bytes.get(i) {
            Some(b'>') => i + 1,
            Some(b'-' | b'+' | b'*') => i + 1,
            Some(b'0'..=b'9') => {
                let digits = bytes[i..].iter().take_while(|b| b.is_ascii_digit()).count();
                match bytes.get(i + digits) {
                    Some(b'.' | b')') if digits <= 9 => i + digits + 1,
                    _ => return i,
                }
            }
            _ => return i,
        };
        // List markers need following whitespace or line end; quotes do not.
        if bytes[i] != b'>'
            && !matches!(
                bytes.get(marker_end),
                None | Some(b' ' | b'\t' | b'\r' | b'\n')
            )
        {
            return i;
        }
        i = marker_end;
    }
}
fn finish_v1(
    doc: DocumentInput,
    source: &str,
    limits: ParseLimits,
) -> Result<Document, ParseError> {
    let offset = doc.metadata.front_matter_span.map_or(0, |s| s.end as usize);
    // Safety net: `localize_source_mapping` already replaced each top-level
    // block with conflicting upstream offsets. If validation still finds a
    // containment/order error, keep the original body readable without
    // inventing offsets or publishing any of the invalid graph's intents.
    let identity = (doc.document_id.clone(), doc.revision_id.clone());
    let source_info = doc.source.clone();
    let metadata = doc.metadata.clone();
    let metadata_diagnostics: Vec<_> = doc
        .diagnostics
        .iter()
        .filter(|d| d.node.is_none() && d.span == metadata.front_matter_span)
        .cloned()
        .collect();
    match Document::new(doc, source, limits.model) {
        Err(error) if matches!(error.code, "child_span" | "sibling_order") => {
            let body_span = span(offset..source.len());
            let id = NodeId("n0".into());
            let mut capabilities = BTreeSet::new();
            let mut diagnostics = metadata_diagnostics;
            if metadata.front_matter_span.is_some() {
                capabilities.insert(Capability::FrontMatter);
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
    let mut lines = source
        .strip_prefix('\u{feff}')
        .unwrap_or(source)
        .split_inclusive('\n');
    if lines.next()?.trim_end_matches(['\r', '\n']) != "---" {
        return None;
    }
    // A blank line immediately after the opening rule means ordinary Markdown.
    if lines
        .clone()
        .next()?
        .trim_end_matches(['\r', '\n'])
        .trim()
        .is_empty()
    {
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
            severity: if code.starts_with("front_matter_") { Severity::Warning } else { Severity::Info },
            message: match code {
                "front_matter_duplicate" => "Duplicate front-matter keys; properties are omitted and source is retained.",
                "front_matter_limit" => "Front matter exceeds supported metadata limits; properties are omitted and source is retained.",
                "front_matter_unsupported" => "Unsupported front matter; properties are omitted and source is retained.",
                "reference_link_deferred" => {
                    "Reference link is preserved as source; resolution is deferred."
                }
                "reference_definition_shadowed" => {
                    "Duplicate reference definition is ignored; the first definition is used."
                }
                "math_unavailable" => "Math is preserved as source; no math engine is available.",
                "footnote_definition_duplicate" => {
                    "Duplicate footnote definition is preserved as source; the first definition is used."
                }
                "footnote_definition_invalid" => {
                    "Footnote definition label is not usable; it is preserved as source."
                }
                "footnote_reference_unresolved" => {
                    "Footnote reference has no usable definition and is preserved as source."
                }
                "voice_script_unavailable" => {
                    "Voice-over script syntax is not interpreted; it reads as ordinary Markdown."
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
        let (start, end) = (range.start as u64, range.end as u64);
        if let Some(parent) = self.stack.last_mut() {
            let bound = self.doc.nodes[parent.node].span;
            if start < parent.last_end || start < bound.start || end > bound.end {
                self.offset_conflict = true;
            }
            parent.last_end = end;
            let children = match &mut self.doc.nodes[parent.node].content {
                NodeKind::Paragraph { children }
                | NodeKind::Heading { children, .. }
                | NodeKind::BlockQuote { children }
                | NodeKind::ListItem { children, .. }
                | NodeKind::TableCell { children }
                | NodeKind::FootnoteDefinition { children, .. }
                | NodeKind::Alert { children, .. }
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
            self.offset_conflict |= start < self.root_end;
            self.root_end = end;
            self.doc.roots.push(id.clone());
        }
        self.doc.nodes.push(Node {
            id,
            span: span(range),
            content,
        });
        Ok(i)
    }
    /// `ENABLE_MATH` splits text at every non-math `$`; rejoin a contiguous split
    /// so prices and shell variables cost no extra nodes. Limited to `$` source
    /// boundaries so trees without dollars keep their frozen shape.
    fn merge_dollar_text(&mut self, range: &Range<usize>, text: &str) -> bool {
        let Some(parent) = self.stack.last() else {
            return false;
        };
        let last = self.doc.nodes.len().saturating_sub(1);
        let sibling = match &self.doc.nodes[parent.node].content {
            NodeKind::Paragraph { children }
            | NodeKind::Heading { children, .. }
            | NodeKind::TableCell { children }
            | NodeKind::Emphasis { children }
            | NodeKind::Strong { children }
            | NodeKind::Strikethrough { children }
            | NodeKind::Link { children, .. } => children.last().cloned(),
            _ => None,
        };
        let bytes = self.source.as_bytes();
        let Some(Node {
            id,
            span: prev,
            content: NodeKind::Text { text: joined },
        }) = self.doc.nodes.get_mut(last)
        else {
            return false;
        };
        let start = range.start as u64;
        let dollar = |i: Option<usize>| i.and_then(|i| bytes.get(i)) == Some(&b'$');
        if sibling.as_ref() != Some(id)
            || prev.end != start
            || !(dollar(Some(range.start)) || dollar(range.start.checked_sub(1)))
        {
            return false;
        }
        joined.push_str(text);
        prev.end = range.end as u64;
        true
    }
    fn inline_parent(&mut self, range: &Range<usize>) -> Result<(), ParseError> {
        // Block containers never hold inlines directly; wrap them as tight items do.
        if self.stack.last().is_some_and(|f| {
            matches!(
                self.doc.nodes[f.node].content,
                NodeKind::ListItem { .. }
                    | NodeKind::FootnoteDefinition { .. }
                    | NodeKind::Alert { .. }
            )
        }) {
            let node = self.add(
                range.clone(),
                NodeKind::Paragraph {
                    children: Vec::new(),
                },
            )?;
            self.stack.push(Frame {
                node,
                end: None,
                last_end: 0,
            });
        }
        if let Some(f) = self.stack.iter().rev().find(|f| f.end.is_none()) {
            self.doc.nodes[f.node].span.end = self.doc.nodes[f.node].span.end.max(range.end as u64);
        }
        Ok(())
    }
    fn event(
        &mut self,
        event: Event<'_>,
        range: Range<usize>,
        definition: Option<SourceSpan>,
    ) -> Result<(), ParseError> {
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
                    // Image alt keeps unrendered math lexemes readable.
                    Event::InlineMath(_) | Event::DisplayMath(_) => self.source.get(range.clone()),
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
            Event::Start(tag) => self.start(tag, range, definition)?,
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
                    Event::InlineMath(_) => NodeKind::UnsupportedInline {
                        feature: "inline_math".into(),
                        reason: UnsupportedReason::Unavailable,
                    },
                    Event::DisplayMath(_) => NodeKind::UnsupportedInline {
                        feature: "display_math".into(),
                        reason: UnsupportedReason::Unavailable,
                    },
                    // Canonicalized against the first definition after parsing.
                    Event::FootnoteReference(label) => NodeKind::FootnoteReference {
                        label: self.text(&label)?,
                    },
                    _ => return Err(ParseError::SourceMapping),
                };
                if content.is_inline() {
                    self.inline_parent(&range)?;
                } else if self.stack.last().is_some_and(|f| f.end.is_none()) {
                    self.stack.pop();
                }
                if let NodeKind::Text { text } = &content
                    && self.merge_dollar_text(&range, text)
                {
                    return Ok(());
                }
                let diagnostic = match &content {
                    NodeKind::UnsupportedInline { feature, .. } if feature == "raw_html" => {
                        Some("raw_html_disabled")
                    }
                    NodeKind::UnsupportedInline { .. } => Some("math_unavailable"),
                    _ => None,
                };
                let reference = matches!(content, NodeKind::FootnoteReference { .. });
                let i = self.add(range, content)?;
                if reference {
                    self.footnote_references.push(i);
                }
                if let Some(code) = diagnostic {
                    self.diagnostic(
                        code,
                        self.doc.nodes[i].span,
                        Some(self.doc.nodes[i].id.clone()),
                    );
                }
            }
        }
        Ok(())
    }
    fn start(
        &mut self,
        tag: Tag<'_>,
        range: Range<usize>,
        definition: Option<SourceSpan>,
    ) -> Result<(), ParseError> {
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
            Tag::BlockQuote(None) => NodeKind::BlockQuote {
                children: Vec::new(),
            },
            Tag::BlockQuote(Some(kind)) => {
                self.doc.capabilities.insert(Capability::Alerts);
                NodeKind::Alert {
                    category: match kind {
                        BlockQuoteKind::Note => AlertKind::Note,
                        BlockQuoteKind::Tip => AlertKind::Tip,
                        BlockQuoteKind::Important => AlertKind::Important,
                        BlockQuoteKind::Warning => AlertKind::Warning,
                        BlockQuoteKind::Caution => AlertKind::Caution,
                    },
                    children: Vec::new(),
                }
            }
            Tag::FootnoteDefinition(label) => {
                let key = UniCase::new(self.text(&label)?);
                let duplicate = self.footnote_labels.contains_key(&key);
                if duplicate || label.trim().is_empty() {
                    capture = Some(Capture::Fallback);
                    diagnostic = Some(if duplicate {
                        "footnote_definition_duplicate"
                    } else {
                        "footnote_definition_invalid"
                    });
                    NodeKind::UnsupportedBlock {
                        feature: "footnote_definition".into(),
                        reason: UnsupportedReason::InvalidInput,
                    }
                } else {
                    let label = self.text(&label)?;
                    self.footnote_labels.insert(key, label.clone());
                    self.doc.capabilities.insert(Capability::Footnotes);
                    NodeKind::FootnoteDefinition {
                        label,
                        children: Vec::new(),
                    }
                }
            }
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
                ) && !(self.references && definition.is_some())
                {
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
                    if let Some(definition) = definition {
                        self.link_definitions.insert(target.clone(), definition);
                    }
                    let intent_span = if let Some(definition) = definition {
                        reference_destination(self.source, definition)?
                    } else {
                        span(range.clone())
                    };
                    let disposition = classify_link(&destination);
                    self.doc.links.push(LinkIntent {
                        id: target.clone(),
                        span: intent_span,
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
                ) && !(self.references && definition.is_some())
                {
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
                    if let Some(definition) = definition {
                        self.resource_definitions
                            .insert(resource.clone(), definition);
                    }
                    let intent_span = if let Some(definition) = definition {
                        reference_destination(self.source, definition)?
                    } else {
                        span(range.clone())
                    };
                    self.doc.resources.push(ResourceIntent {
                        id: resource.clone(),
                        span: intent_span,
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
                last_end: 0,
            });
        }
        Ok(())
    }
    /// Point each reference at its first definition's exact label, using the
    /// parser's own case folding. A reference the parser accepted but whose
    /// definition became a fallback stays visible as inert source.
    fn resolve_footnotes(&mut self) -> Result<(), ParseError> {
        for i in std::mem::take(&mut self.footnote_references) {
            let NodeKind::FootnoteReference { label } = &self.doc.nodes[i].content else {
                return Err(ParseError::SourceMapping);
            };
            match self.footnote_labels.get(&UniCase::new(label.clone())) {
                Some(canonical) if canonical == label => {}
                Some(canonical) => {
                    let canonical = canonical.clone();
                    let canonical = self.text(&canonical)?;
                    self.doc.nodes[i].content = NodeKind::FootnoteReference { label: canonical };
                }
                None => {
                    self.doc.nodes[i].content = NodeKind::UnsupportedInline {
                        feature: "footnote_reference".into(),
                        reason: UnsupportedReason::InvalidInput,
                    };
                    self.diagnostic(
                        "footnote_reference_unresolved",
                        self.doc.nodes[i].span,
                        Some(self.doc.nodes[i].id.clone()),
                    );
                }
            }
        }
        Ok(())
    }
    /// Some upstream tab-expanded container offsets violate child containment
    /// or sibling order. Replace each top-level block whose subtree has such an
    /// offset with one inert source block over that block's own exact span, and
    /// drop the subtree's intents, outline entries and diagnostics. Other blocks
    /// keep full semantics. Unordered roots are left to the whole-body fallback.
    fn localize_source_mapping(&mut self) {
        if !self.offset_conflict {
            return;
        }
        let nodes = &self.doc.nodes;
        let index: HashMap<&str, usize> = nodes
            .iter()
            .enumerate()
            .map(|(i, n)| (n.id.0.as_str(), i))
            .collect();
        let ordered = |ids: &[NodeId]| {
            let mut end = 0;
            ids.iter().all(|id| {
                let s = nodes[index[id.0.as_str()]].span;
                let ok = s.start >= end;
                end = s.end;
                ok
            })
        };
        if !ordered(&self.doc.roots) {
            return;
        }
        let consistent = |root: usize| {
            let mut todo = vec![root];
            while let Some(i) = todo.pop() {
                let (span, children) = (nodes[i].span, nodes[i].content.children());
                if !ordered(children) {
                    return false;
                }
                for id in children {
                    let child = index[id.0.as_str()];
                    let c = nodes[child].span;
                    if c.start < span.start || c.end > span.end {
                        return false;
                    }
                    todo.push(child);
                }
            }
            true
        };
        let bad: Vec<usize> = self
            .doc
            .roots
            .iter()
            .map(|id| index[id.0.as_str()])
            .filter(|&i| !consistent(i))
            .collect();
        if bad.is_empty() {
            return;
        }
        let mut removed = vec![false; nodes.len()];
        let (mut links, mut resources, mut labels) =
            (BTreeSet::new(), BTreeSet::new(), BTreeSet::new());
        for &root in &bad {
            let mut todo: Vec<usize> = nodes[root]
                .content
                .children()
                .iter()
                .map(|id| index[id.0.as_str()])
                .collect();
            while let Some(i) = todo.pop() {
                removed[i] = true;
                match &nodes[i].content {
                    NodeKind::Link { target, .. } => {
                        links.insert(target.clone());
                    }
                    NodeKind::Image { resource, .. } => {
                        resources.insert(resource.clone());
                    }
                    NodeKind::FootnoteDefinition { label, .. } => {
                        labels.insert(label.clone());
                    }
                    _ => {}
                }
                todo.extend(
                    nodes[i]
                        .content
                        .children()
                        .iter()
                        .map(|id| index[id.0.as_str()]),
                );
            }
            if let NodeKind::FootnoteDefinition { label, .. } = &nodes[root].content {
                labels.insert(label.clone());
            }
        }
        let fallbacks: Vec<(NodeId, SourceSpan)> = bad
            .iter()
            .map(|&i| (nodes[i].id.clone(), nodes[i].span))
            .collect();
        let replaced: BTreeSet<NodeId> = fallbacks.iter().map(|(id, _)| id.clone()).collect();
        let dropped: BTreeSet<NodeId> = (0..nodes.len())
            .filter(|&i| removed[i])
            .map(|i| nodes[i].id.clone())
            .collect();
        for &root in &bad {
            self.doc.nodes[root].content = NodeKind::UnsupportedBlock {
                feature: "unrepresentable_source_mapping".into(),
                reason: UnsupportedReason::InvalidInput,
            };
        }
        let mut keep = removed.iter().map(|r| !r);
        self.doc.nodes.retain(|_| keep.next().unwrap_or(true));
        self.doc.links.retain(|l| !links.contains(&l.id));
        self.doc.resources.retain(|r| !resources.contains(&r.id));
        self.doc
            .outline
            .retain(|e| !replaced.contains(&e.node) && !dropped.contains(&e.node));
        self.doc.diagnostics.retain(|d| {
            d.node
                .as_ref()
                .is_none_or(|n| !replaced.contains(n) && !dropped.contains(n))
        });
        for (id, span) in fallbacks {
            self.doc.diagnostics.push(Diagnostic {
                code: "source_mapping_fallback".into(),
                severity: Severity::Warning,
                message: "Container source offsets cannot be represented; this block is preserved as inert source.".into(),
                span: Some(span),
                node: Some(id),
            });
        }
        // A surviving reference whose definition was removed reads as source.
        for i in 0..self.doc.nodes.len() {
            if let NodeKind::FootnoteReference { label } = &self.doc.nodes[i].content
                && labels.contains(label)
            {
                self.doc.nodes[i].content = NodeKind::UnsupportedInline {
                    feature: "footnote_reference".into(),
                    reason: UnsupportedReason::InvalidInput,
                };
                let (span, id) = (self.doc.nodes[i].span, self.doc.nodes[i].id.clone());
                self.diagnostic("footnote_reference_unresolved", span, Some(id));
            }
        }
        let mut capabilities: BTreeSet<Capability> = self
            .doc
            .capabilities
            .iter()
            .copied()
            .filter(|c| *c == Capability::FrontMatter)
            .collect();
        for n in &self.doc.nodes {
            capabilities.extend(match &n.content {
                NodeKind::ListItem {
                    checked: Some(_), ..
                } => Some(Capability::TaskLists),
                NodeKind::Table { .. } => Some(Capability::Tables),
                NodeKind::FootnoteDefinition { .. } | NodeKind::FootnoteReference { .. } => {
                    Some(Capability::Footnotes)
                }
                NodeKind::Strikethrough { .. } => Some(Capability::Strikethrough),
                NodeKind::Alert { .. } => Some(Capability::Alerts),
                _ => None,
            });
        }
        self.doc.capabilities = capabilities;
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
            if self.link_definitions.contains_key(target) {
                return Ok(());
            }
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
            self.doc.links[pos].span = destination_span(
                self.source,
                self.doc.nodes[node].span,
                label_end,
                self.stack
                    .iter()
                    .filter(|f| {
                        matches!(
                            self.doc.nodes[f.node].content,
                            NodeKind::BlockQuote { .. } | NodeKind::Alert { .. }
                        )
                    })
                    .count(),
            )?;
        }
        Ok(())
    }
    fn finish_image_span(&mut self, node: usize, label_end: usize) -> Result<(), ParseError> {
        if let NodeKind::Image { resource, .. } = &self.doc.nodes[node].content {
            if self.resource_definitions.contains_key(resource) {
                return Ok(());
            }
            let pos = resource.0[1..]
                .parse::<usize>()
                .map_err(|_| ParseError::SourceMapping)?;
            self.doc.resources[pos].span = destination_span(
                self.source,
                self.doc.nodes[node].span,
                label_end,
                self.stack
                    .iter()
                    .filter(|f| {
                        matches!(
                            self.doc.nodes[f.node].content,
                            NodeKind::BlockQuote { .. } | NodeKind::Alert { .. }
                        )
                    })
                    .count(),
            )?;
        }
        Ok(())
    }
}

fn destination_span(
    source: &str,
    whole: SourceSpan,
    label_end: usize,
    quotes: usize,
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
    scan_destination(source, label_end + relative, end, quotes)
}
fn scan_destination(
    source: &str,
    mut pos: usize,
    end: usize,
    quotes: usize,
) -> Result<SourceSpan, ParseError> {
    loop {
        let mut continuation = false;
        while source
            .as_bytes()
            .get(pos)
            .is_some_and(u8::is_ascii_whitespace)
        {
            continuation |= source.as_bytes()[pos] == b'\n';
            pos += 1;
        }
        if continuation {
            for _ in 0..quotes {
                if source.as_bytes().get(pos) != Some(&b'>') {
                    break;
                }
                pos += 1;
                while matches!(source.as_bytes().get(pos), Some(b' ' | b'\t')) {
                    pos += 1;
                }
            }
        }
        if !source
            .as_bytes()
            .get(pos)
            .is_some_and(u8::is_ascii_whitespace)
        {
            break;
        }
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
    if !angled && depth == 0 {
        Ok(span(begin..end))
    } else {
        Err(ParseError::SourceMapping)
    }
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

fn reference_destination(source: &str, definition: SourceSpan) -> Result<SourceSpan, ParseError> {
    let raw = definition.slice(source).ok_or(ParseError::SourceMapping)?;
    let start = raw.find('[').ok_or(ParseError::SourceMapping)?;
    let bytes = raw.as_bytes();
    let mut i = start + 1;
    while i + 1 < bytes.len() {
        if bytes[i] == b'\\' {
            i += 2;
            continue;
        }
        if bytes[i] == b']' && bytes[i + 1] == b':' {
            return scan_destination(
                source,
                definition.start as usize + i + 2,
                definition.end as usize,
                source[..definition.start as usize]
                    .rsplit('\n')
                    .next()
                    .unwrap_or("")
                    .chars()
                    .filter(|&c| c == '>')
                    .count(),
            );
        }
        i += 1;
    }
    Err(ParseError::SourceMapping)
}
#[cfg(feature = "model-v2")]
fn finish_v2(
    mut b: Builder<'_>,
    source: &str,
    limits: ParseLimits,
) -> Result<crate::v2::Document, ParseError> {
    use crate::v2;
    let mut links = Vec::new();
    let mut resources = Vec::new();
    for l in std::mem::take(&mut b.doc.links) {
        let target = v2::classify(&l.destination, false);
        charge_target(&mut b, &target)?;
        let provenance = match b.link_definitions.remove(&l.id) {
            Some(definition) => v2::IntentSource::Reference {
                definition,
                span: l.span,
            },
            None => v2::IntentSource::Inline { span: l.span },
        };
        links.push(v2::LinkIntent {
            id: l.id,
            source: provenance,
            destination: l.destination,
            target,
        });
    }
    for r in std::mem::take(&mut b.doc.resources) {
        let target = v2::classify(&r.destination, true);
        charge_target(&mut b, &target)?;
        let provenance = match b.resource_definitions.remove(&r.id) {
            Some(definition) => v2::IntentSource::Reference {
                definition,
                span: r.span,
            },
            None => v2::IntentSource::Inline { span: r.span },
        };
        resources.push(v2::ResourceIntent {
            id: r.id,
            source: provenance,
            destination: r.destination,
            target,
        });
    }
    let identity = (b.doc.document_id.clone(), b.doc.revision_id.clone());
    let source_info = b.doc.source.clone();
    let metadata = b.doc.metadata.clone();
    let mut diagnostics: Vec<_> = b
        .doc
        .diagnostics
        .iter()
        .filter(|d| {
            d.node.is_none()
                && (d.span == metadata.front_matter_span
                    || d.code == "reference_definition_shadowed")
        })
        .cloned()
        .collect();
    match v2::Document::new(
        v2::DocumentInput::from_structure(b.doc, links, resources),
        source,
        limits.model,
    ) {
        Err(error) if matches!(error.code, "child_span" | "sibling_order") => {
            let body = SourceSpan {
                start: metadata.front_matter_span.map_or(0, |s| s.end),
                end: source.len() as u64,
            };
            let id = NodeId("n0".into());
            diagnostics.push(Diagnostic{code:"source_mapping_fallback".into(),severity:Severity::Warning,message:"Container source offsets cannot be represented; the body is preserved as inert source.".into(),span:Some(body),node:Some(id.clone())});
            let capabilities = if metadata.front_matter_span.is_some() {
                BTreeSet::from([Capability::FrontMatter])
            } else {
                BTreeSet::new()
            };
            let d = DocumentInput {
                model_version: 1,
                document_id: identity.0,
                revision_id: identity.1,
                source: source_info,
                capabilities,
                roots: vec![id.clone()],
                nodes: vec![Node {
                    id,
                    span: body,
                    content: NodeKind::UnsupportedBlock {
                        feature: "unrepresentable_source_mapping".into(),
                        reason: UnsupportedReason::InvalidInput,
                    },
                }],
                outline: Vec::new(),
                metadata,
                links: Vec::new(),
                resources: Vec::new(),
                diagnostics,
            };
            v2::Document::new(
                v2::DocumentInput::from_structure(d, Vec::new(), Vec::new()),
                source,
                limits.model,
            )
            .map_err(ParseError::Model)
        }
        result => result.map_err(ParseError::Model),
    }
}
#[cfg(feature = "model-v2")]
fn charge_target(b: &mut Builder<'_>, t: &crate::v2::Target) -> Result<(), ParseError> {
    let len = match t {
        crate::v2::Target::Fragment { anchor } => anchor.len(),
        crate::v2::Target::External { uri } => uri.len(),
        crate::v2::Target::RequiresCapability { path, fragment } => {
            path.len() + fragment.as_ref().map_or(0, String::len)
        }
        _ => 0,
    };
    b.copied = b.copied.checked_add(len).ok_or(ParseError::TextLimit)?;
    if b.copied > b.limits.max_text_bytes {
        return Err(ParseError::TextLimit);
    }
    Ok(())
}

#[cfg(feature = "model-v2")]
pub(crate) struct IntentFact {
    pub destination: String,
    pub source: crate::v2::IntentSource,
}
#[cfg(feature = "model-v2")]
pub(crate) fn intent_facts(
    source: &str,
    limits: ModelLimits,
) -> Result<BTreeMap<(u64, u64, bool), IntentFact>, ModelError> {
    use crate::v2::IntentSource;
    struct Active {
        depth: usize,
        whole: SourceSpan,
        image: bool,
        destination: String,
        definition: Option<SourceSpan>,
        label_end: usize,
        quotes: usize,
    }
    let fail = |code| ModelError {
        code,
        path: "document".into(),
    };
    let offset = front_matter_end(source).unwrap_or(0);
    let mut events = Parser::new_ext(&source[offset..], PARSER_OPTIONS).into_offset_iter();
    let mut facts = BTreeMap::new();
    let mut active: Vec<Active> = Vec::new();
    let mut depth = 0usize;
    let mut quotes = 0usize;
    let mut mask = None;
    let mut copied = 0usize;
    // Finite source/event iterator, source byte cap and depth/fact/string caps
    // bound this pass. Node count is not an event budget (image alt is captured).
    while let Some((event, range)) = events.next() {
        let end_event = matches!(event, Event::End(_));
        for a in &mut active {
            if !(end_event && a.depth == depth) {
                a.label_end = a.label_end.max(range.end + offset);
            }
        }
        match event {
            Event::Start(tag) => {
                depth += 1;
                if depth > limits.max_depth {
                    return Err(fail("source_depth_limit"));
                }
                if matches!(tag, Tag::BlockQuote(_)) {
                    quotes += 1;
                }
                if mask.is_some() {
                    continue;
                }
                let image = matches!(tag, Tag::Image { .. });
                if let Tag::Link {
                    link_type,
                    dest_url,
                    id,
                    ..
                }
                | Tag::Image {
                    link_type,
                    dest_url,
                    id,
                    ..
                } = tag
                {
                    if !image && !active.is_empty() {
                        mask = Some(depth);
                        continue;
                    }
                    if image {
                        mask = Some(depth);
                    }
                    let definition = if matches!(
                        link_type,
                        LinkType::Reference | LinkType::Collapsed | LinkType::Shortcut
                    ) {
                        let d = events
                            .reference_definitions()
                            .get(&id)
                            .ok_or_else(|| fail("source_provenance"))?;
                        Some(span(d.span.start + offset..d.span.end + offset))
                    } else {
                        None
                    };
                    copied = copied
                        .checked_add(
                            dest_url.len() + if link_type == LinkType::Email { 7 } else { 0 },
                        )
                        .ok_or_else(|| fail("source_text_limit"))?;
                    if copied > 64 * 1024 * 1024 {
                        return Err(fail("source_text_limit"));
                    }
                    let destination = if link_type == LinkType::Email {
                        format!("mailto:{dest_url}")
                    } else {
                        dest_url.into_string()
                    };
                    active.push(Active {
                        depth,
                        whole: span(range.start + offset..range.end + offset),
                        image,
                        destination,
                        definition,
                        label_end: range.start + offset + if image { 2 } else { 1 },
                        quotes,
                    });
                }
            }
            Event::End(end) => {
                if active.last().is_some_and(|a| a.depth == depth) {
                    let a = active.pop().ok_or_else(|| fail("source_provenance"))?;
                    let provenance = if let Some(definition) = a.definition {
                        IntentSource::Reference {
                            definition,
                            span: reference_destination(source, definition)
                                .map_err(|_| fail("source_provenance"))?,
                        }
                    } else {
                        IntentSource::Inline {
                            span: destination_span(source, a.whole, a.label_end, a.quotes)
                                .map_err(|_| fail("source_provenance"))?,
                        }
                    };
                    if !lexeme_matches(source, provenance.span(), &a.destination) {
                        return Err(fail("source_provenance"));
                    }
                    if facts.len() >= limits.max_nodes {
                        return Err(fail("source_intent_limit"));
                    }
                    if facts
                        .insert(
                            (a.whole.start, a.whole.end, a.image),
                            IntentFact {
                                destination: a.destination,
                                source: provenance,
                            },
                        )
                        .is_some()
                    {
                        return Err(fail("source_provenance"));
                    }
                }
                if mask == Some(depth) {
                    mask = None;
                }
                if matches!(end, TagEnd::BlockQuote(_)) {
                    quotes = quotes
                        .checked_sub(1)
                        .ok_or_else(|| fail("source_provenance"))?;
                }
                depth = depth
                    .checked_sub(1)
                    .ok_or_else(|| fail("source_provenance"))?;
            }
            _ => {}
        }
    }
    if !active.is_empty() || depth != 0 {
        return Err(fail("source_provenance"));
    }
    Ok(facts)
}
#[cfg(feature = "model-v2")]
fn lexeme_matches(source: &str, span: SourceSpan, destination: &str) -> bool {
    let Some(raw) = span.slice(source) else {
        return false;
    };
    if raw == destination || destination.strip_prefix("mailto:") == Some(raw) {
        return true;
    }
    // Use the pinned parser for CommonMark backslash/entity decoding. Requiring
    // the whole synthetic link prevents candidate text from closing/injecting a
    // second construct. This is bounded by the already bounded source lexeme.
    [format!("[x]({raw})"),format!("[x](<{raw}>)")].iter().any(|sample| {
        Parser::new(sample).into_offset_iter().any(|(event,range)|matches!(event,Event::Start(Tag::Link{dest_url,..}) if range.start==0&&range.end==sample.len()&&dest_url.as_ref()==destination))
    })
}
