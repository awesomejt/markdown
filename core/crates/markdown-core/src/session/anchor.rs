//! Scroll/reading anchors: a value recording a position in one snapshot that
//! resolves best-effort against another snapshot of the same lineage.
//!
//! Anchorable blocks are leaf blocks in reading order (paragraph, heading,
//! code block, rule, table, unsupported block). Container blocks are descended.
//! Within a snapshot a block is identified by its `NodeId`; across snapshots by
//! a content fingerprint, its occurrence ordinal and its neighbours.
use super::{Generation, Snapshot};
use crate::{Node, NodeId, NodeKind};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Current wire version of [`Anchor`].
pub const ANCHOR_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockKind {
    Paragraph,
    Heading,
    CodeBlock,
    ThematicBreak,
    Table,
    Unsupported,
}
impl BlockKind {
    fn tag(self) -> u8 {
        self as u8 + 1
    }
}

/// Persistable position. Fingerprints are deterministic, non-cryptographic
/// FNV-1a hashes of block source; treat anchors as private document-derived
/// state (never log them). All fields are untrusted on input.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Anchor {
    pub version: u32,
    /// Generation it was taken from; informational only across sessions.
    pub generation: Generation,
    /// Snapshot-local block node ID at creation.
    pub node: NodeId,
    pub kind: BlockKind,
    pub fingerprint: u64,
    /// Index among earlier blocks with the same kind and fingerprint.
    pub occurrence: u32,
    /// Byte offset within the block's source span.
    pub offset: u64,
    /// Absolute source byte offset and source length at creation.
    pub source_offset: u64,
    pub source_len: u64,
    /// Fingerprints of the neighbouring anchorable blocks, if any.
    pub previous: Option<u64>,
    pub next: Option<u64>,
    /// Fingerprint of the nearest preceding heading block, if any.
    pub section: Option<u64>,
}

/// How faithfully an anchor resolved.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnchorMatch {
    /// The same block content was found; the intra-block offset is preserved.
    Exact,
    /// The block changed but was identified by kind and neighbours; the offset
    /// is clamped to the new block.
    Relocated,
    /// The block is gone; this is the closest surviving block, offset zero.
    Nearest,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedAnchor {
    pub quality: AnchorMatch,
    pub node: NodeId,
    /// Byte offset within the block span (a UTF-8 boundary).
    pub offset: u64,
    /// Absolute source byte offset (a UTF-8 boundary).
    pub source_offset: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnchorError {
    /// The snapshot has no anchorable block (for example an empty document).
    NoBlocks,
    UnknownNode,
    UnsupportedVersion,
}
impl std::fmt::Display for AnchorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::NoBlocks => "no_blocks",
            Self::UnknownNode => "unknown_node",
            Self::UnsupportedVersion => "unsupported_version",
        })
    }
}
impl std::error::Error for AnchorError {}

struct Block {
    node: usize,
    kind: BlockKind,
    start: u64,
    end: u64,
    fingerprint: u64,
    occurrence: u32,
    section: Option<u64>,
}

/// Lazily built per-snapshot index of anchorable blocks in reading order.
pub(crate) struct BlockIndex {
    blocks: Vec<Block>,
    by_id: HashMap<String, usize>,
}

