//! Native GTK 4 shell for the Markdown reader (LNX-001A).

mod smoke;

use gtk::prelude::*;
use gtk::{gio, glib, pango};

pub const APP_ID: &str = "io.github.awesomejt.markdown";
pub const EMPTY_LABEL_NAME: &str = "empty-document-label";

/// Menu entries and their accelerators, in menu order.
pub const COMMANDS: &[(&str, &str, &str, Option<&str>)] = &[
    ("File", "New Window", "app.new-window", Some("<Control>n")),
    ("File", "Close Window", "window.close", Some("<Control>w")),
    ("File", "Quit", "app.quit", Some("<Control>q")),
    ("Help", "About Markdown", "app.about", None),
];

fn main() -> glib::ExitCode {
    let smoke = smoke::Mode::from_args(std::env::args().skip(1));
    // A smoke run must never hand its checks to an already-running reader over D-Bus.
    let flags = match smoke {
        Some(_) => gio::ApplicationFlags::NON_UNIQUE,
        None => gio::ApplicationFlags::empty(),
    };
    let app = gtk::Application::builder()
        .application_id(APP_ID)
        .flags(flags)
        .build();
    app.connect_startup(install_commands);
    app.connect_activate(move |app| {
        // A second launch activates the primary instance, which opens another window.
        new_window(app);
        if let Some(mode) = smoke {
            smoke::start(app, mode);
        }
    });
    // GApplication would reject the smoke flag, so a smoke run passes on only the program
    // name; normal launches keep GApplication's own options (and, later, files to open).
    let arguments: Vec<String> = match smoke {
        Some(_) => std::env::args().take(1).collect(),
        None => std::env::args().collect(),
    };
    let status = app.run_with_args(&arguments);
    if smoke.is_some() {
        println!("shell-smoke: event loop exited");
    }
    status
}

fn install_commands(app: &gtk::Application) {
    let new = gio::ActionEntry::builder("new-window")
        .activate(|app: &gtk::Application, _, _| {
            new_window(app);
        })
        .build();
    let quit = gio::ActionEntry::builder("quit")
        .activate(|app: &gtk::Application, _, _| app.quit())
        .build();
    let about = gio::ActionEntry::builder("about")
        .activate(|app: &gtk::Application, _, _| show_about(app))
        .build();
    app.add_action_entries([new, quit, about]);

    let menubar = gio::Menu::new();
    for section in ["File", "Help"] {
        let menu = gio::Menu::new();
        for (_, label, action, accel) in COMMANDS.iter().filter(|c| c.0 == section) {
            // Keep Quit in its own section, below the window commands.
            if *action == "app.quit" {
                let group = gio::Menu::new();
                group.append(Some(label), Some(action));
                menu.append_section(None, &group);
            } else {
                menu.append(Some(label), Some(action));
            }
            if let Some(accel) = accel {
                app.set_accels_for_action(action, &[accel]);
            }
        }
        menubar.append_submenu(Some(section), &menu);
    }
    app.set_menubar(Some(&menubar));
}

pub fn new_window(app: &gtk::Application) -> gtk::ApplicationWindow {
    let label = gtk::Label::builder()
        .label("No document open")
        .name(EMPTY_LABEL_NAME)
        .halign(gtk::Align::Center)
        .valign(gtk::Align::Center)
        .hexpand(true)
        .vexpand(true)
        .build();
    let attributes = pango::AttrList::new();
    attributes.insert(pango::AttrFloat::new_scale(1.6));
    attributes.insert(pango::AttrInt::new_weight(pango::Weight::Semibold));
    label.set_attributes(Some(&attributes));
    label.add_css_class("dim-label");

    let window = gtk::ApplicationWindow::builder()
        .application(app)
        .title("Markdown")
        .default_width(900)
        .default_height(640)
        .show_menubar(true)
        .child(&label)
        .build();
    window.set_size_request(480, 320);
    window.present();
    window
}

fn show_about(app: &gtk::Application) {
    let dialog = gtk::AboutDialog::builder()
        .program_name("Markdown")
        .version(env!("CARGO_PKG_VERSION"))
        .comments("A local-first, read-only native Markdown reader.")
        .license_type(gtk::License::Apache20)
        .logo_icon_name("text-x-generic")
        .modal(true)
        .build();
    dialog.set_transient_for(app.active_window().as_ref());
    dialog.present();
}
