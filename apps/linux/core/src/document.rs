use crate::ffi::{self, md_diagnostic, md_intent, md_node, md_outline_entry, md_property, md_str};
use crate::runtime::CoreError;

/// Immutable values copied out of one model-v2 snapshot view before its handle
/// is released, with every cross-table index and string range bounds-checked. No pointer, handle or borrowed core memory is kept: the records
/// are fixed-width integers and string-table ranges (frozen ABI v1), so the
/// document is `Send + Sync` and outlives its session.
#[derive(Clone, Debug)]
pub struct CoreDocument {
    pub generation: u64,
    /// The exact UTF-8 source; node spans are byte offsets into it.
    pub source: Vec<u8>,
    /// The string table every `md_str` indexes; read it through [`CoreDocument::text`].
    pub strings: Vec<u8>,
    pub nodes: Vec<md_node>,
    pub roots: Vec<u32>,
    pub children: Vec<u32>,
    pub alignments: Vec<u32>,
    pub links: Vec<md_intent>,
    pub resources: Vec<md_intent>,
    pub outline: Vec<md_outline_entry>,
    pub diagnostics: Vec<md_diagnostic>,
    pub properties: Vec<md_property>,
    pub string_list: Vec<md_str>,
    pub capabilities: u32,
    pub flags: u32,
    pub front_matter: Option<std::ops::Range<u32>>,
    pub document_id: String,
    pub revision_id: String,
    pub source_sha256: String,
    pub title: Option<String>,
    /// Bytes copied from the view: the source and every table.
    pub copied_bytes: u64,
}

/// A view array: `count` records at `pointer`, which may be null only when empty.
fn records<T: Copy>(pointer: *const T, count: u64) -> Result<Vec<T>, CoreError> {
    if count == 0 {
        return Ok(Vec::new());
    }
    if pointer.is_null() || !pointer.is_aligned() {
        return Err(CoreError::InvalidSnapshot);
    }
    let count = usize::try_from(count).map_err(|_| CoreError::InvalidSnapshot)?;
    // SAFETY: the snapshot handle is live for the whole copy, and the view
    // contract makes `pointer` an aligned array of `count` immutable records.
    Ok(unsafe { std::slice::from_raw_parts(pointer, count) }.to_vec())
}

fn decode(table: &[u8], range: md_str) -> Result<&str, CoreError> {
    let start = range.offset as usize;
    let end = start + range.length as usize;
    let bytes = table.get(start..end).ok_or(CoreError::InvalidSnapshot)?;
    std::str::from_utf8(bytes).map_err(|_| CoreError::InvalidSnapshot)
}

