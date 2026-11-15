//! Native GTK 4 workspace search panel (LNX-002C).

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Arc, mpsc};
use std::time::Duration;

use gtk::{gdk, gio, glib, pango, prelude::*};

use crate::count_model::CountModel;
use markdown_linux::document::FolderCapability;
use markdown_linux::document::{
    WorkspaceContentFileMatch, WorkspaceNameMatch, WorkspaceSearchHandle, WorkspaceSearchRequest,
    WorkspaceSearchUpdate, WorkspaceTree, start_workspace_search,
};
use markdown_linux_core::{ContentHit, ContentProgress, ContentStopReason};

#[derive(Clone, Debug)]
pub enum WorkspaceSearchTarget {
    File(PathBuf),
    ContentHit {
        path: PathBuf,
        span_start: u32,
        source_length: u64,
        source_sha256: [u8; 64],
        anchor: Option<Vec<u8>>,
    },
}

#[derive(Clone, Debug)]
enum SearchRow {
    SectionHeader(String),
    NameHit(WorkspaceNameMatch),
    ContentFileHeader {
        rel_path: String,
        full_path: PathBuf,
        match_count: usize,
        truncated: bool,
        has_invisible: bool,
    },
    ContentHit {
        full_path: PathBuf,
        line: u64,
        hit: ContentHit,
        source_length: u64,
        source_sha256: [u8; 64],
    },
}

struct SearchPanelState {
    rows: Vec<SearchRow>,
    name_hits: Vec<WorkspaceNameMatch>,
    content_files: Vec<WorkspaceContentFileMatch>,
    names_truncated: bool,
    selection: gtk::SingleSelection,
    list_view: gtk::ListView,
    status_label: gtk::Label,
    cancel_button: gtk::Button,
    active_handle: Option<WorkspaceSearchHandle>,
    poll_source: Option<glib::SourceId>,
    search_id: u64,
    include_hidden: bool,
    all_files_for_names: bool,
    folder: Option<FolderCapability>,
    tree: Option<Arc<WorkspaceTree>>,
    on_open_target: Option<Rc<dyn Fn(WorkspaceSearchTarget)>>,
    on_close_search: Option<Rc<dyn Fn()>>,
}

#[derive(Clone)]
pub struct WorkspaceSearchPanel {
    pub widget: gtk::Box,
    pub entry: gtk::SearchEntry,
    pub case_button: gtk::CheckButton,
    pub word_button: gtk::CheckButton,
    pub cancel_button: gtk::Button,
    state: Rc<RefCell<SearchPanelState>>,
    suppress_search_changed: Rc<Cell<bool>>,
}

