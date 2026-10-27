//! Bulk flat encoding of a validated snapshot: fixed-size records, `u32`
//! indices and one UTF-8 string table. Built once per snapshot handle and
//! immutable afterwards, so `md_snapshot_view` pointers stay valid until the
//! handle is released. Every model value is represented (the round trip is
//! tested), so adapters never need the Rust model or its lifetimes.
use crate::abi::*;
use markdown_core::session::{Snapshot, SnapshotModel};
use markdown_core::*;
use std::collections::HashMap;

pub(crate) struct Flat {
    model_version: u32,
    strings: Vec<u8>,
    nodes: Vec<md_node>,
    roots: Vec<u32>,
    children: Vec<u32>,
    alignments: Vec<u32>,
    links: Vec<md_intent>,
    resources: Vec<md_intent>,
    outline: Vec<md_outline_entry>,
    diagnostics: Vec<md_diagnostic>,
    properties: Vec<md_property>,
    string_list: Vec<md_str>,
    capabilities: u32,
    flags: u32,
    front_matter: (u32, u32),
    document_id: md_str,
    revision_id: md_str,
    source_sha256: md_str,
    title: md_str,
}

/// Every table index, span and string offset must fit the ABI's `u32`.
fn small(value: impl TryInto<u32>) -> Result<u32, md_status> {
    value.try_into().map_err(|_| MD_ERR_LIMIT)
}

/// Borrowed model parts shared by v1 and v2.
struct Parts<'a> {
    document_id: &'a DocumentId,
    revision_id: &'a RevisionId,
    source: &'a SourceInfo,
    capabilities: &'a std::collections::BTreeSet<Capability>,
    roots: &'a [NodeId],
    nodes: &'a [Node],
    outline: &'a [OutlineEntry],
    metadata: &'a Metadata,
    diagnostics: &'a [Diagnostic],
}

struct Builder<'a> {
    flat: Flat,
    nodes: HashMap<&'a str, u32>,
    links: HashMap<&'a str, u32>,
    resources: HashMap<&'a str, u32>,
}

impl<'a> Builder<'a> {
    fn string(&mut self, value: &str) -> Result<md_str, md_status> {
        let offset = small(self.flat.strings.len())?;
        let length = small(value.len())?;
        small(self.flat.strings.len() + value.len())?;
        self.flat.strings.extend_from_slice(value.as_bytes());
        Ok(md_str { offset, length })
    }
    fn optional(
        &mut self,
        value: Option<&str>,
        flag: u32,
        flags: &mut u32,
    ) -> Result<md_str, md_status> {
        match value {
            Some(value) => {
                *flags |= flag;
                self.string(value)
            }
            None => Ok(md_str::default()),
        }
    }
    fn node(&self, id: &NodeId) -> Result<u32, md_status> {
        self.nodes
            .get(id.0.as_str())
            .copied()
            .ok_or(MD_ERR_INTERNAL)
    }
    fn span(span: SourceSpan) -> Result<(u32, u32), md_status> {
        Ok((small(span.start)?, small(span.end)?))
    }
}

