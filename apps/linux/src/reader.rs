//! GTK-owned virtual reader. Callbacks carry weak state and opaque row identities.
use crate::count_model::CountModel;
use gtk::{gdk, gio, glib, prelude::*};
use markdown_linux::render::{RenderPlan, Target};
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::Rc,
    sync::Arc,
};

type Binding = Rc<Cell<Option<(u64, u32)>>>;

struct Row {
    binding: Binding,
    labels: [glib::WeakRef<gtk::Label>; 2],
}

struct State {
    rows: HashMap<usize, Row>,
    plan: Option<Arc<RenderPlan>>,
    epoch: u64,
    view: gtk::ListView,
    window: glib::WeakRef<gtk::ApplicationWindow>,
    texture_cache: HashMap<String, gdk::Texture>,
}

#[derive(Clone)]
pub struct Reader {
    pub widget: gtk::Paned,
    state: Rc<RefCell<State>>,
    outline: gtk::ListView,
}

impl Reader {
    pub fn new(window: &gtk::ApplicationWindow) -> Self {
        let provider = gtk::CssProvider::new();
        provider.load_from_string(
            "
.table-row {
    padding-top: 4px;
    padding-bottom: 4px;
    border-bottom: 1px solid alpha(gray, 0.3);
}
.table-header {
    background-color: alpha(gray, 0.15);
    font-weight: bold;
    border-bottom: 2px solid gray;
}
.table-zebra {
    background-color: alpha(gray, 0.08);
}
",
        );
        if let Some(display) = gdk::Display::default() {
            gtk::style_context_add_provider_for_display(
                &display,
                &provider,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        }

        let factory = gtk::SignalListItemFactory::new();
        let view = gtk::ListView::new(None::<gtk::NoSelection>, Some(factory.clone()));
        view.set_widget_name("native-document");
        let state = Rc::new(RefCell::new(State {
            rows: HashMap::new(),
            plan: None,
            epoch: 0,
            view: view.clone(),
            window: window.downgrade(),
            texture_cache: HashMap::new(),
        }));
        let weak = Rc::downgrade(&state);
        factory.connect_setup(move |_, item| {
            let Some(item) = item.downcast_ref::<gtk::ListItem>() else {
                return;
            };
            let row = gtk::Box::new(gtk::Orientation::Vertical, 0);
            let binding = Rc::new(Cell::new(None::<(u64, u32)>));
            let mut labels = Vec::new();
            for heading in [true, false] {
                let label = gtk::Label::builder()
                    .xalign(0.0)
                    .wrap(true)
                    .selectable(true)
                    .width_chars(40)
                    .max_width_chars(40)
                    .hexpand(true)
                    .visible(false)
                    .margin_start(16)
                    .margin_end(16)
                    .margin_top(6)
                    .margin_bottom(6)
                    .accessible_role(if heading {
                        gtk::AccessibleRole::Heading
                    } else {
                        gtk::AccessibleRole::Paragraph
                    })
                    .build();
                label.set_wrap_mode(gtk::pango::WrapMode::WordChar);
                let weak = weak.clone();
                let bound = binding.clone();
                label.connect_activate_link(move |_, token| {
                    if let Some(state) = weak.upgrade() {
                        let action = {
                            let state = state.borrow();
                            bound.get().and_then(|(epoch, index)| {
                                if epoch != state.epoch {
                                    return None;
                                }
                                state
                                    .plan
                                    .as_ref()?
                                    .token_target(token, epoch, index)
                                    .cloned()
                            })
                        };
                        match action {
                            Some(Target::Fragment(index)) => {
                                let view = state.borrow().view.clone();
                                view.scroll_to(index, gtk::ListScrollFlags::FOCUS, None);
                            }
                            Some(Target::External(uri)) => {
                                let window = state.borrow().window.upgrade();
                                if let Some(window) = window {
                                    gtk::UriLauncher::new(&uri).launch(
                                        Some(&window),
                                        None::<&gio::Cancellable>,
                                        |_| {},
                                    );
                                }
                            }
                            _ => {}
                        }
                    }
                    glib::Propagation::Stop
                });
                labels.push(label.downgrade());
                row.append(&label);
            }
            let images_box = gtk::Box::new(gtk::Orientation::Vertical, 6);
            images_box.set_visible(false);
            images_box.set_margin_start(16);
            images_box.set_margin_end(16);
            images_box.set_margin_bottom(6);
            row.append(&images_box);

            if let Some(state) = weak.upgrade() {
                state.borrow_mut().rows.insert(
                    item.as_ptr() as usize,
                    Row {
                        binding,
                        labels: labels.try_into().unwrap(),
                    },
                );
            }
            item.set_child(Some(&row));
        });
        let weak = Rc::downgrade(&state);
        factory.connect_bind(move |_, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            let Some(state) = weak.upgrade() else {
                return;
            };
            let mut state = state.borrow_mut();
            let Some(row_state) = state.rows.get(&(item.as_ptr() as usize)) else {
                return;
            };
            let Some(plan) = state.plan.clone() else {
                return;
            };
            let index = item.position();
            let Some(fragment) = plan.fragments.get(index as usize) else {
                return;
            };
            let row = item.child().and_downcast::<gtk::Box>().unwrap();
            let heading_label = row.first_child().and_downcast::<gtk::Label>().unwrap();
            let paragraph_label = heading_label
                .next_sibling()
                .unwrap()
                .downcast::<gtk::Label>()
                .unwrap();
            let images_box = paragraph_label
                .next_sibling()
                .unwrap()
                .downcast::<gtk::Box>()
                .unwrap();

            let label = if fragment.heading_start {
                &heading_label
            } else {
                &paragraph_label
            };
            label.set_markup(&fragment.markup(state.epoch, index));
            if fragment.heading_start {
                label.update_property(&[gtk::accessible::Property::Level(fragment.heading as i32)]);
            }

            if fragment.is_table_row {
                let attrs = gtk::pango::AttrList::new();
                attrs.insert(gtk::pango::AttrString::new_family("monospace"));
                if fragment.is_table_header {
                    attrs.insert(gtk::pango::AttrInt::new_weight(gtk::pango::Weight::Bold));
                    row.add_css_class("table-header");
                } else if index % 2 == 1 {
                    row.add_css_class("table-zebra");
                }
                row.add_css_class("table-row");
                label.set_attributes(Some(&attrs));
            } else if fragment.heading > 0 {
                let attrs = gtk::pango::AttrList::new();
                attrs.insert(gtk::pango::AttrInt::new_weight(gtk::pango::Weight::Bold));
                attrs.insert(gtk::pango::AttrFloat::new_scale(1.6));
                label.set_attributes(Some(&attrs));
            } else if fragment.code {
                let attrs = gtk::pango::AttrList::new();
                attrs.insert(gtk::pango::AttrString::new_family("monospace"));
                label.set_attributes(Some(&attrs));
            }

            label.set_visible(true);
            row_state.binding.set(Some((state.epoch, index)));

            if !fragment.images.is_empty() {
                while let Some(c) = images_box.first_child() {
                    images_box.remove(&c);
                }
                for img in &fragment.images {
                    if let Some(err) = img.failure {
                        let msg = err.message();
                        let text = if img.alt.is_empty() {
                            format!("[{msg}]")
                        } else {
                            format!("[{} ({msg})]", img.alt)
                        };
                        let status_label = gtk::Label::builder()
                            .label(&text)
                            .xalign(0.0)
                            .wrap(true)
                            .accessible_role(gtk::AccessibleRole::Paragraph)
                            .build();
                        let attrs = gtk::pango::AttrList::new();
                        attrs.insert(gtk::pango::AttrInt::new_style(gtk::pango::Style::Italic));
                        status_label.set_attributes(Some(&attrs));
                        images_box.append(&status_label);
                    } else if let Some(data) = &img.data {
                        let texture = if let Some(tex) = state.texture_cache.get(&img.target) {
                            Some(tex.clone())
                        } else {
                            let bytes = glib::Bytes::from(data);
                            match gdk::Texture::from_bytes(&bytes) {
                                Ok(tex) => {
                                    state.texture_cache.insert(img.target.clone(), tex.clone());
                                    Some(tex)
                                }
                                Err(_) => None,
                            }
                        };
                        if let Some(texture) = texture {
                            let picture = gtk::Picture::for_paintable(&texture);
                            picture.set_can_shrink(true);
                            picture.set_halign(gtk::Align::Start);
                            picture.set_accessible_role(gtk::AccessibleRole::Img);
                            picture.update_property(&[gtk::accessible::Property::Description(
                                if img.alt.is_empty() {
                                    "Image"
                                } else {
                                    &img.alt
                                },
                            )]);
                            images_box.append(&picture);
                        }
                    }
                }
                images_box.set_visible(true);
            }
        });
        let weak = Rc::downgrade(&state);
        factory.connect_unbind(move |_, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            if let Some(state) = weak.upgrade()
                && let Some(binding) = state.borrow().rows.get(&(item.as_ptr() as usize))
            {
                binding.binding.set(None);
            }
            if let Some(row) = item.child().and_downcast::<gtk::Box>() {
                row.remove_css_class("table-row");
                row.remove_css_class("table-header");
                row.remove_css_class("table-zebra");
                let mut child = row.first_child();
                while let Some(widget) = child {
                    child = widget.next_sibling();
                    if let Ok(label) = widget.clone().downcast::<gtk::Label>() {
                        label.select_region(0, 0);
                        label.set_text("");
                        label.set_attributes(None);
                        label.set_visible(false);
                    } else if let Ok(box_widget) = widget.downcast::<gtk::Box>() {
                        while let Some(c) = box_widget.first_child() {
                            box_widget.remove(&c);
                        }
                        box_widget.set_visible(false);
                    }
                }
            }
        });
        let weak = Rc::downgrade(&state);
        factory.connect_teardown(move |_, item| {
            if let Some(state) = weak.upgrade() {
                state.borrow_mut().rows.remove(&(item.as_ptr() as usize));
            }
        });
        let outline_factory = gtk::SignalListItemFactory::new();
        outline_factory.connect_setup(|_, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            item.set_child(Some(
                &gtk::Label::builder()
                    .xalign(0.0)
                    .ellipsize(gtk::pango::EllipsizeMode::End)
                    .margin_start(8)
                    .margin_end(8)
                    .margin_top(6)
                    .margin_bottom(6)
                    .build(),
            ));
        });
        let weak = Rc::downgrade(&state);
        outline_factory.connect_bind(move |_, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            let label = item.child().and_downcast::<gtk::Label>().unwrap();
            if let Some(state) = weak.upgrade() {
                let state = state.borrow();
                if let Some(entry) = state
                    .plan
                    .as_ref()
                    .and_then(|p| p.outline.get(item.position() as usize))
                {
                    label.set_text(&entry.title);
                    label.set_margin_start(8 + (entry.level.saturating_sub(1) * 8) as i32);
                }
            }
        });
        outline_factory.connect_unbind(|_, item| {
            if let Some(label) = item
                .downcast_ref::<gtk::ListItem>()
                .and_then(|i| i.child())
                .and_downcast::<gtk::Label>()
            {
                label.set_text("");
            }
        });
        let outline = gtk::ListView::new(None::<gtk::SingleSelection>, Some(outline_factory));
        outline.set_widget_name("document-outline");
        let weak = Rc::downgrade(&state);
        outline.connect_activate(move |_, index| {
            let jump = weak.upgrade().and_then(|state| {
                let state = state.borrow();
                Some((
                    state.view.clone(),
                    state.plan.as_ref()?.outline.get(index as usize)?.fragment,
                ))
            });
            if let Some((view, index)) = jump {
                view.scroll_to(index, gtk::ListScrollFlags::FOCUS, None);
            }
        });
        let widget = gtk::Paned::new(gtk::Orientation::Horizontal);
        widget.set_start_child(Some(
            &gtk::ScrolledWindow::builder()
                .min_content_width(180)
                .child(&outline)
                .build(),
        ));
        widget.set_end_child(Some(
            &gtk::ScrolledWindow::builder()
                .hexpand(true)
                .vexpand(true)
                .child(&view)
                .build(),
        ));
        widget.set_position(210);
        Self {
            widget,
            state,
            outline,
        }
    }
    // Native smoke observations: no clipboard or external URI dispatch.
    pub fn observed(&self) -> Vec<(u32, gtk::Label)> {
        let state = self.state.borrow();
        state
            .rows
            .values()
            .filter_map(|row| {
                let (epoch, index) = row.binding.get()?;
                if epoch != state.epoch {
                    return None;
                }
                row.labels
                    .iter()
                    .filter_map(|l| l.upgrade())
                    .find(|l| l.is_visible())
                    .map(|label| (index, label))
            })
            .collect()
    }
    pub fn row_count(&self) -> usize {
        self.state.borrow().rows.len()
    }
    pub fn activate_outline(&self, index: u32) {
        self.outline.emit_by_name::<()>("activate", &[&index]);
    }
    pub fn activate_fragment_link(&self) -> bool {
        let observed = self.observed();
        let action = {
            let state = self.state.borrow();
            state.plan.as_ref().and_then(|plan| {
                observed.iter().find_map(|(index, label)| {
                    plan.fragments[*index as usize].runs.iter().find_map(|run| {
                        let link = run.link?;
                        if matches!(plan.targets.get(link as usize), Some(Target::Fragment(_))) {
                            Some((label.clone(), format!("md:{}:{index}:{link}", state.epoch)))
                        } else {
                            None
                        }
                    })
                })
            })
        };
        if let Some((label, token)) = action {
            label.emit_by_name::<bool>("activate-link", &[&token]);
            true
        } else {
            false
        }
    }
    pub fn install(&self, plan: Arc<RenderPlan>) {
        self.clear();
        let fragments = plan.fragments.len() as u32;
        let entries = plan.outline.len() as u32;
        self.state.borrow_mut().plan = Some(plan);
        let view = self.state.borrow().view.clone();
        view.set_model(Some(&gtk::NoSelection::new(Some(CountModel::new(
            fragments,
        )))));
        self.outline
            .set_model(Some(&gtk::SingleSelection::new(Some(CountModel::new(
                entries,
            )))));
        self.widget.set_visible(true);
    }
    pub fn clear(&self) {
        // Model changes emit row signals; never hold the state borrow across them.
        let view = self.state.borrow().view.clone();
        view.set_model(None::<&gtk::NoSelection>);
        self.outline.set_model(None::<&gtk::SingleSelection>);
        let old = {
            let mut state = self.state.borrow_mut();
            state.epoch = state.epoch.checked_add(1).expect("reader epoch exhausted");
            state.texture_cache.clear();
            state.plan.take()
        };
        if let Some(old) = old {
            // Builder::spawn reports failure instead of panicking. If no thread
            // can start, dropping the rejected closure releases the plan here.
            let _ = std::thread::Builder::new()
                .name("markdown-plan-dispose".into())
                .spawn(move || drop(old));
        }
        self.widget.set_visible(false);
    }
}
