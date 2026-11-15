//! Native GTK 4 virtualized workspace browser and keyboard navigation (LNX-002B).

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use gtk::{gdk, gio, glib, pango, prelude::*};

use crate::count_model::CountModel;
use crate::workspace_search_panel::WorkspaceSearchPanel;
use markdown_linux::document::{FolderCapability, WorkspaceBrowserModel, WorkspaceTree};

struct BrowserState {
    model: Option<WorkspaceBrowserModel>,
    selection: gtk::SingleSelection,
    list_view: gtk::ListView,
    folder_label: gtk::Label,
    status_label: gtk::Label,
    on_open_file: Option<Rc<dyn Fn(PathBuf)>>,
    on_refresh: Option<Rc<dyn Fn()>>,
    on_close: Option<Rc<dyn Fn()>>,
}

#[derive(Clone)]
pub struct WorkspaceBrowser {
    pub widget: gtk::Box,
    state: Rc<RefCell<BrowserState>>,
    all_files_btn: gtk::ToggleButton,
    hidden_btn: gtk::ToggleButton,
    search_btn: gtk::ToggleButton,
    stack: gtk::Stack,
    search_panel: WorkspaceSearchPanel,
    loading_serial: Rc<Cell<u64>>,
}

impl WorkspaceBrowser {
    pub fn new() -> Self {
        let widget = gtk::Box::new(gtk::Orientation::Vertical, 4);
        widget.set_widget_name("workspace-sidebar");
        widget.set_width_request(220);
        widget.set_visible(false);

        let header = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        header.set_margin_start(8);
        header.set_margin_end(6);
        header.set_margin_top(6);

        let folder_label = gtk::Label::new(Some("Workspace"));
        folder_label.set_xalign(0.0);
        folder_label.set_hexpand(true);
        folder_label.set_ellipsize(pango::EllipsizeMode::End);
        folder_label.add_css_class("heading");

        let search_btn = gtk::ToggleButton::with_label("🔍");
        search_btn.set_tooltip_text(Some("Search workspace (Ctrl+Shift+F)"));

        let all_files_btn = gtk::ToggleButton::with_label("All");
        all_files_btn.set_tooltip_text(Some("Show all files instead of Markdown only"));

        let hidden_btn = gtk::ToggleButton::with_label(".*");
        hidden_btn.set_tooltip_text(Some("Show hidden files and folders"));

        let refresh_btn = gtk::Button::with_label("↻");
        refresh_btn.set_tooltip_text(Some("Refresh workspace folder"));

        let close_btn = gtk::Button::with_label("×");
        close_btn.set_tooltip_text(Some("Close workspace folder"));

        header.append(&folder_label);
        header.append(&search_btn);
        header.append(&all_files_btn);
        header.append(&hidden_btn);
        header.append(&refresh_btn);
        header.append(&close_btn);

        let status_label = gtk::Label::new(None);
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
        list_view.set_widget_name("workspace-browser");
        list_view.set_single_click_activate(false);

        let scrolled = gtk::ScrolledWindow::builder()
            .hexpand(true)
            .vexpand(true)
            .min_content_width(180)
            .child(&list_view)
            .build();

        let files_box = gtk::Box::new(gtk::Orientation::Vertical, 4);
        files_box.append(&status_label);
        files_box.append(&scrolled);

        let search_panel = WorkspaceSearchPanel::new();

        let stack = gtk::Stack::new();
        stack.set_hexpand(true);
        stack.set_vexpand(true);
        stack.add_named(&files_box, Some("files"));
        stack.add_named(&search_panel.widget, Some("search"));
        stack.set_visible_child_name("files");

        widget.append(&header);
        widget.append(&stack);

        let state = Rc::new(RefCell::new(BrowserState {
            model: None,
            selection: selection.clone(),
            list_view: list_view.clone(),
            folder_label,
            status_label,
            on_open_file: None,
            on_refresh: None,
            on_close: None,
        }));

        factory.connect_setup(|_, item| {
            let Some(item) = item.downcast_ref::<gtk::ListItem>() else {
                return;
            };
            let row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
            row.set_margin_start(6);
            row.set_margin_end(6);
            row.set_margin_top(3);
            row.set_margin_bottom(3);
            row.set_accessible_role(gtk::AccessibleRole::TreeItem);

            let indent = gtk::Box::new(gtk::Orientation::Horizontal, 0);
            let icon = gtk::Label::new(None);
            icon.set_width_chars(2);
            let name = gtk::Label::new(None);
            name.set_xalign(0.0);
            name.set_hexpand(true);
            name.set_ellipsize(pango::EllipsizeMode::End);
            let warn = gtk::Label::new(Some("⚠"));
            warn.set_tooltip_text(Some("Filename contains invisible characters"));
            warn.set_visible(false);

            row.append(&indent);
            row.append(&icon);
            row.append(&name);
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
            let Some(model) = &state.model else {
                return;
            };
            let Some((entry, expanded)) = model.visible_entry(item.position() as usize) else {
                return;
            };
            let Some(row) = item.child().and_downcast::<gtk::Box>() else {
                return;
            };
            let indent = row.first_child().and_downcast::<gtk::Box>().unwrap();
            let icon = indent.next_sibling().and_downcast::<gtk::Label>().unwrap();
            let name = icon.next_sibling().and_downcast::<gtk::Label>().unwrap();
            let warn = name.next_sibling().and_downcast::<gtk::Label>().unwrap();

            indent.set_width_request((entry.depth.min(24) as i32) * 14);
            if entry.is_dir {
                icon.set_text(if expanded { "▾" } else { "▸" });
                row.update_state(&[gtk::accessible::State::Expanded(Some(expanded))]);
            } else {
                icon.set_text("·");
                row.reset_state(gtk::AccessibleState::Expanded);
            }
            name.set_text(&entry.name);
            warn.set_visible(entry.has_invisible);

            row.update_property(&[gtk::accessible::Property::Level((entry.depth + 1) as i32)]);
            let acc_label = if entry.has_invisible {
                format!("{} (contains invisible characters)", entry.name)
            } else {
                entry.name.clone()
            };
            row.update_property(&[gtk::accessible::Property::Label(&acc_label)]);
        });

        let weak_state = Rc::downgrade(&state);
        list_view.connect_activate(move |_, pos| {
            Self::activate_index(&weak_state, pos);
        });

        let key_ctrl = gtk::EventControllerKey::new();
        let weak_state = Rc::downgrade(&state);
        key_ctrl.connect_key_pressed(move |_, keyval, _, _| Self::handle_key(&weak_state, keyval));
        list_view.add_controller(key_ctrl);

        let weak_state = Rc::downgrade(&state);
        let search_clone = search_panel.clone();
        let hidden_clone = hidden_btn.clone();
        all_files_btn.connect_toggled(move |btn| {
            if let Some(state_rc) = weak_state.upgrade() {
                let mut state = state_rc.borrow_mut();
                let md_only = !btn.is_active();
                if let Some(model) = &mut state.model {
                    model.set_markdown_only(md_only);
                }
                drop(state);
                Self::sync_list_model(&state_rc, None);
            }
            search_clone.set_workspace_filters(hidden_clone.is_active(), btn.is_active());
        });

        let weak_state = Rc::downgrade(&state);
        let search_clone = search_panel.clone();
        let all_clone = all_files_btn.clone();
        hidden_btn.connect_toggled(move |btn| {
            if let Some(state_rc) = weak_state.upgrade() {
                let mut state = state_rc.borrow_mut();
                let show = btn.is_active();
                if let Some(model) = &mut state.model {
                    model.set_show_hidden(show);
                }
                drop(state);
                Self::sync_list_model(&state_rc, None);
            }
            search_clone.set_workspace_filters(btn.is_active(), all_clone.is_active());
        });

        let weak_state = Rc::downgrade(&state);
        refresh_btn.connect_clicked(move |_| {
            if let Some(state_rc) = weak_state.upgrade() {
                let cb = state_rc.borrow().on_refresh.clone();
                if let Some(cb) = cb {
                    cb();
                }
            }
        });

        let weak_state = Rc::downgrade(&state);
        close_btn.connect_clicked(move |_| {
            if let Some(state_rc) = weak_state.upgrade() {
                let cb = state_rc.borrow().on_close.clone();
                if let Some(cb) = cb {
                    cb();
                }
            }
        });

        let stack_clone = stack.clone();
        let search_clone = search_panel.clone();
        search_btn.connect_toggled(move |btn| {
            if btn.is_active() {
                stack_clone.set_visible_child_name("search");
                search_clone.focus_entry();
            } else {
                stack_clone.set_visible_child_name("files");
            }
        });

        Self {
            widget,
            state,
            all_files_btn,
            hidden_btn,
            search_btn,
            stack,
            search_panel,
            loading_serial: Rc::new(Cell::new(0)),
        }
    }

