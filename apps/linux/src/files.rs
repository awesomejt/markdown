//! GTK ownership and presentation. All file I/O and parsing belongs to document.
use gtk::prelude::*;
use gtk::{gio, glib};
use markdown_linux::document::{DocumentCapability, DocumentError, DocumentLoader, LoadEvent};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

struct WindowDocument {
    window: glib::WeakRef<gtk::ApplicationWindow>,
    label: glib::WeakRef<gtk::Label>,
    loader: DocumentLoader,
    chooser: Option<gtk::FileChooserNative>,
    reload: gio::SimpleAction,
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
            label.set_text(text);
        }
    }
    fn loading(&self) {
        self.reload.set_enabled(self.loader.has_file());
        self.status("Loading document…");
        if let Some(window) = self.window.upgrade() {
            window.set_title(Some(&format!(
                "{} — Markdown",
                self.loader.display_name().unwrap_or_default()
            )));
        }
    }
    fn open(&mut self, file: &gio::File) {
        let Some(path) = file.path() else {
            self.loader.reject_selection();
            self.reload.set_enabled(false);
            self.status(&DocumentError::NonLocalFile.to_string());
            return;
        };
        match self.loader.open(DocumentCapability::from_local_file(path)) {
            Ok(()) => self.loading(),
            Err(error) => self.status(&error.to_string()),
        }
    }
    fn close(&mut self) {
        if let Some(chooser) = self.chooser.take() {
            chooser.destroy();
        }
        self.loader.close();
    }
}
pub fn attach(window: &gtk::ApplicationWindow, label: &gtk::Label) {
    let reload = gio::SimpleAction::new("reload", None);
    reload.set_enabled(false);
    window.add_action(&reload);
    let state = Rc::new(RefCell::new(WindowDocument {
        window: window.downgrade(),
        label: label.downgrade(),
        loader: DocumentLoader::new(),
        chooser: None,
        reload: reload.clone(),
    }));
    WINDOWS.with(|all| all.borrow_mut().insert(key(window), state.clone()));
    let weak = Rc::downgrade(&state);
    reload.connect_activate(move |_, _| {
        if let Some(state) = weak.upgrade() {
            let mut state = state.borrow_mut();
            state.loader.reload();
            state.loading();
        }
    });
    let weak = Rc::downgrade(&state);
    glib::timeout_add_local(Duration::from_millis(16), move || {
        let Some(state) = weak.upgrade() else {
            return glib::ControlFlow::Break;
        };
        let mut state = state.borrow_mut();
        if let Some(event) = state.loader.poll() {
            match event {
                LoadEvent::Loaded {
                    document,
                    unchanged,
                    ..
                } => state.status(&format!(
                    "Document loaded · {} bytes · {} headings{}",
                    document.source.len(),
                    document.outline.len(),
                    if unchanged { " (unchanged)" } else { "" }
                )),
                LoadEvent::Failed(error) => state.status(&error.to_string()),
            }
        }
        glib::ControlFlow::Continue
    });
    window.connect_close_request(|window| {
        if let Some(state) = WINDOWS.with(|all| all.borrow_mut().remove(&key(window))) {
            state.borrow_mut().close();
        }
        glib::Propagation::Proceed
    });
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
