use crate::*;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

/// Caller-adjustable safety ceilings, not performance or release budgets.
#[derive(Clone, Copy, Debug)]
pub struct ModelLimits {
    pub max_nodes: usize,
    pub max_depth: usize,
    pub max_wire_bytes: usize,
    pub max_source_bytes: usize,
}
impl Default for ModelLimits {
    fn default() -> Self {
        Self {
            max_nodes: 1_000_000,
            max_depth: 256,
            max_wire_bytes: 64 * 1024 * 1024,
            max_source_bytes: 64 * 1024 * 1024,
        }
    }
}

/// Error paths use structural indices, never document contents or private paths.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelError {
    pub code: &'static str,
    pub path: String,
}
impl fmt::Display for ModelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at {}", self.code, self.path)
    }
}
impl std::error::Error for ModelError {}
fn err(code: &'static str, path: impl Into<String>) -> ModelError {
    ModelError {
        code,
        path: path.into(),
    }
}
fn require(ok: bool, code: &'static str, path: impl Into<String>) -> Result<(), ModelError> {
    require_lazy(ok, code, || path)
}
fn require_lazy<P: Into<String>>(
    ok: bool,
    code: &'static str,
    path: impl FnOnce() -> P,
) -> Result<(), ModelError> {
    if ok { Ok(()) } else { Err(err(code, path())) }
}
fn id_valid(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_.:-".contains(&b))
}
fn span_valid(span: SourceSpan, source: &str) -> bool {
    let (Ok(start), Ok(end)) = (usize::try_from(span.start), usize::try_from(span.end)) else {
        return false;
    };
    start <= end
        && end <= source.len()
        && source.is_char_boundary(start)
        && source.is_char_boundary(end)
}
fn contains(outer: SourceSpan, inner: SourceSpan) -> bool {
    outer.start <= inner.start && inner.end <= outer.end
}
fn text_present(value: &str) -> bool {
    !value.trim().is_empty()
}
fn relative(value: &str) -> bool {
    !value.is_empty()
        && !value.starts_with(['/', '\\', '#'])
        && !value.contains([':', '\\'])
        && !value.chars().any(char::is_control)
}
fn external(value: &str) -> bool {
    if value.chars().any(char::is_control) || value.contains('\\') {
        return false;
    }
    let lower = value.to_ascii_lowercase();
    ["https://", "http://", "mailto:"].iter().any(|prefix| {
        lower
            .strip_prefix(prefix)
            .is_some_and(|tail| !tail.is_empty())
    })
}

/// Immutable validated snapshot. Deserialize via `from_json`, not unchecked serde.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct Document {
    input: DocumentInput,
}
impl Document {
    pub fn new(
        input: DocumentInput,
        source: &str,
        limits: ModelLimits,
    ) -> Result<Self, ModelError> {
        validate(&input, source, limits)?;
        Ok(Self { input })
    }
    pub fn from_json(bytes: &[u8], source: &str, limits: ModelLimits) -> Result<Self, ModelError> {
        require(
            bytes.len() <= limits.max_wire_bytes,
            "wire_limit",
            "document",
        )?;
        let input = serde_json::from_slice(bytes).map_err(|_| err("invalid_json", "document"))?;
        Self::new(input, source, limits)
    }
    pub fn to_json(&self) -> Result<Vec<u8>, ModelError> {
        serde_json::to_vec(&self.input).map_err(|_| err("serialization", "document"))
    }
    pub fn input(&self) -> &DocumentInput {
        &self.input
    }
    pub fn into_input(self) -> DocumentInput {
        self.input
    }
}

