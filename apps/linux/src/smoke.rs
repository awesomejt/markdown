//! Opt-in development check, launched by tools/check.py in a bounded child process.

use std::time::{Duration, Instant};

use gtk::prelude::*;
use gtk::{gio, glib};

use crate::{APP_ID, EMPTY_LABEL_NAME};

/// Written out independently of `crate::COMMANDS`, so a changed menu or shortcut fails here.
const EXPECTED: &[(&str, &str, &str, &[&str])] = &[
    ("File", "New Window", "app.new-window", &["<Control>n"]),
    ("File", "Close Window", "window.close", &["<Control>w"]),
    ("File", "Quit", "app.quit", &["<Control>q"]),
    ("Help", "About Markdown", "app.about", &[]),
];

#[derive(Clone, Copy, Debug)]
pub enum Mode {
    /// Windows, menus, accelerators, About and Quit with windows still open.
    Commands,
    /// Closing the only window ends the application.
    LastWindow,
}

impl Mode {
    pub fn from_args(mut args: impl Iterator<Item = String>) -> Option<Mode> {
        match args.next().as_deref() {
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
