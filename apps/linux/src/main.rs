//! Native GTK 4 shell for the Markdown reader (LNX-001A).

mod files;
mod smoke;

use gtk::prelude::*;
use gtk::{gio, glib, pango};

pub const APP_ID: &str = "io.github.awesomejt.markdown";
pub const EMPTY_LABEL_NAME: &str = "empty-document-label";

/// Menu entries and their accelerators, in menu order.
pub const COMMANDS: &[(&str, &str, &str, Option<&str>)] = &[
    ("File", "Open…", "app.open", Some("<Control>o")),
    ("File", "Reload", "win.reload", Some("<Control>r")),
    ("File", "New Window", "app.new-window", Some("<Control>n")),
    ("File", "Close Window", "window.close", Some("<Control>w")),
    ("File", "Quit", "app.quit", Some("<Control>q")),
    ("Help", "About Markdown", "app.about", None),
];

fn main() -> glib::ExitCode {
    // Refuse to start against a core library with another ABI, model set or layout.
    if let Err(error) = markdown_linux_core::validate() {
        eprintln!("Markdown cannot use its core library: {error}");
        return glib::ExitCode::FAILURE;
    }
    let smoke = smoke::Mode::from_args(std::env::args_os().skip(1));
    // Shell smokes are independent; the isolated association smoke must exercise
    // real single-instance forwarding on the private bus provided by its helper.
    let flags = match smoke {
        Some(smoke::Mode::Association) => gio::ApplicationFlags::HANDLES_OPEN,
        Some(_) => gio::ApplicationFlags::NON_UNIQUE | gio::ApplicationFlags::HANDLES_OPEN,
        None => gio::ApplicationFlags::HANDLES_OPEN,
    };
    let app = gtk::Application::builder()
        .application_id(APP_ID)
        .flags(flags)
        .build();
    app.connect_startup(install_commands);
    if matches!(smoke, Some(smoke::Mode::Association)) {
        app.connect_startup(|app| smoke::start(app, smoke::Mode::Association));
    }
    app.connect_open(|app, files, _| files::deliver(app, files));
    app.connect_shutdown(|_| files::shutdown());
    app.connect_activate(move |app| {
        // A second launch activates the primary instance, which opens another window.
        new_window(app);
        if let Some(mode) = smoke.filter(|mode| !matches!(mode, smoke::Mode::Association)) {
            smoke::start(app, mode);
        }
    });
    // GApplication would reject the smoke flag, so a smoke run passes on only the program
    // name; normal launches preserve GApplication options and exact OS file names.
    let arguments: Vec<std::ffi::OsString> = match smoke {
        Some(smoke::Mode::Association) => std::env::args_os()
            .enumerate()
            .filter_map(|(index, arg)| (index != 1).then_some(arg))
            .collect(),
        Some(_) => std::env::args_os().take(1).collect(),
        None => std::env::args_os().collect(),
    };
    let status = app.run_with_args_os(&arguments);
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
    let open = gio::ActionEntry::builder("open")
        .activate(|app: &gtk::Application, _, _| files::choose(app))
        .build();
    app.add_action_entries([new, quit, about, open]);

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
        .wrap(true)
        .margin_start(24)
        .margin_end(24)
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
    files::attach(&window, &label);
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
