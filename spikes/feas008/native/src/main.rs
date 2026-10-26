//! FEAS-008 Linux virtualized semantic-block renderer.
//!
//! A `GtkListView` presents display fragments of the semantic block model.
//! Only visible rows exist as widgets. Selection, copy, find, outline jumps,
//! link activation and image loading operate on the model, never on widgets,
//! so they reach rows that are not realized.

mod model;

use feas008_docmodel::{
    BlockKind, Document, ImageRequest, Item, LinkIntent, Match, Pos, STYLE_BOLD, STYLE_CODE,
    STYLE_ITALIC, STYLE_MARKER, STYLE_STRIKE,
};
use gtk::prelude::*;
use gtk::{gdk, gio, glib, graphene, pango};
use serde_json::{Value, json};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::future::Future;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll, Waker};
use std::time::{Duration, Instant};

const MAX_IMAGE_BYTES: u64 = 20 * 1024 * 1024;
const MAX_IMAGE_SIDE: u32 = 16_384;
const CSS: &str = "
.mdv-row { padding: 3px 18px; }
.mdv-row.mdv-cont { padding-top: 0; }
.mdv-row.mdv-cont-next { padding-bottom: 0; }
.mdv-cont > label { margin-top: 0; }
.mdv-h1 > label { font-size: 200%; font-weight: 700; margin-top: 12px; }
.mdv-h2 > label { font-size: 160%; font-weight: 700; margin-top: 10px; }
.mdv-h3 > label { font-size: 130%; font-weight: 700; margin-top: 8px; }
.mdv-h4 > label, .mdv-h5 > label, .mdv-h6 > label { font-weight: 700; }
.mdv-code > label, .mdv-html > label, .mdv-math > label, .mdv-table > label { font-family: monospace; }
.mdv-code, .mdv-html, .mdv-math { background: alpha(currentColor, 0.06); }
.mdv-quote { border-left: 3px solid alpha(currentColor, 0.3); }
.mdv-alt { font-style: italic; opacity: 0.75; }
.mdv-selected { background: alpha(#3584e4, 0.25); }
listview.mdv-doc > row:focus-within { background: alpha(#3584e4, 0.12); box-shadow: inset 3px 0 #3584e4; }
";

#[derive(Default)]
struct Config {
    path: PathBuf,
    automated: bool,
    control: bool,
    expect_out: Option<PathBuf>,
}

#[derive(Clone)]
struct Row {
    root: gtk::Box,
    heading: gtk::Label,
    text: gtk::Label,
    picture: gtk::Picture,
    item: Rc<Cell<Option<u32>>>,
    lit: Rc<Cell<Option<(u32, u32)>>>,
}

impl Row {
    fn new() -> Row {
        let label = |role| {
            gtk::Label::builder()
                .accessible_role(role)
                .wrap(true)
                .wrap_mode(pango::WrapMode::WordChar)
                // A fixed minimum width spares GTK measuring the minimum of a
                // wrapped label, which dominated row cost (see README).
                .width_chars(40)
                .max_width_chars(40)
                .xalign(0.0)
                .use_markup(true)
                .build()
        };
        let root = gtk::Box::new(gtk::Orientation::Vertical, 2);
        let heading = label(gtk::AccessibleRole::Heading);
        let text = label(gtk::AccessibleRole::Paragraph);
        let picture = gtk::Picture::builder()
            .content_fit(gtk::ContentFit::ScaleDown)
            .halign(gtk::Align::Start)
            .can_shrink(true)
            .build();
        root.append(&heading);
        root.append(&picture);
        root.append(&text);
        Row {
            root,
            heading,
            text,
            picture,
            item: Rc::new(Cell::new(None)),
            lit: Rc::new(Cell::new(None)),
        }
    }

    fn label(&self) -> &gtk::Label {
        if self.heading.is_visible() {
            &self.heading
        } else {
            &self.text
        }
    }
}

#[derive(Default)]
struct Drag {
    start: (f64, f64),
    last: (f64, f64),
    active: bool,
    extend: bool,
    anchor: Option<Pos>,
    timer: Option<glib::SourceId>,
}

enum Texture {
    Pending,
    Ready(gdk::Texture),
    Failed(String),
}

#[derive(Clone, Copy, Debug)]
struct Presented {
    paint_us: i64,
    presentation_us: i64,
}

impl Presented {
    fn best_us(&self) -> i64 {
        if self.presentation_us > 0 {
            self.presentation_us
        } else {
            self.paint_us
        }
    }
}

type Cond = Box<dyn Fn(&App) -> bool>;

struct App {
    doc: Document,
    base_dir: PathBuf,
    slugs: HashMap<String, u32>,
    window: gtk::ApplicationWindow,
    list: gtk::ListView,
    scrolled: gtk::ScrolledWindow,
    search: gtk::SearchEntry,
    status: gtk::Label,
    rows: RefCell<Vec<Row>>,
    selection: Cell<Option<(Pos, Pos)>>,
    drag: RefCell<Drag>,
    textures: RefCell<HashMap<String, Texture>>,
    matches: RefCell<Vec<Match>>,
    match_index: Cell<usize>,
    waiter: RefCell<Option<(Cond, OneShot<Presented>)>>,
    pending: RefCell<Vec<(i64, i64, OneShot<Presented>)>>,
    decisions: RefCell<Vec<Value>>,
    automated: bool,
    binds: Cell<u64>,
    align: Cell<Option<(u32, u8)>>,
}

/// Minimal single-value future, avoiding an async runtime dependency.
struct OneShot<T>(Rc<RefCell<(Option<T>, Option<Waker>)>>);

impl<T> Clone for OneShot<T> {
    fn clone(&self) -> Self {
        OneShot(self.0.clone())
    }
}

impl<T> OneShot<T> {
    fn new() -> Self {
        OneShot(Rc::new(RefCell::new((None, None))))
    }
    fn send(&self, value: T) {
        let mut slot = self.0.borrow_mut();
        slot.0 = Some(value);
        if let Some(waker) = slot.1.take() {
            waker.wake();
        }
    }
}

impl<T> Future for OneShot<T> {
    type Output = T;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<T> {
        let mut slot = self.0.borrow_mut();
        match slot.0.take() {
            Some(value) => Poll::Ready(value),
            None => {
                slot.1 = Some(cx.waker().clone());
                Poll::Pending
            }
        }
    }
}

fn ordered(a: Pos, b: Pos) -> (Pos, Pos) {
    if a <= b { (a, b) } else { (b, a) }
}

fn ms(from_us: i64, to_us: i64) -> f64 {
    (to_us - from_us) as f64 / 1000.0
}

fn style_tags(style: u8) -> (String, String) {
    let mut open = String::new();
    let mut close = String::new();
    for (bit, tag) in [(STYLE_BOLD, "b"), (STYLE_ITALIC, "i"), (STYLE_STRIKE, "s")] {
        if style & bit != 0 {
            open.push_str(&format!("<{tag}>"));
            close.insert_str(0, &format!("</{tag}>"));
        }
    }
    if style & STYLE_CODE != 0 {
        open.push_str("<span font_family=\"monospace\">");
        close.insert_str(0, "</span>");
    }
    if style & STYLE_MARKER != 0 {
        open.push_str("<span alpha=\"70%\">");
        close.insert_str(0, "</span>");
    }
    (open, close)
}

impl App {
    /// Pango markup for one fragment; `selected` is an item-local byte range
    /// drawn as the selection. Highlighting lives in the markup because
    /// GtkLabel merges set_attributes() into markup attributes and keeps them.
    fn markup(&self, item: Item, selected: Option<(u32, u32)>) -> String {
        let block = &self.doc.blocks[item.block as usize];
        let mut out = String::with_capacity((item.end - item.start) as usize + 64);
        let (sel_lo, sel_hi) = selected.map_or((u32::MAX, u32::MAX), |(lo, hi)| {
            (item.start + lo, item.start + hi)
        });
        for run in &block.runs {
            let start = run.range.start.max(item.start);
            let end = run.range.end.min(item.end);
            if start >= end {
                continue;
            }
            let mut text = String::new();
            let mut cuts = vec![start, end];
            for cut in [sel_lo, sel_hi] {
                if cut > start && cut < end {
                    cuts.push(cut);
                }
            }
            cuts.sort_unstable();
            for pair in cuts.windows(2) {
                let piece =
                    glib::markup_escape_text(&block.text[pair[0] as usize..pair[1] as usize]);
                if pair[0] >= sel_lo && pair[1] <= sel_hi {
                    text.push_str("<span background=\"#3584e4\" foreground=\"#ffffff\">");
                    text.push_str(&piece);
                    text.push_str("</span>");
                } else {
                    text.push_str(&piece);
                }
            }
            let mut close = String::new();
            if let Some(link) = run.link {
                let dest = &self.doc.links[link as usize];
                match feas008_docmodel::classify_link(dest, &self.slugs) {
                    LinkIntent::Blocked(_) | LinkIntent::MissingFragment(_) => {
                        out.push_str("<span underline=\"error\">");
                        close.push_str("</span>");
                    }
                    _ => {
                        out.push_str(&format!("<a href=\"{}\">", glib::markup_escape_text(dest)));
                        close.push_str("</a>");
                    }
                }
            }
            let (open, style_close) = style_tags(run.style);
            out.push_str(&open);
            out.push_str(&text);
            out.push_str(&style_close);
            out.push_str(&close);
        }
        out
    }

    fn bind(self: &Rc<Self>, row: &Row, index: u32, list_item: &gtk::ListItem) {
        self.binds.set(self.binds.get() + 1);
        row.item.set(Some(index));
        let item = self.doc.items[index as usize];
        let block = &self.doc.blocks[item.block as usize];
        let class = match block.kind {
            BlockKind::Heading(1) => "mdv-h1",
            BlockKind::Heading(2) => "mdv-h2",
            BlockKind::Heading(3) => "mdv-h3",
            BlockKind::Heading(4) => "mdv-h4",
            BlockKind::Heading(5) => "mdv-h5",
            BlockKind::Heading(_) => "mdv-h6",
            BlockKind::Code => "mdv-code",
            BlockKind::Html => "mdv-html",
            BlockKind::Math => "mdv-math",
            BlockKind::TableRow => "mdv-table",
            BlockKind::Image => "mdv-image",
            _ => "mdv-text",
        };
        let fragment = &block.text[item.start as usize..item.end as usize];
        let wrap = if fragment.contains([' ', '\n']) {
            pango::WrapMode::WordChar
        } else {
            pango::WrapMode::Char
        };
        row.heading.set_wrap_mode(wrap);
        row.text.set_wrap_mode(wrap);
        let mut classes = vec!["mdv-row", class];
        if item.start > 0 {
            classes.push("mdv-cont");
        }
        if (item.end as usize) < block.text.len() {
            classes.push("mdv-cont-next");
        }
        if block.quote {
            classes.push("mdv-quote");
        }
        row.root.set_css_classes(&classes);
        row.root.set_margin_start(block.indent as i32 * 22);
        let selected = self.selected_range(item);
        let markup = self.markup(item, selected);
        let (role, label) = match block.kind {
            BlockKind::Heading(level) => {
                row.heading.set_markup(&markup);
                row.heading
                    .update_property(&[gtk::accessible::Property::Level(level as i32)]);
                (format!("heading level {level}"), &row.heading)
            }
            BlockKind::Image => {
                row.text.set_markup(&markup);
                ("image".to_string(), &row.text)
            }
            BlockKind::Code => {
                row.text.set_markup(&markup);
                ("code".to_string(), &row.text)
            }
            BlockKind::ListItem => {
                row.text.set_markup(&markup);
                ("list item".to_string(), &row.text)
            }
            _ => {
                row.text.set_markup(&markup);
                (
                    if block.quote { "quote" } else { "paragraph" }.to_string(),
                    &row.text,
                )
            }
        };
        row.heading.set_visible(std::ptr::eq(label, &row.heading));
        row.text.set_visible(!std::ptr::eq(label, &row.heading));
        if block.kind == BlockKind::Image {
            row.text.add_css_class("mdv-alt");
        } else {
            row.text.remove_css_class("mdv-alt");
        }
        list_item.set_accessible_label(&block.text[item.start as usize..item.end as usize]);
        list_item.set_accessible_description(&role);
        self.bind_image(row);
        row.lit.set(selected);
        if selected.is_some() && block.kind == BlockKind::Image {
            row.root.add_css_class("mdv-selected");
        } else {
            row.root.remove_css_class("mdv-selected");
        }
    }

    fn bind_image(self: &Rc<Self>, row: &Row) {
        let Some(index) = row.item.get() else { return };
        let item = self.doc.items[index as usize];
        let block = &self.doc.blocks[item.block as usize];
        let request = match (&block.kind, &block.image) {
            (BlockKind::Image, Some(request)) => request,
            _ => {
                row.picture.set_paintable(None::<&gdk::Paintable>);
                row.picture.set_visible(false);
                return;
            }
        };
        row.picture.set_alternative_text(Some(&block.text));
        row.picture
            .update_property(&[gtk::accessible::Property::Label(&block.text)]);
        match request {
            ImageRequest::Blocked(reason) => {
                row.picture.set_visible(false);
                row.text
                    .set_tooltip_text(Some(&format!("Image blocked: {reason}")));
            }
            ImageRequest::Local(rel) => {
                let state = self.textures.borrow().get(rel).map(|t| match t {
                    Texture::Ready(tex) => Ok(tex.clone()),
                    Texture::Failed(reason) => Err(Some(reason.clone())),
                    Texture::Pending => Err(None),
                });
                match state {
                    Some(Ok(tex)) => {
                        row.picture.set_paintable(Some(&tex));
                        row.picture.set_size_request(-1, tex.height().min(360));
                        row.picture.set_visible(true);
                    }
                    Some(Err(reason)) => {
                        row.picture.set_visible(false);
                        if let Some(reason) = reason {
                            row.text
                                .set_tooltip_text(Some(&format!("Image not shown: {reason}")));
                        }
                    }
                    None => {
                        row.picture.set_visible(false);
                        self.load_texture(rel.clone());
                    }
                }
            }
        }
    }

    fn load_texture(self: &Rc<Self>, rel: String) {
        self.textures
            .borrow_mut()
            .insert(rel.clone(), Texture::Pending);
        let base = self.base_dir.clone();
        let app = self.clone();
        glib::spawn_future_local(async move {
            let key = rel.clone();
            let result = gio::spawn_blocking(move || load_image(&base, &rel))
                .await
                .unwrap_or_else(|_| Err("decoder panicked".into()));
            let state = match result {
                Ok(tex) => Texture::Ready(tex),
                Err(reason) => Texture::Failed(reason),
            };
            app.textures.borrow_mut().insert(key, state);
            let rows = app.rows.borrow().clone();
            for row in rows {
                app.bind_image(&row);
            }
        });
    }

    fn selected_range(&self, item: Item) -> Option<(u32, u32)> {
        let (a, b) = self.selection.get()?;
        let (start, end) = ordered(a, b);
        if !(start.block..=end.block).contains(&item.block) {
            return None;
        }
        let from = if item.block == start.block {
            start.offset
        } else {
            0
        };
        let to = if item.block == end.block {
            end.offset
        } else {
            u32::MAX
        };
        let lo = from.max(item.start);
        let hi = to.min(item.end);
        (lo < hi).then(|| (lo - item.start, hi - item.start))
    }

    fn highlight(&self, row: &Row) {
        let Some(index) = row.item.get() else { return };
        let item = self.doc.items[index as usize];
        let range = self.selected_range(item);
        if row.lit.get() == range {
            return;
        }
        row.label().set_markup(&self.markup(item, range));
        row.lit.set(range);
        if range.is_some() && self.doc.blocks[item.block as usize].kind == BlockKind::Image {
            row.root.add_css_class("mdv-selected");
        } else {
            row.root.remove_css_class("mdv-selected");
        }
    }

    fn set_selection(&self, selection: Option<(Pos, Pos)>) {
        trace(&format!("set-selection {selection:?}"));
        self.selection.set(selection.filter(|(a, b)| a != b));
        for row in self.rows.borrow().iter() {
            self.highlight(row);
        }
        let text = match self.selection.get() {
            Some((a, b)) => {
                let (s, e) = ordered(a, b);
                format!("Selected blocks {}–{}", s.block + 1, e.block + 1)
            }
            None => String::new(),
        };
        self.status.set_text(&text);
    }

    fn select_all(&self) {
        self.set_selection(Some((self.doc.start(), self.doc.end())));
    }

    /// Copy the selection as rendered text plus `text/markdown` source.
    fn copy(&self) -> Option<(String, usize)> {
        let (a, b) = self.selection.get()?;
        let plain = self.doc.text_between(a, b);
        let source = self.doc.source_between(a, b);
        let provider = gdk::ContentProvider::new_union(&[
            gdk::ContentProvider::for_value(&plain.to_value()),
            gdk::ContentProvider::for_bytes("text/markdown", &glib::Bytes::from(source.as_bytes())),
        ]);
        self.window.clipboard().set_content(Some(&provider)).ok()?;
        let len = source.len();
        Some((plain, len))
    }

    fn row_for_widget(&self, widget: &gtk::Widget) -> Option<Row> {
        let rows = self.rows.borrow();
        let mut current = Some(widget.clone());
        while let Some(w) = current {
            if let Some(row) = rows.iter().find(|r| {
                r.root.upcast_ref::<gtk::Widget>() == &w || r.root.parent().as_ref() == Some(&w)
            }) {
                return Some(row.clone());
            }
            if &w == self.list.upcast_ref::<gtk::Widget>() {
                return None;
            }
            current = w.parent();
        }
        None
    }

    /// Map list coordinates to a model position through Pango hit testing.
    fn hit(&self, x: f64, y: f64) -> Option<Pos> {
        let height = self.list.height() as f64;
        let y = y.clamp(1.0, (height - 1.0).max(1.0));
        for dy in [0.0, -4.0, 4.0, -10.0, 10.0] {
            let Some(widget) = self.list.pick(x, y + dy, gtk::PickFlags::DEFAULT) else {
                continue;
            };
            let Some(row) = self.row_for_widget(&widget) else {
                continue;
            };
            let Some(index) = row.item.get() else {
                continue;
            };
            let item = self.doc.items[index as usize];
            let label = row.label();
            let Some(point) = self
                .list
                .compute_point(label, &graphene::Point::new(x as f32, (y + dy) as f32))
            else {
                continue;
            };
            let (ox, oy) = label.layout_offsets();
            let layout = label.layout();
            let lx = ((point.x() - ox as f32) * pango::SCALE as f32) as i32;
            let ly = ((point.y() - oy as f32) * pango::SCALE as f32) as i32;
            let text = label.text();
            let (_, byte, trailing) = layout.xy_to_index(lx, ly);
            let mut byte = byte.max(0) as usize;
            for _ in 0..trailing {
                byte += text[byte..].chars().next().map_or(0, char::len_utf8);
            }
            let byte = byte.min(text.len()) as u32;
            return Some(Pos {
                block: item.block,
                offset: item.start + byte,
            });
        }
        None
    }

    fn link_at(&self, pos: Pos) -> Option<String> {
        let block = &self.doc.blocks[pos.block as usize];
        let run = block
            .runs
            .iter()
            .find(|r| r.range.start <= pos.offset && pos.offset < r.range.end)?;
        run.link.map(|l| self.doc.links[l as usize].clone())
    }

    fn drag_to_last(&self) {
        let (x, y, anchor) = {
            let d = self.drag.borrow();
            (d.last.0, d.last.1, d.anchor)
        };
        if let (Some(anchor), Some(focus)) = (anchor, self.hit(x, y)) {
            self.set_selection(Some((anchor, focus)));
        }
    }

    fn scroll_to_item(&self, item: u32, focus: bool) {
        let flags = if focus {
            gtk::ListScrollFlags::FOCUS
        } else {
            gtk::ListScrollFlags::NONE
        };
        self.list.scroll_to(item, flags, None);
    }

    /// Jump to a block and align its row with the top of the viewport.
    /// `scroll_to` only scrolls minimally, so after-paint corrects the offset.
    fn jump_to_block(&self, block: u32, focus: bool) -> u32 {
        let item = self.doc.first_item[block as usize];
        self.scroll_to_item(item, focus);
        self.align.set(Some((item, 0)));
        self.list.queue_draw();
        item
    }

    fn align_step(&self) {
        let Some((item, tries)) = self.align.get() else {
            return;
        };
        let adj = self.scrolled.vadjustment();
        let offset = self.visible_row(item).and_then(|row| {
            let widget = row
                .root
                .parent()
                .unwrap_or_else(|| row.root.clone().upcast());
            widget.compute_bounds(&self.list).map(|b| b.y() as f64)
        });
        let at_end = adj.value() >= adj.upper() - adj.page_size() - 0.5;
        match offset {
            Some(y) if y.abs() < 1.0 || (y > 0.0 && at_end) || tries >= 30 => self.align.set(None),
            Some(y) => {
                adj.set_value(adj.value() + y);
                self.align.set(Some((item, tries + 1)));
            }
            None if tries >= 30 => self.align.set(None),
            None => self.align.set(Some((item, tries + 1))),
        }
        self.list.queue_draw();
    }

    fn extend(&self, to: Pos) {
        let anchor = self
            .selection
            .get()
            .map(|(a, _)| a)
            .unwrap_or_else(|| self.focus_pos());
        self.set_selection(Some((anchor, to)));
        self.scroll_to_item(self.doc.item_for(to), false);
    }

    /// First visible row's item, used when no row holds keyboard focus.
    fn focus_pos_item(&self) -> u32 {
        self.doc.item_for(self.focus_pos())
    }

    fn focused_item(&self) -> Option<u32> {
        let focus = gtk::prelude::GtkWindowExt::focus(&self.window)?;
        self.row_for_widget(&focus)?.item.get()
    }

    fn focus_pos(&self) -> Pos {
        let rows = self.rows.borrow();
        let focused = rows
            .iter()
            .find(|r| r.root.parent().is_some_and(|p| p.has_focus()))
            .or_else(|| {
                rows.iter()
                    .filter(|r| r.item.get().is_some())
                    .min_by_key(|r| r.item.get())
            });
        match focused.and_then(|r| r.item.get()) {
            Some(index) => {
                let item = self.doc.items[index as usize];
                Pos {
                    block: item.block,
                    offset: item.start,
                }
            }
            None => self.doc.start(),
        }
    }

    fn step(&self, pos: Pos, chars: i32) -> Pos {
        let text = &self.doc.blocks[pos.block as usize].text;
        let mut offset = pos.offset as usize;
        if chars > 0 {
            if offset >= text.len() && (pos.block as usize) + 1 < self.doc.blocks.len() {
                return Pos {
                    block: pos.block + 1,
                    offset: 0,
                };
            }
            offset += text[offset..].chars().next().map_or(0, char::len_utf8);
        } else {
            if offset == 0 && pos.block > 0 {
                let prev = pos.block - 1;
                return Pos {
                    block: prev,
                    offset: self.doc.blocks[prev as usize].text.len() as u32,
                };
            }
            offset -= text[..offset].chars().next_back().map_or(0, char::len_utf8);
        }
        Pos {
            block: pos.block,
            offset: offset as u32,
        }
    }

    fn step_item(&self, pos: Pos, down: bool) -> Pos {
        let index = self.doc.item_for(pos) as usize;
        let target = if down {
            (index + 1).min(self.doc.items.len() - 1)
        } else {
            index.saturating_sub(1)
        };
        let item = self.doc.items[target];
        Pos {
            block: item.block,
            offset: if down { item.end } else { item.start },
        }
    }

    fn find(&self, query: &str) -> (usize, f64) {
        let started = Instant::now();
        let matches = self.doc.find(query);
        let elapsed = started.elapsed().as_secs_f64() * 1000.0;
        let count = matches.len();
        *self.matches.borrow_mut() = matches;
        self.match_index.set(usize::MAX);
        self.status.set_text(&format!("{count} matches"));
        (count, elapsed)
    }

    fn next_match(&self, forward: bool) -> Option<Match> {
        let matches = self.matches.borrow();
        if matches.is_empty() {
            return None;
        }
        let len = matches.len();
        let current = self.match_index.get();
        let next = match (current == usize::MAX, forward) {
            (true, _) => 0,
            (false, true) => (current + 1) % len,
            (false, false) => (current + len - 1) % len,
        };
        self.match_index.set(next);
        let m = matches[next];
        drop(matches);
        self.show_match(m);
        self.status
            .set_text(&format!("Match {} of {len}", next + 1));
        Some(m)
    }

    fn show_match(&self, m: Match) {
        let a = Pos {
            block: m.block,
            offset: m.start,
        };
        let b = Pos {
            block: m.block,
            offset: m.end,
        };
        self.set_selection(Some((a, b)));
        self.scroll_to_item(self.doc.item_for(a), false);
    }

    fn activate_link(&self, dest: &str) -> Value {
        let intent = feas008_docmodel::classify_link(dest, &self.slugs);
        let action = match &intent {
            LinkIntent::Fragment(slug) => {
                let block = self.slugs[slug];
                self.jump_to_block(block, true);
                format!("jump to block {block}")
            }
            LinkIntent::External(url) => {
                if !self.automated {
                    gtk::UriLauncher::new(url).launch(
                        Some(&self.window),
                        gio::Cancellable::NONE,
                        |_| {},
                    );
                }
                "open with system handler".to_string()
            }
            LinkIntent::LocalDocument(path) => format!(
                "open document {path} after folder-capability check (not implemented in spike)"
            ),
            LinkIntent::MissingFragment(slug) => format!("ignored: no heading #{slug}"),
            LinkIntent::Blocked(reason) => format!("blocked: {reason}"),
        };
        let decision = json!({"dest": dest, "intent": format!("{intent:?}"), "action": action});
        eprintln!("link: {decision}");
        self.decisions.borrow_mut().push(decision.clone());
        decision
    }

    fn visible_row(&self, item: u32) -> Option<Row> {
        let height = self.list.height() as f32;
        self.rows
            .borrow()
            .iter()
            .find(|r| {
                r.item.get() == Some(item)
                    && r.root.is_mapped()
                    && r.root.compute_bounds(&self.list).is_some_and(|b| {
                        b.height() > 0.0 && b.y() + b.height() > 0.0 && b.y() < height
                    })
            })
            .cloned()
    }

    /// Resolve after the first painted frame for which `cond` holds, with
    /// compositor presentation time when the frame clock reports it.
    fn presented(&self, cond: impl Fn(&App) -> bool + 'static) -> OneShot<Presented> {
        let shot = OneShot::new();
        *self.waiter.borrow_mut() = Some((Box::new(cond), shot.clone()));
        self.list.queue_draw();
        shot
    }

    fn after_paint(&self, clock: &gdk::FrameClock) {
        self.align_step();
        let counter = clock.frame_counter();
        let now = glib::monotonic_time();
        let waiter = self.waiter.borrow_mut().take();
        if let Some((cond, shot)) = waiter {
            if cond(self) {
                self.pending.borrow_mut().push((counter, now, shot));
            } else {
                *self.waiter.borrow_mut() = Some((cond, shot));
            }
        }
        let mut pending = self.pending.borrow_mut();
        pending.retain(|(frame, paint_us, shot)| {
            let timings = clock.timings(*frame);
            let complete = timings.as_ref().is_some_and(|t| t.is_complete());
            if complete || counter - frame > 30 {
                let presentation_us = timings.map_or(0, |t| t.presentation_time());
                shot.send(Presented {
                    paint_us: *paint_us,
                    presentation_us,
                });
                false
            } else {
                true
            }
        });
        if !pending.is_empty() || self.waiter.borrow().is_some() {
            self.list.queue_draw();
        }
    }
}

fn load_image(base: &Path, rel: &str) -> Result<gdk::Texture, String> {
    let base = base
        .canonicalize()
        .map_err(|e| format!("document folder unavailable: {e}"))?;
    let path = base
        .join(rel)
        .canonicalize()
        .map_err(|e| format!("not found: {e}"))?;
    if !path.starts_with(&base) {
        return Err("resolves outside the document folder".into());
    }
    let meta = std::fs::metadata(&path).map_err(|e| e.to_string())?;
    if !meta.is_file() || meta.len() > MAX_IMAGE_BYTES {
        return Err("not a regular file within the size limit".into());
    }
    let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
    let png = bytes.starts_with(b"\x89PNG\r\n\x1a\n");
    let known = png
        || bytes.starts_with(&[0xff, 0xd8, 0xff])
        || bytes.starts_with(b"GIF8")
        || (bytes.len() > 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP");
    if !known {
        return Err("content is not an allowed raster format".into());
    }
    if png && bytes.len() >= 24 {
        let w = u32::from_be_bytes(bytes[16..20].try_into().unwrap());
        let h = u32::from_be_bytes(bytes[20..24].try_into().unwrap());
        if w > MAX_IMAGE_SIDE || h > MAX_IMAGE_SIDE {
            return Err(format!("{w}x{h} exceeds the dimension limit"));
        }
    }
    let texture =
        gdk::Texture::from_bytes(&glib::Bytes::from_owned(bytes)).map_err(|e| e.to_string())?;
    if texture.width() as u32 > MAX_IMAGE_SIDE || texture.height() as u32 > MAX_IMAGE_SIDE {
        return Err("decoded image exceeds the dimension limit".into());
    }
    Ok(texture)
}

fn trace(message: &str) {
    if std::env::var_os("FEAS008_TRACE").is_some() {
        eprintln!("trace: {message}");
    }
}

fn peak_rss_kib() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    status
        .lines()
        .find(|l| l.starts_with("VmHWM:"))?
        .split_whitespace()
        .nth(1)?
        .parse()
        .ok()
}

fn parse_args() -> Config {
    let mut config = Config::default();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--automated" => config.automated = true,
            "--control" => config.control = true,
            "--expect-out" => config.expect_out = args.next().map(PathBuf::from),
            other => config.path = PathBuf::from(other),
        }
    }
    if config.path.as_os_str().is_empty() {
        eprintln!("usage: feas008-native FILE.md [--automated] [--control] [--expect-out FILE]");
        std::process::exit(2);
    }
    config
}

fn event(value: Value) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "EVENT {value}");
    let _ = out.flush();
}

