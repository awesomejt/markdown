//! Opt-in development check, launched by tools/check.py in a bounded child process.

use std::time::{Duration, Instant};

use gtk::prelude::*;
use gtk::{gio, glib};

use crate::{APP_ID, EMPTY_LABEL_NAME};

/// Written out independently of `crate::COMMANDS`, so a changed menu or shortcut fails here.
const EXPECTED: &[(&str, &str, &str, &[&str])] = &[
    ("File", "Open…", "app.open", &["<Control>o"]),
    ("File", "Reload", "win.reload", &["<Control>r"]),
    ("File", "New Window", "app.new-window", &["<Control>n"]),
    ("File", "Close Window", "window.close", &["<Control>w"]),
    ("File", "Quit", "app.quit", &["<Control>q"]),
    ("Edit", "Find…", "win.find", &["<Control>f"]),
    ("Edit", "Find Next", "win.find-next", &["F3"]),
    ("Edit", "Find Previous", "win.find-prev", &["<Shift>F3"]),
    (
        "View",
        "Make Text Bigger",
        "win.zoom-in",
        &["<Control>plus", "<Control>equal"],
    ),
    (
        "View",
        "Make Text Smaller",
        "win.zoom-out",
        &["<Control>minus"],
    ),
    ("View", "Actual Size", "win.zoom-reset", &["<Control>0"]),
    ("View", "Theme: System", "app.theme-system", &[]),
    ("View", "Theme: Light", "app.theme-light", &[]),
    ("View", "Theme: Dark", "app.theme-dark", &[]),
    ("Help", "About Markdown", "app.about", &[]),
];

#[derive(Clone, Copy, Debug)]
pub enum Mode {
    /// Windows, menus, accelerators, About and Quit with windows still open.
    Commands,
    /// Isolated unique-instance desktop-entry/second-process test.
    Association,
    /// Closing the only window ends the application.
    LastWindow,
}

impl Mode {
    pub fn from_args(mut args: impl Iterator<Item = std::ffi::OsString>) -> Option<Mode> {
        match args.next().as_ref().and_then(|a| a.to_str()) {
            Some("--association-test") => Some(Mode::Association),
            Some("--smoke-test") => Some(Mode::Commands),
            Some("--smoke-test=last-window") => Some(Mode::LastWindow),
            _ => None,
        }
    }
}

pub fn start(app: &gtk::Application, mode: Mode) {
    let app = app.clone();
    // Exercise the running GTK main loop, not just the constructors.
    glib::spawn_future_local(async move {
        let result = match mode {
            Mode::Commands => commands(&app).await,
            Mode::Association => association(&app).await,
            Mode::LastWindow => last_window(&app).await,
        };
        if let Err(message) = result {
            eprintln!("shell-smoke: FAIL: {message}");
            std::process::exit(1);
        }
    });
}