fn fnv1a(tag: u8, bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in std::iter::once(&tag).chain(bytes) {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash
}
fn leaf_kind(kind: &NodeKind) -> Option<BlockKind> {
    Some(match kind {
        NodeKind::Paragraph { .. } => BlockKind::Paragraph,
        NodeKind::Heading { .. } => BlockKind::Heading,
        NodeKind::CodeBlock { .. } => BlockKind::CodeBlock,
        NodeKind::ThematicBreak {} => BlockKind::ThematicBreak,
        NodeKind::Table { .. } => BlockKind::Table,
        NodeKind::UnsupportedBlock { .. } => BlockKind::Unsupported,
        _ => return None,
    })
}
fn container(kind: &NodeKind) -> bool {
    matches!(
        kind,
        NodeKind::BlockQuote { .. }
            | NodeKind::List { .. }
            | NodeKind::ListItem { .. }
            | NodeKind::FootnoteDefinition { .. }
            | NodeKind::Alert { .. }
    )
}

impl BlockIndex {
    /// Iterative walk over a validated tree; bounded by the node count.
    pub(crate) fn new(source: &str, roots: &[NodeId], nodes: &[Node]) -> Self {
        let by_id: HashMap<String, usize> = nodes
            .iter()
            .enumerate()
            .map(|(i, n)| (n.id.0.clone(), i))
            .collect();
        let mut blocks = Vec::new();
        let mut occurrences: HashMap<(BlockKind, u64), u32> = HashMap::new();
        let mut section = None;
        let mut stack: Vec<&NodeId> = roots.iter().rev().collect();
        while let Some(id) = stack.pop() {
            let Some(&i) = by_id.get(&id.0) else { continue };
            let node = &nodes[i];
            if let Some(kind) = leaf_kind(&node.content) {
                let text = node.span.slice(source).unwrap_or("");
                let fingerprint = fnv1a(kind.tag(), text.as_bytes());
                let seen = occurrences.entry((kind, fingerprint)).or_insert(0);
                let occurrence = *seen;
                *seen = seen.saturating_add(1);
                blocks.push(Block {
                    node: i,
                    kind,
                    start: node.span.start,
                    end: node.span.end,
                    fingerprint,
                    occurrence,
                    section,
                });
                if kind == BlockKind::Heading {
                    section = Some(fingerprint);
                }
            } else if container(&node.content) {
                stack.extend(node.content.children().iter().rev());
            }
        }
        Self { blocks, by_id }
    }
    fn fingerprint_at(&self, i: Option<usize>) -> Option<u64> {
        i.and_then(|i| self.blocks.get(i)).map(|b| b.fingerprint)
    }
    fn neighbours(&self, i: usize) -> (Option<u64>, Option<u64>) {
        (
            self.fingerprint_at(i.checked_sub(1)),
            self.fingerprint_at(i.checked_add(1)),
        )
    }
    /// Block containing `offset`, else the first block after it, else the last.
    fn locate(&self, offset: u64) -> Option<usize> {
        let after = self.blocks.partition_point(|b| b.end <= offset);
        if after < self.blocks.len() {
            Some(after)
        } else {
            self.blocks.len().checked_sub(1)
        }
    }
}

fn floor_boundary(source: &str, mut offset: usize) -> usize {
    offset = offset.min(source.len());
    while !source.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}
/// Distance between a candidate start in the new source and the anchor's
/// proportional position, compared without floating point.
fn distance(anchor: &Anchor, start: u64, new_len: u64) -> u128 {
    let candidate = u128::from(start) * u128::from(anchor.source_len.max(1));
    let target = u128::from(anchor.source_offset) * u128::from(new_len.max(1));
    candidate.abs_diff(target)
}

impl Snapshot {
    /// Anchor for an absolute source byte offset (for example the first
    /// visible character). Offsets between blocks snap to the next block.
    pub fn anchor_at(&self, source_offset: u64) -> Result<Anchor, AnchorError> {
        let index = self.blocks();
        let i = index.locate(source_offset).ok_or(AnchorError::NoBlocks)?;
        let block = &index.blocks[i];
        let source_offset = source_offset.clamp(block.start, block.end);
        let source_offset = floor_boundary(
            self.source(),
            usize::try_from(source_offset).unwrap_or(usize::MAX),
        ) as u64;
        let (previous, next) = index.neighbours(i);
        let (_, nodes) = self.structure();
        Ok(Anchor {
            version: ANCHOR_VERSION,
            generation: self.generation,
            node: nodes[block.node].id.clone(),
            kind: block.kind,
            fingerprint: block.fingerprint,
            occurrence: block.occurrence,
            offset: source_offset.saturating_sub(block.start),
            source_offset,
            source_len: self.source().len() as u64,
            previous,
            next,
            section: block.section,
        })
    }
    /// Anchor for a byte offset within any node (block, container or inline).
    pub fn anchor_for(&self, node: &NodeId, offset: u64) -> Result<Anchor, AnchorError> {
        let index = self.blocks();
        let (_, nodes) = self.structure();
        let &i = index.by_id.get(&node.0).ok_or(AnchorError::UnknownNode)?;
        let span = nodes[i].span;
        self.anchor_at(span.start.saturating_add(offset).min(span.end))
    }
    /// Resolve an anchor (possibly from an older snapshot or a previous
    /// process) against this snapshot. `Ok(None)` means the position is lost
    /// because this snapshot has no anchorable blocks.
    pub fn resolve(&self, anchor: &Anchor) -> Result<Option<ResolvedAnchor>, AnchorError> {
        if anchor.version != ANCHOR_VERSION {
            return Err(AnchorError::UnsupportedVersion);
        }
        let index = self.blocks();
        if index.blocks.is_empty() {
            return Ok(None);
        }
        let new_len = self.source().len() as u64;
        let best = |candidates: &mut dyn Iterator<Item = (usize, u32)>| {
            candidates
                .max_by(|(a, sa), (b, sb)| {
                    let (da, db) = (
                        distance(anchor, index.blocks[*a].start, new_len),
                        distance(anchor, index.blocks[*b].start, new_len),
                    );
                    // Highest score, then nearest proportional position, then earliest.
                    sa.cmp(sb).then(db.cmp(&da)).then(b.cmp(a))
                })
                .map(|(i, _)| i)
        };
        let context = |i: usize| {
            let (previous, next) = index.neighbours(i);
            (previous == anchor.previous, next == anchor.next)
        };
        // 1. Identical content.
        let exact = best(
            &mut index
                .blocks
                .iter()
                .enumerate()
                .filter(|(_, b)| b.kind == anchor.kind && b.fingerprint == anchor.fingerprint)
                .map(|(i, b)| {
                    let (p, n) = context(i);
                    let score = 2 * u32::from(p)
                        + 2 * u32::from(n)
                        + u32::from(b.occurrence == anchor.occurrence);
                    (i, score)
                }),
        );
        if let Some(i) = exact {
            return Ok(Some(self.resolved(i, anchor.offset, AnchorMatch::Exact)));
        }
        // 2. Same kind in the same neighbourhood, excluding the neighbours themselves.
        let relocated = best(&mut index.blocks.iter().enumerate().filter_map(|(i, b)| {
            if b.kind != anchor.kind
                || Some(b.fingerprint) == anchor.previous
                || Some(b.fingerprint) == anchor.next
            {
                return None;
            }
            let (p, n) = context(i);
            (p || n).then_some((i, u32::from(p) + u32::from(n)))
        }));
        if let Some(i) = relocated {
            return Ok(Some(self.resolved(
                i,
                anchor.offset,
                AnchorMatch::Relocated,
            )));
        }
        // 3. Nearest survivor: after the old predecessor, the old successor,
        // the enclosing heading, then the proportional source position.
        let find = |fingerprint: Option<u64>| {
            let fingerprint = fingerprint?;
            best(
                &mut index
                    .blocks
                    .iter()
                    .enumerate()
                    .filter(|(_, b)| b.fingerprint == fingerprint)
                    .map(|(i, _)| (i, 0)),
            )
        };
        let nearest = find(anchor.previous)
            .map(|i| (i + 1).min(index.blocks.len() - 1))
            .or_else(|| find(anchor.next))
            .or_else(|| find(anchor.section))
            .or_else(|| {
                let target = u128::from(anchor.source_offset) * u128::from(new_len)
                    / u128::from(anchor.source_len.max(1));
                index.locate(u64::try_from(target).unwrap_or(u64::MAX))
            });
        Ok(nearest.map(|i| self.resolved(i, 0, AnchorMatch::Nearest)))
    }
    fn resolved(&self, i: usize, offset: u64, quality: AnchorMatch) -> ResolvedAnchor {
        let index = self.blocks();
        let block = &index.blocks[i];
        let (_, nodes) = self.structure();
        let wanted = block.start.saturating_add(offset).min(block.end);
        let source_offset =
            floor_boundary(self.source(), usize::try_from(wanted).unwrap_or(usize::MAX)) as u64;
        // Spans are UTF-8 boundaries, so flooring never crosses the block start.
        let source_offset = source_offset.max(block.start);
        ResolvedAnchor {
            quality,
            node: nodes[block.node].id.clone(),
            offset: source_offset - block.start,
            source_offset,
        }
    }
}