struct Startup {
    main_us: i64,
    read_ms: f64,
    parse_ms: f64,
}

fn main() -> glib::ExitCode {
    let main_us = glib::monotonic_time();
    let launch_us = std::env::var("FEAS008_LAUNCH_NS")
        .ok()
        .and_then(|v| v.parse::<i64>().ok())
        .map(|ns| ns / 1000);
    let config = parse_args();
    let path = config.path.clone();
    let parse = std::thread::spawn(move || {
        let started = Instant::now();
        let source = std::fs::read_to_string(&path).expect("read document");
        let read_ms = started.elapsed().as_secs_f64() * 1000.0;
        let parsed = Instant::now();
        let fragment = std::env::var("FEAS008_FRAGMENT_BYTES")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(feas008_docmodel::FRAGMENT_BYTES);
        let doc = Document::parse_with(source, fragment);
        (doc, read_ms, parsed.elapsed().as_secs_f64() * 1000.0)
    });
    let parse = RefCell::new(Some(parse));
    let config = Rc::new(config);
    let application = gtk::Application::builder()
        .application_id("dev.markdown.Feas008Native")
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    application.connect_activate(move |application| {
        let (doc, read_ms, parse_ms) = parse
            .borrow_mut()
            .take()
            .expect("activate once")
            .join()
            .expect("parse thread");
        let startup = Startup {
            main_us,
            read_ms,
            parse_ms,
        };
        build(application, doc, &config, startup, launch_us);
    });
    application.run_with_args::<&str>(&[])
}