fn kind_code(kind: &NodeKind) -> u32 {
    match kind {
        NodeKind::Paragraph { .. } => MD_NODE_PARAGRAPH,
        NodeKind::Heading { .. } => MD_NODE_HEADING,
        NodeKind::BlockQuote { .. } => MD_NODE_BLOCK_QUOTE,
        NodeKind::List { .. } => MD_NODE_LIST,
        NodeKind::ListItem { .. } => MD_NODE_LIST_ITEM,
        NodeKind::CodeBlock { .. } => MD_NODE_CODE_BLOCK,
        NodeKind::ThematicBreak {} => MD_NODE_THEMATIC_BREAK,
        NodeKind::Table { .. } => MD_NODE_TABLE,
        NodeKind::TableRow { .. } => MD_NODE_TABLE_ROW,
        NodeKind::TableCell { .. } => MD_NODE_TABLE_CELL,
        NodeKind::FootnoteDefinition { .. } => MD_NODE_FOOTNOTE_DEFINITION,
        NodeKind::Alert { .. } => MD_NODE_ALERT,
        NodeKind::UnsupportedBlock { .. } => MD_NODE_UNSUPPORTED_BLOCK,
        NodeKind::Text { .. } => MD_NODE_TEXT,
        NodeKind::Emphasis { .. } => MD_NODE_EMPHASIS,
        NodeKind::Strong { .. } => MD_NODE_STRONG,
        NodeKind::Strikethrough { .. } => MD_NODE_STRIKETHROUGH,
        NodeKind::CodeSpan { .. } => MD_NODE_CODE_SPAN,
        NodeKind::SoftBreak {} => MD_NODE_SOFT_BREAK,
        NodeKind::HardBreak {} => MD_NODE_HARD_BREAK,
        NodeKind::Link { .. } => MD_NODE_LINK,
        NodeKind::Image { .. } => MD_NODE_IMAGE,
        NodeKind::FootnoteReference { .. } => MD_NODE_FOOTNOTE_REFERENCE,
        NodeKind::UnsupportedInline { .. } => MD_NODE_UNSUPPORTED_INLINE,
    }
}
fn alignment_code(a: Alignment) -> u32 {
    match a {
        Alignment::None => MD_ALIGN_NONE,
        Alignment::Left => MD_ALIGN_LEFT,
        Alignment::Center => MD_ALIGN_CENTER,
        Alignment::Right => MD_ALIGN_RIGHT,
    }
}
fn alert_code(a: AlertKind) -> u32 {
    match a {
        AlertKind::Note => MD_ALERT_NOTE,
        AlertKind::Tip => MD_ALERT_TIP,
        AlertKind::Important => MD_ALERT_IMPORTANT,
        AlertKind::Warning => MD_ALERT_WARNING,
        AlertKind::Caution => MD_ALERT_CAUTION,
    }
}
fn reason_code(r: UnsupportedReason) -> u32 {
    match r {
        UnsupportedReason::Unavailable => MD_UNSUPPORTED_UNAVAILABLE,
        UnsupportedReason::Disabled => MD_UNSUPPORTED_DISABLED,
        UnsupportedReason::InvalidInput => MD_UNSUPPORTED_INVALID_INPUT,
    }
}
fn severity_code(s: Severity) -> u32 {
    match s {
        Severity::Info => MD_SEVERITY_INFO,
        Severity::Warning => MD_SEVERITY_WARNING,
        Severity::Error => MD_SEVERITY_ERROR,
    }
}
fn capability_bit(c: Capability) -> u32 {
    match c {
        Capability::Tables => MD_CAP_TABLES,
        Capability::TaskLists => MD_CAP_TASK_LISTS,
        Capability::Footnotes => MD_CAP_FOOTNOTES,
        Capability::Strikethrough => MD_CAP_STRIKETHROUGH,
        Capability::FrontMatter => MD_CAP_FRONT_MATTER,
        Capability::Alerts => MD_CAP_ALERTS,
    }
}

