//! Native GTK 4 Preferences dialog (LNX-002D).

use std::cell::Cell;
use std::rc::Rc;

use gtk::prelude::*;
use markdown_linux::preferences::{
    self, Preferences, SCALE_STEPS, ThemePreference, closest_scale_index,
};

pub const PREFERENCES_WINDOW_NAME: &str = "preferences-window";

fn theme_to_index(theme: ThemePreference) -> u32 {
    match theme {
        ThemePreference::System => 0,
        ThemePreference::Light => 1,
        ThemePreference::Dark => 2,
    }
}

fn index_to_theme(idx: u32) -> ThemePreference {
    match idx {
        1 => ThemePreference::Light,
        2 => ThemePreference::Dark,
        _ => ThemePreference::System,
    }
}

pub fn show(app: &gtk::Application) {
    if let Some(existing) = app
        .windows()
        .into_iter()
        .find(|w| w.widget_name() == PREFERENCES_WINDOW_NAME)
    {
        existing.present();
        return;
    }

    let prefs = preferences::load();
    let updating = Rc::new(Cell::new(false));

    let dialog = gtk::Window::builder()
        .application(app)
        .title("Preferences")
        .name(PREFERENCES_WINDOW_NAME)
        .modal(true)
        .resizable(false)
        .default_width(440)
        .build();
    if let Some(parent) = app
        .active_window()
        .filter(|w| w.widget_name() != PREFERENCES_WINDOW_NAME)
    {
        dialog.set_transient_for(Some(&parent));
    }

    let root = gtk::Box::new(gtk::Orientation::Vertical, 14);
    root.set_margin_start(18);
    root.set_margin_end(18);
    root.set_margin_top(18);
    root.set_margin_bottom(18);

    // Appearance section
    let appearance_heading = gtk::Label::builder()
        .label("Appearance")
        .xalign(0.0)
        .build();
    appearance_heading.add_css_class("heading");
    root.append(&appearance_heading);

    let theme_row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let theme_label = gtk::Label::builder()
        .label("Theme")
        .xalign(0.0)
        .hexpand(true)
        .build();
    let theme_dropdown = gtk::DropDown::from_strings(&["System", "Light", "Dark"]);
    theme_dropdown.set_widget_name("pref-theme-dropdown");
    theme_dropdown.set_selected(theme_to_index(prefs.theme));
    theme_label.set_mnemonic_widget(Some(&theme_dropdown));
    theme_row.append(&theme_label);
    theme_row.append(&theme_dropdown);
    root.append(&theme_row);

    root.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

    // Reading section
    let reading_heading = gtk::Label::builder().label("Reading").xalign(0.0).build();
    reading_heading.add_css_class("heading");
    root.append(&reading_heading);

    let scale_row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let scale_label = gtk::Label::builder()
        .label("Default text size")
        .xalign(0.0)
        .hexpand(true)
        .build();
    let scale_labels: Vec<String> = SCALE_STEPS
        .iter()
        .map(|s| format!("{}%", (s * 100.0).round() as u32))
        .collect();
    let scale_refs: Vec<&str> = scale_labels.iter().map(|s| s.as_str()).collect();
    let scale_dropdown = gtk::DropDown::from_strings(&scale_refs);
    scale_dropdown.set_widget_name("pref-scale-dropdown");
    scale_dropdown.set_selected(closest_scale_index(prefs.text_scale) as u32);
    scale_label.set_mnemonic_widget(Some(&scale_dropdown));
    scale_row.append(&scale_label);
    scale_row.append(&scale_dropdown);
    root.append(&scale_row);

    let outline_check = gtk::CheckButton::with_label("Show document outline by default");
    outline_check.set_widget_name("pref-outline-check");
    outline_check.set_active(prefs.show_outline_by_default);
    root.append(&outline_check);

    root.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

    // Startup section
    let startup_heading = gtk::Label::builder().label("Startup").xalign(0.0).build();
    startup_heading.add_css_class("heading");
    root.append(&startup_heading);

    let restore_check = gtk::CheckButton::with_label("Restore previous session on launch");
    restore_check.set_widget_name("pref-restore-check");
    restore_check.set_active(prefs.restore_session_on_launch);
    root.append(&restore_check);

    // Footer buttons
    let footer = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    footer.set_margin_top(6);
    let reset_btn = gtk::Button::with_label("Reset to Defaults");
    reset_btn.set_widget_name("pref-reset-button");
    let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    let close_btn = gtk::Button::with_label("Close");
    close_btn.set_widget_name("pref-close-button");
    footer.append(&reset_btn);
    footer.append(&spacer);
    footer.append(&close_btn);
    root.append(&footer);

    dialog.set_child(Some(&root));

    // Signal handlers
    {
        let updating = updating.clone();
        theme_dropdown.connect_selected_notify(move |dd| {
            if updating.get() {
                return;
            }
            let theme = index_to_theme(dd.selected());
            crate::apply_theme(theme);
            let mut cur = preferences::load();
            cur.theme = theme;
            preferences::save(&cur);
        });
    }

    {
        let updating = updating.clone();
        let app = app.clone();
        scale_dropdown.connect_selected_notify(move |dd| {
            if updating.get() {
                return;
            }
            let idx = (dd.selected() as usize).min(SCALE_STEPS.len() - 1);
            let scale = SCALE_STEPS[idx];
            crate::files::apply_text_scale_to_all_windows(&app, scale);
            let mut cur = preferences::load();
            cur.text_scale = scale;
            preferences::save(&cur);
        });
    }

    {
        let updating = updating.clone();
        outline_check.connect_toggled(move |cb| {
            if updating.get() {
                return;
            }
            let mut cur = preferences::load();
            cur.show_outline_by_default = cb.is_active();
            preferences::save(&cur);
        });
    }

    {
        let updating = updating.clone();
        restore_check.connect_toggled(move |cb| {
            if updating.get() {
                return;
            }
            let mut cur = preferences::load();
            cur.restore_session_on_launch = cb.is_active();
            preferences::save(&cur);
        });
    }

    {
        let updating = updating.clone();
        let app = app.clone();
        let theme_dd = theme_dropdown.clone();
        let scale_dd = scale_dropdown.clone();
        let outline_cb = outline_check.clone();
        let restore_cb = restore_check.clone();
        reset_btn.connect_clicked(move |_| {
            let defaults = Preferences::default();
            updating.set(true);
            theme_dd.set_selected(theme_to_index(defaults.theme));
            scale_dd.set_selected(closest_scale_index(defaults.text_scale) as u32);
            outline_cb.set_active(defaults.show_outline_by_default);
            restore_cb.set_active(defaults.restore_session_on_launch);
            updating.set(false);

            crate::apply_theme(defaults.theme);
            crate::files::apply_text_scale_to_all_windows(&app, defaults.text_scale);
            preferences::save(&defaults);
        });
    }

    {
        let weak_dialog = dialog.downgrade();
        close_btn.connect_clicked(move |_| {
            if let Some(dlg) = weak_dialog.upgrade() {
                dlg.close();
            }
        });
    }

    let key_ctrl = gtk::EventControllerKey::new();
    let weak_dialog = dialog.downgrade();
    key_ctrl.connect_key_pressed(move |_, keyval, _, _| {
        if keyval == gtk::gdk::Key::Escape
            && let Some(dlg) = weak_dialog.upgrade()
        {
            dlg.close();
            return gtk::glib::Propagation::Stop;
        }
        gtk::glib::Propagation::Proceed
    });
    dialog.add_controller(key_ctrl);

    dialog.present();
}
