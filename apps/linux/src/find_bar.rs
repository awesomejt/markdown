//! GTK 4 find bar widget and interactions (LNX-001F).

use gtk::prelude::*;

#[derive(Clone)]
pub struct FindBar {
    pub container: gtk::Revealer,
    pub entry: gtk::SearchEntry,
    pub prev_button: gtk::Button,
    pub next_button: gtk::Button,
    pub status_label: gtk::Label,
    pub case_button: gtk::CheckButton,
    pub word_button: gtk::CheckButton,
    pub close_button: gtk::Button,
}

impl FindBar {
    pub fn new() -> Self {
        let entry = gtk::SearchEntry::builder()
            .placeholder_text("Find in document…")
            .hexpand(false)
            .width_chars(24)
            .build();

        let prev_button = gtk::Button::builder()
            .label("‹")
            .tooltip_text("Find Previous (Shift+F3)")
            .build();

        let next_button = gtk::Button::builder()
            .label("›")
            .tooltip_text("Find Next (F3)")
            .build();

        let status_label = gtk::Label::builder()
            .xalign(0.0)
            .hexpand(false)
            .margin_start(4)
            .margin_end(4)
            .build();

        let case_button = gtk::CheckButton::builder()
            .label("Match Case")
            .tooltip_text("Case sensitive search")
            .build();

        let word_button = gtk::CheckButton::builder()
            .label("Whole Words")
            .tooltip_text("Match whole words only")
            .build();

        let close_button = gtk::Button::builder()
            .label("✕")
            .tooltip_text("Close (Escape)")
            .build();

        let box_widget = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        box_widget.set_margin_start(16);
        box_widget.set_margin_end(16);
        box_widget.set_margin_top(4);
        box_widget.set_margin_bottom(4);

        box_widget.append(&entry);
        box_widget.append(&prev_button);
        box_widget.append(&next_button);
        box_widget.append(&status_label);
        box_widget.append(&case_button);
        box_widget.append(&word_button);
        let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        spacer.set_hexpand(true);
        box_widget.append(&spacer);
        box_widget.append(&close_button);

        let container = gtk::Revealer::builder()
            .transition_type(gtk::RevealerTransitionType::SlideDown)
            .reveal_child(false)
            .child(&box_widget)
            .build();

        Self {
            container,
            entry,
            prev_button,
            next_button,
            status_label,
            case_button,
            word_button,
            close_button,
        }
    }

    pub fn is_revealed(&self) -> bool {
        self.container.reveals_child()
    }

    pub fn show(&self) {
        self.container.set_reveal_child(true);
        self.entry.grab_focus();
    }

    pub fn hide(&self) {
        self.container.set_reveal_child(false);
    }
}

impl Default for FindBar {
    fn default() -> Self {
        Self::new()
    }
}