impl WorkspaceSearchPanel {
    pub fn new() -> Self {
        let widget = gtk::Box::new(gtk::Orientation::Vertical, 4);
        widget.set_widget_name("workspace-search-panel");

        let entry = gtk::SearchEntry::builder()
            .placeholder_text("Search workspace…")
            .hexpand(true)
            .margin_start(8)
            .margin_end(8)
            .build();
        entry.set_widget_name("workspace-search-entry");

        let options_row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        options_row.set_margin_start(8);
        options_row.set_margin_end(8);

        let case_button = gtk::CheckButton::builder()
            .label("Aa")
            .tooltip_text("Match Case")
            .build();
        let word_button = gtk::CheckButton::builder()
            .label("W")
            .tooltip_text("Whole Words")
            .build();
        let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        spacer.set_hexpand(true);
        let cancel_button = gtk::Button::builder()
            .label("Stop")
            .tooltip_text("Cancel active workspace search")
            .sensitive(false)
            .build();

        options_row.append(&case_button);
        options_row.append(&word_button);
        options_row.append(&spacer);
        options_row.append(&cancel_button);

        let status_label = gtk::Label::new(Some("Type to search workspace"));
        status_label.set_xalign(0.0);
        status_label.set_margin_start(8);
        status_label.set_margin_end(8);
        status_label.set_ellipsize(pango::EllipsizeMode::End);
        status_label.add_css_class("dim-label");

        let factory = gtk::SignalListItemFactory::new();
        let selection = gtk::SingleSelection::new(None::<gio::ListModel>);
        selection.set_autoselect(false);
        selection.set_can_unselect(true);

        let list_view = gtk::ListView::new(Some(selection.clone()), Some(factory.clone()));
        list_view.set_widget_name("workspace-search-results");
        list_view.set_single_click_activate(false);

        let scrolled = gtk::ScrolledWindow::builder()
            .hexpand(true)
            .vexpand(true)
            .min_content_width(180)
            .child(&list_view)
            .build();

        widget.append(&entry);
        widget.append(&options_row);
        widget.append(&status_label);
        widget.append(&scrolled);

        let state = Rc::new(RefCell::new(SearchPanelState {
            rows: Vec::new(),
            name_hits: Vec::new(),
            content_files: Vec::new(),
            names_truncated: false,
            selection: selection.clone(),
            list_view: list_view.clone(),
            status_label: status_label.clone(),
            cancel_button: cancel_button.clone(),
            active_handle: None,
            poll_source: None,
            search_id: 0,
            include_hidden: false,
            all_files_for_names: false,
            folder: None,
            tree: None,
            on_open_target: None,
            on_close_search: None,
        }));

        factory.connect_setup(|_, item| {
            let Some(item) = item.downcast_ref::<gtk::ListItem>() else {
                return;
            };
            let row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
            row.set_margin_start(8);
            row.set_margin_end(8);
            row.set_margin_top(3);
            row.set_margin_bottom(3);
            row.set_accessible_role(gtk::AccessibleRole::ListItem);

            let label = gtk::Label::new(None);
            label.set_xalign(0.0);
            label.set_hexpand(true);
            label.set_ellipsize(pango::EllipsizeMode::End);

            let warn = gtk::Label::new(Some("⚠"));
            warn.set_tooltip_text(Some("Filename contains invisible characters"));
            warn.set_visible(false);

            row.append(&label);
            row.append(&warn);
            item.set_child(Some(&row));
        });

        let weak_state = Rc::downgrade(&state);
        factory.connect_bind(move |_, item| {
            let Some(item) = item.downcast_ref::<gtk::ListItem>() else {
                return;
            };
            let Some(state_rc) = weak_state.upgrade() else {
                return;
            };
            let state = state_rc.borrow();
            let Some(search_row) = state.rows.get(item.position() as usize) else {
                return;
            };
            let Some(row) = item.child().and_downcast::<gtk::Box>() else {
                return;
            };
            let label = row.first_child().and_downcast::<gtk::Label>().unwrap();
            let warn = label.next_sibling().and_downcast::<gtk::Label>().unwrap();
            label.set_attributes(None);
            label.remove_css_class("heading");
            label.remove_css_class("dim-label");
            warn.set_visible(false);

            match search_row {
                SearchRow::SectionHeader(title) => {
                    label.set_text(title);
                    label.add_css_class("heading");
                    row.update_property(&[gtk::accessible::Property::Label(title)]);
                }
                SearchRow::NameHit(hit) => {
                    let prefix = "📄 ";
                    let text = format!("{prefix}{}", hit.rel_path);
                    label.set_text(&text);
                    let attrs = pango::AttrList::new();
                    let offset = prefix.len() as u32;
                    let start = offset.saturating_add(hit.match_start);
                    let end = offset.saturating_add(hit.match_end);
                    if start < end {
                        let mut bold = pango::AttrInt::new_weight(pango::Weight::Bold);
                        bold.set_start_index(start);
                        bold.set_end_index(end);
                        attrs.insert(bold);
                    }
                    label.set_attributes(Some(&attrs));
                    warn.set_visible(hit.has_invisible);
                    let acc = if hit.has_invisible {
                        format!("{} (contains invisible characters)", hit.rel_path)
                    } else {
                        hit.rel_path.clone()
                    };
                    row.update_property(&[gtk::accessible::Property::Label(&acc)]);
                }
                SearchRow::ContentFileHeader {
                    rel_path,
                    full_path: _,
                    match_count,
                    truncated,
                    has_invisible,
                } => {
                    let suffix = if *truncated { "+" } else { "" };
                    let text = format!("📄 {rel_path} ({match_count}{suffix})");
                    label.set_text(&text);
                    let attrs = pango::AttrList::new();
                    let mut bold = pango::AttrInt::new_weight(pango::Weight::Bold);
                    bold.set_start_index(0);
                    bold.set_end_index(text.len() as u32);
                    attrs.insert(bold);
                    label.set_attributes(Some(&attrs));
                    warn.set_visible(*has_invisible);
                    row.update_property(&[gtk::accessible::Property::Label(&text)]);
                }
                SearchRow::ContentHit { line, hit, .. } => {
                    let lead = if hit.leading_more { "…" } else { "" };
                    let trail = if hit.trailing_more { "…" } else { "" };
                    let prefix = format!("  L{line}: {lead}");
                    let text = format!("{prefix}{}{trail}", hit.snippet);
                    label.set_text(&text);
                    let attrs = pango::AttrList::new();
                    let base = prefix.len() as u32;
                    let start = base.saturating_add(hit.match_start);
                    let end = base.saturating_add(hit.match_end);
                    if start < end {
                        let mut bg = pango::AttrColor::new_background(65535, 55000, 26000);
                        bg.set_start_index(start);
                        bg.set_end_index(end);
                        attrs.insert(bg);
                        let mut fg = pango::AttrColor::new_foreground(0, 0, 0);
                        fg.set_start_index(start);
                        fg.set_end_index(end);
                        attrs.insert(fg);
                        let mut bold = pango::AttrInt::new_weight(pango::Weight::Bold);
                        bold.set_start_index(start);
                        bold.set_end_index(end);
                        attrs.insert(bold);
                    }
                    label.set_attributes(Some(&attrs));
                    let acc = format!("Line {line}: {}", hit.snippet);
                    row.update_property(&[gtk::accessible::Property::Label(&acc)]);
                }
            }
        });

        let weak_state = Rc::downgrade(&state);
        list_view.connect_activate(move |_, pos| {
            Self::activate_index(&weak_state, pos);
        });

        let key_ctrl = gtk::EventControllerKey::new();
        let weak_state = Rc::downgrade(&state);
        key_ctrl.connect_key_pressed(move |_, keyval, _, _| {
            if keyval == gdk::Key::Escape {
                if let Some(state_rc) = weak_state.upgrade()
                    && let Some(cb) = state_rc.borrow().on_close_search.clone()
                {
                    cb();
                }
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        widget.add_controller(key_ctrl);

        let panel = Self {
            widget,
            entry: entry.clone(),
            case_button: case_button.clone(),
            word_button: word_button.clone(),
            cancel_button: cancel_button.clone(),
            state: state.clone(),
            suppress_search_changed: Rc::new(Cell::new(false)),
        };

        let panel_clone = panel.clone();
        entry.connect_search_changed(move |_| {
            if !panel_clone.suppress_search_changed.get() {
                panel_clone.trigger_search();
            }
        });

        let panel_clone = panel.clone();
        entry.connect_activate(move |_| {
            panel_clone.trigger_search();
        });

        let panel_clone = panel.clone();
        entry.connect_stop_search(move |_| {
            if let Some(cb) = panel_clone.state.borrow().on_close_search.clone() {
                cb();
            }
        });

        let panel_clone = panel.clone();
        case_button.connect_toggled(move |_| {
            if !panel_clone.entry.text().trim().is_empty() {
                panel_clone.trigger_search();
            }
        });

        let panel_clone = panel.clone();
        word_button.connect_toggled(move |_| {
            if !panel_clone.entry.text().trim().is_empty() {
                panel_clone.trigger_search();
            }
        });

        let panel_clone = panel.clone();
        cancel_button.connect_clicked(move |_| {
            panel_clone.cancel_search();
        });

        panel
    }

    pub fn set_callbacks(
        &self,
        on_open_target: impl Fn(WorkspaceSearchTarget) + 'static,
        on_close_search: impl Fn() + 'static,
    ) {
        let mut state = self.state.borrow_mut();
        state.on_open_target = Some(Rc::new(on_open_target));
        state.on_close_search = Some(Rc::new(on_close_search));
    }

    pub fn set_workspace(
        &self,
        folder: Option<FolderCapability>,
        tree: Option<Arc<WorkspaceTree>>,
    ) {
        self.cancel_search();
        {
            let mut state = self.state.borrow_mut();
            state.folder = folder;
            state.tree = tree;
            state.rows.clear();
            state.name_hits.clear();
            state.content_files.clear();
            state.selection.set_model(None::<&gio::ListModel>);
            if state.folder.is_none() {
                state
                    .status_label
                    .set_text("Open a folder to search workspace");
            } else {
                state.status_label.set_text("Type to search workspace");
            }
        }
        if !self.entry.text().trim().is_empty() && self.state.borrow().tree.is_some() {
            self.trigger_search();
        }
    }

    pub fn set_workspace_filters(&self, include_hidden: bool, all_files_for_names: bool) {
        let changed = {
            let mut state = self.state.borrow_mut();
            let diff = state.include_hidden != include_hidden
                || state.all_files_for_names != all_files_for_names;
            state.include_hidden = include_hidden;
            state.all_files_for_names = all_files_for_names;
            diff
        };
        if changed && !self.entry.text().trim().is_empty() && self.state.borrow().tree.is_some() {
            self.trigger_search();
        }
    }

    pub fn set_query(&self, query: &str) {
        self.suppress_search_changed.set(true);
        self.entry.set_text(query);
        self.suppress_search_changed.set(false);
        self.trigger_search();
    }

    pub fn focus_entry(&self) {
        self.entry.grab_focus();
    }

    pub fn cancel_search(&self) {
        let mut state = self.state.borrow_mut();
        if let Some(handle) = state.active_handle.take() {
            handle.cancel();
        }
        if let Some(source) = state.poll_source.take() {
            source.remove();
        }
        state.cancel_button.set_sensitive(false);
        if !state.rows.is_empty() || state.status_label.text() == "Searching…" {
            let name_count = state.name_hits.len();
            let content_count: usize = state.content_files.iter().map(|f| f.hits.len()).sum();
            state.status_label.set_text(&format!(
                "Search cancelled ({name_count} name, {content_count} content matches)"
            ));
        }
    }

    pub fn clear(&self) {
        self.cancel_search();
        self.suppress_search_changed.set(true);
        self.entry.set_text("");
        self.suppress_search_changed.set(false);
        let mut state = self.state.borrow_mut();
        state.folder = None;
        state.tree = None;
        state.rows.clear();
        state.name_hits.clear();
        state.content_files.clear();
        state.selection.set_model(None::<&gio::ListModel>);
        state
            .status_label
            .set_text("Open a folder to search workspace");
    }

    pub fn trigger_search(&self) {
        let query = self.entry.text().to_string();
        let case_sensitive = self.case_button.is_active();
        let whole_word = self.word_button.is_active();

        let (folder, tree, search_id, include_hidden, all_files_for_names) = {
            let mut state = self.state.borrow_mut();
            if let Some(handle) = state.active_handle.take() {
                handle.cancel();
            }
            if let Some(source) = state.poll_source.take() {
                source.remove();
            }
            state.search_id = state.search_id.wrapping_add(1);
            state.rows.clear();
            state.name_hits.clear();
            state.content_files.clear();
            state.names_truncated = false;
            state.selection.set_model(None::<&gio::ListModel>);
            (
                state.folder.clone(),
                state.tree.clone(),
                state.search_id,
                state.include_hidden,
                state.all_files_for_names,
            )
        };

        if query.trim().is_empty() {
            let state = self.state.borrow();
            state.cancel_button.set_sensitive(false);
            if folder.is_none() {
                state
                    .status_label
                    .set_text("Open a folder to search workspace");
            } else {
                state.status_label.set_text("Type to search workspace");
            }
            return;
        }

        let (Some(folder), Some(tree)) = (folder, tree) else {
            let state = self.state.borrow();
            state.cancel_button.set_sensitive(false);
            state
                .status_label
                .set_text("Open a folder to search workspace");
            return;
        };

        let mut request = WorkspaceSearchRequest::new(query);
        request.case_sensitive = case_sensitive;
        request.whole_word = whole_word;
        request.include_hidden = include_hidden;
        request.all_files_for_names = all_files_for_names;

        let (tx, rx) = mpsc::channel();
        let handle = start_workspace_search(folder, tree, request, search_id, tx);

        {
            let mut state = self.state.borrow_mut();
            state.active_handle = Some(handle);
            state.cancel_button.set_sensitive(true);
            state.status_label.set_text("Searching…");
        }

        let weak_state = Rc::downgrade(&self.state);
        let source = glib::timeout_add_local(Duration::from_millis(25), move || {
            let Some(state_rc) = weak_state.upgrade() else {
                return glib::ControlFlow::Break;
            };
            let mut finished = false;
            let mut updated_rows = false;
            let mut final_status: Option<String> = None;

            loop {
                match rx.try_recv() {
                    Ok(WorkspaceSearchUpdate::Names {
                        search_id: id,
                        hits,
                        truncated,
                        ..
                    }) => {
                        let mut state = state_rc.borrow_mut();
                        if state.search_id != id {
                            continue;
                        }
                        state.name_hits = hits;
                        state.names_truncated = truncated;
                        updated_rows = true;
                    }
                    Ok(WorkspaceSearchUpdate::ContentBatch {
                        search_id: id,
                        files,
                        progress,
                    }) => {
                        let mut state = state_rc.borrow_mut();
                        if state.search_id != id {
                            continue;
                        }
                        state.content_files.extend(files);
                        state.status_label.set_text(&format!(
                            "Searching… {} files, {} matches",
                            progress.files_read(),
                            progress.matches()
                        ));
                        updated_rows = true;
                    }
                    Ok(WorkspaceSearchUpdate::Finished {
                        search_id: id,
                        progress,
                        cancelled,
                        error,
                    }) => {
                        let mut state = state_rc.borrow_mut();
                        if state.search_id != id {
                            finished = true;
                            break;
                        }
                        state.active_handle = None;
                        state.poll_source = None;
                        state.cancel_button.set_sensitive(false);
                        final_status = Some(format_summary(
                            &state.name_hits,
                            &state.content_files,
                            state.names_truncated,
                            &progress,
                            cancelled,
                            error.as_deref(),
                        ));
                        finished = true;
                        break;
                    }
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        let mut state = state_rc.borrow_mut();
                        state.active_handle = None;
                        state.poll_source = None;
                        state.cancel_button.set_sensitive(false);
                        if final_status.is_none() {
                            final_status = Some("Search worker exited unexpectedly".to_string());
                        }
                        finished = true;
                        break;
                    }
                }
            }

            if updated_rows {
                Self::rebuild_rows(&state_rc);
            }
            if let Some(summary) = final_status {
                state_rc.borrow().status_label.set_text(&summary);
            }
            if finished {
                glib::ControlFlow::Break
            } else {
                glib::ControlFlow::Continue
            }
        });

        self.state.borrow_mut().poll_source = Some(source);
    }

    fn rebuild_rows(state_rc: &Rc<RefCell<SearchPanelState>>) {
        let count = {
            let mut state = state_rc.borrow_mut();
            let mut rows = Vec::new();
            if !state.name_hits.is_empty() {
                let suffix = if state.names_truncated { "+" } else { "" };
                rows.push(SearchRow::SectionHeader(format!(
                    "File Name Matches ({}{suffix})",
                    state.name_hits.len()
                )));
                for hit in &state.name_hits {
                    rows.push(SearchRow::NameHit(hit.clone()));
                }
            }
            if !state.content_files.is_empty() {
                let total_hits: usize = state.content_files.iter().map(|f| f.hits.len()).sum();
                rows.push(SearchRow::SectionHeader(format!(
                    "Content Matches ({total_hits} in {} files)",
                    state.content_files.len()
                )));
                for file in &state.content_files {
                    rows.push(SearchRow::ContentFileHeader {
                        rel_path: file.rel_path.clone(),
                        full_path: file.full_path.clone(),
                        match_count: file.hits.len(),
                        truncated: file.truncated,
                        has_invisible: file.has_invisible,
                    });
                    for hit in &file.hits {
                        rows.push(SearchRow::ContentHit {
                            full_path: file.full_path.clone(),
                            line: hit.line,
                            hit: hit.clone(),
                            source_length: file.source_length,
                            source_sha256: file.source_sha256,
                        });
                    }
                }
            }
            let count = rows.len() as u32;
            state.rows = rows;
            count
        };
        let state = state_rc.borrow();
        if let Some(existing) = state
            .selection
            .model()
            .and_then(|m| m.downcast::<CountModel>().ok())
        {
            existing.set_count(count);
        } else {
            let model = CountModel::new(count);
            state.selection.set_model(Some(&model));
        }
    }

    fn activate_index(weak_state: &std::rc::Weak<RefCell<SearchPanelState>>, pos: u32) {
        let Some(state_rc) = weak_state.upgrade() else {
            return;
        };
        let (target, cb) = {
            let state = state_rc.borrow();
            let Some(row) = state.rows.get(pos as usize) else {
                return;
            };
            let target = match row {
                SearchRow::SectionHeader(_) => return,
                SearchRow::NameHit(hit) => WorkspaceSearchTarget::File(hit.full_path.clone()),
                SearchRow::ContentFileHeader { full_path, .. } => {
                    WorkspaceSearchTarget::File(full_path.clone())
                }
                SearchRow::ContentHit {
                    full_path,
                    hit,
                    source_length,
                    source_sha256,
                    ..
                } => WorkspaceSearchTarget::ContentHit {
                    path: full_path.clone(),
                    span_start: hit.span_start,
                    source_length: *source_length,
                    source_sha256: *source_sha256,
                    anchor: hit.anchor.clone(),
                },
            };
            (target, state.on_open_target.clone())
        };
        if let Some(cb) = cb {
            cb(target);
        }
    }

    pub fn result_count(&self) -> u32 {
        self.state.borrow().rows.len() as u32
    }

    pub fn status_text(&self) -> String {
        self.state.borrow().status_label.text().to_string()
    }

    pub fn is_searching(&self) -> bool {
        self.state.borrow().active_handle.is_some()
    }

    pub fn first_name_hit_index(&self) -> Option<u32> {
        self.state
            .borrow()
            .rows
            .iter()
            .position(|r| matches!(r, SearchRow::NameHit(_)))
            .map(|i| i as u32)
    }

    pub fn first_content_hit_index(&self) -> Option<u32> {
        self.state
            .borrow()
            .rows
            .iter()
            .position(|r| matches!(r, SearchRow::ContentHit { .. }))
            .map(|i| i as u32)
    }

    pub fn activate_row(&self, index: u32) -> bool {
        let state = self.state.borrow();
        if (index as usize) >= state.rows.len() {
            return false;
        }
        state.selection.set_selected(index);
        state
            .list_view
            .scroll_to(index, gtk::ListScrollFlags::NONE, None);
        drop(state);
        Self::activate_index(&Rc::downgrade(&self.state), index);
        true
    }
}

impl Default for WorkspaceSearchPanel {
    fn default() -> Self {
        Self::new()
    }
}

fn format_summary(
    name_hits: &[WorkspaceNameMatch],
    content_files: &[WorkspaceContentFileMatch],
    names_truncated: bool,
    progress: &ContentProgress,
    cancelled: bool,
    error: Option<&str>,
) -> String {
    if let Some(err) = error {
        return err.to_string();
    }
    let name_count = name_hits.len();
    let content_count: usize = content_files.iter().map(|f| f.hits.len()).sum();
    let file_count = content_files.len();
    if cancelled {
        return format!("Search cancelled ({name_count} name, {content_count} content matches)");
    }
    if let Some(reason) = progress.stopped() {
        let why = match reason {
            ContentStopReason::Files => "file limit",
            ContentStopReason::Bytes => "byte limit",
            ContentStopReason::Matches => "match limit",
        };
        return format!(
            "Stopped ({why}): {name_count} name, {content_count} content in {file_count} files"
        );
    }
    if name_count == 0 && content_count == 0 {
        return "No matches found".to_string();
    }
    let trunc = if names_truncated { "+" } else { "" };
    format!("{name_count}{trunc} name, {content_count} content in {file_count} files")
}