async fn commands(app: &gtk::Application) -> Result<(), String> {
    note("started");
    let first = launched_window(app).await?;
    check(
        first.is_resizable() && first.shows_menubar(),
        "resizable window with a menu bar",
    )?;
    note("launch/window checked");
    core().await?;

    for (menu, label, action, accels) in EXPECTED {
        check(
            menu_has(app, menu, label, action),
            &format!("menu item {menu}/{label}"),
        )?;
        let installed = app.accels_for_action(action);
        check(
            installed
                .iter()
                .map(|a| a.as_str())
                .eq(accels.iter().copied()),
            &format!("accelerator for {label}"),
        )?;
    }
    note("menus/accelerators checked");
    documents(app, &first).await?;
    reader(&first).await?;
    find_and_zoom(app, &first).await?;

    app.activate_action("new-window", None);
    check(
        app.windows().len() == 2,
        "New Window creates another window",
    )?;
    let closed = {
        let second = app.active_window().ok_or("New Window is active")?;
        check(
            second != *first.upcast_ref::<gtk::Window>(),
            "New Window is a new window",
        )?;
        let reference = second.downgrade();
        WidgetExt::activate_action(&second, "window.close", None)
            .map_err(|e| format!("Close Window routes: {e}"))?;
        reference
    };
    wait_until("closing a window removes it", || app.windows().len() == 1).await?;
    // GTK finalizes the destroyed window once its main-loop references are dropped.
    wait_until("closed window is not retained", || {
        closed.upgrade().is_none()
    })
    .await?;
    note("new/close checked");

    app.activate_action("about", None);
    wait_until("About opens a dialog for the window", || {
        about_dialog().is_some_and(|d| d.transient_for().as_ref() == Some(first.upcast_ref()))
    })
    .await?;
    let about = about_dialog().ok_or("About dialog")?;
    check(
        about.program_name().as_deref() == Some("Markdown")
            && about.version().as_deref() == Some(env!("CARGO_PKG_VERSION")),
        "About names the program and version",
    )?;
    about.close();
    wait_until("About closes", || about_dialog().is_none()).await?;
    check(
        app.windows().len() == 1,
        "About does not change the window list",
    )?;
    note("about checked");

    app.activate_action("new-window", None);
    check(app.windows().len() == 2, "second window before Quit")?;
    note("pass (application, windows, menus, accelerators, close, about, quit)");
    // Quit with two windows open; tools/check.py requires the event loop to exit cleanly.
    app.activate_action("quit", None);
    Ok(())
}

/// The app links the matching core package: parse off the main thread through the
/// adapter, then confirm every native handle is released.
async fn core() -> Result<(), String> {
    use markdown_linux_core as core;
    let expected = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    check(
        core::PACKAGE_CONFIGURATION == expected,
        "core package matches the build profile",
    )?;
    let parsed = gio::spawn_blocking(|| {
        let session = core::CoreSession::new("smoke")?;
        let result = session.parse_blocking(b"# Smoke heading\n\nBody.\n".to_vec(), None, true);
        session.close()?;
        result
    })
    .await
    .map_err(|_| "core worker panicked".to_owned())?
    .map_err(|error| format!("core parse: {error}"))?;
    let document = parsed.document.ok_or("core parse published no document")?;
    let title = document
        .outline
        .first()
        .map(|entry| document.text(entry.title));
    check(
        parsed.outcome == core::CoreOutcome::Published && title == Some(Ok("Smoke heading")),
        "core model v2 parse",
    )?;
    let handles = core::live_handles().map_err(|error| error.to_string())?;
    check(handles.total() == 0, "core handles released")?;
    note(&format!(
        "core checked ({} package, model v2)",
        core::PACKAGE_CONFIGURATION
    ));
    Ok(())
}

async fn last_window(app: &gtk::Application) -> Result<(), String> {
    note("started");
    let window = launched_window(app).await?;
    note("pass (closing the last window ends the application)");
    WidgetExt::activate_action(&window, "window.close", None)
        .map_err(|e| format!("Close Window routes: {e}"))
}

async fn launched_window(app: &gtk::Application) -> Result<gtk::ApplicationWindow, String> {
    check(
        app.application_id().as_deref() == Some(APP_ID),
        "application ID",
    )?;
    check(
        app.flags().contains(gio::ApplicationFlags::NON_UNIQUE),
        "smoke runs as its own instance",
    )?;
    check(app.windows().len() == 1, "launch creates one window")?;
    let window = app
        .windows()
        .into_iter()
        .next()
        .and_then(|w| w.downcast::<gtk::ApplicationWindow>().ok())
        .ok_or("launch window is an application window")?;
    wait_until("window is mapped by the compositor", || window.is_mapped()).await?;
    check(
        window.title().as_deref() == Some("Markdown"),
        "window title",
    )?;
    let label = window
        .child()
        .and_then(|c| c.first_child())
        .and_then(|c| c.downcast::<gtk::Label>().ok())
        .ok_or("window shows an empty-state label")?;
    check(
        label.widget_name() == EMPTY_LABEL_NAME
            && label.text() == "No document open"
            && label.property::<gtk::AccessibleRole>("accessible-role")
                == gtk::AccessibleRole::Label
            && label.is_mapped(),
        "accessible native empty state",
    )?;
    Ok(window)
}