    pub fn set_callbacks(
        &self,
        on_open_file: impl Fn(PathBuf) + 'static,
        on_refresh: impl Fn() + 'static,
        on_close: impl Fn() + 'static,
    ) {
        let mut state = self.state.borrow_mut();
        state.on_open_file = Some(Rc::new(on_open_file));
        state.on_refresh = Some(Rc::new(on_refresh));
        state.on_close = Some(Rc::new(on_close));
    }

    pub fn next_load_serial(&self) -> u64 {
        let next = self.loading_serial.get().wrapping_add(1);
        self.loading_serial.set(next);
        next
    }

    pub fn current_load_serial(&self) -> u64 {
        self.loading_serial.get()
    }

    pub fn set_loading(&self, folder_name: &str, reveal: bool) {
        let state = self.state.borrow();
        state.folder_label.set_text(folder_name);
        state.status_label.set_text("Loading folder…");
        if reveal {
            self.widget.set_visible(true);
        }
    }

    pub fn set_tree(
        &self,
        folder: Option<FolderCapability>,
        tree: WorkspaceTree,
        preserve_state: bool,
        reveal: bool,
    ) {
        let tree_arc = Arc::new(tree.clone());
        let mut model = WorkspaceBrowserModel::new(tree);
        model.set_markdown_only(!self.all_files_btn.is_active());
        model.set_show_hidden(self.hidden_btn.is_active());
        let restored_sel = {
            let mut state = self.state.borrow_mut();
            let mut sel_rel = None;
            if preserve_state && let Some(prev) = &state.model {
                let prev_expanded: Vec<String> =
                    prev.expanded_paths().map(str::to_string).collect();
                let prev_sel = state.selection.selected();
                if prev_sel != gtk::INVALID_LIST_POSITION {
                    sel_rel = prev
                        .visible_entry(prev_sel as usize)
                        .map(|(e, _)| e.rel_path.clone());
                }
                model.restore_expanded(prev_expanded.iter().map(String::as_str));
            }
            let idx = sel_rel
                .as_deref()
                .and_then(|p| model.row_index_for_rel_path(p))
                .map(|i| i as u32);
            state.folder_label.set_text(&model.tree.root_name);
            state.model = Some(model);
            idx
        };
        Self::sync_list_model(&self.state, restored_sel);
        self.search_panel
            .set_workspace_filters(self.hidden_btn.is_active(), self.all_files_btn.is_active());
        self.search_panel.set_workspace(folder, Some(tree_arc));
        if reveal {
            self.widget.set_visible(true);
        }
    }