impl Flat {
    pub(crate) fn encode(snapshot: &Snapshot) -> Result<Self, md_status> {
        Self::encode_model(snapshot.model())
    }
    pub(crate) fn encode_model(model: &SnapshotModel) -> Result<Self, md_status> {
        match model {
            SnapshotModel::V1(document) => {
                let d = document.input();
                let mut b = Builder::start(
                    MD_MODEL_V1,
                    &Parts {
                        document_id: &d.document_id,
                        revision_id: &d.revision_id,
                        source: &d.source,
                        capabilities: &d.capabilities,
                        roots: &d.roots,
                        nodes: &d.nodes,
                        outline: &d.outline,
                        metadata: &d.metadata,
                        diagnostics: &d.diagnostics,
                    },
                )?;
                for (i, link) in d.links.iter().enumerate() {
                    b.links.insert(&link.id.0, small(i)?);
                    let intent =
                        b.v1_intent(&link.id.0, &link.destination, link.span, |b, intent| {
                            match &link.disposition {
                                LinkDisposition::Fragment { anchor } => {
                                    intent.target_kind = MD_TARGET_FRAGMENT;
                                    intent.target = b.string(anchor)?;
                                }
                                LinkDisposition::RequiresCapability {} => {
                                    intent.target_kind = MD_TARGET_REQUIRES_CAPABILITY
                                }
                                LinkDisposition::External {} => {
                                    intent.target_kind = MD_TARGET_EXTERNAL
                                }
                                LinkDisposition::Denied { reason } => {
                                    intent.target_kind = MD_TARGET_DENIED;
                                    intent.target = b.string(reason)?;
                                }
                            }
                            Ok(())
                        })?;
                    b.flat.links.push(intent);
                }
                for (i, resource) in d.resources.iter().enumerate() {
                    b.resources.insert(&resource.id.0, small(i)?);
                    let intent = b.v1_intent(
                        &resource.id.0,
                        &resource.destination,
                        resource.span,
                        |b, intent| {
                            match &resource.access {
                                ResourceAccess::RequiresCapability {} => {
                                    intent.target_kind = MD_TARGET_REQUIRES_CAPABILITY
                                }
                                ResourceAccess::Denied { reason } => {
                                    intent.target_kind = MD_TARGET_DENIED;
                                    intent.target = b.string(reason)?;
                                }
                            }
                            Ok(())
                        },
                    )?;
                    b.flat.resources.push(intent);
                }
                b.finish(&d.nodes)
            }
            #[cfg(feature = "model-v2")]
            SnapshotModel::V2(document) => {
                let d = document.input();
                let mut b = Builder::start(
                    MD_MODEL_V2,
                    &Parts {
                        document_id: &d.document_id,
                        revision_id: &d.revision_id,
                        source: &d.source,
                        capabilities: &d.capabilities,
                        roots: &d.roots,
                        nodes: &d.nodes,
                        outline: &d.outline,
                        metadata: &d.metadata,
                        diagnostics: &d.diagnostics,
                    },
                )?;
                for (i, link) in d.links.iter().enumerate() {
                    b.links.insert(&link.id.0, small(i)?);
                    let intent =
                        b.v2_intent(&link.id.0, &link.destination, &link.source, &link.target)?;
                    b.flat.links.push(intent);
                }
                for (i, resource) in d.resources.iter().enumerate() {
                    b.resources.insert(&resource.id.0, small(i)?);
                    let intent = b.v2_intent(
                        &resource.id.0,
                        &resource.destination,
                        &resource.source,
                        &resource.target,
                    )?;
                    b.flat.resources.push(intent);
                }
                b.finish(&d.nodes)
            }
        }
    }

    /// A view with pointers into this immutable encoding and `source`.
    pub(crate) fn view(&self, source: &str, generation: u64) -> md_snapshot_view {
        fn ptr<T>(values: &[T]) -> *const T {
            if values.is_empty() {
                std::ptr::null()
            } else {
                values.as_ptr()
            }
        }
        // Every table length was checked against u32 by `finish`.
        let count = |n: usize| n as u32;
        let source = source.as_bytes();
        md_snapshot_view {
            struct_size: size::<md_snapshot_view>(),
            model_version: self.model_version,
            generation,
            source_length: source.len() as u64,
            strings_length: self.strings.len() as u64,
            source: ptr(source),
            strings: ptr(&self.strings),
            nodes: ptr(&self.nodes),
            roots: ptr(&self.roots),
            children: ptr(&self.children),
            alignments: ptr(&self.alignments),
            links: ptr(&self.links),
            resources: ptr(&self.resources),
            outline: ptr(&self.outline),
            diagnostics: ptr(&self.diagnostics),
            properties: ptr(&self.properties),
            string_list: ptr(&self.string_list),
            node_count: count(self.nodes.len()),
            root_count: count(self.roots.len()),
            child_count: count(self.children.len()),
            alignment_count: count(self.alignments.len()),
            link_count: count(self.links.len()),
            resource_count: count(self.resources.len()),
            outline_count: count(self.outline.len()),
            diagnostic_count: count(self.diagnostics.len()),
            property_count: count(self.properties.len()),
            string_list_count: count(self.string_list.len()),
            capabilities: self.capabilities,
            flags: self.flags,
            front_matter_start: self.front_matter.0,
            front_matter_end: self.front_matter.1,
            document_id: self.document_id,
            revision_id: self.revision_id,
            source_sha256: self.source_sha256,
            title: self.title,
        }
    }
}