fn menu_has(app: &gtk::Application, menu: &str, label: &str, action: &str) -> bool {
    let Some(bar) = app.menubar() else {
        return false;
    };
    (0..bar.n_items()).any(|i| {
        string_attribute(&bar, i, "label").as_deref() == Some(menu)
            && bar
                .item_link(i, "submenu")
                .is_some_and(|m| model_has(&m, label, action))
    })
}

fn model_has(model: &gio::MenuModel, label: &str, action: &str) -> bool {
    (0..model.n_items()).any(|i| {
        (string_attribute(model, i, "label").as_deref() == Some(label)
            && string_attribute(model, i, "action").as_deref() == Some(action))
            || model
                .item_link(i, "section")
                .is_some_and(|s| model_has(&s, label, action))
    })
}

fn string_attribute(model: &gio::MenuModel, item: i32, name: &str) -> Option<String> {
    model
        .item_attribute_value(item, name, Some(glib::VariantTy::STRING))
        .and_then(|v| v.get::<String>())
}

fn about_dialog() -> Option<gtk::AboutDialog> {
    gtk::Window::list_toplevels()
        .into_iter()
        .filter_map(|w| w.downcast::<gtk::AboutDialog>().ok())
        .find(|d| d.is_visible())
}

async fn wait_until(message: &str, condition: impl Fn() -> bool) -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !condition() && Instant::now() < deadline {
        glib::timeout_future(Duration::from_millis(50)).await;
    }
    check(condition(), message)
}

fn note(message: &str) {
    println!("shell-smoke: {message}");
}

fn check(value: bool, message: &str) -> Result<(), String> {
    if value {
        Ok(())
    } else {
        Err(message.to_string())
    }
}

async fn association(app: &gtk::Application) -> Result<(), String> {
    check(
        !app.flags().contains(gio::ApplicationFlags::NON_UNIQUE),
        "association remains single-instance",
    )?;
    println!("association-smoke: ready");
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if crate::files::loaded_sources(app)
            == [
                b"# Desktop one\n".to_vec(),
                b"# Desktop two\n".to_vec(),
                b"# First delivery\n".to_vec(),
            ]
        {
            break;
        }
        check(
            Instant::now() < deadline,
            "desktop startup and second-process deliveries loaded exact bytes in three windows",
        )?;
        glib::timeout_future(Duration::from_millis(20)).await;
    }
    println!("association-smoke: pass (startup, desktop entry, second process, exact bytes)");
    app.quit();
    Ok(())
}