fn validate(d: &DocumentInput, source: &str, limits: ModelLimits) -> Result<(), ModelError> {
    require(
        d.model_version == MODEL_VERSION,
        "model_version",
        "model_version",
    )?;
    require(id_valid(&d.document_id.0), "invalid_id", "document_id")?;
    require(id_valid(&d.revision_id.0), "invalid_id", "revision_id")?;
    require(
        source.len() <= limits.max_source_bytes,
        "source_limit",
        "source",
    )?;
    require(
        d.source == SourceInfo::of(source),
        "source_mismatch",
        "source",
    )?;
    require(d.nodes.len() <= limits.max_nodes, "node_limit", "nodes")?;
    let mut index = BTreeMap::new();
    let mut caps = BTreeSet::new();
    let mut anchors = BTreeSet::new();
    let mut footnotes = BTreeSet::new();
    for (i, node) in d.nodes.iter().enumerate() {
        let path = || format!("nodes[{i}]");
        require_lazy(id_valid(&node.id.0), "invalid_id", || {
            format!("nodes[{i}].id")
        })?;
        require_lazy(index.insert(&node.id, i).is_none(), "duplicate_id", || {
            format!("nodes[{i}].id")
        })?;
        require_lazy(span_valid(node.span, source), "invalid_span", || {
            format!("nodes[{i}].span")
        })?;
        match &node.content {
            NodeKind::Heading { level, anchor, .. } => {
                require_lazy((1..=6).contains(level), "heading_level", path)?;
                require_lazy(
                    !anchor.is_empty() && !anchor.chars().any(char::is_whitespace),
                    "heading_anchor",
                    path,
                )?;
                require_lazy(anchors.insert(anchor), "duplicate_anchor", path)?;
            }
            NodeKind::List {
                ordered,
                start,
                items,
                ..
            } => {
                require_lazy(
                    *ordered == start.is_some() && !items.is_empty(),
                    "list_shape",
                    path,
                )?;
            }
            NodeKind::ListItem {
                checked: Some(_), ..
            } => {
                caps.insert(Capability::TaskLists);
            }
            NodeKind::Table { alignments, rows } => {
                require_lazy(
                    !alignments.is_empty() && !rows.is_empty(),
                    "table_shape",
                    path,
                )?;
                caps.insert(Capability::Tables);
            }
            NodeKind::FootnoteDefinition { label, .. } => {
                require_lazy(
                    text_present(label) && footnotes.insert(label),
                    "footnote_label",
                    path,
                )?;
                caps.insert(Capability::Footnotes);
            }
            NodeKind::FootnoteReference { .. } => {
                caps.insert(Capability::Footnotes);
            }
            NodeKind::Strikethrough { .. } => {
                caps.insert(Capability::Strikethrough);
            }
            NodeKind::Alert { .. } => {
                caps.insert(Capability::Alerts);
            }
            NodeKind::UnsupportedBlock { feature, .. }
            | NodeKind::UnsupportedInline { feature, .. } => {
                require_lazy(text_present(feature), "unsupported_feature", path)?;
            }
            _ => {}
        }
    }
    let mut parents = vec![0usize; d.nodes.len()];
    let resolve = |id: &NodeId| {
        index
            .get(id)
            .copied()
            .ok_or_else(|| err("unknown_node", "tree"))
    };
    let ordered = |ids: &[NodeId]| -> Result<(), ModelError> {
        let mut end = 0;
        for id in ids {
            let node = &d.nodes[resolve(id)?];
            require(node.span.start >= end, "sibling_order", "tree")?;
            end = node.span.end;
        }
        Ok(())
    };
    ordered(&d.roots)?;
    for root in &d.roots {
        let i = resolve(root)?;
        parents[i] += 1;
        require(d.nodes[i].content.is_block(), "root_kind", "roots")?;
    }
    for (i, node) in d.nodes.iter().enumerate() {
        let ids = node.content.children();
        ordered(ids)?;
        for (position, id) in ids.iter().enumerate() {
            let child_index = resolve(id)?;
            parents[child_index] += 1;
            let child = &d.nodes[child_index];
            require_lazy(contains(node.span, child.span), "child_span", || {
                format!("nodes[{i}]")
            })?;
            let category = match &node.content {
                NodeKind::List { .. } => matches!(child.content, NodeKind::ListItem { .. }),
                NodeKind::Table { alignments, .. } => {
                    matches!(&child.content, NodeKind::TableRow { header, cells }
                        if *header == (position == 0) && cells.len() == alignments.len())
                }
                NodeKind::TableRow { .. } => matches!(child.content, NodeKind::TableCell { .. }),
                NodeKind::BlockQuote { .. }
                | NodeKind::ListItem { .. }
                | NodeKind::FootnoteDefinition { .. }
                | NodeKind::Alert { .. } => child.content.is_block(),
                _ => child.content.is_inline(),
            };
            require_lazy(category, "child_kind", || format!("nodes[{i}]"))?;
        }
        if let NodeKind::FootnoteReference { label } = &node.content {
            require_lazy(footnotes.contains(label), "unknown_footnote", || {
                format!("nodes[{i}]")
            })?;
        }
    }
    require(parents.iter().all(|n| *n == 1), "parent_count", "tree")?;
    let mut stack: Vec<_> = d.roots.iter().rev().map(|id| (id, 1usize, false)).collect();
    let mut visited = BTreeSet::new();
    let mut headings = Vec::new();
    while let Some((id, depth, inside_link)) = stack.pop() {
        let i = resolve(id)?;
        require(depth <= limits.max_depth, "depth_limit", "tree")?;
        require(visited.insert(i), "cycle", "tree")?;
        let node = &d.nodes[i];
        let link = matches!(node.content, NodeKind::Link { .. });
        require_lazy(!(inside_link && link), "nested_link", || {
            format!("nodes[{i}]")
        })?;
        if matches!(node.content, NodeKind::Heading { .. }) {
            headings.push(i);
        }
        stack.extend(
            node.content
                .children()
                .iter()
                .rev()
                .map(|child| (child, depth + 1, inside_link || link)),
        );
    }
    require(visited.len() == d.nodes.len(), "unreachable_node", "tree")?;
    require(
        headings.len() == d.outline.len(),
        "outline_coverage",
        "outline",
    )?;
    for (heading, entry) in headings.into_iter().zip(&d.outline) {
        let node = &d.nodes[heading];
        if let NodeKind::Heading { level, anchor, .. } = &node.content {
            require(
                entry.node == node.id && entry.level == *level && entry.anchor == *anchor,
                "outline_mismatch",
                "outline",
            )?;
        }
    }
    if let Some(span) = d.metadata.front_matter_span {
        require(
            span_valid(span, source) && span.start == 0,
            "front_matter_span",
            "metadata",
        )?;
        require(
            d.roots
                .first()
                .is_none_or(|id| d.nodes[index[id]].span.start >= span.end),
            "front_matter_overlap",
            "metadata",
        )?;
        caps.insert(Capability::FrontMatter);
    } else {
        require(
            d.metadata.properties.is_empty(),
            "metadata_without_front_matter",
            "metadata",
        )?;
    }
    require(
        d.metadata.properties.keys().all(|k| text_present(k)),
        "metadata_key",
        "metadata",
    )?;
    require(d.capabilities == caps, "capabilities", "capabilities")?;
    validate_intents(d, source)?;
    for (i, diagnostic) in d.diagnostics.iter().enumerate() {
        require_lazy(
            text_present(&diagnostic.code) && text_present(&diagnostic.message),
            "diagnostic_text",
            || format!("diagnostics[{i}]"),
        )?;
        require_lazy(
            diagnostic.span.is_none_or(|s| span_valid(s, source)),
            "invalid_span",
            || format!("diagnostics[{i}].span"),
        )?;
        require_lazy(
            diagnostic
                .node
                .as_ref()
                .is_none_or(|id| index.contains_key(id)),
            "unknown_node",
            || format!("diagnostics[{i}].node"),
        )?;
    }
    Ok(())
}