impl<'a> Builder<'a> {
    fn start(model_version: u32, d: &Parts<'a>) -> Result<Self, md_status> {
        small(d.source.bytes)?;
        let mut b = Builder {
            flat: Flat {
                model_version,
                strings: Vec::new(),
                nodes: Vec::with_capacity(d.nodes.len()),
                roots: Vec::with_capacity(d.roots.len()),
                children: Vec::new(),
                alignments: Vec::new(),
                links: Vec::new(),
                resources: Vec::new(),
                outline: Vec::with_capacity(d.outline.len()),
                diagnostics: Vec::with_capacity(d.diagnostics.len()),
                properties: Vec::new(),
                string_list: Vec::new(),
                capabilities: d.capabilities.iter().map(|c| capability_bit(*c)).sum(),
                flags: 0,
                front_matter: (0, 0),
                document_id: md_str::default(),
                revision_id: md_str::default(),
                source_sha256: md_str::default(),
                title: md_str::default(),
            },
            nodes: HashMap::with_capacity(d.nodes.len()),
            links: HashMap::new(),
            resources: HashMap::new(),
        };
        small(d.nodes.len())?;
        for (i, node) in d.nodes.iter().enumerate() {
            b.nodes.insert(&node.id.0, i as u32);
        }
        b.flat.document_id = b.string(&d.document_id.0)?;
        b.flat.revision_id = b.string(&d.revision_id.0)?;
        b.flat.source_sha256 = b.string(&d.source.sha256)?;
        let mut flags = 0;
        b.flat.title = b.optional(d.metadata.title.as_deref(), MD_VIEW_HAS_TITLE, &mut flags)?;
        if let Some(span) = d.metadata.front_matter_span {
            flags |= MD_VIEW_HAS_FRONT_MATTER;
            b.flat.front_matter = Self::span(span)?;
        }
        b.flat.flags = flags;
        for root in d.roots {
            let index = b.node(root)?;
            b.flat.roots.push(index);
        }
        for entry in d.outline {
            let record = md_outline_entry {
                node: b.node(&entry.node)?,
                level: u32::from(entry.level),
                title: b.string(&entry.title)?,
                anchor: b.string(&entry.anchor)?,
            };
            b.flat.outline.push(record);
        }
        for diagnostic in d.diagnostics {
            let mut record = md_diagnostic {
                code: b.string(&diagnostic.code)?,
                message: b.string(&diagnostic.message)?,
                severity: severity_code(diagnostic.severity),
                node: match &diagnostic.node {
                    Some(id) => b.node(id)?,
                    None => MD_NONE,
                },
                ..md_diagnostic::default()
            };
            if let Some(span) = diagnostic.span {
                record.flags = MD_DIAGNOSTIC_HAS_SPAN;
                (record.span_start, record.span_end) = Self::span(span)?;
            }
            b.flat.diagnostics.push(record);
        }
        for (key, value) in &d.metadata.properties {
            let mut record = md_property {
                key: b.string(key)?,
                ..md_property::default()
            };
            match value {
                MetadataValue::Null {} => record.kind = MD_VALUE_NULL,
                MetadataValue::Boolean { value } => {
                    record.kind = MD_VALUE_BOOLEAN;
                    record.boolean = u32::from(*value);
                }
                MetadataValue::Integer { value } => {
                    record.kind = MD_VALUE_INTEGER;
                    record.integer = *value;
                }
                MetadataValue::String { value } => {
                    record.kind = MD_VALUE_STRING;
                    record.string = b.string(value)?;
                }
                MetadataValue::Strings { value } => {
                    record.kind = MD_VALUE_STRINGS;
                    record.first_string = small(b.flat.string_list.len())?;
                    record.string_count = small(value.len())?;
                    for item in value {
                        let item = b.string(item)?;
                        b.flat.string_list.push(item);
                    }
                }
            }
            b.flat.properties.push(record);
        }
        Ok(b)
    }