async fn documents(app: &gtk::Application, window: &gtk::ApplicationWindow) -> Result<(), String> {
    let directory = std::env::temp_dir().join(format!("markdown-gtk-smoke-{}", std::process::id()));
    std::fs::create_dir(&directory).map_err(|_| "create smoke fixture")?;
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(directory.clone());
    let path = directory.join("文 space.md");
    std::fs::write(&path, b"# Native open\n").map_err(|_| "write fixture")?;
    check(
        !crate::files::reload_enabled(window),
        "Reload disabled without a file",
    )?;
    check(
        !crate::files::polling(window),
        "empty window has no polling source",
    )?;
    app.activate_action("open", None);
    check(
        crate::files::chooser_visible(window),
        "Open presents a native chooser",
    )?;
    wait_until("native fallback chooser is ready for Cancel", || {
        crate::files::chooser_ready(window)
    })
    .await?;
    crate::files::chooser_cancel(window)?;
    wait_until("Cancel dismisses the native chooser", || {
        !crate::files::chooser_visible(window) && !crate::files::chooser_ready(window)
    })
    .await?;
    check(
        !crate::files::reload_enabled(window),
        "chooser cancel retains empty state",
    )?;
    // Programmatic acceptance uses the same response callback as a real picker.
    app.activate_action("open", None);
    glib::timeout_future(Duration::from_millis(250)).await;
    crate::files::chooser_select(window, &gio::File::for_path(&path))?;
    wait_until("chooser file selection settles", || {
        crate::files::chooser_selected(window, &gio::File::for_path(&path))
    })
    .await?;
    crate::files::chooser_accept(window)?;
    check(
        crate::files::polling(window),
        "selection starts completion polling",
    )?;
    wait_until("picker file loads", || {
        crate::files::source(window).as_deref() == Some(b"# Native open\n")
    })
    .await?;
    check(
        !crate::files::polling(window),
        "loaded window stops completion polling",
    )?;
    check(
        crate::files::reload_enabled(window),
        "Reload enabled after selection",
    )?;
    std::fs::write(&path, b"# Changed\n").map_err(|_| "modify fixture")?;
    WidgetExt::activate_action(window, "win.reload", None).map_err(|_| "Reload action")?;
    wait_until("Reload publishes changed bytes", || {
        crate::files::source(window).as_deref() == Some(b"# Changed\n")
    })
    .await?;
    std::fs::remove_file(&path).map_err(|_| "remove fixture")?;
    WidgetExt::activate_action(window, "win.reload", None).map_err(|_| "Reload missing")?;
    wait_until("missing file has path-free recovery", || {
        crate::files::status(window).starts_with("The file is missing.")
    })
    .await?;
    std::fs::write(&path, b"# Recovered\n").map_err(|_| "restore fixture")?;
    WidgetExt::activate_action(window, "win.reload", None).map_err(|_| "Reload recovery")?;
    wait_until("Reload recovers access", || {
        crate::files::source(window).as_deref() == Some(b"# Recovered\n")
    })
    .await?;
    // Reject remote delivery after starting another load: no obsolete result may win.
    crate::files::open_for_smoke(window, &gio::File::for_path(&path));
    crate::files::open_for_smoke(
        window,
        &gio::File::for_uri("https://invalid.example/document.md"),
    );
    glib::timeout_future(Duration::from_millis(100)).await;
    check(
        crate::files::status(window).starts_with("Only local files"),
        "remote rejection invalidates previous load",
    )?;
    check(
        !crate::files::reload_enabled(window),
        "remote selection grants no reload capability",
    )?;
    let large = directory.join("large.md");
    std::fs::write(&large, b"a ".repeat(2_500_000)).map_err(|_| "large fixture")?;
    // Opening a new window while a large load is pending proves event-loop progress.
    crate::files::open_for_smoke(window, &gio::File::for_path(&large));
    let closing = crate::new_window(app);
    crate::files::open_for_smoke(&closing, &gio::File::for_path(&large));
    closing.close();
    let destroyed = crate::new_window(app);
    crate::files::open_for_smoke(&destroyed, &gio::File::for_path(&large));
    check(
        crate::files::attached(&destroyed) && crate::files::polling(&destroyed),
        "programmatic destroy starts with owned pending load",
    )?;
    destroyed.destroy();
    check(
        !crate::files::attached(&destroyed) && !crate::files::polling(&destroyed),
        "programmatic destroy detaches state and polling",
    )?;
    glib::timeout_future(Duration::from_millis(1)).await;
    check(
        window.is_mapped(),
        "GTK remains responsive during load/close",
    )?;
    // Leave one large pending load for the commands smoke's explicit Quit path.
    crate::files::open_for_smoke(window, &gio::File::for_path(&path));
    note(
        "documents checked (native chooser routing/cancel, Reload, recovery, unsupported URI, load/close)",
    );
    Ok(())
}

