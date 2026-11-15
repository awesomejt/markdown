// FileChooserNative preserves the existing native/portal capability flow.
// Its GTK 4.10 deprecation becomes visible with the renderer v4_12 feature.
#![allow(deprecated)]
//! GTK ownership and presentation. All file I/O and parsing belongs to document.
use gtk::prelude::*;
use gtk::{gio, glib};
use markdown_linux::document::{
    DocumentCapability, DocumentError, DocumentLoader, FolderCapability, FolderError, LoadEvent,
    sanitize_basename,
};
use markdown_linux::recents::{self, RecentKind, RecentStatus};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::{Rc, Weak};
use std::sync::Arc;
use std::time::Duration;

#[derive(Clone, Debug)]
enum RecoveryTarget {
    File(Option<PathBuf>),
    Folder(Option<PathBuf>),
}

#[derive(Clone)]
struct RecoveryBar {
    container: gtk::Box,
    label: gtk::Label,
    action_button: gtk::Button,
    dismiss_button: gtk::Button,
    target: Rc<RefCell<Option<RecoveryTarget>>>,
}

impl RecoveryBar {
    fn new() -> Self {
        let container = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        container.set_margin_start(12);
        container.set_margin_end(12);
        container.set_margin_top(6);
        container.set_margin_bottom(6);
        container.set_visible(false);

        let label = gtk::Label::new(None);
        label.set_hexpand(true);
        label.set_xalign(0.0);
        label.set_wrap(true);

        let action_button = gtk::Button::with_label("Relocate…");
        let dismiss_button = gtk::Button::with_label("Dismiss");

        container.append(&label);
        container.append(&action_button);
        container.append(&dismiss_button);

        Self {
            container,
            label,
            action_button,
            dismiss_button,
            target: Rc::new(RefCell::new(None)),
        }
    }

    fn show(&self, message: &str, button_label: &str, target: RecoveryTarget) {
        self.label.set_text(message);
        self.action_button.set_label(button_label);
        *self.target.borrow_mut() = Some(target);
        self.container.set_visible(true);
    }

    fn hide(&self) {
        *self.target.borrow_mut() = None;
        self.container.set_visible(false);
    }

    fn is_visible(&self) -> bool {
        self.container.is_visible()
    }
}

struct WorkspaceSearchJump {
    path: std::path::PathBuf,
    span_start: u32,
    source_length: u64,
    source_sha256: [u8; 64],
    anchor: Option<Vec<u8>>,
}

fn jump_to_search_hit(
    reader: &crate::reader::Reader,
    document: &markdown_linux_core::CoreDocument,
    jump: &WorkspaceSearchJump,
) {
    let Some(plan) = reader.plan() else {
        return;
    };
    let same_bytes = document.source.len() as u64 == jump.source_length
        && document.source_sha256.as_bytes() == jump.source_sha256.as_slice();
    let target_offset = if same_bytes {
        Some(jump.span_start as u64)
    } else if let Some(anchor) = jump.anchor.as_deref() {
        document
            .resolve_anchor(anchor)
            .ok()
            .flatten()
            .map(|res| res.source_offset)
    } else {
        None
    };
    if let Some(offset) = target_offset {
        let pos = plan.position_at_source_offset(offset, document);
        reader.scroll_to_fragment(pos.fragment_index);
    }
}