fn validate_intents(d: &DocumentInput, source: &str) -> Result<(), ModelError> {
    let mut links = BTreeMap::new();
    for (i, link) in d.links.iter().enumerate() {
        let path = || format!("links[{i}]");
        require_lazy(
            id_valid(&link.id.0) && links.insert(&link.id, i).is_none(),
            "intent_id",
            path,
        )?;
        require_lazy(span_valid(link.span, source), "invalid_span", path)?;
        let allowed = match &link.disposition {
            LinkDisposition::Fragment { anchor } => {
                !anchor.is_empty() && link.destination == format!("#{anchor}")
            }
            LinkDisposition::RequiresCapability {} => relative(&link.destination),
            LinkDisposition::External {} => external(&link.destination),
            LinkDisposition::Denied { reason } => text_present(reason),
        };
        require_lazy(allowed, "link_policy", path)?;
    }
    let mut resources = BTreeMap::new();
    for (i, resource) in d.resources.iter().enumerate() {
        let path = || format!("resources[{i}]");
        require_lazy(
            id_valid(&resource.id.0) && resources.insert(&resource.id, i).is_none(),
            "intent_id",
            path,
        )?;
        require_lazy(span_valid(resource.span, source), "invalid_span", path)?;
        let allowed = match &resource.access {
            ResourceAccess::RequiresCapability {} => relative(&resource.destination),
            ResourceAccess::Denied { reason } => text_present(reason),
        };
        require_lazy(allowed, "resource_policy", path)?;
    }
    let mut link_uses = vec![0; d.links.len()];
    let mut resource_uses = vec![0; d.resources.len()];
    for (i, node) in d.nodes.iter().enumerate() {
        match &node.content {
            NodeKind::Link { target, .. } => {
                let pos = *links
                    .get(target)
                    .ok_or_else(|| err("unknown_link", format!("nodes[{i}]")))?;
                link_uses[pos] += 1;
                require_lazy(
                    contains(node.span, d.links[pos].span),
                    "intent_span",
                    || format!("nodes[{i}]"),
                )?;
            }
            NodeKind::Image { resource, .. } => {
                let pos = *resources
                    .get(resource)
                    .ok_or_else(|| err("unknown_resource", format!("nodes[{i}]")))?;
                resource_uses[pos] += 1;
                require_lazy(
                    contains(node.span, d.resources[pos].span),
                    "intent_span",
                    || format!("nodes[{i}]"),
                )?;
            }
            _ => {}
        }
    }
    require(
        link_uses.iter().all(|n| *n == 1) && resource_uses.iter().all(|n| *n == 1),
        "intent_uses",
        "intents",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validation_paths_are_only_built_for_errors() {
        let mut calls = 0;
        require_lazy(true, "example", || {
            calls += 1;
            "unused"
        })
        .unwrap();
        assert_eq!(calls, 0);
        let error = require_lazy(false, "example", || {
            calls += 1;
            "nodes[7]"
        })
        .unwrap_err();
        assert_eq!(calls, 1);
        assert_eq!(error, err("example", "nodes[7]"));
    }
}