    fn v1_intent(
        &mut self,
        id: &str,
        destination: &str,
        span: SourceSpan,
        target: impl FnOnce(&mut Self, &mut md_intent) -> Result<(), md_status>,
    ) -> Result<md_intent, md_status> {
        let mut intent = md_intent {
            id: self.string(id)?,
            destination: self.string(destination)?,
            source_kind: MD_SOURCE_UNSPECIFIED,
            ..md_intent::default()
        };
        (intent.span_start, intent.span_end) = Self::span(span)?;
        target(self, &mut intent)?;
        Ok(intent)
    }

    #[cfg(feature = "model-v2")]
    fn v2_intent(
        &mut self,
        id: &str,
        destination: &str,
        source: &markdown_core::v2::IntentSource,
        target: &markdown_core::v2::Target,
    ) -> Result<md_intent, md_status> {
        use markdown_core::v2::{Denial, IntentSource, Target};
        let mut intent = md_intent {
            id: self.string(id)?,
            destination: self.string(destination)?,
            ..md_intent::default()
        };
        match source {
            IntentSource::Inline { span } => {
                intent.source_kind = MD_SOURCE_INLINE;
                (intent.span_start, intent.span_end) = Self::span(*span)?;
            }
            IntentSource::Reference { definition, span } => {
                intent.source_kind = MD_SOURCE_REFERENCE;
                (intent.span_start, intent.span_end) = Self::span(*span)?;
                (intent.definition_start, intent.definition_end) = Self::span(*definition)?;
            }
        }
        match target {
            Target::Fragment { anchor } => {
                intent.target_kind = MD_TARGET_FRAGMENT;
                intent.target = self.string(anchor)?;
            }
            Target::RequiresCapability { path, fragment } => {
                intent.target_kind = MD_TARGET_REQUIRES_CAPABILITY;
                intent.target = self.string(path)?;
                let mut flags = 0;
                intent.fragment =
                    self.optional(fragment.as_deref(), MD_INTENT_HAS_FRAGMENT, &mut flags)?;
                intent.flags = flags;
            }
            Target::External { uri } => {
                intent.target_kind = MD_TARGET_EXTERNAL;
                intent.target = self.string(uri)?;
            }
            Target::Denied { reason } => {
                intent.target_kind = MD_TARGET_DENIED;
                intent.denial = match reason {
                    Denial::Unsafe => MD_DENIAL_UNSAFE,
                    Denial::InvalidUri => MD_DENIAL_INVALID_URI,
                    Denial::UnsupportedScheme => MD_DENIAL_UNSUPPORTED_SCHEME,
                    Denial::RemoteResource => MD_DENIAL_REMOTE_RESOURCE,
                    Denial::Limit => MD_DENIAL_LIMIT,
                    Denial::LegacyPolicy => MD_DENIAL_LEGACY_POLICY,
                };
            }
        }
        Ok(intent)
    }

