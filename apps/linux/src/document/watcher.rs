use gtk::gio::{self, prelude::*};
use gtk::glib;
use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

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

/// Bounded, change-storm-coalescing directory watcher for an active workspace folder (LNX-002E).
pub struct WorkspaceWatcher {
    root: PathBuf,
    root_parent_monitor: RefCell<Option<gio::FileMonitor>>,
    dir_monitors: RefCell<Vec<(PathBuf, gio::FileMonitor)>>,
    watched_dirs: RefCell<HashSet<PathBuf>>,
    debounce_source: RefCell<Option<glib::SourceId>>,
    first_event_at: RefCell<Option<Instant>>,
    raw_event_count: Cell<u64>,
    refresh_count: Cell<u64>,
    callback: Rc<dyn Fn()>,
}

impl WorkspaceWatcher {
    pub const DEBOUNCE_MS: u64 = 150;
    pub const MAX_COALESCE_MS: u64 = 1000;
    pub const MAX_WATCHED_DIRS: usize = 256;

    pub fn new<F: Fn() + 'static>(root: impl AsRef<Path>, callback: F) -> Rc<Self> {
        let root = root.as_ref().to_path_buf();
        let watcher = Rc::new(Self {
            root: root.clone(),
            root_parent_monitor: RefCell::new(None),
            dir_monitors: RefCell::new(Vec::new()),
            watched_dirs: RefCell::new(HashSet::new()),
            debounce_source: RefCell::new(None),
            first_event_at: RefCell::new(None),
            raw_event_count: Cell::new(0),
            refresh_count: Cell::new(0),
            callback: Rc::new(callback),
        });
        watcher.arm_root_parent();
        watcher.sync_dirs(std::slice::from_ref(&root));
        watcher
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn watched_dir_count(&self) -> usize {
        self.dir_monitors.borrow().len()
    }

    pub fn raw_event_count(&self) -> u64 {
        self.raw_event_count.get()
    }

    pub fn refresh_count(&self) -> u64 {
        self.refresh_count.get()
    }

    pub fn is_armed(&self) -> bool {
        self.root_parent_monitor.borrow().is_some() || !self.dir_monitors.borrow().is_empty()
    }

    pub fn cancel(&self) {
        if let Some(source) = self.debounce_source.borrow_mut().take() {
            source.remove();
        }
        *self.first_event_at.borrow_mut() = None;
        if let Some(m) = self.root_parent_monitor.borrow_mut().take() {
            m.cancel();
        }
        for (_, m) in self.dir_monitors.borrow_mut().drain(..) {
            m.cancel();
        }
        self.watched_dirs.borrow_mut().clear();
    }

    fn arm_root_parent(self: &Rc<Self>) {
        if let Some(m) = self.root_parent_monitor.borrow_mut().take() {
            m.cancel();
        }
        let Some(parent) = self.root.parent() else {
            return;
        };
        let Some(target_name) = self.root.file_name().map(|n| n.to_os_string()) else {
            return;
        };
        let dir = gio::File::for_path(parent);
        let flags = gio::FileMonitorFlags::WATCH_MOUNTS | gio::FileMonitorFlags::SEND_MOVED;
        if let Ok(monitor) = dir.monitor_directory(flags, gio::Cancellable::NONE) {
            let weak = Rc::downgrade(self);
            monitor.connect_changed(move |_m, f, other, event| {
                let matches_root = f.basename().as_deref() == Some(Path::new(&target_name))
                    || other.as_ref().and_then(|o| o.basename()).as_deref()
                        == Some(Path::new(&target_name));
                if matches_root && let Some(this) = weak.upgrade() {
                    this.on_event(event);
                }
            });
            *self.root_parent_monitor.borrow_mut() = Some(monitor);
        }
    }

    /// Synchronize watched directories with the latest enumerated workspace directories,
    /// capped at `MAX_WATCHED_DIRS`.
    pub fn sync_dirs(self: &Rc<Self>, dirs: &[PathBuf]) {
        let mut desired: Vec<PathBuf> =
            Vec::with_capacity(dirs.len().min(Self::MAX_WATCHED_DIRS) + 1);
        let mut desired_set: HashSet<PathBuf> = HashSet::new();
        if desired_set.insert(self.root.clone()) {
            desired.push(self.root.clone());
        }
        for d in dirs {
            if desired.len() >= Self::MAX_WATCHED_DIRS {
                break;
            }
            if desired_set.insert(d.clone()) {
                desired.push(d.clone());
            }
        }

        // Cancel monitors for directories no longer present.
        {
            let mut monitors = self.dir_monitors.borrow_mut();
            let mut watched = self.watched_dirs.borrow_mut();
            let mut kept = Vec::with_capacity(monitors.len());
            for (path, monitor) in monitors.drain(..) {
                if desired_set.contains(&path) {
                    kept.push((path, monitor));
                } else {
                    watched.remove(&path);
                    monitor.cancel();
                }
            }
            *monitors = kept;
        }

        // Add monitors for newly discovered directories.
        let flags = gio::FileMonitorFlags::WATCH_MOUNTS | gio::FileMonitorFlags::SEND_MOVED;
        for path in desired {
            if self.watched_dirs.borrow().contains(&path) {
                continue;
            }
            let file = gio::File::for_path(&path);
            if let Ok(monitor) = file.monitor_directory(flags, gio::Cancellable::NONE) {
                let weak = Rc::downgrade(self);
                monitor.connect_changed(move |_m, _f, _other, event| {
                    if let Some(this) = weak.upgrade() {
                        this.on_event(event);
                    }
                });
                self.watched_dirs.borrow_mut().insert(path.clone());
                self.dir_monitors.borrow_mut().push((path, monitor));
            }
        }
    }