    pub fn clear(&self) {
        self.next_load_serial();
        self.search_panel.clear();
        self.search_btn.set_active(false);
        self.stack.set_visible_child_name("files");
        {
            let mut state = self.state.borrow_mut();
            state.model = None;
            state.selection.set_model(None::<&gio::ListModel>);
            state.folder_label.set_text("Workspace");
            state.status_label.set_text("");
        }
        self.widget.set_visible(false);
    }

    pub fn search_panel(&self) -> &WorkspaceSearchPanel {
        &self.search_panel
    }

    pub fn show_search(&self) {
        self.widget.set_visible(true);
        self.search_btn.set_active(true);
        self.stack.set_visible_child_name("search");
        self.search_panel.focus_entry();
    }

    pub fn show_files(&self) {
        self.search_btn.set_active(false);
        self.stack.set_visible_child_name("files");
    }

    pub fn is_search_visible(&self) -> bool {
        self.stack.visible_child_name().as_deref() == Some("search")
    }

    pub fn has_workspace(&self) -> bool {
        self.state.borrow().model.is_some()
    }

    pub fn visible_count(&self) -> u32 {
        self.state
            .borrow()
            .model
            .as_ref()
            .map_or(0, |m| m.visible_rows().len() as u32)
    }

    pub fn selected_index(&self) -> Option<u32> {
        let pos = self.state.borrow().selection.selected();
        (pos != gtk::INVALID_LIST_POSITION).then_some(pos)
    }

    pub fn select_index(&self, index: u32) {
        let state = self.state.borrow();
        state.selection.set_selected(index);
        state
            .list_view
            .scroll_to(index, gtk::ListScrollFlags::NONE, None);
    }

    pub fn activate_row_for_smoke(&self, index: u32) {
        let weak = Rc::downgrade(&self.state);
        Self::activate_index(&weak, index);
    }

