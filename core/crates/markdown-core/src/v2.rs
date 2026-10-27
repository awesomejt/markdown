//! Opt-in model v2: source provenance and typed, policy-checked URI targets.
use crate::{
    Capability, Diagnostic, DocumentId, LinkId, Metadata, ModelError, ModelLimits, Node, NodeKind,
    OutlineEntry, ResourceId, RevisionId, SourceInfo, SourceSpan,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const MODEL_VERSION: u32 = 2;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum IntentSource {
    Inline {
        span: SourceSpan,
    },
    Reference {
        definition: SourceSpan,
        span: SourceSpan,
    },
}
impl IntentSource {
    pub fn span(&self) -> SourceSpan {
        match self {
            Self::Inline { span } | Self::Reference { span, .. } => *span,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum Denial {
    Unsafe,
    InvalidUri,
    UnsupportedScheme,
    RemoteResource,
    Limit,
    LegacyPolicy,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Target {
    Fragment {
        anchor: String,
    },
    RequiresCapability {
        path: String,
        fragment: Option<String>,
    },
    External {
        uri: String,
    },
    Denied {
        reason: Denial,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct LinkIntent {
    pub id: LinkId,
    pub source: IntentSource,
    pub destination: String,
    pub target: Target,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct ResourceIntent {
    pub id: ResourceId,
    pub source: IntentSource,
    pub destination: String,
    pub target: Target,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct DocumentInput {
    #[cfg_attr(feature = "schema", schemars(range(min = 2, max = 2)))]
    pub model_version: u32,
    pub document_id: DocumentId,
    pub revision_id: RevisionId,
    pub source: SourceInfo,
    #[serde(deserialize_with = "crate::model::unique_capabilities")]
    pub capabilities: BTreeSet<Capability>,
    pub roots: Vec<crate::NodeId>,
    pub nodes: Vec<Node>,
    pub outline: Vec<OutlineEntry>,
    pub metadata: Metadata,
    pub links: Vec<LinkIntent>,
    pub resources: Vec<ResourceIntent>,
    pub diagnostics: Vec<Diagnostic>,
}
impl DocumentInput {
    pub(crate) fn from_structure(
        d: crate::DocumentInput,
        links: Vec<LinkIntent>,
        resources: Vec<ResourceIntent>,
    ) -> Self {
        Self {
            model_version: MODEL_VERSION,
            document_id: d.document_id,
            revision_id: d.revision_id,
            source: d.source,
            capabilities: d.capabilities,
            roots: d.roots,
            nodes: d.nodes,
            outline: d.outline,
            metadata: d.metadata,
            links,
            resources,
            diagnostics: d.diagnostics,
        }
    }
    fn split(self) -> (crate::DocumentInput, Vec<LinkIntent>, Vec<ResourceIntent>) {
        let structure = crate::DocumentInput {
            model_version: 1,
            document_id: self.document_id,
            revision_id: self.revision_id,
            source: self.source,
            capabilities: self.capabilities,
            roots: self.roots,
            nodes: self.nodes,
            outline: self.outline,
            metadata: self.metadata,
            links: Vec::new(),
            resources: Vec::new(),
            diagnostics: self.diagnostics,
        };
        (structure, self.links, self.resources)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct Document {
    input: DocumentInput,
}
fn error(code: &'static str) -> ModelError {
    ModelError {
        code,
        path: "document".into(),
    }
}
impl Document {
    pub fn new(
        input: DocumentInput,
        source: &str,
        limits: ModelLimits,
    ) -> Result<Self, ModelError> {
        if input.model_version != MODEL_VERSION {
            return Err(error("model_version"));
        }
        let (structure, links, resources) = input.split();
        // Shares only the version-independent tree/source/metadata checks. This
        // unchecked structural DTO is never published as a validated v1 Document.
        crate::validate::validate_structure(&structure, source, limits)?;
        validate_intents(&structure, &links, &resources, source, limits)?;
        Ok(Self {
            input: DocumentInput::from_structure(structure, links, resources),
        })
    }
    pub fn from_json(bytes: &[u8], source: &str, limits: ModelLimits) -> Result<Self, ModelError> {
        if bytes.len() > limits.max_wire_bytes {
            return Err(error("wire_limit"));
        }
        let input = serde_json::from_slice(bytes).map_err(|_| error("invalid_json"))?;
        Self::new(input, source, limits)
    }
    pub fn to_json(&self) -> Result<Vec<u8>, ModelError> {
        serde_json::to_vec(&self.input).map_err(|_| error("serialization"))
    }
    pub fn input(&self) -> &DocumentInput {
        &self.input
    }
    pub fn into_input(self) -> DocumentInput {
        self.input
    }
    /// Explicit upgrade; a legacy denial can never become an actionable target.
    pub fn from_v1(
        document: crate::Document,
        source: &str,
        limits: ModelLimits,
    ) -> Result<Self, ModelError> {
        let mut d = document.into_input();
        let links = std::mem::take(&mut d.links)
            .into_iter()
            .map(|l| LinkIntent {
                target: if matches!(l.disposition, crate::LinkDisposition::Denied { .. }) {
                    denied(Denial::LegacyPolicy)
                } else {
                    classify(&l.destination, false)
                },
                id: l.id,
                source: IntentSource::Inline { span: l.span },
                destination: l.destination,
            })
            .collect();
        let resources = std::mem::take(&mut d.resources)
            .into_iter()
            .map(|r| ResourceIntent {
                target: if matches!(r.access, crate::ResourceAccess::Denied { .. }) {
                    denied(Denial::LegacyPolicy)
                } else {
                    classify(&r.destination, true)
                },
                id: r.id,
                source: IntentSource::Inline { span: r.span },
                destination: r.destination,
            })
            .collect();
        Self::new(
            DocumentInput::from_structure(d, links, resources),
            source,
            limits,
        )
    }
}
fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"_.:-".contains(&c))
}
fn contains(a: SourceSpan, b: SourceSpan) -> bool {
    a.start <= b.start && b.end <= a.end
}
fn provenance(s: &IntentSource, node: SourceSpan, source: &str) -> Result<(), ModelError> {
    let destination = s.span();
    if destination.slice(source).is_none() {
        return Err(error("invalid_span"));
    }
    match s {
        IntentSource::Inline { .. } if !contains(node, destination) => Err(error("intent_span")),
        IntentSource::Reference { definition, .. }
            if definition.slice(source).is_none()
                || !contains(*definition, destination)
                || (node.start < definition.end && definition.start < node.end) =>
        {
            Err(error("reference_span"))
        }
        _ => Ok(()),
    }
}
fn validate_intents(
    d: &crate::DocumentInput,
    links: &[LinkIntent],
    resources: &[ResourceIntent],
    source: &str,
    limits: ModelLimits,
) -> Result<(), ModelError> {
    let facts = if links.is_empty() && resources.is_empty() {
        BTreeMap::new()
    } else {
        crate::parse::intent_facts(source, limits)?
    };
    let mut link_map = BTreeMap::new();
    for (i, l) in links.iter().enumerate() {
        if !valid_id(&l.id.0) || link_map.insert(&l.id, i).is_some() {
            return Err(error("intent_id"));
        }
        if !matches!(l.target, Target::Denied { .. }) && l.target != classify(&l.destination, false)
        {
            return Err(error("link_policy"));
        }
    }
    let mut resource_map = BTreeMap::new();
    for (i, r) in resources.iter().enumerate() {
        if !valid_id(&r.id.0) || resource_map.insert(&r.id, i).is_some() {
            return Err(error("intent_id"));
        }
        if !matches!(r.target, Target::Denied { .. }) && r.target != classify(&r.destination, true)
        {
            return Err(error("resource_policy"));
        }
    }
    let mut lu = vec![0usize; links.len()];
    let mut ru = vec![0usize; resources.len()];
    for node in &d.nodes {
        match &node.content {
            NodeKind::Link { target, .. } => {
                let i = *link_map.get(target).ok_or_else(|| error("unknown_link"))?;
                lu[i] += 1;
                provenance(&links[i].source, node.span, source)?;
                let fact = facts
                    .get(&(node.span.start, node.span.end, false))
                    .ok_or_else(|| error("source_provenance"))?;
                if fact.destination != links[i].destination || fact.source != links[i].source {
                    return Err(error("source_provenance"));
                }
            }
            NodeKind::Image { resource, .. } => {
                let i = *resource_map
                    .get(resource)
                    .ok_or_else(|| error("unknown_resource"))?;
                ru[i] += 1;
                provenance(&resources[i].source, node.span, source)?;
                let fact = facts
                    .get(&(node.span.start, node.span.end, true))
                    .ok_or_else(|| error("source_provenance"))?;
                if fact.destination != resources[i].destination
                    || fact.source != resources[i].source
                {
                    return Err(error("source_provenance"));
                }
            }
            _ => {}
        }
    }
    if lu.iter().chain(&ru).any(|n| *n != 1) {
        return Err(error("intent_uses"));
    }
    Ok(())
}
fn denied(reason: Denial) -> Target {
    Target::Denied { reason }
}
fn unsafe_text(s: &str) -> bool {
    s.chars()
        .any(|c| crate::metadata::unsafe_character(c) || (c.is_whitespace() && c != ' '))
}
fn decode(s: &str) -> Option<String> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            let hi = (*b.get(i + 1)? as char).to_digit(16)?;
            let lo = (*b.get(i + 2)? as char).to_digit(16)?;
            out.push((hi * 16 + lo) as u8);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}
/// Pure classification: never opens a URI, file, socket or process. Local paths
/// are decoded exactly once; native capability adapters MUST NOT decode again.
pub fn classify(raw: &str, image: bool) -> Target {
    if raw.len() > 8192 {
        return denied(Denial::Limit);
    }
    if raw.is_empty() || raw.trim() != raw || unsafe_text(raw) || raw.contains('\\') {
        return denied(Denial::Unsafe);
    }
    let Some(decoded) = decode(raw) else {
        return denied(Denial::InvalidUri);
    };
    if unsafe_text(&decoded) || decoded.contains('\\') {
        return denied(Denial::Unsafe);
    }
    if let Some(anchor) = raw.strip_prefix('#') {
        return if image || anchor.is_empty() {
            denied(Denial::InvalidUri)
        } else {
            Target::Fragment {
                anchor: decode(anchor).unwrap_or_default(),
            }
        };
    }
    if let Some(colon) = raw
        .find(':')
        .filter(|&i| !raw[..i].contains(['/', '?', '#']))
    {
        let scheme = &raw[..colon];
        if !matches!(
            scheme.to_ascii_lowercase().as_str(),
            "http" | "https" | "mailto"
        ) {
            return denied(Denial::UnsupportedScheme);
        }
        if image {
            return denied(Denial::RemoteResource);
        }
        let Ok(uri) = url::Url::parse(raw) else {
            return denied(Denial::InvalidUri);
        };
        if uri.scheme() == "mailto" {
            let address = &raw[scheme.len() + 1..];
            if !address.is_ascii() || address.contains('%') {
                return denied(Denial::InvalidUri);
            }
            let Some((local, domain)) = address.split_once('@') else {
                return denied(Denial::InvalidUri);
            };
            if local.is_empty()
                || domain.is_empty()
                || domain.contains('@')
                || !domain
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b".-".contains(&c))
                || uri.query().is_some()
                || uri.fragment().is_some()
                || raw.contains(' ')
            {
                return denied(Denial::InvalidUri);
            }
        } else if raw[scheme.len()..]
            .strip_prefix("://")
            .is_none_or(|s| s.is_empty() || s.starts_with('/'))
            || uri.host_str().is_none_or(str::is_empty)
            || !uri.username().is_empty()
            || uri.password().is_some()
        {
            return denied(Denial::InvalidUri);
        }
        return Target::External { uri: uri.into() };
    }
    let (raw_path, raw_fragment) = raw
        .split_once('#')
        .map_or((raw, None), |(p, f)| (p, Some(f)));
    if raw_path.contains('?') {
        return denied(Denial::InvalidUri);
    }
    let Some(path) = decode(raw_path) else {
        return denied(Denial::InvalidUri);
    };
    if path.is_empty() || path.starts_with('/') || path.contains([':', '\\', '?']) {
        return denied(Denial::Unsafe);
    }
    for segment in raw_path.split('/') {
        let Some(value) = decode(segment) else {
            return denied(Denial::InvalidUri);
        };
        if value.contains('/') || matches!(value.as_str(), "." | "..") && segment != value {
            return denied(Denial::Unsafe);
        }
        if !matches!(value.as_str(), "." | "..")
            && (value.ends_with(['.', ' ']) || windows_device(&value))
        {
            return denied(Denial::Unsafe);
        }
    }
    let fragment = match raw_fragment {
        Some(f) => decode(f),
        None => None,
    };
    Target::RequiresCapability { path, fragment }
}
fn windows_device(segment: &str) -> bool {
    let base = segment
        .split('.')
        .next()
        .unwrap_or("")
        .trim_end_matches(' ')
        .to_ascii_uppercase();
    matches!(
        base.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
    ) || ["COM", "LPT"].iter().any(|p| {
        base.strip_prefix(p).is_some_and(|n| {
            matches!(
                n,
                "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
            )
        })
    })
}
#[cfg(feature = "schema")]
pub fn document_schema() -> schemars::Schema {
    let mut s = schemars::schema_for!(DocumentInput);
    s.insert(
        "$id".into(),
        serde_json::Value::String("urn:markdown:semantic-document:2".into()),
    );
    s
}
