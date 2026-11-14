use gtk::gio::{self, prelude::*};
use gtk::glib;
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

pub struct DocumentWatcher {
    path: PathBuf,
    file_monitor: RefCell<Option<gio::FileMonitor>>,
    dir_monitor: RefCell<Option<gio::FileMonitor>>,
    debounce_source: RefCell<Option<glib::SourceId>>,
    callback: Rc<dyn Fn()>,
}

impl DocumentWatcher {
    pub const DELAY_MS: u32 = 100;

    pub fn new<F: Fn() + 'static>(path: impl AsRef<Path>, callback: F) -> Rc<Self> {
        let path = path.as_ref().to_path_buf();
        let watcher = Rc::new(Self {
            path,
            file_monitor: RefCell::new(None),
            dir_monitor: RefCell::new(None),
            debounce_source: RefCell::new(None),
            callback: Rc::new(callback),
        });
        watcher.arm();
        watcher
    }

    pub fn is_armed(&self) -> bool {
        self.file_monitor.borrow().is_some() || self.dir_monitor.borrow().is_some()
    }

    pub fn cancel(&self) {
        if let Some(source) = self.debounce_source.borrow_mut().take() {
            source.remove();
        }
        if let Some(m) = self.file_monitor.borrow_mut().take() {
            m.cancel();
        }
        if let Some(m) = self.dir_monitor.borrow_mut().take() {
            m.cancel();
        }
    }

    pub fn arm(self: &Rc<Self>) {
        self.cancel();
        let file = gio::File::for_path(&self.path);
        let flags = gio::FileMonitorFlags::WATCH_MOUNTS | gio::FileMonitorFlags::SEND_MOVED;

        if let Ok(monitor) = file.monitor_file(flags, gio::Cancellable::NONE) {
            let weak = Rc::downgrade(self);
            monitor.connect_changed(move |_m, _f, _o, event| {
                if let Some(this) = weak.upgrade() {
                    this.on_event(event);
                }
            });
            *self.file_monitor.borrow_mut() = Some(monitor);
        }

        if let Some(parent) = self.path.parent() {
            let dir = gio::File::for_path(parent);
            if let Ok(monitor) = dir.monitor_directory(flags, gio::Cancellable::NONE) {
                let weak = Rc::downgrade(self);
                let target_name = self.path.file_name().map(|n| n.to_os_string());
                monitor.connect_changed(move |_m, f, other, event| {
                    let matches_target = target_name.as_ref().is_some_and(|name| {
                        f.basename().as_deref() == Some(Path::new(name))
                            || other.as_ref().and_then(|o| o.basename()).as_deref()
                                == Some(Path::new(name))
                    });
                    if matches_target && let Some(this) = weak.upgrade() {
                        this.on_event(event);
                    }
                });
                *self.dir_monitor.borrow_mut() = Some(monitor);
            }
        }
    }

    fn on_event(self: &Rc<Self>, event: gio::FileMonitorEvent) {
        use gio::FileMonitorEvent::*;
        match event {
            Changed | ChangesDoneHint | AttributeChanged | Created | Moved | MovedIn => {
                self.schedule_debounce();
            }
            Deleted | MovedOut | Unmounted => {
                // If the file was deleted or moved out, we may need to re-arm when it reappears
                self.schedule_debounce();
            }
            _ => {}
        }
    }

    fn schedule_debounce(self: &Rc<Self>) {
        if let Some(source) = self.debounce_source.borrow_mut().take() {
            source.remove();
        }
        let weak = Rc::downgrade(self);
        let source =
            glib::timeout_add_local_once(Duration::from_millis(Self::DELAY_MS as u64), move || {
                if let Some(this) = weak.upgrade() {
                    this.debounce_source.borrow_mut().take();
                    (this.callback)();
                }
            });
        *self.debounce_source.borrow_mut() = Some(source);
    }
}

impl Drop for DocumentWatcher {
    fn drop(&mut self) {
        if let Some(source) = self.debounce_source.borrow_mut().take() {
            source.remove();
        }
        if let Some(m) = self.file_monitor.borrow_mut().take() {
            m.cancel();
        }
        if let Some(m) = self.dir_monitor.borrow_mut().take() {
            m.cancel();
        }
    }
}