    pub fn send_key_for_smoke(&self, keyval: gdk::Key) -> bool {
        let weak = Rc::downgrade(&self.state);
        Self::handle_key(&weak, keyval) == glib::Propagation::Stop
    }

    pub fn focus_list(&self) {
        self.state.borrow().list_view.grab_focus();
    }

    fn sync_list_model(state_rc: &Rc<RefCell<BrowserState>>, keep_selected: Option<u32>) {
        let (count, status) = {
            let state = state_rc.borrow();
            let Some(model) = &state.model else {
                state.selection.set_model(None::<&gio::ListModel>);
                state.status_label.set_text("");
                return;
            };
            let count = model.visible_rows().len() as u32;
            let mut status = if count == 1 {
                "1 item".to_string()
            } else {
                format!("{count} items")
            };
            if model.tree.truncated {
                status.push_str(" (truncated)");
            }
            if model.tree.skipped_invalid > 0 {
                status.push_str(&format!(", {} skipped", model.tree.skipped_invalid));
            }
            (count, status)
        };

        let list_model = CountModel::new(count);
        let state = state_rc.borrow();
        state.status_label.set_text(&status);
        state.selection.set_model(Some(&list_model));
        if let Some(pos) = keep_selected
            && pos < count
        {
            state.selection.set_selected(pos);
        }
    }

    fn activate_index(weak_state: &std::rc::Weak<RefCell<BrowserState>>, pos: u32) {
        let Some(state_rc) = weak_state.upgrade() else {
            return;
        };
        let action = {
            let mut state = state_rc.borrow_mut();
            let Some(model) = &mut state.model else {
                return;
            };
            let Some((entry, _)) = model.visible_entry(pos as usize) else {
                return;
            };
            if entry.is_dir {
                model.toggle_expand(pos as usize);
                None
            } else {
                Some((entry.full_path().to_path_buf(), state.on_open_file.clone()))
            }
        };
        match action {
            None => Self::sync_list_model(&state_rc, Some(pos)),
            Some((path, Some(cb))) => cb(path),
            Some((_, None)) => {}
        }
    }

    fn handle_key(
        weak_state: &std::rc::Weak<RefCell<BrowserState>>,
        keyval: gdk::Key,
    ) -> glib::Propagation {
        let Some(state_rc) = weak_state.upgrade() else {
            return glib::Propagation::Proceed;
        };
        let pos = {
            let state = state_rc.borrow();
            let p = state.selection.selected();
            if p == gtk::INVALID_LIST_POSITION {
                return glib::Propagation::Proceed;
            }
            p
        };

        match keyval {
            gdk::Key::Right | gdk::Key::KP_Right => {
                let (changed, next_sel) = {
                    let mut state = state_rc.borrow_mut();
                    let Some(model) = &mut state.model else {
                        return glib::Propagation::Proceed;
                    };
                    let Some((entry, expanded)) = model.visible_entry(pos as usize) else {
                        return glib::Propagation::Proceed;
                    };
                    if !entry.is_dir {
                        return glib::Propagation::Proceed;
                    }
                    if !expanded {
                        model.expand_row(pos as usize);
                        (true, Some(pos))
                    } else {
                        let next = pos + 1;
                        if (next as usize) < model.visible_rows().len() {
                            (false, Some(next))
                        } else {
                            (false, None)
                        }
                    }
                };
                if changed {
                    Self::sync_list_model(&state_rc, next_sel);
                } else if let Some(next) = next_sel {
                    let state = state_rc.borrow();
                    state.selection.set_selected(next);
                    state
                        .list_view
                        .scroll_to(next, gtk::ListScrollFlags::NONE, None);
                }
                glib::Propagation::Stop
            }
            gdk::Key::Left | gdk::Key::KP_Left => {
                let target = {
                    let mut state = state_rc.borrow_mut();
                    let Some(model) = &mut state.model else {
                        return glib::Propagation::Proceed;
                    };
                    model.collapse_or_parent(pos as usize)
                };
                if let Some(target_idx) = target {
                    Self::sync_list_model(&state_rc, Some(target_idx as u32));
                    let state = state_rc.borrow();
                    state
                        .list_view
                        .scroll_to(target_idx as u32, gtk::ListScrollFlags::NONE, None);
                    glib::Propagation::Stop
                } else {
                    glib::Propagation::Proceed
                }
            }
            _ => glib::Propagation::Proceed,
        }
    }
}