async fn reader(window: &gtk::ApplicationWindow) -> Result<(), String> {
    let directory =
        std::env::temp_dir().join(format!("markdown-reader-smoke-{}", std::process::id()));
    std::fs::create_dir_all(&directory).map_err(|_| "reader fixture directory")?;
    let path = directory.join("reader.md");
    let mut source = String::from(
        "# First\n\n[jump](#last) **styled** `<&>`\n\n3. list item\n\n```\ncode <&>\n```\n\n",
    );
    for index in 0..1600 {
        source.push_str(&format!(
            "## Middle {index}\n\n{}\n\n",
            "word 文 ".repeat(40)
        ));
    }
    source.push_str("# Last\n\nlast body\n");
    std::fs::write(&path, &source).map_err(|_| "reader fixture")?;
    crate::files::open_for_smoke(window, &gio::File::for_path(&path));
    wait_until("reader publishes owned values", || {
        crate::files::source(window).as_deref() == Some(source.as_bytes())
    })
    .await?;
    let reader = crate::files::reader(window).ok_or("reader owner")?;
    wait_until("first native heading", || {
        reader
            .observed()
            .iter()
            .any(|(_, l)| l.text() == "First" && l.is_mapped())
    })
    .await?;
    check(reader.row_count() < 1000, "bounded lazy rows")?;
    let observed = reader.observed();
    check(
        observed
            .iter()
            .any(|(_, l)| l.text().contains("3. list item")),
        "native list marker",
    )?;
    check(
        observed.iter().any(|(_, l)| l.text().contains("code <&>")),
        "native code escaping",
    )?;
    let label = observed
        .iter()
        .find(|(_, l)| l.text().contains("styled"))
        .ok_or("styled native label")?
        .1
        .clone();
    label.select_region(0, 4);
    check(
        label.selection_bounds() == Some((0, 4)),
        "within-fragment native selection bounds",
    )?;
    check(
        reader.activate_fragment_link(),
        "native fragment link dispatch",
    )?;
    wait_until("fragment link reveals far heading", || {
        reader
            .observed()
            .iter()
            .any(|(_, l)| l.text() == "Last" && l.is_mapped())
    })
    .await?;
    reader.activate_outline(0);
    wait_until("outline returns first heading", || {
        reader
            .observed()
            .iter()
            .any(|(_, l)| l.text() == "First" && l.is_mapped())
    })
    .await?;
    reader.activate_outline(1601);
    wait_until("far outline reveals last heading", || {
        reader
            .observed()
            .iter()
            .any(|(_, l)| l.text() == "Last" && l.is_mapped())
    })
    .await?;
    check(
        reader.row_count() < 1000,
        "recycling keeps realized row ownership bounded",
    )?;
    for (_, label) in reader.observed() {
        check(
            label.selection_bounds().is_none(),
            "recycling clears selection",
        )?;
        let row = label.parent().ok_or("row parent")?;
        let mut child = row.first_child();
        let mut visible = 0;
        while let Some(w) = child {
            child = w.next_sibling();
            visible += usize::from(w.is_visible());
        }
        check(visible == 1, "one visible semantic label per row")?;
    }
    WidgetExt::activate_action(window, "win.reload", None).map_err(|_| "reader reload")?;
    check(
        reader.observed().is_empty(),
        "reload invalidates row bindings",
    )?;
    wait_until("reader reload completes", || !crate::files::polling(window)).await?;
    let long_title = format!("{}end", "word ".repeat(220));
    let heading_source = format!("## {long_title}\n");
    std::fs::write(&path, &heading_source).map_err(|_| "long heading fixture")?;
    crate::files::open_for_smoke(window, &gio::File::for_path(&path));
    wait_until("long heading publishes", || {
        crate::files::source(window).as_deref() == Some(heading_source.as_bytes())
    })
    .await?;
    wait_until("long heading fragments realized", || {
        reader.observed().len() == 3
    })
    .await?;
    let mut labels = reader.observed();
    labels.sort_by_key(|(index, _)| *index);
    check(
        labels
            .iter()
            .map(|(_, label)| label.text().to_string())
            .collect::<String>()
            == long_title,
        "long heading text reconstructed",
    )?;
    check(
        labels
            .iter()
            .filter(|(_, label)| label.accessible_role() == gtk::AccessibleRole::Heading)
            .count()
            == 1,
        "one semantic heading across native fragments",
    )?;
    check(
        labels[0].1.accessible_role() == gtk::AccessibleRole::Heading,
        "first fragment is heading",
    )?;
    for (_, label) in labels.iter().skip(1) {
        check(
            label.accessible_role() == gtk::AccessibleRole::Paragraph
                && label.attributes().is_some(),
            "continuation retains heading style with paragraph role",
        )?;
    }

    let rich_path = directory.join("rich.md");
    let img_path = directory.join("local.png");
    std::fs::write(&img_path, minimal_png()).map_err(|_| "write smoke png")?;
    let rich_source = "# Rich Blocks\n\n\
                       | Col A | Col B |\n\
                       | :--- | ---: |\n\
                       | val1 | val2 |\n\n\
                       - [x] checked task\n\
                       - [ ] open task\n\n\
                       ![local image](local.png)\n\n\
                       ![remote image](https://example.com/pic.png)\n\n\
                       ![escape image](../outside.png)\n";
    std::fs::write(&rich_path, rich_source).map_err(|_| "write rich source")?;
    crate::files::open_for_smoke(window, &gio::File::for_path(&rich_path));
    wait_until("rich blocks publish", || {
        crate::files::source(window).as_deref() == Some(rich_source.as_bytes())
    })
    .await?;
    wait_until("rich blocks realized", || {
        let obs = reader.observed();
        obs.iter().any(|(_, l)| l.text().contains("Col A\tCol B"))
            && obs.iter().any(|(_, l)| l.text().contains("☑ checked task"))
    })
    .await?;
    let obs = reader.observed();
    check(
        obs.iter().any(|(_, l)| l.text().contains("Col A\tCol B")),
        "table header row with tab separation",
    )?;
    check(
        obs.iter().any(|(_, l)| l.text().contains("val1\tval2")),
        "table body row with tab separation",
    )?;
    check(
        obs.iter().any(|(_, l)| l.text().contains("☑ checked task")),
        "task checked marker rendered",
    )?;
    check(
        obs.iter().any(|(_, l)| l.text().contains("☐ open task")),
        "task unchecked marker rendered",
    )?;

    let mut found_picture = false;
    let mut found_remote_denied = false;
    let mut found_escape_denied = false;
    for (_, label) in &obs {
        if let Some(row) = label.parent() {
            let mut child = row.first_child();
            while let Some(w) = child {
                child = w.next_sibling();
                if let Ok(box_widget) = w.downcast::<gtk::Box>() {
                    let mut img_child = box_widget.first_child();
                    while let Some(ic) = img_child {
                        img_child = ic.next_sibling();
                        if ic.downcast_ref::<gtk::Picture>().is_some() {
                            found_picture = true;
                        }
                        if let Some(lbl) = ic.downcast_ref::<gtk::Label>() {
                            if lbl.text().contains("Remote images are blocked") {
                                found_remote_denied = true;
                            }
                            if lbl
                                .text()
                                .contains("Images outside the document’s folder are not shown")
                            {
                                found_escape_denied = true;
                            }
                        }
                    }
                }
            }
        }
    }
    check(found_picture, "local image rendered in GTK Picture")?;
    check(found_remote_denied, "remote image shows blocked status")?;
    check(
        found_escape_denied,
        "outside-folder image shows outside status",
    )?;

    std::fs::remove_dir_all(directory).map_err(|_| "remove reader fixtures")?;
    note(
        "reader checked (native text, escaping, bounded rows, fragment/outline jumps, selection, recycling, reload, tables, task lists, images)",
    );
    Ok(())
}