fn build(
    application: &gtk::Application,
    doc: Document,
    config: &Rc<Config>,
    startup: Startup,
    launch_us: Option<i64>,
) {
    let provider = gtk::CssProvider::new();
    provider.load_from_string(CSS);
    gtk::style_context_add_provider_for_display(
        &gdk::Display::default().unwrap(),
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    let title = config
        .path
        .file_name()
        .map_or("document".into(), |n| n.to_string_lossy().to_string());
    let window = gtk::ApplicationWindow::builder()
        .application(application)
        .title(format!("FEAS-008 native — {title}"))
        .default_width(1200)
        .default_height(900)
        .build();
    let header = gtk::HeaderBar::new();
    let search = gtk::SearchEntry::builder()
        .placeholder_text("Find")
        .width_chars(28)
        .build();
    let status = gtk::Label::new(None);
    header.pack_start(&search);
    header.pack_end(&status);
    window.set_titlebar(Some(&header));

    let items_model = model::CountModel::new(doc.items.len() as u32);
    let factory = gtk::SignalListItemFactory::new();
    let list = gtk::ListView::new(
        Some(gtk::NoSelection::new(Some(items_model))),
        None::<gtk::ListItemFactory>,
    );
    list.update_property(&[gtk::accessible::Property::Label(&format!(
        "Document {title}"
    ))]);
    list.add_css_class("mdv-doc");
    list.set_widget_name("document");
    let scrolled = gtk::ScrolledWindow::builder()
        .child(&list)
        .hexpand(true)
        .vexpand(true)
        .build();

    let outline_model = model::CountModel::new(doc.outline.len() as u32);
    let outline_factory = gtk::SignalListItemFactory::new();
    let outline_selection = gtk::SingleSelection::new(Some(outline_model));
    outline_selection.set_autoselect(false);
    outline_selection.set_can_unselect(true);
    let outline = gtk::ListView::new(
        Some(outline_selection.clone()),
        None::<gtk::ListItemFactory>,
    );
    outline.set_widget_name("outline");
    outline.update_property(&[gtk::accessible::Property::Label("Outline")]);
    let outline_scrolled = gtk::ScrolledWindow::builder()
        .child(&outline)
        .width_request(260)
        .build();
    let paned = gtk::Paned::builder()
        .orientation(gtk::Orientation::Horizontal)
        .start_child(&outline_scrolled)
        .end_child(&scrolled)
        .position(260)
        .build();
    window.set_child(Some(&paned));

    let base_dir = config
        .path
        .canonicalize()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."));
    let slugs = doc.slugs();
    let app = Rc::new(App {
        doc,
        base_dir,
        slugs,
        window: window.clone(),
        list: list.clone(),
        scrolled: scrolled.clone(),
        search: search.clone(),
        status,
        rows: RefCell::new(Vec::new()),
        selection: Cell::new(None),
        drag: RefCell::new(Drag::default()),
        textures: RefCell::new(HashMap::new()),
        matches: RefCell::new(Vec::new()),
        match_index: Cell::new(usize::MAX),
        waiter: RefCell::new(None),
        pending: RefCell::new(Vec::new()),
        decisions: RefCell::new(Vec::new()),
        automated: config.automated,
        binds: Cell::new(0),
        align: Cell::new(None),
    });

    let a = app.clone();
    factory.connect_setup(move |_, obj| {
        let list_item = obj.downcast_ref::<gtk::ListItem>().unwrap();
        list_item.set_activatable(false);
        let row = Row::new();
        for label in [&row.heading, &row.text] {
            let a = a.clone();
            label.connect_activate_link(move |_, uri| {
                a.activate_link(uri);
                glib::Propagation::Stop
            });
        }
        list_item.set_child(Some(&row.root));
        a.rows.borrow_mut().push(row);
    });
    let a = app.clone();
    factory.connect_bind(move |_, obj| {
        let list_item = obj.downcast_ref::<gtk::ListItem>().unwrap();
        let child = list_item.child().unwrap();
        let row = a
            .rows
            .borrow()
            .iter()
            .find(|r| r.root.upcast_ref::<gtk::Widget>() == &child)
            .cloned();
        if let Some(row) = row {
            a.bind(&row, list_item.position(), list_item);
        }
    });
    let a = app.clone();
    factory.connect_unbind(move |_, obj| {
        let child = obj
            .downcast_ref::<gtk::ListItem>()
            .unwrap()
            .child()
            .unwrap();
        if let Some(row) = a
            .rows
            .borrow()
            .iter()
            .find(|r| r.root.upcast_ref::<gtk::Widget>() == &child)
        {
            row.item.set(None);
        }
    });
    let a = app.clone();
    factory.connect_teardown(move |_, obj| {
        if let Some(child) = obj.downcast_ref::<gtk::ListItem>().unwrap().child() {
            a.rows
                .borrow_mut()
                .retain(|r| r.root.upcast_ref::<gtk::Widget>() != &child);
        }
    });

    let a = app.clone();
    outline_factory.connect_setup(|_, obj| {
        let label = gtk::Label::builder()
            .xalign(0.0)
            .ellipsize(pango::EllipsizeMode::End)
            .build();
        obj.downcast_ref::<gtk::ListItem>()
            .unwrap()
            .set_child(Some(&label));
    });
    outline_factory.connect_bind(move |_, obj| {
        let list_item = obj.downcast_ref::<gtk::ListItem>().unwrap();
        let entry = &a.doc.outline[list_item.position() as usize];
        let label = list_item.child().unwrap().downcast::<gtk::Label>().unwrap();
        label.set_text(&entry.title);
        label.set_margin_start(6 + (entry.level as i32 - 1) * 14);
    });
    // Attach factories only now: the list realizes rows as soon as it has one.
    list.set_factory(Some(&factory));
    outline.set_factory(Some(&outline_factory));
    let a = app.clone();
    // Clicking or arrowing selects an entry and the document follows; Enter
    // or double-click also moves keyboard focus into the document.
    outline.connect_activate(move |_, position| {
        a.jump_to_block(a.doc.outline[position as usize].block, true);
    });
    let a = app.clone();
    outline_selection.connect_selected_notify(move |selection| {
        let position = selection.selected();
        if position != gtk::INVALID_LIST_POSITION {
            a.jump_to_block(a.doc.outline[position as usize].block, false);
        }
    });

    // Document-level pointer selection.
    let drag = gtk::GestureDrag::new();
    drag.set_propagation_phase(gtk::PropagationPhase::Capture);
    drag.set_button(gdk::BUTTON_PRIMARY);
    let a = app.clone();
    drag.connect_drag_begin(move |gesture, x, y| {
        let extend = gesture
            .current_event_state()
            .contains(gdk::ModifierType::SHIFT_MASK);
        *a.drag.borrow_mut() = Drag {
            start: (x, y),
            last: (x, y),
            active: false,
            extend,
            anchor: None,
            timer: None,
        };
        trace(&format!(
            "drag-begin {x:.0},{y:.0} extend={extend} selection={:?}",
            a.selection.get()
        ));
        // Claim at once: row widgets otherwise take the sequence after a few
        // pixels of motion. Pointer link activation moves to drag-end.
        gesture.set_state(gtk::EventSequenceState::Claimed);
        a.list.grab_focus();
    });
    let a = app.clone();
    drag.connect_drag_update(move |gesture, dx, dy| {
        let (start, active, extend) = {
            let d = a.drag.borrow();
            (d.start, d.active, d.extend)
        };
        if !active {
            if dx * dx + dy * dy < 16.0 {
                return;
            }
            let _ = gesture;
            let anchor = match (extend, a.selection.get()) {
                (true, Some((anchor, _))) => Some(anchor),
                _ => a.hit(start.0, start.1),
            };
            trace(&format!("drag-claim anchor={anchor:?}"));
            let Some(anchor) = anchor else { return };
            {
                let mut d = a.drag.borrow_mut();
                d.anchor = Some(anchor);
                d.active = true;
            }
            let auto = a.clone();
            let timer = glib::timeout_add_local(Duration::from_millis(16), move || {
                let y = auto.drag.borrow().last.1;
                let height = auto.list.height() as f64;
                let adj = auto.scrolled.vadjustment();
                let delta = if y < 24.0 {
                    (y - 24.0).max(-200.0)
                } else if y > height - 24.0 {
                    (y - height + 24.0).min(200.0)
                } else {
                    0.0
                };
                if delta != 0.0 {
                    adj.set_value(adj.value() + delta);
                    auto.drag_to_last();
                }
                glib::ControlFlow::Continue
            });
            a.drag.borrow_mut().timer = Some(timer);
        }
        a.drag.borrow_mut().last = (start.0 + dx, start.1 + dy);
        a.drag_to_last();
    });
    let a = app.clone();
    drag.connect_drag_end(move |_, dx, dy| {
        trace(&format!(
            "drag-end {dx:.0},{dy:.0} active={} selection={:?}",
            a.drag.borrow().active,
            a.selection.get()
        ));
        let mut d = a.drag.borrow_mut();
        if let Some(timer) = d.timer.take() {
            timer.remove();
        }
        if d.active {
            d.active = false;
        } else if !d.extend && dx * dx + dy * dy < 16.0 {
            let (x, y) = d.start;
            drop(d);
            a.set_selection(None);
            if let Some(dest) = a.hit(x, y).and_then(|pos| a.link_at(pos)) {
                a.activate_link(&dest);
            }
        } else if d.extend {
            let (x, y) = d.start;
            drop(d);
            if let Some(focus) = a.hit(x, y) {
                a.extend(focus);
            }
        }
    });
    list.add_controller(drag);

    // Keyboard selection, copy and find.
    let keys = gtk::EventControllerKey::new();
    keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    let a = app.clone();
    keys.connect_key_pressed(move |_, key, _, state| {
        let ctrl = state.contains(gdk::ModifierType::CONTROL_MASK);
        let shift = state.contains(gdk::ModifierType::SHIFT_MASK);
        let focus = || {
            a.selection
                .get()
                .map(|(_, f)| f)
                .unwrap_or_else(|| a.focus_pos())
        };
        match key {
            gdk::Key::a | gdk::Key::A if ctrl => a.select_all(),
            gdk::Key::c | gdk::Key::C | gdk::Key::Insert if ctrl => {
                a.copy();
            }
            gdk::Key::End if ctrl && shift => a.extend(a.doc.end()),
            gdk::Key::Home if ctrl && shift => a.extend(a.doc.start()),
            gdk::Key::Right if shift => a.extend(a.step(focus(), 1)),
            gdk::Key::Left if shift => a.extend(a.step(focus(), -1)),
            gdk::Key::Down if shift => a.extend(a.step_item(focus(), true)),
            gdk::Key::Up if shift => a.extend(a.step_item(focus(), false)),
            gdk::Key::Escape => a.set_selection(None),
            // A focused label consumes plain arrows; move row focus here so
            // keyboard and screen-reader reading always advance by row.
            gdk::Key::Down | gdk::Key::Up if !ctrl => {
                let focus = gtk::prelude::GtkWindowExt::focus(&a.window);
                trace(&format!(
                    "arrow focus={:?} item={:?}",
                    focus.as_ref().map(|w| w.type_().name()),
                    a.focused_item()
                ));
                let Some(item) = a.focused_item().or_else(|| {
                    a.doc
                        .items
                        .get(a.focus_pos_item() as usize)
                        .map(|_| a.focus_pos_item())
                }) else {
                    return glib::Propagation::Proceed;
                };
                let last = a.doc.items.len() as u32 - 1;
                let next = if key == gdk::Key::Down {
                    (item + 1).min(last)
                } else {
                    item.saturating_sub(1)
                };
                a.scroll_to_item(next, true);
            }
            _ => return glib::Propagation::Proceed,
        }
        glib::Propagation::Stop
    });
    scrolled.add_controller(keys);
    let window_keys = gtk::EventControllerKey::new();
    window_keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    let a = app.clone();
    window_keys.connect_key_pressed(move |_, key, _, state| {
        trace(&format!(
            "window-key {key:?} focus={:?}",
            gtk::prelude::GtkWindowExt::focus(&a.window)
                .map(|w| (w.type_().name().to_string(), w.widget_name().to_string()))
        ));
        if state.contains(gdk::ModifierType::CONTROL_MASK)
            && matches!(key, gdk::Key::f | gdk::Key::F)
        {
            a.search.grab_focus();
            return glib::Propagation::Stop;
        }
        glib::Propagation::Proceed
    });
    window.add_controller(window_keys);
    let a = app.clone();
    search.connect_search_changed(move |entry| {
        a.find(&entry.text());
    });
    let a = app.clone();
    search.connect_activate(move |_| {
        a.next_match(true);
    });
    let a = app.clone();
    search.connect_next_match(move |_| {
        a.next_match(true);
    });
    let a = app.clone();
    search.connect_previous_match(move |_| {
        a.next_match(false);
    });
    let a = app.clone();
    search.connect_stop_search(move |_| {
        a.list.grab_focus();
    });

    let a = app.clone();
    window.connect_realize(move |window| {
        let clock = window.frame_clock().expect("frame clock");
        let b = a.clone();
        clock.connect_after_paint(move |clock| b.after_paint(clock));
    });
    window.connect_focus_widget_notify(|w| {
        let focus = gtk::prelude::GtkWindowExt::focus(w);
        trace(&format!(
            "focus-change {:?} in={:?}",
            focus.as_ref().map(|f| f.type_().name()),
            focus.and_then(|f| f.parent()).map(|p| p.widget_name())
        ));
    });
    window.present();
    list.grab_focus();

    let a = app.clone();
    let config = config.clone();
    glib::spawn_future_local(async move {
        let presented = a
            .presented(|app| {
                app.rows
                    .borrow()
                    .iter()
                    .any(|r| r.item.get().is_some() && r.root.is_mapped() && r.root.height() > 0)
            })
            .await;
        let origin = launch_us.unwrap_or(startup.main_us);
        let mut report = json!({
            "document": a.doc.source.len(),
            "blocks": a.doc.blocks.len(),
            "items": a.doc.items.len(),
            "headings": a.doc.outline.len(),
            "launch_origin": if launch_us.is_some() { "harness CLOCK_MONOTONIC before spawn" } else { "process main" },
            "launch_to_main_ms": ms(origin, startup.main_us),
            "read_ms": startup.read_ms,
            "parse_ms": startup.parse_ms,
            "launch_to_paint_ms": ms(origin, presented.paint_us),
            "launch_to_presented_ms": ms(origin, presented.best_us()),
            "presentation_time_available": presented.presentation_us > 0,
            "binds_at_presented": a.binds.get(),
            "rows_at_presented": a.rows.borrow().len(),
            "gtk": format!("{}.{}.{}", gtk::major_version(), gtk::minor_version(), gtk::micro_version()),
            "backend": WidgetExt::display(&a.window).type_().name(),
        });
        event(
            json!({"event": "presented", "launch_to_presented_ms": report["launch_to_presented_ms"]}),
        );
        if config.automated {
            automated(&a, &mut report, &config).await;
        }
        if config.control {
            control(&a).await;
        }
        report["binds"] = json!(a.binds.get());
        report["realized_rows"] = json!(a.rows.borrow().len());
        report["peak_rss_kib"] = json!(peak_rss_kib());
        report["link_decisions"] = json!(*a.decisions.borrow());
        println!("REPORT {report}");
        if config.automated || config.control {
            a.window.application().unwrap().quit();
        }
    });
}

async fn jump_presented(app: &Rc<App>, item: u32, focus: bool) -> (f64, Option<Row>) {
    let requested = glib::monotonic_time();
    app.scroll_to_item(item, focus);
    let presented = app
        .presented(move |app| app.visible_row(item).is_some())
        .await;
    (ms(requested, presented.best_us()), app.visible_row(item))
}

async fn automated(app: &Rc<App>, report: &mut Value, config: &Config) {
    let doc = &app.doc;
    // Raw Pango cost of laying out the first 205 fragments once at the row width.
    {
        let width = (app.list.width() - 36).max(100) * pango::SCALE;
        let started = Instant::now();
        for item in doc.items.iter().take(205) {
            let layout = app.list.create_pango_layout(Some(
                &doc.blocks[item.block as usize].text[item.start as usize..item.end as usize],
            ));
            layout.set_wrap(pango::WrapMode::WordChar);
            layout.set_width(width);
            let _ = layout.pixel_size();
        }
        report["pango_205_layouts_ms"] = json!(started.elapsed().as_secs_f64() * 1000.0);
    }
    // Highlight state as drawn: background attributes in each bound row's layout.
    let highlighted = |app: &App| -> usize {
        app.rows
            .borrow()
            .iter()
            .filter(|r| r.item.get().is_some())
            .filter(|r| {
                r.label().layout().attributes().is_some_and(|attrs| {
                    attrs
                        .attributes()
                        .iter()
                        .any(|a| a.type_() == pango::AttrType::Background)
                })
            })
            .count()
    };
    {
        let first = doc.items[0];
        let one = |b: u32| {
            doc.blocks[b as usize]
                .text
                .chars()
                .next()
                .map_or(0, char::len_utf8) as u32
        };
        let last = doc.items[3.min(doc.items.len() - 1)].block;
        app.set_selection(Some((
            Pos {
                block: first.block,
                offset: 0,
            },
            Pos {
                block: last,
                offset: one(last),
            },
        )));
        let selected = highlighted(app);
        app.set_selection(None);
        let cleared = highlighted(app);
        let b = doc.items[6.min(doc.items.len() - 1)].block;
        app.set_selection(Some((
            Pos {
                block: b,
                offset: 0,
            },
            Pos {
                block: b,
                offset: one(b),
            },
        )));
        let replaced = highlighted(app);
        app.set_selection(None);
        report["highlight_rows"] = json!({"selected": selected, "after_clear": cleared, "after_reselect_one_block": replaced});
    }
    // Whole-document selection and copy.
    let started = Instant::now();
    app.select_all();
    let (expected, source_len) = app.copy().expect("copy");
    let copy_ms = started.elapsed().as_secs_f64() * 1000.0;
    let read = app.window.clipboard().read_text_future().await;
    let readback = read
        .ok()
        .flatten()
        .map(|t| t.to_string())
        .unwrap_or_default();
    if let Some(path) = &config.expect_out {
        std::fs::write(path, &expected).expect("write expected text");
    }
    report["select_all_copy"] = json!({
        "copy_ms": copy_ms,
        "plain_bytes": expected.len(),
        "markdown_bytes": source_len,
        "markdown_equals_source": source_len == doc.source.len(),
        "local_readback_equals_model": readback == expected,
    });

    // Sampled presentation: displayed fragment equals the copied text at its offset.
    let mut samples = Vec::new();
    let count = doc.items.len();
    for fraction in [0.1, 0.35, 0.6, 0.85, 1.0] {
        let index = (((count - 1) as f64) * fraction) as u32;
        let (jump_ms, row) = jump_presented(app, index, false).await;
        let item = doc.items[index as usize];
        let want = &doc.blocks[item.block as usize].text[item.start as usize..item.end as usize];
        let shown = row.map(|r| r.label().text().to_string());
        let at = doc.plain_offset(Pos {
            block: item.block,
            offset: item.start,
        });
        samples.push(json!({
            "item": index,
            "jump_to_presented_ms": jump_ms,
            "displayed_equals_model": shown.as_deref() == Some(want),
            "copy_contains_at_offset": expected.get(at..at + want.len()) == Some(want),
        }));
    }
    report["samples"] = json!(samples);
    app.set_selection(None);

    // Find a phrase near the end and jump to its off-screen row.
    let target = doc
        .blocks
        .iter()
        .enumerate()
        .skip(doc.blocks.len() * 92 / 100)
        .find(|(_, b)| {
            b.kind == BlockKind::Paragraph
                && b.text.len() > 60
                && b.text.is_char_boundary(20)
                && b.text.is_char_boundary(44)
        });
    if let Some((block, b)) = target {
        let query = b.text[20..44].to_string();
        let (matches, find_ms) = app.find(&query);
        app.search.set_text(&query);
        // Start just before the first match in the target block, then step once.
        let first = app
            .matches
            .borrow()
            .iter()
            .position(|m| m.block as usize >= block)
            .expect("match");
        app.match_index
            .set(if first == 0 { usize::MAX } else { first - 1 });
        let m = app.next_match(true).expect("match");
        let item = doc.item_for(Pos {
            block: m.block,
            offset: m.start,
        });
        let (jump_ms, row) = jump_presented(app, item, false).await;
        let shown = row
            .map(|r| r.label().text().to_lowercase())
            .unwrap_or_default();
        report["find"] = json!({
            "query_bytes": query.len(),
            "matches": matches,
            "find_ms": find_ms,
            "target_block": m.block,
            "jump_to_presented_ms": jump_ms,
            "presented_row_contains_query": shown.contains(&query.to_lowercase()),
            "selection_is_match": app.selection.get() == Some((Pos { block: m.block, offset: m.start }, Pos { block: m.block, offset: m.end })),
        });
    }

    // Outline jumps: last heading, then first.
    let mut jumps = Vec::new();
    let mut entries: Vec<usize> = vec![];
    if !doc.outline.is_empty() {
        entries.push(doc.outline.len() - 1);
        entries.push(doc.outline.len() / 2);
        entries.push(0);
    }
    for entry in entries {
        let heading = &doc.outline[entry];
        let requested = glib::monotonic_time();
        let item = app.jump_to_block(heading.block, true);
        let presented = app
            .presented(move |app| app.align.get().is_none() && app.visible_row(item).is_some())
            .await;
        let jump_ms = ms(requested, presented.best_us());
        let row = app.visible_row(item);
        let top = row
            .as_ref()
            .and_then(|r| r.root.parent())
            .and_then(|w| w.compute_bounds(&app.list))
            .map(|b| b.y());
        let ok = row
            .as_ref()
            .is_some_and(|r| r.heading.is_visible() && r.heading.text() == heading.title);
        let focused = row
            .as_ref()
            .is_some_and(|r| r.root.parent().is_some_and(|p| p.has_focus()));
        jumps.push(json!({"entry": entry, "block": heading.block, "jump_to_presented_ms": jump_ms, "heading_presented": ok, "row_focused": focused, "row_top_px": top}));
    }
    report["outline_jumps"] = json!(jumps);

    // Link policy and fragment activation.
    for dest in doc.links.iter().take(40) {
        let decision = app.activate_link(dest);
        if decision["intent"]
            .as_str()
            .is_some_and(|i| i.starts_with("Fragment"))
        {
            let block = app.slugs[&dest[1..]];
            let item = doc.first_item[block as usize];
            let (jump_ms, row) = jump_presented(app, item, true).await;
            report["fragment_link"] =
                json!({"dest": dest, "jump_to_presented_ms": jump_ms, "presented": row.is_some()});
        }
    }
    app.decisions.borrow_mut().clear();
    for dest in doc.links.iter().take(40) {
        let intent = feas008_docmodel::classify_link(dest, &app.slugs);
        app.decisions
            .borrow_mut()
            .push(json!({"dest": dest, "intent": format!("{intent:?}")}));
    }

    // Images: presented, blocked or failed under policy.
    let mut images = Vec::new();
    for (block, b) in doc
        .blocks
        .iter()
        .enumerate()
        .filter(|(_, b)| b.kind == BlockKind::Image)
        .take(20)
    {
        let item = doc.first_item[block];
        let (_, _) = jump_presented(app, item, false).await;
        let mut outcome = None;
        for _ in 0..200 {
            let state = match &b.image {
                Some(ImageRequest::Local(rel)) => match app.textures.borrow().get(rel) {
                    Some(Texture::Ready(t)) => {
                        Some(format!("presented {}x{}", t.width(), t.height()))
                    }
                    Some(Texture::Failed(reason)) => Some(format!("refused: {reason}")),
                    _ => None,
                },
                Some(ImageRequest::Blocked(reason)) => {
                    Some(format!("blocked by model policy: {reason}"))
                }
                None => Some("no request".into()),
            };
            if state.is_some() {
                outcome = state;
                break;
            }
            glib::timeout_future(Duration::from_millis(10)).await;
        }
        let picture_shown = app
            .presented(move |app| app.visible_row(item).is_some())
            .await;
        let _ = picture_shown;
        let visible = app
            .visible_row(item)
            .is_some_and(|r| r.picture.is_visible() && r.picture.paintable().is_some());
        images.push(json!({"alt": b.text, "request": format!("{:?}", b.image), "outcome": outcome, "picture_visible": visible}));
    }
    report["images"] = json!(images);
}

/// Line commands on stdin for external probes (clipboard, AT-SPI).
async fn control(app: &Rc<App>) {
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines() {
            let Ok(line) = line else { break };
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    event(json!({"event": "ready"}));
    loop {
        let line = loop {
            match rx.try_recv() {
                Ok(line) => break Some(line),
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    glib::timeout_future(Duration::from_millis(20)).await
                }
                Err(_) => break None,
            }
        };
        let Some(line) = line else { return };
        let mut parts = line.split_whitespace();
        match (
            parts.next(),
            parts.next().and_then(|n| n.parse::<u32>().ok()),
        ) {
            (Some("copy-all"), _) => {
                app.select_all();
                let (plain, source) = app.copy().expect("copy");
                event(
                    json!({"event": "copied", "plain_bytes": plain.len(), "markdown_bytes": source}),
                );
            }
            (Some("item"), Some(n)) => {
                let (jump_ms, _) = jump_presented(app, n, true).await;
                event(json!({"event": "presented", "item": n, "jump_to_presented_ms": jump_ms}));
            }
            (Some("outline"), Some(n)) => {
                let block = app.doc.outline[n as usize].block;
                let item = app.doc.first_item[block as usize];
                let (jump_ms, _) = jump_presented(app, item, true).await;
                event(json!({"event": "presented", "item": item, "jump_to_presented_ms": jump_ms}));
            }
            (Some("texts"), Some(from)) => {
                let doc = &app.doc;
                let to = parts
                    .next()
                    .and_then(|n| n.parse::<u32>().ok())
                    .unwrap_or(from + 1)
                    .min(doc.items.len() as u32);
                let texts: Vec<&str> = (from..to)
                    .map(|i| {
                        let item = doc.items[i as usize];
                        &doc.blocks[item.block as usize].text
                            [item.start as usize..item.end as usize]
                    })
                    .collect();
                event(
                    json!({"event": "texts", "from": from, "texts": texts, "items": doc.items.len()}),
                );
            }
            (Some("quit"), _) => return,
            _ => event(json!({"event": "error", "line": line})),
        }
    }
}