    /// Encode nodes after intents so link/resource IDs map to table indices.
    fn finish(mut self, nodes: &'a [Node]) -> Result<Flat, md_status> {
        let mut parents = vec![MD_NONE; nodes.len()];
        for (i, node) in nodes.iter().enumerate() {
            let mut record = md_node {
                kind: kind_code(&node.content),
                parent: MD_NONE,
                first_child: small(self.flat.children.len())?,
                child_count: small(node.content.children().len())?,
                id: self.string(&node.id.0)?,
                aux: MD_NONE,
                ..md_node::default()
            };
            (record.span_start, record.span_end) = Self::span(node.span)?;
            for child in node.content.children() {
                let child = self.node(child)?;
                self.flat.children.push(child);
                parents[child as usize] = i as u32;
            }
            let mut flags = 0;
            match &node.content {
                NodeKind::Heading { level, anchor, .. } => {
                    record.value = u32::from(*level);
                    record.text = self.string(anchor)?;
                }
                NodeKind::List {
                    ordered,
                    start,
                    tight,
                    ..
                } => {
                    if *ordered {
                        flags |= MD_NODE_ORDERED;
                    }
                    if *tight {
                        flags |= MD_NODE_TIGHT;
                    }
                    if let Some(start) = start {
                        flags |= MD_NODE_HAS_START;
                        record.number = *start;
                    }
                }
                NodeKind::ListItem {
                    checked: Some(checked),
                    ..
                } => {
                    flags |= MD_NODE_HAS_CHECKED;
                    if *checked {
                        flags |= MD_NODE_CHECKED;
                    }
                }
                NodeKind::CodeBlock { info, text } => {
                    record.text = self.string(text)?;
                    record.extra = self.optional(info.as_deref(), MD_NODE_HAS_INFO, &mut flags)?;
                }
                NodeKind::Table { alignments, .. } => {
                    record.aux = small(self.flat.alignments.len())?;
                    record.aux_count = small(alignments.len())?;
                    self.flat
                        .alignments
                        .extend(alignments.iter().map(|a| alignment_code(*a)));
                }
                NodeKind::TableRow { header, .. } => {
                    if *header {
                        flags |= MD_NODE_HEADER;
                    }
                }
                NodeKind::FootnoteDefinition { label, .. }
                | NodeKind::FootnoteReference { label } => record.text = self.string(label)?,
                NodeKind::Alert { category, .. } => record.value = alert_code(*category),
                NodeKind::UnsupportedBlock { feature, reason }
                | NodeKind::UnsupportedInline { feature, reason } => {
                    record.value = reason_code(*reason);
                    record.text = self.string(feature)?;
                }
                NodeKind::Text { text } | NodeKind::CodeSpan { text } => {
                    record.text = self.string(text)?
                }
                NodeKind::Link { target, title, .. } => {
                    record.aux = *self.links.get(target.0.as_str()).ok_or(MD_ERR_INTERNAL)?;
                    record.extra =
                        self.optional(title.as_deref(), MD_NODE_HAS_TITLE, &mut flags)?;
                }
                NodeKind::Image {
                    resource,
                    alt,
                    title,
                } => {
                    record.aux = *self
                        .resources
                        .get(resource.0.as_str())
                        .ok_or(MD_ERR_INTERNAL)?;
                    record.text = self.string(alt)?;
                    record.extra =
                        self.optional(title.as_deref(), MD_NODE_HAS_TITLE, &mut flags)?;
                }
                _ => {}
            }
            record.flags = flags;
            self.flat.nodes.push(record);
        }
        let f = &self.flat;
        for len in [
            f.roots.len(),
            f.children.len(),
            f.alignments.len(),
            f.links.len(),
            f.resources.len(),
            f.outline.len(),
            f.diagnostics.len(),
            f.properties.len(),
            f.string_list.len(),
        ] {
            small(len)?;
        }
        for (record, parent) in self.flat.nodes.iter_mut().zip(parents) {
            record.parent = parent;
        }
        Ok(self.flat)
    }
}
