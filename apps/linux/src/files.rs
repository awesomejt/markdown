// FileChooserNative preserves the existing native/portal capability flow.
// Its GTK 4.10 deprecation becomes visible with the renderer v4_12 feature.
#![allow(deprecated)]
//! GTK ownership and presentation. All file I/O and parsing belongs to document.
use gtk::prelude::*;
use gtk::{gio, glib};
use markdown_linux::document::{DocumentCapability, DocumentError, DocumentLoader, LoadEvent};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::{Rc, Weak};
use std::sync::Arc;
use std::time::Duration;

struct WindowDocument {
    window: glib::WeakRef<gtk::ApplicationWindow>,
    label: glib::WeakRef<gtk::Label>,
    loader: DocumentLoader,
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
    fn save_restoration(&self) {
        let Some(cap) = self.loader.capability() else {
            return;
        };
        let Some(doc) = &self.loader.document else {
            return;
        };
        let pos = self.reader.save_position(doc);
        let state = markdown_linux::restoration::WindowRestorationState::from_saved_position(
            cap.path().to_string_lossy().to_string(),
            pos.as_ref(),
            self.reader.text_scale(),
            true,
        );
        markdown_linux::restoration::save(&[state]);
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
                    let mut state = state.borrow_mut();
                    let Some(event) = state.loader.poll() else {
                        return glib::ControlFlow::Continue;
                    };
                    state.poll_source.take();
                    match event {
                        LoadEvent::Loaded {
                            document,
                            plan,
                            unchanged,
                            ..
                        } => {
                            state.reader.install_document(plan, Some(document.clone()));
                            if let Some(saved) = state.saved_position.take() {
                                state.reader.restore_position(&saved, &document);
                            }
                            state.arm_watcher();
                            state.save_restoration();
                            state.status(&format!(
                                "Document loaded · {} bytes · {} headings{}",
                                document.source.len(),
                                document.outline.len(),
                                if unchanged { " (unchanged)" } else { "" }
                            ));
                            if let Some(label) = state.label.upgrade() {
                                label.set_visible(false);
                            }
                            if state.find_bar.is_revealed()
                                && !state.find_bar.entry.text().is_empty()
                            {
                                state.run_find();
                            }
                        }
                        LoadEvent::Failed(error) => {
                            if state.loader.document.is_some() {
                                state.status(&format!("{error} · Retaining last version"));
                            } else {
                                state.reader.clear();
                                state.status(&error.to_string());
                            }
                        }
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
    fn open(&mut self, file: &gio::File) {
        let Some(path) = file.path() else {
            self.reader.clear();
            self.loader.reject_selection();
            self.stop_polling();
            self.reload.set_enabled(false);
            self.status(&DocumentError::NonLocalFile.to_string());
            return;
        };
        match self.loader.open(DocumentCapability::from_local_file(path)) {
            Ok(()) => self.loading(),
            Err(error) => {
                self.reload.set_enabled(self.loader.has_file());
                self.status(&error.to_string());
            }
        }
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
        if let Some(watcher) = self.watcher.take() {
            watcher.cancel();
        }
        self.reader.clear();
        self.loader.close();
    }
}
pub fn attach(window: &gtk::ApplicationWindow, label: &gtk::Label) {
    let reader = crate::reader::Reader::new(window);
    let prefs = markdown_linux::preferences::load();
    reader.set_text_scale(prefs.text_scale);

    let find_bar = crate::find_bar::FindBar::new();

    let container = gtk::Box::new(gtk::Orientation::Vertical, 0);
    window.set_child(None::<&gtk::Widget>);
    container.append(label);
    container.append(&find_bar.container);
    container.append(&reader.widget);
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
            weak_self: weak.clone(),
        })
    });
    WINDOWS.with(|all| all.borrow_mut().insert(key(window), state.clone()));

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
pub fn detach(window: &gtk::ApplicationWindow) {
    if let Some(state) = WINDOWS.with(|all| all.borrow_mut().remove(&key(window))) {
        state.borrow_mut().close();
    }
}
pub fn shutdown() {
    let states = WINDOWS.with(|all| std::mem::take(&mut *all.borrow_mut()));
    for state in states.into_values() {
        state.borrow_mut().close();
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