    fn on_event(self: &Rc<Self>, event: gio::FileMonitorEvent) {
        use gio::FileMonitorEvent::*;
        match event {
            Changed | ChangesDoneHint | AttributeChanged | Created | Deleted | Moved | MovedIn
            | MovedOut | Unmounted => {
                self.raw_event_count
                    .set(self.raw_event_count.get().saturating_add(1));
                self.schedule_coalesced_refresh();
            }
            _ => {}
        }
    }

    fn schedule_coalesced_refresh(self: &Rc<Self>) {
        let now = Instant::now();
        let max_elapsed = {
            let mut first = self.first_event_at.borrow_mut();
            match *first {
                Some(t) => {
                    now.saturating_duration_since(t) >= Duration::from_millis(Self::MAX_COALESCE_MS)
                }
                None => {
                    *first = Some(now);
                    false
                }
            }
        };

        // If a storm has lasted longer than MAX_COALESCE_MS and a timer is already pending,
        // let that timer fire without pushing it out further.
        if max_elapsed && self.debounce_source.borrow().is_some() {
            return;
        }

        if let Some(source) = self.debounce_source.borrow_mut().take() {
            source.remove();
        }
        let delay = if max_elapsed { 0 } else { Self::DEBOUNCE_MS };
        let weak = Rc::downgrade(self);
        let source = glib::timeout_add_local_once(Duration::from_millis(delay), move || {
            if let Some(this) = weak.upgrade() {
                this.debounce_source.borrow_mut().take();
                *this.first_event_at.borrow_mut() = None;
                this.refresh_count
                    .set(this.refresh_count.get().saturating_add(1));
                (this.callback)();
            }
        });
        *self.debounce_source.borrow_mut() = Some(source);
    }
}

impl Drop for WorkspaceWatcher {
    fn drop(&mut self) {
        if let Some(source) = self.debounce_source.borrow_mut().take() {
            source.remove();
        }
        if let Some(m) = self.root_parent_monitor.borrow_mut().take() {
            m.cancel();
        }
        for (_, m) in self.dir_monitors.borrow_mut().drain(..) {
            m.cancel();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn workspace_watcher_coalesces_change_storm_and_caps_dirs() {
        let dir = std::env::temp_dir().join(format!(
            "md-ws-watch-storm-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let sub = dir.join("sub");
        std::fs::create_dir_all(&sub).unwrap();

        let ctx = glib::MainContext::default();
        let _guard = ctx.acquire().unwrap();

        let fires = Rc::new(Cell::new(0u32));
        let fires_clone = fires.clone();
        let watcher = WorkspaceWatcher::new(&dir, move || {
            fires_clone.set(fires_clone.get() + 1);
        });
        assert!(watcher.is_armed());

        let mut many_dirs = Vec::with_capacity(260);
        for i in 0..260 {
            let d = dir.join(format!("sub_{i:03}"));
            std::fs::create_dir_all(&d).unwrap();
            many_dirs.push(d);
        }
        watcher.sync_dirs(&many_dirs);
        assert_eq!(
            watcher.watched_dir_count(),
            WorkspaceWatcher::MAX_WATCHED_DIRS
        );

        // Simulate a burst of 200 file writes.
        for i in 0..200 {
            std::fs::write(sub.join(format!("f_{i}.md")), b"# hi").unwrap();
        }

        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            while ctx.pending() {
                ctx.iteration(false);
            }
            if fires.get() >= 1 && watcher.raw_event_count() >= 1 {
                // Drain one more short window to let any trailing debounce settle.
                let settle = Instant::now() + Duration::from_millis(250);
                while Instant::now() < settle {
                    while ctx.pending() {
                        ctx.iteration(false);
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }

        assert!(
            fires.get() >= 1,
            "expected at least 1 coalesced refresh, got 0"
        );
        assert!(
            watcher.refresh_count() <= watcher.raw_event_count(),
            "expected coalesced refreshes ({}) <= raw events ({})",
            watcher.refresh_count(),
            watcher.raw_event_count()
        );

        watcher.cancel();
        assert!(!watcher.is_armed());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
