//! Bounded owned presentation values. No GTK, file access or parser internals.
use crate::document::{ImageBudget, ResourceError};
use markdown_linux_core::{CoreDocument, ffi::*};
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
}

impl Fragment {
    pub fn text(&self) -> String {
        self.runs.iter().map(|r| r.text.as_str()).collect()
    }

    pub fn has_link(&self, index: u32) -> bool {
        self.runs.iter().any(|r| r.link == Some(index))
    }

    pub fn markup(&self, epoch: u64, index: u32) -> String {
        let mut out = String::new();
        for run in &self.runs {
            if let Some(link) = run.link {
                out.push_str(&format!("<a href=\"md:{epoch}:{index}:{link}\">"));
            }
            let tags = [(1, "b"), (2, "i"), (4, "s"), (8, "tt")];
            for (bit, tag) in tags {
                if run.style & bit != 0 {
                    out.push_str(&format!("<{tag}>"));
                }
            }
            if run.style & 32 != 0 {
                // TaskChecked: accent bold
                out.push_str("<span foreground=\"#3584e4\"><b>");
            } else if run.style & 64 != 0 {
                // TaskUnchecked: muted
                out.push_str("<span foreground=\"gray\">");
            }
            for ch in run.text.chars() {
                match ch {
                    '&' => out.push_str("&amp;"),
                    '<' => out.push_str("&lt;"),
                    '>' => out.push_str("&gt;"),
                    '"' => out.push_str("&quot;"),
                    '\'' => out.push_str("&apos;"),
                    _ => out.push(ch),
                }
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

#[derive(Debug)]
pub struct Outline {
    pub title: String,
    pub anchor: String,
    pub level: u32,
    pub fragment: u32,
}

#[derive(Debug, Default)]
pub struct RenderPlan {
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
            plan: Self::default(),
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
        runs.push(Run { text, style, link });
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
                            self.run(runs, display_text, style, None)?;
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
                                    self.run(runs, display_text, style, None)?;
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
        self.run(runs, display_text, style, None)?;
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
            MD_NODE_SOFT_BREAK => self.run(runs, " ".into(), style, link)?,
            MD_NODE_HARD_BREAK => self.run(runs, "\n".into(), style, link)?,
            MD_NODE_TEXT | MD_NODE_CODE_SPAN | MD_NODE_FOOTNOTE_REFERENCE => {
                let text = self.text(n.text)?;
                self.run(runs, text, style, link)?;
            }
            MD_NODE_IMAGE => {
                self.image(id, depth, style, runs)?;
            }
            MD_NODE_UNSUPPORTED_INLINE => {
                let text = self.fallback(n)?;
                self.run(runs, text, style, link)?;
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
                    self.run(&mut row_runs, "\t".into(), 0, None)?;
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
            self.run(&mut runs, m, 16 | s, None)?;
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
                self.run(&mut runs, marker_str, 16 | marker_style, None)?;
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
                )?;
            } else if n.kind == MD_NODE_THEMATIC_BREAK {
                self.run(&mut runs, "—".into(), 0, None)?;
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
        let mut fragment = Fragment {
            runs: Vec::new(),
            heading,
            heading_start: heading > 0,
            code,
            is_table_row: false,
            is_table_header: false,
            table_row_ordinal: 0,
            images: std::mem::take(&mut self.current_block_images),
        };
        let mut used = 0;
        for run in runs {
            let mut rest = run.text.as_str();
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
                    };
                    used = 0;
                    continue;
                }
                self.charge(size_of::<Run>() + end)?;
                fragment.runs.push(Run {
                    text: rest[..end].into(),
                    style: run.style,
                    link: run.link,
                });
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
