//! Bounded owned presentation values. No GTK, file access or parser internals.
use crate::document::{ImageBudget, ResourceError};
use markdown_linux_core::{CoreDocument, ffi::*, find::CoreFindResult};
use std::{collections::BTreeMap, mem::size_of, sync::Arc};

pub const PLAN_BUDGET: usize = 128 * 1024 * 1024;

#[derive(Debug, PartialEq, Eq)]
pub enum RenderError {
    Cancelled,
    Budget,
    InvalidModel,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    Fragment(u32),
    External(String),
    Inert,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderedImage {
    pub target: String,
    pub alt: String,
    pub failure: Option<ResourceError>,
    pub data: Option<Arc<[u8]>>,
    pub width: usize,
    pub height: usize,
}

impl RenderedImage {
    pub fn failure_message(&self) -> Option<&'static str> {
        self.failure.as_ref().map(ResourceError::message)
    }
}

#[derive(Clone, Debug)]
pub struct Run {
    pub text: String,
    pub style: u8,
    pub link: Option<u32>,
    pub node_id: Option<u32>,
    pub node_char_offset: usize,
    pub atomic: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct DocumentPosition {
    pub fragment_index: usize,
    pub char_offset: usize,
}

impl DocumentPosition {
    pub fn new(fragment_index: usize, char_offset: usize) -> Self {
        Self {
            fragment_index,
            char_offset,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DocumentSelection {
    pub anchor: DocumentPosition,
    pub active: DocumentPosition,
}

impl DocumentSelection {
    pub fn new(anchor: DocumentPosition, active: DocumentPosition) -> Self {
        Self { anchor, active }
    }

    pub fn start(&self) -> DocumentPosition {
        self.anchor.min(self.active)
    }

    pub fn end(&self) -> DocumentPosition {
        self.anchor.max(self.active)
    }

    pub fn is_empty(&self) -> bool {
        self.anchor == self.active
    }
}

#[derive(Debug)]
pub struct Fragment {
    pub runs: Vec<Run>,
    pub heading: u32,
    /// Only the first fragment represents a semantic heading; continuations
    /// retain the heading level for visual styling.
    pub heading_start: bool,
    pub code: bool,
    pub is_table_row: bool,
    pub is_table_header: bool,
    pub table_row_ordinal: usize,
    pub images: Vec<RenderedImage>,
    pub starts_block: bool,
    pub separator_before: &'static str,
}

impl Fragment {
    pub fn text(&self) -> String {
        self.runs.iter().map(|r| r.text.as_str()).collect()
    }

    pub fn has_link(&self, index: u32) -> bool {
        self.runs.iter().any(|r| r.link == Some(index))
    }

    pub fn markup(&self, epoch: u64, index: u32) -> String {
        self.markup_with_highlights_and_selection(epoch, index, &[], None)
    }

    pub fn markup_with_highlights(
        &self,
        epoch: u64,
        index: u32,
        highlights: &[(usize, usize, bool)],
    ) -> String {
        self.markup_with_highlights_and_selection(epoch, index, highlights, None)
    }

    pub fn markup_with_highlights_and_selection(
        &self,
        epoch: u64,
        index: u32,
        highlights: &[(usize, usize, bool)],
        selection: Option<(usize, usize)>,
    ) -> String {
        let mut out = String::new();
        let mut frag_char_idx = 0;
        let tags = [(1, "b"), (2, "i"), (4, "s"), (8, "tt")];

        for run in &self.runs {
            if let Some(link) = run.link {
                out.push_str(&format!("<a href=\"md:{epoch}:{index}:{link}\">"));
            }
            for (bit, tag) in tags {
                if run.style & bit != 0 {
                    out.push_str(&format!("<{tag}>"));
                }
            }
            if run.style & 32 != 0 {
                out.push_str("<span foreground=\"#3584e4\"><b>");
            } else if run.style & 64 != 0 {
                out.push_str("<span foreground=\"gray\">");
            }

            #[derive(PartialEq, Eq, Clone, Copy)]
            enum HighlightKind {
                Selection,
                FindActive,
                FindPassive,
            }

            let mut active_hl: Option<HighlightKind> = None;

            for ch in run.text.chars() {
                let is_selected =
                    selection.is_some_and(|(s, e)| frag_char_idx >= s && frag_char_idx < e);
                let cur_hl = if is_selected {
                    Some(HighlightKind::Selection)
                } else {
                    highlights
                        .iter()
                        .find(|(s, e, _)| frag_char_idx >= *s && frag_char_idx < *e)
                        .map(|(_, _, is_cur)| {
                            if *is_cur {
                                HighlightKind::FindActive
                            } else {
                                HighlightKind::FindPassive
                            }
                        })
                };

                if cur_hl != active_hl {
                    if active_hl.is_some() {
                        out.push_str("</span>");
                    }
                    if let Some(hl) = cur_hl {
                        match hl {
                            HighlightKind::Selection => {
                                out.push_str(
                                    "<span background=\"#3584e4\" foreground=\"#ffffff\">",
                                );
                            }
                            HighlightKind::FindActive => {
                                out.push_str(
                                    "<span background=\"#ff7800\" foreground=\"#ffffff\">",
                                );
                            }
                            HighlightKind::FindPassive => {
                                out.push_str(
                                    "<span background=\"#f6d32d\" foreground=\"#000000\">",
                                );
                            }
                        }
                    }
                    active_hl = cur_hl;
                }

                match ch {
                    '&' => out.push_str("&amp;"),
                    '<' => out.push_str("&lt;"),
                    '>' => out.push_str("&gt;"),
                    '"' => out.push_str("&quot;"),
                    '\'' => out.push_str("&apos;"),
                    _ => out.push(ch),
                }
                frag_char_idx += 1;
            }

            if active_hl.is_some() {
                out.push_str("</span>");
            }

            if run.style & 32 != 0 {
                out.push_str("</b></span>");
            } else if run.style & 64 != 0 {
                out.push_str("</span>");
            }
            for (bit, tag) in tags.into_iter().rev() {
                if run.style & bit != 0 {
                    out.push_str(&format!("</{tag}>"));
                }
            }
            if run.link.is_some() {
                out.push_str("</a>");
            }
        }
        out
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FindMatchRange {
    pub fragment_index: usize,
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FindMatch {
    pub index: usize,
    pub unit_node: u32,
    pub span_start: u32,
    pub span_end: u32,
    pub exact: bool,
    pub primary_fragment_index: usize,
    pub ranges: Vec<FindMatchRange>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FindMatches {
    pub generation: u64,
    pub matches: Vec<FindMatch>,
    pub truncated: bool,
}

impl FindMatches {
    pub fn is_empty(&self) -> bool {
        self.matches.is_empty()
    }

    pub fn count(&self) -> usize {
        self.matches.len()
    }

    pub fn status(&self, current: Option<usize>) -> String {
        if self.matches.is_empty() {
            return "Not found".to_string();
        }
        let total = format!(
            "{}{}",
            self.matches.len(),
            if self.truncated { "+" } else { "" }
        );
        match current {
            None => {
                if self.matches.len() == 1 {
                    "1 match".to_string()
                } else {
                    format!("{total} matches")
                }
            }
            Some(curr) => format!("{} of {total}", curr + 1),
        }
    }

    pub fn index_at_or_after(&self, fragment_index: usize) -> Option<usize> {
        if self.matches.is_empty() {
            return None;
        }
        for (i, m) in self.matches.iter().enumerate() {
            if m.primary_fragment_index >= fragment_index {
                return Some(i);
            }
        }
        Some(0)
    }

    pub fn index_before(&self, fragment_index: usize) -> Option<usize> {
        if self.matches.is_empty() {
            return None;
        }
        for (i, m) in self.matches.iter().enumerate().rev() {
            if m.primary_fragment_index < fragment_index {
                return Some(i);
            }
        }
        Some(self.matches.len() - 1)
    }

    pub fn index_after(&self, current: usize, backwards: bool) -> Option<usize> {
        if self.matches.is_empty() {
            return None;
        }
        let count = self.matches.len();
        if backwards {
            Some((current + count - 1) % count)
        } else {
            Some((current + 1) % count)
        }
    }

    pub fn highlights_for_fragment(
        &self,
        fragment_index: usize,
        current: Option<usize>,
    ) -> Vec<(usize, usize, bool)> {
        let mut list = Vec::new();
        for (idx, m) in self.matches.iter().enumerate() {
            let is_current = current == Some(idx);
            for r in &m.ranges {
                if r.fragment_index == fragment_index {
                    list.push((r.start, r.end, is_current));
                }
            }
        }
        list
    }
}

#[derive(Debug)]
pub struct Outline {
    pub title: String,
    pub anchor: String,
    pub level: u32,
    pub fragment: u32,
}

#[derive(Debug, Default)]
pub struct RenderPlan {
    pub generation: u64,
    pub fragments: Vec<Fragment>,
    pub outline: Vec<Outline>,
    pub targets: Vec<Target>,
    pub images: Vec<RenderedImage>,
    pub accounted_bytes: usize,
}

pub type ImageLoader<'a> =
    dyn Fn(&str, usize) -> Result<(Vec<u8>, usize, usize), ResourceError> + 'a;

struct Builder<'a, F> {
    doc: &'a CoreDocument,
    plan: RenderPlan,
    cancelled: F,
    budget: usize,
    visits: usize,
    first: Vec<Option<u32>>,
    loader: Option<&'a ImageLoader<'a>>,
    image_budget: ImageBudget,
    image_cache: BTreeMap<String, RenderedImage>,
    current_block_images: Vec<RenderedImage>,
    accounted_decoded_bytes: usize,
}

impl RenderPlan {
    pub fn build(doc: &CoreDocument, cancelled: impl Fn() -> bool) -> Result<Self, RenderError> {
        Self::with_budget(doc, cancelled, PLAN_BUDGET)
    }

    pub fn with_budget(
        doc: &CoreDocument,
        cancelled: impl Fn() -> bool,
        budget: usize,
    ) -> Result<Self, RenderError> {
        Self::build_internal(doc, cancelled, budget, None, ImageBudget::default())
    }

    pub fn build_with_resources<'a>(
        doc: &CoreDocument,
        cancelled: impl Fn() -> bool,
        budget: usize,
        loader: Option<&'a ImageLoader<'a>>,
        image_budget: ImageBudget,
    ) -> Result<Self, RenderError> {
        Self::build_internal(doc, cancelled, budget, loader, image_budget)
    }

    fn build_internal<'a>(
        doc: &CoreDocument,
        cancelled: impl Fn() -> bool,
        budget: usize,
        loader: Option<&'a ImageLoader<'a>>,
        image_budget: ImageBudget,
    ) -> Result<Self, RenderError> {
        let mut b = Builder {
            doc,
            plan: RenderPlan {
                generation: doc.generation,
                ..Self::default()
            },
            cancelled,
            budget,
            visits: 0,
            first: Vec::new(),
            loader,
            image_budget,
            image_cache: BTreeMap::new(),
            current_block_images: Vec::new(),
            accounted_decoded_bytes: 0,
        };
        b.charge(
            doc.nodes
                .len()
                .checked_mul(size_of::<Option<u32>>())
                .ok_or(RenderError::Budget)?,
        )?;
        b.first.resize(doc.nodes.len(), None);
        for &root in &doc.roots {
            b.block(root, 0, None)?;
        }
        let mut anchors = BTreeMap::new();
        for entry in &doc.outline {
            b.check()?;
            let fragment = b
                .first
                .get(entry.node as usize)
                .copied()
                .flatten()
                .ok_or(RenderError::InvalidModel)?;
            let title = b.text(entry.title)?;
            let anchor = b.text(entry.anchor)?;
            b.charge(size_of::<Outline>() + title.len() + anchor.len())?;
            b.charge(size_of::<(String, u32)>() + anchor.len())?;
            anchors.insert(anchor.clone(), fragment);
            b.plan.outline.push(Outline {
                title,
                anchor,
                level: entry.level,
                fragment,
            });
        }
        for intent in &doc.links {
            b.check()?;
            let target = b.text(intent.target)?;
            let resolved = match intent.target_kind {
                MD_TARGET_FRAGMENT => anchors
                    .get(&target)
                    .copied()
                    .map(Target::Fragment)
                    .unwrap_or(Target::Inert),
                MD_TARGET_EXTERNAL
                    if ["https:", "http:", "mailto:"]
                        .iter()
                        .any(|s| target.starts_with(s)) =>
                {
                    Target::External(target)
                }
                _ => Target::Inert,
            };
            b.charge(
                size_of::<Target>()
                    + match &resolved {
                        Target::External(s) => s.len(),
                        _ => 0,
                    },
            )?;
            b.plan.targets.push(resolved);
        }
        Ok(b.plan)
    }

    pub fn token_target(&self, token: &str, epoch: u64, fragment: u32) -> Option<&Target> {
        let mut parts = token.split(':');
        if parts.next()? != "md"
            || parts.next()?.parse::<u64>().ok()? != epoch
            || parts.next()?.parse::<u32>().ok()? != fragment
        {
            return None;
        }
        let link = parts.next()?.parse::<u32>().ok()?;
        if parts.next().is_some() || !self.fragments.get(fragment as usize)?.has_link(link) {
            return None;
        }
        self.targets.get(link as usize)
    }
}

impl<F: Fn() -> bool> Builder<'_, F> {
    fn check(&self) -> Result<(), RenderError> {
        if (self.cancelled)() {
            Err(RenderError::Cancelled)
        } else {
            Ok(())
        }
    }

    fn charge(&mut self, n: usize) -> Result<(), RenderError> {
        self.check()?;
        self.plan.accounted_bytes = self
            .plan
            .accounted_bytes
            .checked_add(n)
            .filter(|n| *n <= self.budget)
            .ok_or(RenderError::Budget)?;
        Ok(())
    }

    fn text(&mut self, s: md_str) -> Result<String, RenderError> {
        self.charge(s.length as usize)?;
        self.doc
            .text(s)
            .map(str::to_owned)
            .map_err(|_| RenderError::InvalidModel)
    }

    fn node(&mut self, id: u32, depth: usize) -> Result<md_node, RenderError> {
        self.check()?;
        self.visits += 1;
        if depth > 256 || self.visits > self.doc.nodes.len().saturating_mul(2) {
            return Err(RenderError::InvalidModel);
        }
        self.doc
            .nodes
            .get(id as usize)
            .copied()
            .ok_or(RenderError::InvalidModel)
    }

    fn children(&mut self, n: md_node) -> Result<Vec<u32>, RenderError> {
        let end = (n.first_child as usize)
            .checked_add(n.child_count as usize)
            .ok_or(RenderError::InvalidModel)?;
        self.charge(
            (n.child_count as usize)
                .checked_mul(size_of::<u32>())
                .ok_or(RenderError::Budget)?,
        )?;
        self.doc
            .children
            .get(n.first_child as usize..end)
            .map(|s| s.to_vec())
            .ok_or(RenderError::InvalidModel)
    }

    fn run(
        &mut self,
        runs: &mut Vec<Run>,
        mut text: String,
        style: u8,
        link: Option<u32>,
        node_id: Option<u32>,
        atomic: bool,
    ) -> Result<(), RenderError> {
        self.charge(size_of::<Run>() + text.len())?;
        // Pango markup is XML and GTK strings are NUL terminated. Preserve a
        // visible replacement in both presentation and native selected text.
        let invalid = |c: char| {
            matches!(
                c,
                '\u{0}'..='\u{8}' | '\u{b}' | '\u{c}' | '\u{e}'..='\u{1f}' | '\u{fffe}' | '\u{ffff}'
            )
        };
        if text.chars().any(invalid) {
            let bytes = text
                .chars()
                .map(|c| if invalid(c) { 3 } else { c.len_utf8() })
                .sum();
            self.charge(bytes)?;
            text = text
                .chars()
                .map(|c| if invalid(c) { '\u{fffd}' } else { c })
                .collect();
        }
        runs.push(Run {
            text,
            style,
            link,
            node_id,
            node_char_offset: 0,
            atomic,
        });
        Ok(())
    }

    fn fallback(&mut self, n: md_node) -> Result<String, RenderError> {
        let text = std::str::from_utf8(
            self.doc
                .source
                .get(n.span_start as usize..n.span_end as usize)
                .ok_or(RenderError::InvalidModel)?,
        )
        .map_err(|_| RenderError::InvalidModel)?;
        self.charge(text.len())?;
        Ok(text.to_owned())
    }

    fn image(
        &mut self,
        id: u32,
        depth: usize,
        style: u8,
        runs: &mut Vec<Run>,
    ) -> Result<(), RenderError> {
        let n = self.node(id, depth)?;
        let alt = self.text(n.text)?;
        let mut target_str = String::new();
        let failure;

        if (n.aux as usize) < self.doc.resources.len() {
            let intent = &self.doc.resources[n.aux as usize];
            target_str = self.text(intent.target)?;
            match intent.target_kind {
                MD_TARGET_REQUIRES_CAPABILITY => {
                    // Sibling image beside document
                    if let Some(cached) = self.image_cache.get(&target_str).cloned() {
                        if cached.failure.is_none() {
                            // Deduplicated hit on a loaded image: share the existing Arc data
                            // and charge only shallow metadata without multiplying pixel memory.
                            self.charge(size_of::<RenderedImage>() + target_str.len() + alt.len())?;
                            let mut rendered = cached;
                            rendered.alt = alt.clone();
                            self.plan.images.push(rendered.clone());
                            self.current_block_images.push(rendered);
                            self.first[id as usize] = Some(self.plan.fragments.len() as u32);
                            let display_text = if alt.is_empty() {
                                "[image]".to_string()
                            } else {
                                format!("[{alt}]")
                            };
                            self.run(runs, display_text, style, None, Some(id), true)?;
                            return Ok(());
                        }
                        failure = cached.failure;
                    } else if self.image_cache.len() >= self.image_budget.max_images {
                        failure = Some(ResourceError::Limit);
                    } else if let Some(loader) = self.loader {
                        self.check()?;
                        match loader(&target_str, self.image_budget.max_file_bytes) {
                            Ok((data, w, h)) => {
                                let decoded_bytes = w.saturating_mul(h).saturating_mul(4);
                                if self.accounted_decoded_bytes + decoded_bytes
                                    > self.image_budget.max_decoded_bytes
                                {
                                    failure = Some(ResourceError::Limit);
                                } else {
                                    self.accounted_decoded_bytes += decoded_bytes;
                                    self.charge(
                                        data.len()
                                            + size_of::<RenderedImage>()
                                            + target_str.len()
                                            + alt.len(),
                                    )?;
                                    let rendered = RenderedImage {
                                        target: target_str.clone(),
                                        alt: alt.clone(),
                                        failure: None,
                                        data: Some(Arc::from(data.into_boxed_slice())),
                                        width: w,
                                        height: h,
                                    };
                                    self.image_cache
                                        .insert(target_str.clone(), rendered.clone());
                                    self.plan.images.push(rendered.clone());
                                    self.current_block_images.push(rendered);
                                    self.first[id as usize] =
                                        Some(self.plan.fragments.len() as u32);
                                    let display_text = if alt.is_empty() {
                                        "[image]".to_string()
                                    } else {
                                        format!("[{alt}]")
                                    };
                                    self.run(runs, display_text, style, None, Some(id), true)?;
                                    return Ok(());
                                }
                            }
                            Err(err) => {
                                failure = Some(err);
                            }
                        }
                    } else {
                        failure = Some(ResourceError::NotLoaded);
                    }
                }
                MD_TARGET_DENIED => {
                    if intent.denial == MD_DENIAL_REMOTE_RESOURCE {
                        failure = Some(ResourceError::Remote);
                    } else {
                        failure = Some(ResourceError::Blocked);
                    }
                }
                MD_TARGET_EXTERNAL => {
                    failure = Some(ResourceError::Remote);
                }
                _ => {
                    failure = Some(ResourceError::Blocked);
                }
            }
        } else {
            failure = Some(ResourceError::Blocked);
        }

        self.charge(size_of::<RenderedImage>() + target_str.len() + alt.len())?;
        let rendered = if let Some(cached) = self.image_cache.get(&target_str) {
            let mut r = cached.clone();
            r.alt = alt.clone();
            r
        } else {
            let r = RenderedImage {
                target: target_str.clone(),
                alt: alt.clone(),
                failure,
                data: None,
                width: 0,
                height: 0,
            };
            if failure != Some(ResourceError::Remote)
                && failure != Some(ResourceError::Blocked)
                && failure != Some(ResourceError::OutsideFolder)
            {
                self.image_cache.insert(target_str.clone(), r.clone());
            }
            r
        };

        self.plan.images.push(rendered.clone());
        self.current_block_images.push(rendered);
        self.first[id as usize] = Some(self.plan.fragments.len() as u32);
        let display_text = if alt.is_empty() {
            "[image]".to_string()
        } else {
            format!("[{alt}]")
        };
        self.run(runs, display_text, style, None, Some(id), true)?;
        Ok(())
    }

    fn inline(
        &mut self,
        id: u32,
        depth: usize,
        style: u8,
        link: Option<u32>,
        runs: &mut Vec<Run>,
    ) -> Result<(), RenderError> {
        let n = self.node(id, depth)?;
        let style = style
            | match n.kind {
                MD_NODE_STRONG => 1,
                MD_NODE_EMPHASIS => 2,
                MD_NODE_STRIKETHROUGH => 4,
                MD_NODE_CODE_SPAN => 8,
                _ => 0,
            };
        let link = if n.kind == MD_NODE_LINK {
            if n.aux as usize >= self.doc.links.len() {
                return Err(RenderError::InvalidModel);
            }
            Some(n.aux)
        } else {
            link
        };
        match n.kind {
            MD_NODE_SOFT_BREAK => self.run(runs, " ".into(), style, link, Some(id), true)?,
            MD_NODE_HARD_BREAK => self.run(runs, "\n".into(), style, link, Some(id), true)?,
            MD_NODE_TEXT | MD_NODE_CODE_SPAN | MD_NODE_FOOTNOTE_REFERENCE => {
                let text = self.text(n.text)?;
                self.run(runs, text, style, link, Some(id), false)?;
            }
            MD_NODE_IMAGE => {
                self.image(id, depth, style, runs)?;
            }
            MD_NODE_UNSUPPORTED_INLINE => {
                let text = self.fallback(n)?;
                self.run(runs, text, style, link, Some(id), false)?;
            }
            _ => {
                for child in self.children(n)? {
                    self.inline(child, depth + 1, style, link, runs)?;
                }
            }
        }
        Ok(())
    }

    fn table(&mut self, id: u32, depth: usize) -> Result<(), RenderError> {
        let n = self.node(id, depth)?;
        self.first[id as usize] = Some(self.plan.fragments.len() as u32);
        let col_count = n.aux_count as usize;
        let rows = self.children(n)?;

        for (r, &row_id) in rows.iter().enumerate() {
            let row_node = self.node(row_id, depth + 1)?;
            self.first[row_id as usize] = Some(self.plan.fragments.len() as u32);
            let is_header = (row_node.flags & 32) != 0 || r == 0;
            let cells = self.children(row_node)?;
            let mut row_runs = Vec::new();
            let total_cols = col_count.max(cells.len());

            for c in 0..total_cols {
                if c > 0 {
                    self.run(&mut row_runs, "\t".into(), 0, None, None, false)?;
                }
                if c < cells.len() {
                    let cell_id = cells[c];
                    let cell_node = self.node(cell_id, depth + 2)?;
                    for child in self.children(cell_node)? {
                        self.inline(
                            child,
                            depth + 3,
                            if is_header { 1 } else { 0 },
                            None,
                            &mut row_runs,
                        )?;
                    }
                }
            }

            let initial = self.plan.fragments.len();
            let separator_before = if initial == 0 {
                ""
            } else if is_header || !self.plan.fragments[initial - 1].is_table_row {
                "\n\n"
            } else {
                "\n"
            };

            self.charge(size_of::<Fragment>())?;
            self.plan.fragments.push(Fragment {
                runs: row_runs,
                heading: 0,
                heading_start: false,
                code: false,
                is_table_row: true,
                is_table_header: is_header,
                table_row_ordinal: r,
                images: Vec::new(),
                starts_block: true,
                separator_before,
            });
        }
        self.current_block_images.clear();
        Ok(())
    }

    fn block(
        &mut self,
        id: u32,
        depth: usize,
        marker: Option<(String, u8)>,
    ) -> Result<(), RenderError> {
        let n = self.node(id, depth)?;
        if (n.kind == MD_NODE_LIST || (n.kind == MD_NODE_LIST_ITEM && n.child_count == 0))
            && marker.is_some()
        {
            let (m, s) = marker.clone().unwrap();
            let mut runs = Vec::new();
            self.run(&mut runs, m, 16 | s, None, None, true)?;
            self.fragments(runs, 0, false)?;
        }
        if n.kind == MD_NODE_TABLE {
            return self.table(id, depth);
        }
        if n.kind == MD_NODE_LIST {
            let ordered = (n.flags & MD_NODE_ORDERED) != 0;
            let start = n.number;
            for (i, child) in self.children(n)?.into_iter().enumerate() {
                let child_node = self.node(child, depth + 1)?;
                let (marker_str, marker_style) = if (child_node.flags & 8) != 0 {
                    let is_checked = (child_node.flags & 16) != 0;
                    if is_checked {
                        (format!("{}☑ ", "  ".repeat(depth)), 32)
                    } else {
                        (format!("{}☐ ", "  ".repeat(depth)), 64)
                    }
                } else if ordered {
                    (
                        format!("{}{}. ", "  ".repeat(depth), start.saturating_add(i as u64)),
                        0,
                    )
                } else {
                    (format!("{}• ", "  ".repeat(depth)), 0)
                };
                self.block(child, depth + 1, Some((marker_str, marker_style)))?;
            }
            return Ok(());
        }
        if matches!(
            n.kind,
            MD_NODE_PARAGRAPH
                | MD_NODE_HEADING
                | MD_NODE_CODE_BLOCK
                | MD_NODE_UNSUPPORTED_BLOCK
                | MD_NODE_THEMATIC_BREAK
                | MD_NODE_TABLE_CELL
        ) {
            self.first[id as usize] = Some(self.plan.fragments.len() as u32);
            let mut runs = Vec::new();
            if let Some((marker_str, marker_style)) = marker {
                self.run(&mut runs, marker_str, 16 | marker_style, None, None, true)?;
            }
            if n.kind == MD_NODE_CODE_BLOCK || n.kind == MD_NODE_UNSUPPORTED_BLOCK {
                let text = if n.kind == MD_NODE_UNSUPPORTED_BLOCK {
                    self.fallback(n)?
                } else {
                    self.text(n.text)?
                };
                self.run(
                    &mut runs,
                    text,
                    if n.kind == MD_NODE_CODE_BLOCK { 8 } else { 0 },
                    None,
                    Some(id),
                    false,
                )?;
            } else if n.kind == MD_NODE_THEMATIC_BREAK {
                self.run(&mut runs, "—".into(), 0, None, Some(id), true)?;
            } else {
                for child in self.children(n)? {
                    self.inline(child, depth + 1, 0, None, &mut runs)?;
                }
            }
            self.fragments(
                runs,
                if n.kind == MD_NODE_HEADING {
                    n.value
                } else {
                    0
                },
                n.kind == MD_NODE_CODE_BLOCK,
            )?;
        } else {
            for (i, child) in self.children(n)?.into_iter().enumerate() {
                self.block(child, depth + 1, if i == 0 { marker.clone() } else { None })?;
            }
        }
        Ok(())
    }

    fn fragments(&mut self, runs: Vec<Run>, heading: u32, code: bool) -> Result<(), RenderError> {
        let initial = self.plan.fragments.len();
        let separator_before = if initial == 0 { "" } else { "\n\n" };
        let mut fragment = Fragment {
            runs: Vec::new(),
            heading,
            heading_start: heading > 0,
            code,
            is_table_row: false,
            is_table_header: false,
            table_row_ordinal: 0,
            images: std::mem::take(&mut self.current_block_images),
            starts_block: true,
            separator_before,
        };
        let mut used = 0;
        for run in runs {
            let mut rest = run.text.as_str();
            let mut pos_chars = 0;
            while !rest.is_empty() {
                self.check()?;
                let available = 512 - used;
                let mut end = available.min(rest.len());
                while !rest.is_char_boundary(end) {
                    end -= 1;
                }
                if end < rest.len()
                    && let Some((i, c)) = rest[..end]
                        .char_indices()
                        .rev()
                        .find(|(_, c)| c.is_whitespace())
                {
                    end = i + c.len_utf8();
                }
                debug_assert!(used + end <= 512);
                if end == 0 {
                    self.push(fragment)?;
                    fragment = Fragment {
                        runs: Vec::new(),
                        heading,
                        heading_start: false,
                        code,
                        is_table_row: false,
                        is_table_header: false,
                        table_row_ordinal: 0,
                        images: Vec::new(),
                        starts_block: false,
                        separator_before: "",
                    };
                    used = 0;
                    continue;
                }
                self.charge(size_of::<Run>() + end)?;
                let piece_chars = rest[..end].chars().count();
                fragment.runs.push(Run {
                    text: rest[..end].into(),
                    style: run.style,
                    link: run.link,
                    node_id: run.node_id,
                    node_char_offset: run.node_char_offset + pos_chars,
                    atomic: run.atomic,
                });
                pos_chars += piece_chars;
                used += end;
                rest = &rest[end..];
                if used >= 512 {
                    self.push(fragment)?;
                    fragment = Fragment {
                        runs: Vec::new(),
                        heading,
                        heading_start: false,
                        code,
                        is_table_row: false,
                        is_table_header: false,
                        table_row_ordinal: 0,
                        images: Vec::new(),
                        starts_block: false,
                        separator_before: "",
                    };
                    used = 0;
                }
            }
        }
        if !fragment.runs.is_empty() || self.plan.fragments.len() == initial {
            self.push(fragment)?;
        }
        Ok(())
    }

    fn push(&mut self, f: Fragment) -> Result<(), RenderError> {
        self.charge(size_of::<Fragment>())?;
        self.plan.fragments.push(f);
        Ok(())
    }
}

fn byte_offset_to_char_offset(s: &str, byte_offset: usize) -> usize {
    let bounded = byte_offset.min(s.len());
    let mut b = bounded;
    while b > 0 && !s.is_char_boundary(b) {
        b -= 1;
    }
    s[..b].chars().count()
}

impl RenderPlan {
    pub fn map_matches(&self, doc: &CoreDocument, result: &CoreFindResult) -> FindMatches {
        if result.generation != self.generation || result.generation != doc.generation {
            return FindMatches {
                generation: self.generation,
                matches: Vec::new(),
                truncated: false,
            };
        }

        type NodeRunEntry = (usize, usize, usize, usize, bool);
        let mut node_runs: BTreeMap<u32, Vec<NodeRunEntry>> = BTreeMap::new();
        for (f_idx, fragment) in self.fragments.iter().enumerate() {
            let mut frag_char_offset = 0;
            for run in &fragment.runs {
                let char_len = run.text.chars().count();
                if let Some(node_id) = run.node_id {
                    node_runs.entry(node_id).or_default().push((
                        f_idx,
                        frag_char_offset,
                        run.node_char_offset,
                        char_len,
                        run.atomic,
                    ));
                }
                frag_char_offset += char_len;
            }
        }

        let mut matches = Vec::with_capacity(result.matches.len());
        for (m_idx, m) in result.matches.iter().enumerate() {
            let mut match_ranges = Vec::new();
            let seg_end = m.first_segment + m.segment_count;
            for s_idx in m.first_segment..seg_end.min(result.segments.len()) {
                let seg = &result.segments[s_idx];
                let Some(run_entries) = node_runs.get(&seg.node) else {
                    continue;
                };

                let node_text = if (seg.node as usize) < doc.nodes.len() {
                    let n = &doc.nodes[seg.node as usize];
                    if n.kind == MD_NODE_UNSUPPORTED_INLINE || n.kind == MD_NODE_UNSUPPORTED_BLOCK {
                        let span_start = n.span_start as usize;
                        let span_end = n.span_end as usize;
                        doc.source
                            .get(span_start..span_end)
                            .and_then(|b| std::str::from_utf8(b).ok())
                            .unwrap_or("")
                    } else {
                        doc.text(n.text).unwrap_or("")
                    }
                } else {
                    ""
                };

                let char_start = byte_offset_to_char_offset(node_text, seg.text_start as usize);
                let char_end = byte_offset_to_char_offset(node_text, seg.text_end as usize);

                for &(f_idx, frag_char_offset, run_node_char_offset, run_char_len, atomic) in
                    run_entries
                {
                    if atomic {
                        match_ranges.push(FindMatchRange {
                            fragment_index: f_idx,
                            start: frag_char_offset,
                            end: frag_char_offset + run_char_len,
                        });
                        continue;
                    }
                    let run_start_in_node = run_node_char_offset;
                    let run_end_in_node = run_node_char_offset + run_char_len;
                    let overlap_start = char_start.max(run_start_in_node);
                    let overlap_end = char_end.min(run_end_in_node);
                    if overlap_start < overlap_end {
                        let local_start = frag_char_offset + (overlap_start - run_start_in_node);
                        let local_end = frag_char_offset + (overlap_end - run_start_in_node);
                        match_ranges.push(FindMatchRange {
                            fragment_index: f_idx,
                            start: local_start,
                            end: local_end,
                        });
                    }
                }
            }

            if match_ranges.is_empty() {
                continue;
            }

            match_ranges.sort_by(|a, b| {
                a.fragment_index
                    .cmp(&b.fragment_index)
                    .then_with(|| a.start.cmp(&b.start))
            });

            let mut merged: Vec<FindMatchRange> = Vec::with_capacity(match_ranges.len());
            let mut curr = match_ranges[0].clone();
            for next in match_ranges.into_iter().skip(1) {
                if next.fragment_index == curr.fragment_index && next.start <= curr.end {
                    curr.end = curr.end.max(next.end);
                } else {
                    merged.push(curr);
                    curr = next;
                }
            }
            merged.push(curr);

            let primary = merged[0].fragment_index;
            matches.push(FindMatch {
                index: m_idx,
                unit_node: m.unit,
                span_start: m.span_start,
                span_end: m.span_end,
                exact: m.exact,
                primary_fragment_index: primary,
                ranges: merged,
            });
        }

        FindMatches {
            generation: self.generation,
            matches,
            truncated: result.truncated,
        }
    }

    pub fn text_between(&self, start: DocumentPosition, end: DocumentPosition) -> String {
        if self.fragments.is_empty() {
            return String::new();
        }
        let (s, e) = if start <= end {
            (start, end)
        } else {
            (end, start)
        };
        let s_frag = s.fragment_index.min(self.fragments.len() - 1);
        let e_frag = e.fragment_index.min(self.fragments.len() - 1);

        let mut out = String::new();
        for f in s_frag..=e_frag {
            let frag = &self.fragments[f];
            let frag_chars: Vec<char> = frag.text().chars().collect();
            let from = if f == s_frag {
                s.char_offset.min(frag_chars.len())
            } else {
                0
            };
            let to = if f == e_frag {
                e.char_offset.min(frag_chars.len())
            } else {
                frag_chars.len()
            };

            if f > s_frag && !frag.separator_before.is_empty() {
                out.push_str(frag.separator_before);
            }
            if to > from {
                out.extend(&frag_chars[from..to]);
            }
        }
        out
    }

    pub fn source_between<'a>(
        &self,
        start: DocumentPosition,
        end: DocumentPosition,
        doc: &'a CoreDocument,
    ) -> Option<&'a [u8]> {
        if doc.source.is_empty() || self.fragments.is_empty() {
            return None;
        }
        let (s, e) = if start <= end {
            (start, end)
        } else {
            (end, start)
        };
        let last_frag = self.fragments.len() - 1;
        let last_len = self.fragments[last_frag].text().chars().count();

        // Whole-document selection returns source bytes exactly
        if s.fragment_index == 0
            && s.char_offset == 0
            && e.fragment_index >= last_frag
            && e.char_offset >= last_len
        {
            return Some(&doc.source);
        }

        let s_frag = s.fragment_index.min(last_frag);
        let e_frag = e.fragment_index.min(last_frag);

        // Whole-fragment check: selection must start at offset 0 of s_frag and cover through end of e_frag
        let end_len = self.fragments[e_frag].text().chars().count();
        let is_whole_fragments = s.char_offset == 0 && e.char_offset >= end_len;

        if is_whole_fragments {
            let mut min_span = u32::MAX;
            let mut max_span = 0;
            let mut found = false;

            for f in s_frag..=e_frag {
                for run in &self.fragments[f].runs {
                    if let Some(node_id) = run.node_id
                        && let Some(node) = doc.nodes.get(node_id as usize)
                        && node.span_start <= node.span_end
                        && (node.span_end as usize) <= doc.source.len()
                    {
                        if node.span_start < min_span {
                            min_span = node.span_start;
                        }
                        if node.span_end > max_span {
                            max_span = node.span_end;
                        }
                        found = true;
                    }
                }
            }

            if found && min_span <= max_span && (max_span as usize) <= doc.source.len() {
                return Some(&doc.source[min_span as usize..max_span as usize]);
            }
        }

        // For partial / mid-fragment selections, return None so callers fall back to plain text bytes
        None
    }

    pub fn selection_for_fragment(
        &self,
        fragment_index: usize,
        selection: Option<DocumentSelection>,
    ) -> Option<(usize, usize)> {
        let sel = selection?;
        if sel.is_empty() {
            return None;
        }
        let start = sel.start();
        let end = sel.end();
        if fragment_index < start.fragment_index || fragment_index > end.fragment_index {
            return None;
        }
        let frag_len = self.fragments.get(fragment_index)?.text().chars().count();
        let s = if fragment_index == start.fragment_index {
            start.char_offset.min(frag_len)
        } else {
            0
        };
        let e = if fragment_index == end.fragment_index {
            end.char_offset.min(frag_len)
        } else {
            frag_len
        };
        if e > s { Some((s, e)) } else { None }
    }

    pub fn source_offset_at(&self, pos: DocumentPosition, doc: &CoreDocument) -> u64 {
        if doc.source.is_empty() || self.fragments.is_empty() {
            return 0;
        }

        let f_idx = pos
            .fragment_index
            .min(self.fragments.len().saturating_sub(1));
        let frag = &self.fragments[f_idx];
        let c_offset = pos.char_offset.min(frag.text().chars().count());

        let mut char_pos = 0;
        let mut matched_run: Option<&Run> = None;
        let mut run_start_char = 0;

        for run in &frag.runs {
            let run_len = run.text.chars().count();
            let run_end = char_pos + run_len;
            if c_offset >= char_pos
                && (c_offset < run_end
                    || (c_offset == run_end && run_end == frag.text().chars().count()))
            {
                matched_run = Some(run);
                run_start_char = char_pos;
                break;
            }
            char_pos = run_end;
        }

        if let Some(run) = matched_run
            && let Some(node_id) = run.node_id
            && let Some(node) = doc.nodes.get(node_id as usize)
            && node.span_start <= node.span_end
            && (node.span_end as usize) <= doc.source.len()
        {
            if run.atomic {
                return node.span_start as u64;
            }
            let local_char = c_offset.saturating_sub(run_start_char);
            let node_char = run.node_char_offset + local_char;
            let node_bytes = &doc.source[node.span_start as usize..node.span_end as usize];
            let byte_offset = char_offset_to_utf8_byte_offset(node_bytes, node_char);
            return ((node.span_start as usize) + byte_offset).min(doc.source.len()) as u64;
        }

        // Backward fallback
        for f in (0..=f_idx).rev() {
            for r in &self.fragments[f].runs {
                if let Some(node_id) = r.node_id
                    && let Some(node) = doc.nodes.get(node_id as usize)
                {
                    return node.span_start as u64;
                }
            }
        }

        // Forward fallback
        for f in (f_idx + 1)..self.fragments.len() {
            for r in &self.fragments[f].runs {
                if let Some(node_id) = r.node_id
                    && let Some(node) = doc.nodes.get(node_id as usize)
                {
                    return node.span_start as u64;
                }
            }
        }

        0
    }

    pub fn position_at_source_offset(
        &self,
        source_offset: u64,
        doc: &CoreDocument,
    ) -> DocumentPosition {
        if self.fragments.is_empty() {
            return DocumentPosition::default();
        }

        let mut nearest_after: Option<(usize, usize)> = None;
        let mut nearest_after_offset = u64::MAX;

        for (f_idx, frag) in self.fragments.iter().enumerate() {
            let mut char_pos = 0;
            for run in &frag.runs {
                let run_start = char_pos;
                let run_len = run.text.chars().count();
                char_pos += run_len;

                let Some(node_id) = run.node_id else { continue };
                let Some(node) = doc.nodes.get(node_id as usize) else {
                    continue;
                };

                if (source_offset as u32) >= node.span_start
                    && (source_offset as u32) < node.span_end
                {
                    if run.atomic {
                        return DocumentPosition::new(f_idx, run_start);
                    }
                    let byte_in_node = ((source_offset as u32) - node.span_start) as usize;
                    let node_text = doc.text(node.text).unwrap_or("");
                    let char_in_node = byte_offset_to_char_offset(node_text, byte_in_node);
                    if char_in_node >= run.node_char_offset
                        && char_in_node <= run.node_char_offset + run_len
                    {
                        let local_char = char_in_node - run.node_char_offset;
                        return DocumentPosition::new(f_idx, run_start + local_char);
                    }
                } else if (node.span_start as u64) >= source_offset
                    && (node.span_start as u64) < nearest_after_offset
                {
                    nearest_after_offset = node.span_start as u64;
                    nearest_after = Some((f_idx, run_start));
                }
            }
        }

        if let Some((f_idx, c_offset)) = nearest_after {
            return DocumentPosition::new(f_idx, c_offset);
        }

        // Fallback to end
        let last_f = self.fragments.len().saturating_sub(1);
        let last_c = self.fragments[last_f].text().chars().count();
        DocumentPosition::new(last_f, last_c)
    }
}

fn char_offset_to_utf8_byte_offset(bytes: &[u8], char_offset: usize) -> usize {
    if let Ok(s) = std::str::from_utf8(bytes) {
        s.char_indices()
            .nth(char_offset)
            .map(|(b, _)| b)
            .unwrap_or(bytes.len())
    } else {
        char_offset.min(bytes.len())
    }
}
