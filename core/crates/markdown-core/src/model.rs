use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

pub const MODEL_VERSION: u32 = 1;

macro_rules! id {
    ($name:ident) => {
        #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
        #[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
        #[serde(transparent)]
        pub struct $name(
            #[cfg_attr(
                feature = "schema",
                schemars(regex(pattern = r"^[A-Za-z0-9_.:-]{1,128}$"))
            )]
            pub String,
        );
    };
}
id!(DocumentId);
id!(RevisionId);
id!(NodeId);
id!(LinkId);
id!(ResourceId);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct SourceSpan {
    pub start: u64,
    pub end: u64,
}
impl SourceSpan {
    pub fn slice<'a>(&self, source: &'a str) -> Option<&'a str> {
        source.get(usize::try_from(self.start).ok()?..usize::try_from(self.end).ok()?)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct SourceInfo {
    pub bytes: u64,
    #[cfg_attr(feature = "schema", schemars(regex(pattern = r"^[0-9a-f]{64}$")))]
    pub sha256: String,
}
impl SourceInfo {
    pub fn of(source: &str) -> Self {
        Self {
            bytes: source.len() as u64,
            sha256: format!("{:x}", Sha256::digest(source.as_bytes())),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct DocumentInput {
    #[cfg_attr(feature = "schema", schemars(range(min = 1, max = 1)))]
    pub model_version: u32,
    pub document_id: DocumentId,
    pub revision_id: RevisionId,
    pub source: SourceInfo,
    #[serde(deserialize_with = "unique_capabilities")]
    pub capabilities: BTreeSet<Capability>,
    pub roots: Vec<NodeId>,
    pub nodes: Vec<Node>,
    pub outline: Vec<OutlineEntry>,
    pub metadata: Metadata,
    pub links: Vec<LinkIntent>,
    pub resources: Vec<ResourceIntent>,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Node {
    pub id: NodeId,
    pub span: SourceSpan,
    pub content: NodeKind,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum NodeKind {
    Paragraph {
        children: Vec<NodeId>,
    },
    Heading {
        #[cfg_attr(feature = "schema", schemars(range(min = 1, max = 6)))]
        level: u8,
        anchor: String,
        children: Vec<NodeId>,
    },
    BlockQuote {
        children: Vec<NodeId>,
    },
    List {
        ordered: bool,
        start: Option<u64>,
        tight: bool,
        items: Vec<NodeId>,
    },
    ListItem {
        checked: Option<bool>,
        children: Vec<NodeId>,
    },
    CodeBlock {
        info: Option<String>,
        text: String,
    },
    ThematicBreak {},
    Table {
        alignments: Vec<Alignment>,
        rows: Vec<NodeId>,
    },
    TableRow {
        header: bool,
        cells: Vec<NodeId>,
    },
    TableCell {
        children: Vec<NodeId>,
    },
    FootnoteDefinition {
        label: String,
        children: Vec<NodeId>,
    },
    Alert {
        category: AlertKind,
        children: Vec<NodeId>,
    },
    UnsupportedBlock {
        feature: String,
        reason: UnsupportedReason,
    },
    Text {
        text: String,
    },
    Emphasis {
        children: Vec<NodeId>,
    },
    Strong {
        children: Vec<NodeId>,
    },
    Strikethrough {
        children: Vec<NodeId>,
    },
    CodeSpan {
        text: String,
    },
    SoftBreak {},
    HardBreak {},
    Link {
        target: LinkId,
        title: Option<String>,
        children: Vec<NodeId>,
    },
    Image {
        resource: ResourceId,
        alt: String,
        title: Option<String>,
    },
    FootnoteReference {
        label: String,
    },
    UnsupportedInline {
        feature: String,
        reason: UnsupportedReason,
    },
}

impl NodeKind {
    pub fn children(&self) -> &[NodeId] {
        match self {
            Self::Paragraph { children }
            | Self::Heading { children, .. }
            | Self::BlockQuote { children }
            | Self::ListItem { children, .. }
            | Self::TableCell { children }
            | Self::FootnoteDefinition { children, .. }
            | Self::Alert { children, .. }
            | Self::Emphasis { children }
            | Self::Strong { children }
            | Self::Strikethrough { children }
            | Self::Link { children, .. } => children,
            Self::List { items, .. } => items,
            Self::Table { rows, .. } => rows,
            Self::TableRow { cells, .. } => cells,
            _ => &[],
        }
    }
    pub fn is_inline(&self) -> bool {
        matches!(
            self,
            Self::Text { .. }
                | Self::Emphasis { .. }
                | Self::Strong { .. }
                | Self::Strikethrough { .. }
                | Self::CodeSpan { .. }
                | Self::SoftBreak { .. }
                | Self::HardBreak { .. }
                | Self::Link { .. }
                | Self::Image { .. }
                | Self::FootnoteReference { .. }
                | Self::UnsupportedInline { .. }
        )
    }
    pub fn is_block(&self) -> bool {
        !self.is_inline()
            && !matches!(
                self,
                Self::ListItem { .. } | Self::TableRow { .. } | Self::TableCell { .. }
            )
    }
}

macro_rules! enumeration {
    ($name:ident { $($variant:ident),+ $(,)? }) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
        #[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
        #[serde(rename_all = "snake_case")]
        pub enum $name { $($variant),+ }
    };
}
enumeration!(Capability {
    Tables,
    TaskLists,
    Footnotes,
    Strikethrough,
    FrontMatter,
    Alerts
});
enumeration!(Alignment {
    None,
    Left,
    Center,
    Right
});
enumeration!(AlertKind {
    Note,
    Tip,
    Important,
    Warning,
    Caution
});
enumeration!(UnsupportedReason {
    Unavailable,
    Disabled,
    InvalidInput
});
enumeration!(Severity {
    Info,
    Warning,
    Error
});

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct OutlineEntry {
    pub node: NodeId,
    #[cfg_attr(feature = "schema", schemars(range(min = 1, max = 6)))]
    pub level: u8,
    pub title: String,
    pub anchor: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Metadata {
    pub front_matter_span: Option<SourceSpan>,
    pub title: Option<String>,
    #[serde(deserialize_with = "unique_properties")]
    pub properties: BTreeMap<String, MetadataValue>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum MetadataValue {
    Null {},
    Boolean { value: bool },
    Integer { value: i64 },
    String { value: String },
    Strings { value: Vec<String> },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct LinkIntent {
    pub id: LinkId,
    pub span: SourceSpan,
    pub destination: String,
    pub disposition: LinkDisposition,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum LinkDisposition {
    Fragment { anchor: String },
    RequiresCapability {},
    External {},
    Denied { reason: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct ResourceIntent {
    pub id: ResourceId,
    pub span: SourceSpan,
    pub destination: String,
    pub access: ResourceAccess,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ResourceAccess {
    RequiresCapability {},
    Denied { reason: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Diagnostic {
    pub code: String,
    pub severity: Severity,
    pub message: String,
    pub span: Option<SourceSpan>,
    pub node: Option<NodeId>,
}

pub(crate) fn unique_capabilities<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<BTreeSet<Capability>, D::Error> {
    let values = Vec::<Capability>::deserialize(deserializer)?;
    let mut set = BTreeSet::new();
    for value in values {
        if !set.insert(value) {
            return Err(serde::de::Error::custom("duplicate capability"));
        }
    }
    Ok(set)
}

pub(crate) fn unique_properties<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<BTreeMap<String, MetadataValue>, D::Error> {
    struct Properties;
    impl<'de> serde::de::Visitor<'de> for Properties {
        type Value = BTreeMap<String, MetadataValue>;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("unique metadata properties")
        }
        fn visit_map<A: serde::de::MapAccess<'de>>(
            self,
            mut map: A,
        ) -> Result<Self::Value, A::Error> {
            let mut result = BTreeMap::new();
            while let Some((key, value)) = map.next_entry::<String, MetadataValue>()? {
                if result.insert(key, value).is_some() {
                    return Err(serde::de::Error::custom("duplicate metadata property"));
                }
            }
            Ok(result)
        }
    }
    deserializer.deserialize_map(Properties)
}