impl CoreDocument {
    /// Copies `view`, which must belong to a live snapshot for the whole call.
    pub(crate) fn copying(view: &ffi::md_snapshot_view, budget: u64) -> Result<Self, CoreError> {
        if view.model_version != ffi::MD_MODEL_V2 {
            return Err(CoreError::IncompatibleAbi);
        }
        // Price the whole copy before allocating any array; every step is checked.
        let mut bytes = 0u64;
        let mut price = |count: u64, size: usize| -> Result<(), CoreError> {
            bytes = count
                .checked_mul(size as u64)
                .and_then(|cost| bytes.checked_add(cost))
                .filter(|total| *total <= budget)
                .ok_or(CoreError::SnapshotBudget)?;
            Ok(())
        };
        use std::mem::size_of;
        price(view.source_length, 1)?;
        price(view.strings_length, 1)?;
        price(view.node_count.into(), size_of::<md_node>())?;
        price(view.root_count.into(), 4)?;
        price(view.child_count.into(), 4)?;
        price(view.alignment_count.into(), 4)?;
        price(view.link_count.into(), size_of::<md_intent>())?;
        price(view.resource_count.into(), size_of::<md_intent>())?;
        price(view.outline_count.into(), size_of::<md_outline_entry>())?;
        price(view.diagnostic_count.into(), size_of::<md_diagnostic>())?;
        price(view.property_count.into(), size_of::<md_property>())?;
        price(view.string_list_count.into(), size_of::<md_str>())?;
        if view.front_matter_start > view.front_matter_end
            || u64::from(view.front_matter_end) > view.source_length
        {
            return Err(CoreError::InvalidSnapshot);
        }
        let strings = records(view.strings, view.strings_length)?;
        let text = |range| decode(&strings, range).map(str::to_owned);
        let document_id = text(view.document_id)?;
        let revision_id = text(view.revision_id)?;
        let source_sha256 = text(view.source_sha256)?;
        let title = match view.flags & ffi::MD_VIEW_HAS_TITLE {
            0 => None,
            _ => Some(text(view.title)?),
        };
        let document = CoreDocument {
            generation: view.generation,
            source: records(view.source, view.source_length)?,
            nodes: records(view.nodes, view.node_count.into())?,
            roots: records(view.roots, view.root_count.into())?,
            children: records(view.children, view.child_count.into())?,
            alignments: records(view.alignments, view.alignment_count.into())?,
            links: records(view.links, view.link_count.into())?,
            resources: records(view.resources, view.resource_count.into())?,
            outline: records(view.outline, view.outline_count.into())?,
            diagnostics: records(view.diagnostics, view.diagnostic_count.into())?,
            properties: records(view.properties, view.property_count.into())?,
            string_list: records(view.string_list, view.string_list_count.into())?,
            capabilities: view.capabilities,
            flags: view.flags,
            front_matter: (view.flags & ffi::MD_VIEW_HAS_FRONT_MATTER != 0)
                .then_some(view.front_matter_start..view.front_matter_end),
            document_id,
            revision_id,
            source_sha256,
            title,
            strings,
            copied_bytes: bytes,
        };
        document.check_indices()?;
        Ok(document)
    }

    /// Cross-table bounds, so a core bug surfaces as `InvalidSnapshot` here and
    /// never as an index panic in a later consumer.
    fn check_indices(&self) -> Result<(), CoreError> {
        let nodes = self.nodes.len();
        let node = |index: u32| (index as usize) < nodes;
        let span = |start: u32, end: u32| start <= end && end as usize <= self.source.len();
        let string = |range: md_str| {
            (range.offset as usize)
                .checked_add(range.length as usize)
                .is_some_and(|end| end <= self.strings.len())
        };
        let valid = self.roots.iter().all(|&root| node(root))
            && self.children.iter().all(|&child| node(child))
            && self.nodes.iter().all(|entry| {
                (entry.parent == ffi::MD_NONE || node(entry.parent))
                    && (entry.first_child as usize)
                        .checked_add(entry.child_count as usize)
                        .is_some_and(|end| end <= self.children.len())
                    && span(entry.span_start, entry.span_end)
                    && string(entry.id)
                    && string(entry.text)
                    && string(entry.extra)
            })
            && self
                .outline
                .iter()
                .all(|entry| node(entry.node) && string(entry.title) && string(entry.anchor))
            && self.diagnostics.iter().all(|entry| {
                (entry.node == ffi::MD_NONE || node(entry.node))
                    && string(entry.code)
                    && string(entry.message)
            })
            && self.links.iter().chain(&self.resources).all(|intent| {
                span(intent.span_start, intent.span_end)
                    && [
                        intent.id,
                        intent.destination,
                        intent.target,
                        intent.fragment,
                    ]
                    .into_iter()
                    .all(string)
            })
            && self.properties.iter().all(|property| {
                string(property.key)
                    && string(property.string)
                    && (property.first_string as usize)
                        .checked_add(property.string_count as usize)
                        .is_some_and(|end| end <= self.string_list.len())
            })
            && self.string_list.iter().all(|&range| string(range));
        if valid {
            Ok(())
        } else {
            Err(CoreError::InvalidSnapshot)
        }
    }

    /// A string-table entry, checked for range and UTF-8.
    pub fn text(&self, range: md_str) -> Result<&str, CoreError> {
        decode(&self.strings, range)
    }
}