async fn find_and_zoom(
    app: &gtk::Application,
    window: &gtk::ApplicationWindow,
) -> Result<(), String> {
    let find_bar = crate::files::find_bar(window).ok_or("find bar present")?;
    check(!find_bar.is_revealed(), "find bar starts hidden")?;

    WidgetExt::activate_action(window, "win.find", None)
        .map_err(|e| format!("win.find routes: {e}"))?;
    wait_until("find bar revealed", || find_bar.is_revealed()).await?;

    find_bar.entry.set_text("Rich");
    wait_until("find matches query", || {
        find_bar.status_label.text().contains("1")
    })
    .await?;
    check(
        find_bar.prev_button.is_sensitive(),
        "prev button sensitive with matches",
    )?;
    check(
        find_bar.next_button.is_sensitive(),
        "next button sensitive with matches",
    )?;

    WidgetExt::activate_action(window, "win.find-next", None)
        .map_err(|e| format!("win.find-next routes: {e}"))?;
    WidgetExt::activate_action(window, "win.find-prev", None)
        .map_err(|e| format!("win.find-prev routes: {e}"))?;

    find_bar.close_button.emit_clicked();
    wait_until("close button hides find bar", || !find_bar.is_revealed()).await?;

    // Also test entry stop-search (Escape) dismisses find bar
    WidgetExt::activate_action(window, "win.find", None)
        .map_err(|e| format!("win.find routes: {e}"))?;
    wait_until("find bar revealed again", || find_bar.is_revealed()).await?;
    find_bar.entry.emit_stop_search();
    wait_until("stop-search hides find bar", || !find_bar.is_revealed()).await?;

    check(
        (crate::files::text_scale(window).unwrap_or(0.0) - 1.0).abs() < 0.001,
        "initial text scale 1.0",
    )?;

    WidgetExt::activate_action(window, "win.zoom-in", None)
        .map_err(|e| format!("win.zoom-in routes: {e}"))?;
    check(
        (crate::files::text_scale(window).unwrap_or(0.0) - 1.15).abs() < 0.001,
        "zoomed in to 1.15",
    )?;

    WidgetExt::activate_action(window, "win.zoom-reset", None)
        .map_err(|e| format!("win.zoom-reset routes: {e}"))?;
    check(
        (crate::files::text_scale(window).unwrap_or(0.0) - 1.0).abs() < 0.001,
        "reset to 1.0",
    )?;

    WidgetExt::activate_action(window, "win.zoom-out", None)
        .map_err(|e| format!("win.zoom-out routes: {e}"))?;
    check(
        (crate::files::text_scale(window).unwrap_or(0.0) - 0.85).abs() < 0.001,
        "zoomed out to 0.85",
    )?;

    WidgetExt::activate_action(window, "win.zoom-reset", None)
        .map_err(|e| format!("win.zoom-reset routes: {e}"))?;
    check(
        (crate::files::text_scale(window).unwrap_or(0.0) - 1.0).abs() < 0.001,
        "reset to 1.0",
    )?;

    app.activate_action("theme-dark", None);
    check(
        markdown_linux::preferences::load().theme
            == markdown_linux::preferences::ThemePreference::Dark,
        "theme preference dark saved",
    )?;
    if let Some(s) = gtk::Settings::default() {
        check(
            s.is_gtk_application_prefer_dark_theme(),
            "gtk dark theme enabled",
        )?;
    }

    app.activate_action("theme-light", None);
    check(
        markdown_linux::preferences::load().theme
            == markdown_linux::preferences::ThemePreference::Light,
        "theme preference light saved",
    )?;
    if let Some(s) = gtk::Settings::default() {
        check(
            !s.is_gtk_application_prefer_dark_theme(),
            "gtk light theme enabled",
        )?;
    }

    app.activate_action("theme-system", None);
    check(
        markdown_linux::preferences::load().theme
            == markdown_linux::preferences::ThemePreference::System,
        "theme preference system saved",
    )?;

    note("find, theme and text zoom checked");
    Ok(())
}

fn minimal_png() -> Vec<u8> {
    vec![
        137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 6,
        0, 0, 0, 31, 21, 196, 137, 0, 0, 0, 13, 73, 68, 65, 84, 120, 218, 99, 252, 207, 192, 80,
        15, 0, 4, 133, 1, 128, 132, 169, 140, 33, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
    ]
}