struct WindowDocument {
    window: glib::WeakRef<gtk::ApplicationWindow>,
    label: glib::WeakRef<gtk::Label>,
    loader: DocumentLoader,
    active_folder: Option<FolderCapability>,
    workspace_browser: crate::workspace_browser::WorkspaceBrowser,
    recovery_bar: RecoveryBar,
    recents_window: Option<gtk::Window>,
    reader: crate::reader::Reader,
    find_bar: crate::find_bar::FindBar,
    find_gate: Option<markdown_linux_core::FindGate>,
    find_matches: Option<Arc<markdown_linux::render::FindMatches>>,
    find_request_id: u64,
    chooser: Option<gtk::FileChooserNative>,
    reload: gio::SimpleAction,
    poll_source: Option<glib::SourceId>,
    watcher: Option<Rc<markdown_linux::document::DocumentWatcher>>,
    saved_position: Option<markdown_linux::restoration::SavedPosition>,
    pending_search_jump: Option<WorkspaceSearchJump>,
    weak_self: Weak<RefCell<WindowDocument>>,
}
thread_local! { static WINDOWS: RefCell<HashMap<usize, Rc<RefCell<WindowDocument>>>> = RefCell::new(HashMap::new()); }
fn key(window: &gtk::ApplicationWindow) -> usize {
    window.as_ptr() as usize
}
fn state(window: &gtk::ApplicationWindow) -> Option<Rc<RefCell<WindowDocument>>> {
    WINDOWS.with(|all| all.borrow().get(&key(window)).cloned())
}
impl WindowDocument {
    fn status(&self, text: &str) {
        if let Some(label) = self.label.upgrade() {
            label.set_visible(true);
            label.set_text(text);
        }
    }
    fn dismiss_find(&mut self) {
        self.find_request_id = self.find_request_id.wrapping_add(1);
        self.find_bar.hide();
        if let Some(gate) = self.find_gate.take() {
            gate.cancel();
        }
        self.find_matches = None;
        self.reader.set_find_matches(None);
    }
    fn loading(&mut self) {
        self.find_request_id = self.find_request_id.wrapping_add(1);
        if let Some(gate) = self.find_gate.take() {
            gate.cancel();
        }
        self.find_matches = None;
        self.reader.set_find_matches(None);
        self.find_bar.status_label.set_text("");
        self.find_bar.prev_button.set_sensitive(false);
        self.find_bar.next_button.set_sensitive(false);

        self.reload.set_enabled(self.loader.has_file());
        if let Some(doc) = &self.loader.document {
            self.saved_position = self.reader.save_position(doc);
        }
        self.reader.clear();
        self.status("Loading document…");
        self.start_poll();

        if let Some(window) = self.window.upgrade() {
            window.set_title(Some(&format!(
                "{} — Markdown",
                self.loader.display_name().unwrap_or_default()
            )));
        }
    }
    fn refresh_external(&mut self) {
        if let Some(doc) = &self.loader.document {
            self.saved_position = self.reader.save_position(doc);
        }
        self.loader.refresh_external();
        self.start_poll();
    }
    fn arm_watcher(&mut self) {
        let Some(cap) = self.loader.capability() else {
            self.watcher = None;
            return;
        };
        let weak = self.weak_self.clone();
        let watcher = markdown_linux::document::DocumentWatcher::new(cap.path(), move || {
            let weak = weak.clone();
            glib::idle_add_local(move || {
                if let Some(state) = weak.upgrade() {
                    state.borrow_mut().refresh_external();
                }
                glib::ControlFlow::Break
            });
        });
        self.watcher = Some(watcher);
    }
    fn start_poll(&mut self) {
        if self.poll_source.is_none() {
            let weak = self.weak_self.clone();
            self.poll_source = Some(glib::timeout_add_local(
                Duration::from_millis(16),
                move || {
                    let Some(state) = weak.upgrade() else {
                        return glib::ControlFlow::Break;
                    };
                    let mut state_ref = state.borrow_mut();
                    let Some(event) = state_ref.loader.poll() else {
                        return glib::ControlFlow::Continue;
                    };
                    state_ref.poll_source.take();
                    let should_save = match event {
                        LoadEvent::Loaded {
                            document,
                            plan,
                            unchanged,
                            ..
                        } => {
                            state_ref.recovery_bar.hide();
                            if !unchanged {
                                if let Some(cap) = state_ref.loader.capability() {
                                    recents::record_file(cap.path(), &cap.display_name());
                                }
                                state_ref
                                    .reader
                                    .install_document(plan, Some(document.clone()));
                                if let Some(saved) = state_ref.saved_position.take() {
                                    state_ref.reader.restore_position(&saved, &document);
                                }
                                if let Some(jump) = state_ref.pending_search_jump.take()
                                    && state_ref
                                        .loader
                                        .capability()
                                        .is_some_and(|cap| cap.path() == jump.path.as_path())
                                {
                                    jump_to_search_hit(&state_ref.reader, &document, &jump);
                                }
                                state_ref.arm_watcher();
                            }
                            state_ref.status(&format!(
                                "Document loaded · {} bytes · {} headings{}",
                                document.source.len(),
                                document.outline.len(),
                                if unchanged { " (unchanged)" } else { "" }
                            ));
                            if let Some(label) = state_ref.label.upgrade() {
                                label.set_visible(false);
                            }
                            if state_ref.find_bar.is_revealed()
                                && !state_ref.find_bar.entry.text().is_empty()
                            {
                                state_ref.run_find();
                            }
                            !unchanged
                        }
                        LoadEvent::Failed(error) => {
                            state_ref.pending_search_jump = None;
                            if state_ref.loader.document.is_some() {
                                state_ref.status(&format!("{error} · Retaining last version"));
                            } else {
                                state_ref.reader.clear();
                                state_ref.status(&error.to_string());
                            }
                            if let Some((btn_label, target_path)) =
                                recoverable_file_error(error, state_ref.loader.capability())
                            {
                                state_ref.recovery_bar.show(
                                    &error.to_string(),
                                    btn_label,
                                    RecoveryTarget::File(target_path),
                                );
                            }
                            false
                        }
                    };
                    drop(state_ref);
                    if should_save {
                        save_all_restoration();
                    }
                    glib::ControlFlow::Break
                },
            ));
        }
    }
    fn run_find(&mut self) {
        self.find_request_id = self.find_request_id.wrapping_add(1);
        let req_id = self.find_request_id;
        let query = self.find_bar.entry.text().to_string();
        if query.is_empty() {
            if let Some(gate) = self.find_gate.take() {
                gate.cancel();
            }
            self.find_matches = None;
            self.reader.set_find_matches(None);
            self.find_bar.status_label.set_text("");
            self.find_bar.prev_button.set_sensitive(false);
            self.find_bar.next_button.set_sensitive(false);
            return;
        }

        let Some(doc) = self.loader.document.clone() else {
            self.find_bar.status_label.set_text("Not found");
            return;
        };
        let Some(plan) = self.reader.plan() else {
            self.find_bar.status_label.set_text("Not found");
            return;
        };
        let Some(session) = self.loader.session() else {
            self.find_bar.status_label.set_text("Not found");
            return;
        };

        let options = markdown_linux_core::find::CoreFindOptions {
            case_sensitive: self.find_bar.case_button.is_active(),
            whole_word: self.find_bar.word_button.is_active(),
            ..Default::default()
        };

        if let Some(gate) = self.find_gate.take() {
            gate.cancel();
        }
        let gate = markdown_linux_core::FindGate::new();
        self.find_gate = Some(gate.clone());
        let idle_gate = gate.clone();

        let doc_clone = doc.clone();
        let plan_clone = plan.clone();
        let window_key = self.window.upgrade().map(|w| key(&w));
        let Some(window_key) = window_key else {
            return;
        };

        let query_str = query.clone();
        session.find(&query, options, doc.generation, Some(gate), move |result| {
            if idle_gate.is_cancelled() {
                return;
            }
            let mapped = match result {
                Ok(core_result) => {
                    let matches = plan_clone.map_matches(&doc_clone, &core_result);
                    Ok(Arc::new(matches))
                }
                Err(err) => Err(err),
            };
            glib::idle_add_once(move || {
                if idle_gate.is_cancelled() {
                    return;
                }
                WINDOWS.with(|all| {
                    let Some(state) = all.borrow().get(&window_key).cloned() else {
                        return;
                    };
                    let mut state = state.borrow_mut();
                    if state.find_request_id != req_id
                        || !state.find_bar.is_revealed()
                        || state.find_bar.entry.text().as_str() != query_str
                    {
                        return;
                    }
                    match mapped {
                        Ok(matches) => {
                            let count = matches.count();
                            state.find_bar.prev_button.set_sensitive(count > 0);
                            state.find_bar.next_button.set_sensitive(count > 0);
                            let status_text =
                                matches.status(if count > 0 { Some(0) } else { None });
                            state.find_bar.status_label.set_text(&status_text);
                            state.find_matches = Some(matches.clone());
                            state.reader.set_find_matches(Some(matches));
                        }
                        Err(markdown_linux_core::CoreError::Cancelled)
                        | Err(markdown_linux_core::CoreError::Superseded { .. }) => {}
                        Err(markdown_linux_core::CoreError::QueryTooLong) => {
                            state.find_bar.status_label.set_text("Query too long");
                            state.find_bar.prev_button.set_sensitive(false);
                            state.find_bar.next_button.set_sensitive(false);
                            state.find_matches = None;
                            state.reader.set_find_matches(None);
                        }
                        Err(_) => {
                            state.find_bar.status_label.set_text("Search error");
                            state.find_bar.prev_button.set_sensitive(false);
                            state.find_bar.next_button.set_sensitive(false);
                            state.find_matches = None;
                            state.reader.set_find_matches(None);
                        }
                    }
                });
            });
        });
    }
    fn next_find_match(&mut self) {
        let Some(matches) = &self.find_matches else {
            return;
        };
        if matches.is_empty() {
            return;
        }
        let cur = self.reader.current_find_match().unwrap_or(0);
        let next = matches.index_after(cur, false).unwrap_or(0);
        self.reader.select_find_match(next);
        self.find_bar
            .status_label
            .set_text(&matches.status(Some(next)));
    }
    fn prev_find_match(&mut self) {
        let Some(matches) = &self.find_matches else {
            return;
        };
        if matches.is_empty() {
            return;
        }
        let cur = self.reader.current_find_match().unwrap_or(0);
        let prev = matches.index_after(cur, true).unwrap_or(0);
        self.reader.select_find_match(prev);
        self.find_bar
            .status_label
            .set_text(&matches.status(Some(prev)));
    }
    fn open_internal(
        &mut self,
        file: &gio::File,
        restore: Option<markdown_linux::restoration::SavedPosition>,
    ) {
        if let Some(watcher) = self.watcher.take() {
            watcher.cancel();
        }
        self.saved_position = restore;
        let Some(path) = file.path() else {
            self.saved_position = None;
            self.reader.clear();
            self.loader.reject_selection();
            self.stop_polling();
            self.reload.set_enabled(false);
            self.recovery_bar.hide();
            self.status(&DocumentError::NonLocalFile.to_string());
            return;
        };
        let cap = if let Some(folder) = &self.active_folder {
            match folder.open_document(&path) {
                Ok(cap) => cap,
                Err(FolderError::OutsideFolder) => DocumentCapability::from_local_file(path),
                Err(error) => {
                    let folder_path = folder.path().to_path_buf();
                    self.active_folder = None;
                    self.workspace_browser.clear();
                    self.saved_position = None;
                    self.reader.clear();
                    self.stop_polling();
                    self.reload.set_enabled(false);
                    self.status(&error.to_string());
                    if let Some(btn_label) = recoverable_folder_error(error) {
                        self.recovery_bar.show(
                            &error.to_string(),
                            btn_label,
                            RecoveryTarget::Folder(Some(folder_path)),
                        );
                    }
                    return;
                }
            }
        } else {
            DocumentCapability::from_local_file(path)
        };
        match self.loader.open(cap) {
            Ok(()) => {
                self.recovery_bar.hide();
                self.loading();
            }
            Err(error) => {
                self.saved_position = None;
                self.reload.set_enabled(self.loader.has_file());
                self.status(&error.to_string());
            }
        }
    }
    fn open(&mut self, file: &gio::File) {
        self.pending_search_jump = None;
        self.open_internal(file, None);
    }
    fn open_from_workspace(&mut self, path: &std::path::Path) {
        let Some(folder) = &self.active_folder else {
            self.pending_search_jump = None;
            self.workspace_browser.clear();
            self.status(&FolderError::AccessRevoked.to_string());
            return;
        };
        let cap = match folder.open_document(path) {
            Ok(cap) => cap,
            Err(FolderError::OutsideFolder) => {
                self.pending_search_jump = None;
                self.status(&FolderError::OutsideFolder.to_string());
                return;
            }
            Err(error) => {
                let folder_path = folder.path().to_path_buf();
                self.active_folder = None;
                self.workspace_browser.clear();
                self.saved_position = None;
                self.pending_search_jump = None;
                self.reader.clear();
                self.stop_polling();
                self.reload.set_enabled(false);
                self.status(&error.to_string());
                if let Some(btn_label) = recoverable_folder_error(error) {
                    self.recovery_bar.show(
                        &error.to_string(),
                        btn_label,
                        RecoveryTarget::Folder(Some(folder_path)),
                    );
                }
                return;
            }
        };
        match self.loader.open(cap) {
            Ok(()) => {
                self.recovery_bar.hide();
                self.loading();
            }
            Err(error) => {
                self.saved_position = None;
                self.pending_search_jump = None;
                self.reload.set_enabled(self.loader.has_file());
                self.status(&error.to_string());
            }
        }
    }
    fn open_folder(&mut self, folder: &gio::File) {
        let Some(path) = folder.path() else {
            self.status(&FolderError::NonLocalFolder.to_string());
            return;
        };
        match FolderCapability::from_local_folder(path.clone()) {
            Ok(cap) => {
                let name = cap.display_name();
                recents::record_workspace(cap.path(), &name);
                self.active_folder = Some(cap.clone());
                self.recovery_bar.hide();
                if !self.loader.has_file() {
                    self.status(&format!("Workspace folder ready: {name}"));
                    if let Some(window) = self.window.upgrade() {
                        window.set_title(Some(&format!("{name} — Markdown")));
                    }
                }
                self.load_workspace_tree(&cap, false, true);
            }
            Err(error) => {
                self.active_folder = None;
                self.workspace_browser.clear();
                self.status(&error.to_string());
                if let Some(btn_label) = recoverable_folder_error(error) {
                    self.recovery_bar.show(
                        &error.to_string(),
                        btn_label,
                        RecoveryTarget::Folder(Some(path)),
                    );
                }
            }
        }
    }
    fn load_workspace_tree(&self, cap: &FolderCapability, preserve_state: bool, reveal: bool) {
        let name = cap.display_name();
        let serial = self.workspace_browser.next_load_serial();
        self.workspace_browser.set_loading(&name, reveal);
        let cap_clone = cap.clone();
        let cap_for_search = cap.clone();
        let browser = self.workspace_browser.clone();
        let weak = self.weak_self.clone();
        glib::spawn_future_local(async move {
            let result = gio::spawn_blocking(move || {
                markdown_linux::document::WorkspaceTree::enumerate(&cap_clone)
            })
            .await;
            if browser.current_load_serial() != serial {
                return;
            }
            match result {
                Ok(Ok(tree)) => {
                    browser.set_tree(Some(cap_for_search), tree, preserve_state, reveal);
                }
                Ok(Err(error)) => {
                    browser.clear();
                    if let Some(state_rc) = weak.upgrade()
                        && let Ok(mut state) = state_rc.try_borrow_mut()
                    {
                        let folder_path =
                            state.active_folder.as_ref().map(|f| f.path().to_path_buf());
                        state.active_folder = None;
                        state.status(&error.to_string());
                        if let Some(btn_label) = recoverable_folder_error(error) {
                            state.recovery_bar.show(
                                &error.to_string(),
                                btn_label,
                                RecoveryTarget::Folder(folder_path),
                            );
                        }
                    }
                }
                Err(_) => {
                    browser.clear();
                }
            }
        });
    }
    fn start_relocate_chooser(&mut self, target: RecoveryTarget) {
        let Some(window) = self.window.upgrade() else {
            return;
        };
        if let Some(existing) = self.chooser.take() {
            existing.destroy();
        }
        let (title, action, is_folder, old_path) = match target {
            RecoveryTarget::File(old) => (
                "Relocate Document",
                gtk::FileChooserAction::Open,
                false,
                old,
            ),
            RecoveryTarget::Folder(old) => (
                "Relocate Workspace Folder",
                gtk::FileChooserAction::SelectFolder,
                true,
                old,
            ),
        };
        let chooser = gtk::FileChooserNative::new(
            Some(title),
            Some(&window),
            action,
            Some("Open"),
            Some("Cancel"),
        );
        if !is_folder {
            let markdown = gtk::FileFilter::new();
            markdown.set_name(Some("Markdown and text"));
            for pattern in ["*.md", "*.markdown", "*.mdown", "*.mkd", "*.txt"] {
                markdown.add_pattern(pattern);
            }
            chooser.add_filter(&markdown);
            let all = gtk::FileFilter::new();
            all.set_name(Some("All files"));
            all.add_pattern("*");
            chooser.add_filter(&all);
        }
        let weak = self.weak_self.clone();
        chooser.connect_response(move |chooser, response| {
            if let Some(state_rc) = weak.upgrade()
                && let Ok(mut state) = state_rc.try_borrow_mut()
            {
                state.chooser = None;
                if response == gtk::ResponseType::Accept
                    && let Some(file) = chooser.file()
                    && let Some(new_path) = file.path()
                {
                    if let Some(old) = &old_path {
                        let kind = if is_folder {
                            RecentKind::Workspace
                        } else {
                            RecentKind::File
                        };
                        let name = sanitize_basename(&new_path);
                        recents::relocate_entry(kind, old, &new_path, &name);
                    }
                    if is_folder {
                        state.open_folder(&file);
                    } else {
                        state.open(&file);
                    }
                }
            }
            chooser.destroy();
        });
        self.chooser = Some(chooser.clone());
        chooser.show();
    }
    fn open_recents_dialog(&mut self) {
        if let Some(existing) = &self.recents_window {
            existing.present();
            return;
        }
        let Some(parent) = self.window.upgrade() else {
            return;
        };

        let dialog = gtk::Window::builder()
            .title("Recent Files & Workspaces")
            .transient_for(&parent)
            .modal(true)
            .default_width(540)
            .default_height(420)
            .build();

        let root = gtk::Box::new(gtk::Orientation::Vertical, 8);
        root.set_margin_start(12);
        root.set_margin_end(12);
        root.set_margin_top(12);
        root.set_margin_bottom(12);

        let scrolled = gtk::ScrolledWindow::builder()
            .hexpand(true)
            .vexpand(true)
            .build();
        let list_box = gtk::Box::new(gtk::Orientation::Vertical, 12);
        scrolled.set_child(Some(&list_box));
        root.append(&scrolled);

        let footer = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let clear_btn = gtk::Button::with_label("Clear All");
        let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        spacer.set_hexpand(true);
        let close_btn = gtk::Button::with_label("Close");
        footer.append(&clear_btn);
        footer.append(&spacer);
        footer.append(&close_btn);
        root.append(&footer);

        dialog.set_child(Some(&root));

        populate_recents_box(&list_box, &dialog, &self.weak_self);

        let weak_list = list_box.downgrade();
        let weak_dialog = dialog.downgrade();
        let weak_state = self.weak_self.clone();
        clear_btn.connect_clicked(move |_| {
            recents::clear();
            if let (Some(lb), Some(dlg)) = (weak_list.upgrade(), weak_dialog.upgrade()) {
                populate_recents_box(&lb, &dlg, &weak_state);
            }
        });

        let weak_dialog = dialog.downgrade();
        close_btn.connect_clicked(move |_| {
            if let Some(dlg) = weak_dialog.upgrade() {
                dlg.close();
            }
        });

        let key_ctrl = gtk::EventControllerKey::new();
        let weak_dialog = dialog.downgrade();
        key_ctrl.connect_key_pressed(move |_, keyval, _, _| {
            if keyval == gtk::gdk::Key::Escape
                && let Some(dlg) = weak_dialog.upgrade()
            {
                dlg.close();
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        dialog.add_controller(key_ctrl);

        let weak_state = self.weak_self.clone();
        dialog.connect_close_request(move |_| {
            if let Some(state_rc) = weak_state.upgrade()
                && let Ok(mut st) = state_rc.try_borrow_mut()
            {
                st.recents_window = None;
            }
            glib::Propagation::Proceed
        });

        self.recents_window = Some(dialog.clone());
        dialog.present();
    }
    fn stop_polling(&mut self) {
        if let Some(source) = self.poll_source.take() {
            source.remove();
        }
    }
    fn close(&mut self) {
        self.stop_polling();
        if let Some(gate) = self.find_gate.take() {
            gate.cancel();
        }
        if let Some(chooser) = self.chooser.take() {
            chooser.destroy();
        }
        if let Some(win) = self.recents_window.take() {
            win.close();
        }
        if let Some(watcher) = self.watcher.take() {
            watcher.cancel();
        }
        self.reader.clear();
        self.loader.close();
    }
}

fn populate_recents_box(
    container: &gtk::Box,
    dialog: &gtk::Window,
    weak_state: &Weak<RefCell<WindowDocument>>,
) {
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }
    let state = recents::load();
    append_recents_section(
        container,
        "Recent Workspaces",
        &state.workspaces,
        dialog,
        weak_state,
    );
    append_recents_section(container, "Recent Files", &state.files, dialog, weak_state);
}

fn append_recents_section(
    container: &gtk::Box,
    heading: &str,
    entries: &[recents::RecentEntry],
    dialog: &gtk::Window,
    weak_state: &Weak<RefCell<WindowDocument>>,
) {
    let header = gtk::Label::new(Some(heading));
    header.set_xalign(0.0);
    header.add_css_class("heading");
    container.append(&header);

    if entries.is_empty() {
        let empty = gtk::Label::new(Some("None"));
        empty.set_xalign(0.0);
        empty.add_css_class("dim-label");
        container.append(&empty);
        return;
    }

    for entry in entries {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let text_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
        text_box.set_hexpand(true);

        let title = gtk::Label::new(Some(&entry.display_name));
        title.set_xalign(0.0);
        title.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
        let subtitle = gtk::Label::new(Some("Checking…"));
        subtitle.set_xalign(0.0);
        subtitle.add_css_class("dim-label");
        text_box.append(&title);
        text_box.append(&subtitle);
        row.append(&text_box);

        let actions_box = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        row.append(&actions_box);

        let remove_btn = gtk::Button::with_label("Remove");
        let entry_path = entry.path.clone();
        let entry_kind = entry.kind;
        let weak_container = container.downgrade();
        let weak_dlg = dialog.downgrade();
        let weak_st = weak_state.clone();
        remove_btn.connect_clicked(move |_| {
            recents::remove_entry(entry_kind, &entry_path);
            if let (Some(lb), Some(dlg)) = (weak_container.upgrade(), weak_dlg.upgrade()) {
                populate_recents_box(&lb, &dlg, &weak_st);
            }
        });
        row.append(&remove_btn);
        container.append(&row);

        let entry_clone = entry.clone();
        let entry_path = entry.path.clone();
        let entry_kind = entry.kind;
        let weak_sub = subtitle.downgrade();
        let weak_actions = actions_box.downgrade();
        let weak_container = container.downgrade();
        let weak_dlg = dialog.downgrade();
        let weak_st = weak_state.clone();
        glib::spawn_future_local(async move {
            let status = gio::spawn_blocking(move || recents::check_availability(&entry_clone))
                .await
                .unwrap_or(RecentStatus::InvalidTarget);
            let (Some(sub), Some(acts)) = (weak_sub.upgrade(), weak_actions.upgrade()) else {
                return;
            };
            sub.set_text(status.message());
            if status == RecentStatus::Available {
                let open_btn = gtk::Button::with_label("Open");
                let entry_path_open = entry_path.clone();
                let weak_st_open = weak_st.clone();
                let weak_dlg_open = weak_dlg.clone();
                open_btn.connect_clicked(move |_| {
                    if let Some(state_rc) = weak_st_open.upgrade() {
                        let gfile = gio::File::for_path(&entry_path_open);
                        let mut st = state_rc.borrow_mut();
                        match entry_kind {
                            RecentKind::File => st.open(&gfile),
                            RecentKind::Workspace => st.open_folder(&gfile),
                        }
                    }
                    if let Some(dlg) = weak_dlg_open.upgrade() {
                        dlg.close();
                    }
                });
                acts.append(&open_btn);

                let revoke_btn = gtk::Button::with_label("Revoke");
                let entry_path_rev = entry_path.clone();
                let weak_lb_rev = weak_container.clone();
                let weak_dlg_rev = weak_dlg.clone();
                let weak_st_rev = weak_st.clone();
                revoke_btn.connect_clicked(move |_| {
                    recents::revoke_grant(entry_kind, &entry_path_rev);
                    if let (Some(lb), Some(dlg)) = (weak_lb_rev.upgrade(), weak_dlg_rev.upgrade()) {
                        populate_recents_box(&lb, &dlg, &weak_st_rev);
                    }
                });
                acts.append(&revoke_btn);
            } else if status.is_recoverable() {
                let btn_label = match status {
                    RecentStatus::PermissionDenied | RecentStatus::AccessRevoked => "Re-grant…",
                    _ => "Relocate…",
                };
                let recover_btn = gtk::Button::with_label(btn_label);
                let entry_path_rec = entry_path;
                let weak_st_rec = weak_st;
                let weak_dlg_rec = weak_dlg;
                recover_btn.connect_clicked(move |_| {
                    if let Some(dlg) = weak_dlg_rec.upgrade() {
                        dlg.close();
                    }
                    if let Some(state_rc) = weak_st_rec.upgrade() {
                        let target = match entry_kind {
                            RecentKind::File => RecoveryTarget::File(Some(entry_path_rec.clone())),
                            RecentKind::Workspace => {
                                RecoveryTarget::Folder(Some(entry_path_rec.clone()))
                            }
                        };
                        state_rc.borrow_mut().start_relocate_chooser(target);
                    }
                });
                acts.append(&recover_btn);
            }
        });
    }
}

fn recoverable_file_error(
    error: DocumentError,
    cap: Option<&DocumentCapability>,
) -> Option<(&'static str, Option<PathBuf>)> {
    let label = match error {
        DocumentError::NotFound | DocumentError::VolumeUnavailable => "Relocate…",
        DocumentError::PermissionDenied | DocumentError::AccessRevoked => "Re-grant Access…",
        _ => return None,
    };
    Some((label, cap.map(|c| c.path().to_path_buf())))
}

fn recoverable_folder_error(error: FolderError) -> Option<&'static str> {
    match error {
        FolderError::NotFound | FolderError::VolumeUnavailable => Some("Relocate…"),
        FolderError::PermissionDenied | FolderError::AccessRevoked => Some("Re-grant Access…"),
        _ => None,
    }
}
pub fn attach(window: &gtk::ApplicationWindow, label: &gtk::Label) {
    let reader = crate::reader::Reader::new(window);
    let prefs = markdown_linux::preferences::load();
    reader.set_text_scale(prefs.text_scale);
    reader.set_outline_visible(prefs.show_outline_by_default);

    let find_bar = crate::find_bar::FindBar::new();
    let recovery_bar = RecoveryBar::new();
    let workspace_browser = crate::workspace_browser::WorkspaceBrowser::new();

    let workspace_paned = gtk::Paned::new(gtk::Orientation::Horizontal);
    workspace_paned.set_start_child(Some(&workspace_browser.widget));
    workspace_paned.set_resize_start_child(false);
    workspace_paned.set_shrink_start_child(false);
    workspace_paned.set_end_child(Some(&reader.widget));
    workspace_paned.set_position(240);
    workspace_paned.set_hexpand(true);
    workspace_paned.set_vexpand(true);

    let container = gtk::Box::new(gtk::Orientation::Vertical, 0);
    window.set_child(None::<&gtk::Widget>);
    container.append(label);
    container.append(&recovery_bar.container);
    container.append(&find_bar.container);
    container.append(&workspace_paned);
    reader.widget.set_hexpand(true);
    reader.widget.set_vexpand(true);
    reader.clear();
    window.set_child(Some(&container));

    let reload = gio::SimpleAction::new("reload", None);
    reload.set_enabled(false);
    window.add_action(&reload);

    let state = Rc::new_cyclic(|weak| {
        RefCell::new(WindowDocument {
            window: window.downgrade(),
            label: label.downgrade(),
            loader: DocumentLoader::new(),
            active_folder: None,
            workspace_browser: workspace_browser.clone(),
            recovery_bar: recovery_bar.clone(),
            recents_window: None,
            reader: reader.clone(),
            find_bar: find_bar.clone(),
            find_gate: None,
            find_matches: None,
            find_request_id: 0,
            chooser: None,
            reload: reload.clone(),
            poll_source: None,
            watcher: None,
            saved_position: None,
            pending_search_jump: None,
            weak_self: weak.clone(),
        })
    });
    WINDOWS.with(|all| all.borrow_mut().insert(key(window), state.clone()));

    let weak_open = Rc::downgrade(&state);
    let weak_refresh = Rc::downgrade(&state);
    let weak_close = Rc::downgrade(&state);
    workspace_browser.set_callbacks(
        move |path| {
            if let Some(state_rc) = weak_open.upgrade() {
                state_rc.borrow_mut().open_from_workspace(&path);
            }
        },
        move || {
            if let Some(state_rc) = weak_refresh.upgrade() {
                let st = state_rc.borrow();
                if let Some(cap) = &st.active_folder {
                    st.load_workspace_tree(cap, true, false);
                }
            }
        },
        move || {
            if let Some(state_rc) = weak_close.upgrade() {
                let mut st = state_rc.borrow_mut();
                st.active_folder = None;
                st.workspace_browser.clear();
            }
        },
    );

    let toggle_ws = gio::SimpleAction::new("toggle-workspace", None);
    let weak = Rc::downgrade(&state);
    toggle_ws.connect_activate(move |_, _| {
        if let Some(state_rc) = weak.upgrade() {
            let st = state_rc.borrow();
            if st.workspace_browser.has_workspace() {
                let visible = st.workspace_browser.widget.is_visible();
                st.workspace_browser.widget.set_visible(!visible);
            }
        }
    });
    window.add_action(&toggle_ws);

    let weak_target = Rc::downgrade(&state);
    let weak_close_search = Rc::downgrade(&state);
    workspace_browser.search_panel().set_callbacks(
        move |target| {
            let Some(state_rc) = weak_target.upgrade() else {
                return;
            };
            let mut st = state_rc.borrow_mut();
            match target {
                crate::workspace_search_panel::WorkspaceSearchTarget::File(path) => {
                    st.pending_search_jump = None;
                    st.open_from_workspace(&path);
                }
                crate::workspace_search_panel::WorkspaceSearchTarget::ContentHit {
                    path,
                    span_start,
                    source_length,
                    source_sha256,
                    anchor,
                } => {
                    let jump = WorkspaceSearchJump {
                        path: path.clone(),
                        span_start,
                        source_length,
                        source_sha256,
                        anchor,
                    };
                    let already_open = st
                        .loader
                        .capability()
                        .is_some_and(|cap| cap.path() == path.as_path());
                    if already_open && let Some(doc) = st.reader.document() {
                        st.pending_search_jump = None;
                        jump_to_search_hit(&st.reader, &doc, &jump);
                    } else {
                        st.pending_search_jump = Some(jump);
                        st.open_from_workspace(&path);
                    }
                }
            }
        },
        move || {
            if let Some(state_rc) = weak_close_search.upgrade() {
                state_rc.borrow().workspace_browser.show_files();
            }
        },
    );

    let ws_search_act = gio::SimpleAction::new("workspace-search", None);
    let weak = Rc::downgrade(&state);
    ws_search_act.connect_activate(move |_, _| {
        if let Some(state_rc) = weak.upgrade() {
            state_rc.borrow().workspace_browser.show_search();
        }
    });
    window.add_action(&ws_search_act);

    let weak = Rc::downgrade(&state);
    recovery_bar.action_button.connect_clicked(move |_| {
        if let Some(state_rc) = weak.upgrade() {
            let target = state_rc.borrow().recovery_bar.target.borrow().clone();
            if let Some(target) = target {
                state_rc.borrow_mut().start_relocate_chooser(target);
            }
        }
    });

    let weak = Rc::downgrade(&state);
    recovery_bar.dismiss_button.connect_clicked(move |_| {
        if let Some(state_rc) = weak.upgrade() {
            state_rc.borrow().recovery_bar.hide();
        }
    });

    let recents_act = gio::SimpleAction::new("recents", None);
    let weak = Rc::downgrade(&state);
    recents_act.connect_activate(move |_, _| {
        if let Some(state_rc) = weak.upgrade() {
            state_rc.borrow_mut().open_recents_dialog();
        }
    });
    window.add_action(&recents_act);

    let weak = Rc::downgrade(&state);
    reload.connect_activate(move |_, _| {
        if let Some(state) = weak.upgrade() {
            let mut state = state.borrow_mut();
            state.loader.reload();
            state.loading();
        }
    });

    // Window actions for edit, find and zoom
    let copy_act = gio::SimpleAction::new("copy", None);
    let weak = Rc::downgrade(&state);
    let win_weak = window.downgrade();
    copy_act.connect_activate(move |_, _| {
        if let Some(win) = win_weak.upgrade()
            && let Some(focus) = gtk::prelude::GtkWindowExt::focus(&win)
            && let Ok(editable) = focus.downcast::<gtk::Editable>()
        {
            editable.emit_by_name::<()>("copy-clipboard", &[]);
            return;
        }
        if let Some(state) = weak.upgrade() {
            state.borrow().reader.copy_selection();
        }
    });
    window.add_action(&copy_act);

    let select_all_act = gio::SimpleAction::new("select-all", None);
    let weak = Rc::downgrade(&state);
    let win_weak = window.downgrade();
    select_all_act.connect_activate(move |_, _| {
        if let Some(win) = win_weak.upgrade()
            && let Some(focus) = gtk::prelude::GtkWindowExt::focus(&win)
            && let Ok(editable) = focus.downcast::<gtk::Editable>()
        {
            editable.select_region(0, -1);
            return;
        }
        if let Some(state) = weak.upgrade() {
            state.borrow().reader.select_all();
        }
    });
    window.add_action(&select_all_act);

    let find_act = gio::SimpleAction::new("find", None);
    let weak = Rc::downgrade(&state);
    find_act.connect_activate(move |_, _| {
        if let Some(state) = weak.upgrade() {
            let mut state = state.borrow_mut();
            state.find_bar.show();
            if !state.find_bar.entry.text().is_empty() {
                state.run_find();
            }
        }
    });
    window.add_action(&find_act);

    let find_next_act = gio::SimpleAction::new("find-next", None);
    let weak = Rc::downgrade(&state);
    find_next_act.connect_activate(move |_, _| {
        if let Some(state) = weak.upgrade() {
            let mut state = state.borrow_mut();
            if !state.find_bar.is_revealed() {
                state.find_bar.show();
                if !state.find_bar.entry.text().is_empty() {
                    state.run_find();
                }
            } else {
                state.next_find_match();
            }
        }
    });
    window.add_action(&find_next_act);

    let find_prev_act = gio::SimpleAction::new("find-prev", None);
    let weak = Rc::downgrade(&state);
    find_prev_act.connect_activate(move |_, _| {
        if let Some(state) = weak.upgrade() {
            let mut state = state.borrow_mut();
            if !state.find_bar.is_revealed() {
                state.find_bar.show();
                if !state.find_bar.entry.text().is_empty() {
                    state.run_find();
                }
            } else {
                state.prev_find_match();
            }
        }
    });
    window.add_action(&find_prev_act);

    let zoom_in_act = gio::SimpleAction::new("zoom-in", None);
    let weak = Rc::downgrade(&state);
    zoom_in_act.connect_activate(move |_, _| {
        if let Some(state) = weak.upgrade() {
            let state = state.borrow();
            let cur = state.reader.text_scale();
            let next = markdown_linux::preferences::next_scale(cur);
            state.reader.set_text_scale(next);
            let mut prefs = markdown_linux::preferences::load();
            prefs.text_scale = next;
            markdown_linux::preferences::save(&prefs);
        }
    });
    window.add_action(&zoom_in_act);

    let zoom_out_act = gio::SimpleAction::new("zoom-out", None);
    let weak = Rc::downgrade(&state);
    zoom_out_act.connect_activate(move |_, _| {
        if let Some(state) = weak.upgrade() {
            let state = state.borrow();
            let cur = state.reader.text_scale();
            let prev = markdown_linux::preferences::prev_scale(cur);
            state.reader.set_text_scale(prev);
            let mut prefs = markdown_linux::preferences::load();
            prefs.text_scale = prev;
            markdown_linux::preferences::save(&prefs);
        }
    });
    window.add_action(&zoom_out_act);

    let zoom_reset_act = gio::SimpleAction::new("zoom-reset", None);
    let weak = Rc::downgrade(&state);
    zoom_reset_act.connect_activate(move |_, _| {
        if let Some(state) = weak.upgrade() {
            let state = state.borrow();
            let reset = markdown_linux::preferences::reset_scale();
            state.reader.set_text_scale(reset);
            let mut prefs = markdown_linux::preferences::load();
            prefs.text_scale = reset;
            markdown_linux::preferences::save(&prefs);
        }
    });
    window.add_action(&zoom_reset_act);

    // Event handlers for FindBar controls
    let weak = Rc::downgrade(&state);
    find_bar.entry.connect_search_changed(move |_| {
        if let Some(state) = weak.upgrade() {
            state.borrow_mut().run_find();
        }
    });

    let weak = Rc::downgrade(&state);
    find_bar.prev_button.connect_clicked(move |_| {
        if let Some(state) = weak.upgrade() {
            state.borrow_mut().prev_find_match();
        }
    });

    let weak = Rc::downgrade(&state);
    find_bar.next_button.connect_clicked(move |_| {
        if let Some(state) = weak.upgrade() {
            state.borrow_mut().next_find_match();
        }
    });

    let weak = Rc::downgrade(&state);
    find_bar.case_button.connect_toggled(move |_| {
        if let Some(state) = weak.upgrade() {
            state.borrow_mut().run_find();
        }
    });

    let weak = Rc::downgrade(&state);
    find_bar.word_button.connect_toggled(move |_| {
        if let Some(state) = weak.upgrade() {
            state.borrow_mut().run_find();
        }
    });

    let weak = Rc::downgrade(&state);
    find_bar.close_button.connect_clicked(move |_| {
        if let Some(state) = weak.upgrade() {
            state.borrow_mut().dismiss_find();
        }
    });

    let weak = Rc::downgrade(&state);
    find_bar.entry.connect_stop_search(move |_| {
        if let Some(state) = weak.upgrade() {
            state.borrow_mut().dismiss_find();
        }
    });

    // Key events: Escape to dismiss find bar; Return / Shift+Return to cycle matches
    let key_controller = gtk::EventControllerKey::new();
    let weak = Rc::downgrade(&state);
    key_controller.connect_key_pressed(move |_, keyval, _, _| {
        if keyval == gtk::gdk::Key::Escape
            && let Some(state) = weak.upgrade()
        {
            let mut state = state.borrow_mut();
            if state.find_bar.is_revealed() {
                state.dismiss_find();
                return glib::Propagation::Stop;
            }
        }
        glib::Propagation::Proceed
    });
    window.add_controller(key_controller);

    let entry_key = gtk::EventControllerKey::new();
    let weak = Rc::downgrade(&state);
    entry_key.connect_key_pressed(move |_, keyval, _, mods| {
        if keyval == gtk::gdk::Key::Return || keyval == gtk::gdk::Key::KP_Enter {
            let is_shift = mods.contains(gtk::gdk::ModifierType::SHIFT_MASK);
            if let Some(state) = weak.upgrade() {
                let mut state = state.borrow_mut();
                if is_shift {
                    state.prev_find_match();
                } else {
                    state.next_find_match();
                }
                return glib::Propagation::Stop;
            }
        }
        glib::Propagation::Proceed
    });
    find_bar.entry.add_controller(entry_key);

    window.connect_close_request(|window| {
        detach(window);
        glib::Propagation::Proceed
    });
    window.connect_destroy(detach);
}
pub fn save_all_restoration() {
    let mut states = Vec::new();
    WINDOWS.with(|all| {
        for doc_rc in all.borrow().values() {
            if let Ok(doc) = doc_rc.try_borrow()
                && let Some(cap) = doc.loader.capability()
                && let Some(core_doc) = &doc.loader.document
            {
                let path_str = cap.path().to_string_lossy().to_string();
                let saved = doc.reader.save_position(core_doc);
                let state =
                    markdown_linux::restoration::WindowRestorationState::from_saved_position(
                        path_str,
                        saved.as_ref(),
                        doc.reader.text_scale(),
                        doc.reader.is_outline_visible(),
                    );
                states.push(state);
            }
        }
    });
    if !states.is_empty() {
        markdown_linux::restoration::save(&states);
    }
}

pub fn detach(window: &gtk::ApplicationWindow) {
    let k = key(window);
    let count = WINDOWS.with(|all| all.borrow().len());
    if count == 1 {
        // Last window closing: save its position right before close so it is restored on relaunch.
        save_all_restoration();
    }
    WINDOWS.with(|all| {
        if let Some(state) = all.borrow_mut().remove(&k) {
            state.borrow_mut().close();
        }
    });
    if count > 1 {
        save_all_restoration();
    }
}

pub fn shutdown() {
    save_all_restoration();
    let states = WINDOWS.with(|all| std::mem::take(&mut *all.borrow_mut()));
    for state in states.into_values() {
        state.borrow_mut().close();
    }
}

pub fn open_restored_state(
    window: &gtk::ApplicationWindow,
    rest: &markdown_linux::restoration::WindowRestorationState,
) {
    if let Some(state_rc) = state(window) {
        let mut st = state_rc.borrow_mut();
        st.reader.set_text_scale(rest.text_scale);
        st.reader.set_outline_visible(rest.outline_shown);
        let file = gio::File::for_path(Path::new(&rest.file_path));
        let pos = rest.to_saved_position();
        st.open_internal(&file, pos);
    }
}
pub fn choose(app: &gtk::Application) {
    let window = app
        .active_window()
        .and_then(|w| w.downcast::<gtk::ApplicationWindow>().ok())
        .unwrap_or_else(|| crate::new_window(app));
    let Some(state) = state(&window) else {
        return;
    };
    if let Some(chooser) = &state.borrow().chooser {
        chooser.show();
        return;
    }
    let chooser = gtk::FileChooserNative::new(
        Some("Open Document"),
        Some(&window),
        gtk::FileChooserAction::Open,
        Some("Open"),
        Some("Cancel"),
    );
    let markdown = gtk::FileFilter::new();
    markdown.set_name(Some("Markdown and text"));
    for pattern in ["*.md", "*.markdown", "*.mdown", "*.mkd", "*.txt"] {
        markdown.add_pattern(pattern);
    }
    chooser.add_filter(&markdown);
    let all = gtk::FileFilter::new();
    all.set_name(Some("All files"));
    all.add_pattern("*");
    chooser.add_filter(&all);
    let weak = Rc::downgrade(&state);
    chooser.connect_response(move |chooser, response| {
        if let Some(state) = weak.upgrade()
            && let Ok(mut state) = state.try_borrow_mut()
        {
            state.chooser = None;
            if response == gtk::ResponseType::Accept
                && let Some(file) = chooser.file()
            {
                state.open(&file);
            }
        }
        chooser.destroy();
    });
    state.borrow_mut().chooser = Some(chooser.clone());
    chooser.show();
}
pub fn choose_folder(app: &gtk::Application) {
    let window = app
        .active_window()
        .and_then(|w| w.downcast::<gtk::ApplicationWindow>().ok())
        .unwrap_or_else(|| crate::new_window(app));
    let Some(state) = state(&window) else {
        return;
    };
    if let Some(chooser) = &state.borrow().chooser {
        chooser.show();
        return;
    }
    let chooser = gtk::FileChooserNative::new(
        Some("Open Folder"),
        Some(&window),
        gtk::FileChooserAction::SelectFolder,
        Some("Open"),
        Some("Cancel"),
    );
    let weak = Rc::downgrade(&state);
    chooser.connect_response(move |chooser, response| {
        if let Some(state) = weak.upgrade()
            && let Ok(mut state) = state.try_borrow_mut()
        {
            state.chooser = None;
            if response == gtk::ResponseType::Accept
                && let Some(folder) = chooser.file()
            {
                state.open_folder(&folder);
            }
        }
        chooser.destroy();
    });
    state.borrow_mut().chooser = Some(chooser.clone());
    chooser.show();
}
pub fn deliver(app: &gtk::Application, files: &[gio::File]) {
    for (index, file) in files.iter().enumerate() {
        let empty = if index == 0 {
            app.active_window()
                .and_then(|w| w.downcast::<gtk::ApplicationWindow>().ok())
                .filter(|w| state(w).is_some_and(|s| !s.borrow().loader.has_file()))
        } else {
            None
        };
        let window = empty.unwrap_or_else(|| crate::new_window(app));
        if let Some(state) = state(&window) {
            state.borrow_mut().open(file);
        }
        window.present();
    }
}

// Development smoke observations stay in the host; no file path is logged.
pub fn source(window: &gtk::ApplicationWindow) -> Option<Vec<u8>> {
    state(window).and_then(|s| {
        s.borrow()
            .loader
            .document
            .as_ref()
            .map(|d| d.source.clone())
    })
}
pub fn loaded_sources(app: &gtk::Application) -> Vec<Vec<u8>> {
    let mut sources: Vec<_> = app
        .windows()
        .into_iter()
        .filter_map(|w| w.downcast::<gtk::ApplicationWindow>().ok())
        .filter_map(|w| source(&w))
        .collect();
    sources.sort();
    // Match the independently specified lexicographic fixture order in smoke.rs.
    sources
}
pub fn status(window: &gtk::ApplicationWindow) -> String {
    state(window)
        .and_then(|s| s.borrow().label.upgrade().map(|l| l.text().to_string()))
        .unwrap_or_default()
}
pub fn reload_enabled(window: &gtk::ApplicationWindow) -> bool {
    state(window).is_some_and(|s| s.borrow().reload.is_enabled())
}
pub fn chooser_visible(window: &gtk::ApplicationWindow) -> bool {
    state(window).is_some_and(|s| s.borrow().chooser.as_ref().is_some_and(|c| c.is_visible()))
}
fn fallback_chooser(window: &gtk::ApplicationWindow) -> Option<gtk::FileChooserDialog> {
    gtk::Window::list_toplevels()
        .into_iter()
        .filter_map(|w| w.downcast::<gtk::FileChooserDialog>().ok())
        .find(|d| d.transient_for().as_ref() == Some(window.upcast_ref()) && d.is_visible())
}
pub fn chooser_ready(window: &gtk::ApplicationWindow) -> bool {
    fallback_chooser(window).is_some_and(|dialog| dialog.is_mapped())
}
pub fn chooser_cancel(window: &gtk::ApplicationWindow) -> Result<(), String> {
    let dialog = fallback_chooser(window).ok_or("GTK native fallback chooser for Cancel")?;
    dialog.response(gtk::ResponseType::Cancel);
    Ok(())
}
pub fn chooser_select(window: &gtk::ApplicationWindow, file: &gio::File) -> Result<(), String> {
    let chooser = fallback_chooser(window).ok_or("GTK native fallback chooser")?;
    chooser
        .set_file(file)
        .map_err(|_| "chooser selected file".to_string())
}
pub fn chooser_selected(window: &gtk::ApplicationWindow, file: &gio::File) -> bool {
    fallback_chooser(window)
        .and_then(|c| c.file())
        .is_some_and(|selected| selected.equal(file))
}
pub fn chooser_accept(window: &gtk::ApplicationWindow) -> Result<(), String> {
    let chooser = fallback_chooser(window).ok_or("GTK native fallback chooser")?;
    chooser.response(gtk::ResponseType::Accept);
    Ok(())
}
pub fn open_for_smoke(window: &gtk::ApplicationWindow, file: &gio::File) {
    if let Some(state) = state(window) {
        state.borrow_mut().open(file);
    }
}

// Observe lifecycle ownership in the native regression smoke.
pub fn polling(window: &gtk::ApplicationWindow) -> bool {
    state(window).is_some_and(|s| s.borrow().poll_source.is_some())
}
pub fn attached(window: &gtk::ApplicationWindow) -> bool {
    state(window).is_some()
}

pub fn reader(window: &gtk::ApplicationWindow) -> Option<crate::reader::Reader> {
    state(window).map(|s| s.borrow().reader.clone())
}

pub fn find_bar(window: &gtk::ApplicationWindow) -> Option<crate::find_bar::FindBar> {
    state(window).map(|s| s.borrow().find_bar.clone())
}

pub fn text_scale(window: &gtk::ApplicationWindow) -> Option<f64> {
    state(window).map(|s| s.borrow().reader.text_scale())
}

pub fn apply_text_scale_to_all_windows(app: &gtk::Application, scale: f64) {
    for win in app.windows() {
        if let Some(app_win) = win.downcast_ref::<gtk::ApplicationWindow>()
            && let Some(st) = state(app_win)
        {
            st.borrow().reader.set_text_scale(scale);
        }
    }
}

pub fn open_folder_for_smoke(window: &gtk::ApplicationWindow, folder: &gio::File) {
    if let Some(state) = state(window) {
        state.borrow_mut().open_folder(folder);
    }
}

pub fn active_folder_name(window: &gtk::ApplicationWindow) -> Option<String> {
    state(window).and_then(|s| {
        s.borrow()
            .active_folder
            .as_ref()
            .map(FolderCapability::display_name)
    })
}

pub fn recovery_visible(window: &gtk::ApplicationWindow) -> bool {
    state(window).is_some_and(|s| s.borrow().recovery_bar.is_visible())
}

pub fn dismiss_recovery_for_smoke(window: &gtk::ApplicationWindow) {
    if let Some(state) = state(window) {
        state.borrow().recovery_bar.hide();
    }
}

pub fn recents_dialog_visible(window: &gtk::ApplicationWindow) -> bool {
    state(window).is_some_and(|s| {
        s.borrow()
            .recents_window
            .as_ref()
            .is_some_and(|w| w.is_visible())
    })
}

pub fn close_recents_dialog_for_smoke(window: &gtk::ApplicationWindow) {
    if let Some(state) = state(window)
        && let Some(win) = state.borrow_mut().recents_window.take()
    {
        win.close();
    }
}

pub fn workspace_browser(
    window: &gtk::ApplicationWindow,
) -> Option<crate::workspace_browser::WorkspaceBrowser> {
    state(window).map(|s| s.borrow().workspace_browser.clone())
}
