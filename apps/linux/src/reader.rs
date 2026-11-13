//! GTK-owned virtual reader. Callbacks carry weak state and opaque row identities.
use crate::count_model::CountModel;
use gtk::{gdk, gio, glib, prelude::*};
use markdown_linux::{
    document::ResourceError,
    render::{DocumentPosition, DocumentSelection, RenderPlan, Target},
};
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::Rc,
    sync::Arc,
};

type Binding = Rc<Cell<Option<(u64, u32)>>>;

struct Row {
    binding: Binding,
    row_box: glib::WeakRef<gtk::Box>,
    labels: [glib::WeakRef<gtk::Label>; 2],
}

struct State {
    rows: HashMap<usize, Row>,
    plan: Option<Arc<RenderPlan>>,
    document: Option<Arc<markdown_linux_core::CoreDocument>>,
    epoch: u64,
    view: gtk::ListView,
    window: glib::WeakRef<gtk::ApplicationWindow>,
    texture_cache: HashMap<String, gdk::Texture>,
    find_matches: Option<Arc<markdown_linux::render::FindMatches>>,
    current_find_match: Option<usize>,
    selection: Option<DocumentSelection>,
    drag_active: bool,
    drag_extend: bool,
    drag_start: (f64, f64),
    drag_last: (f64, f64),
    drag_anchor: Option<DocumentPosition>,
    drag_timer: Option<glib::SourceId>,
    scrolled: glib::WeakRef<gtk::ScrolledWindow>,
    text_scale: f64,
    scale_provider: gtk::CssProvider,
    class_name: String,
}

impl Drop for State {
    fn drop(&mut self) {
        if let Some(timer) = self.drag_timer.take() {
            timer.remove();
        }
        if let Some(display) = gdk::Display::default() {
            gtk::style_context_remove_provider_for_display(&display, &self.scale_provider);
        }
    }
}

#[derive(Clone)]
pub struct Reader {
    pub widget: gtk::Paned,
    state: Rc<RefCell<State>>,
    outline: gtk::ListView,
}

static NEXT_READER_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
static INIT_TABLE_CSS: std::sync::Once = std::sync::Once::new();

impl Reader {
    pub fn new(window: &gtk::ApplicationWindow) -> Self {
        INIT_TABLE_CSS.call_once(|| {
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
        });

        let scale_provider = gtk::CssProvider::new();
        if let Some(display) = gdk::Display::default() {
            gtk::style_context_add_provider_for_display(
                &display,
                &scale_provider,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION + 1,
            );
        }

        let reader_id = NEXT_READER_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let class_name = format!("reader-scale-{reader_id}");

        let factory = gtk::SignalListItemFactory::new();
        let view = gtk::ListView::new(None::<gtk::NoSelection>, Some(factory.clone()));
        view.set_widget_name("native-document");
        view.add_css_class(&class_name);
        let state = Rc::new(RefCell::new(State {
            rows: HashMap::new(),
            plan: None,
            document: None,
            epoch: 0,
            view: view.clone(),
            window: window.downgrade(),
            texture_cache: HashMap::new(),
            find_matches: None,
            current_find_match: None,
            selection: None,
            drag_active: false,
            drag_extend: false,
            drag_start: (0.0, 0.0),
            drag_last: (0.0, 0.0),
            drag_anchor: None,
            drag_timer: None,
            scrolled: glib::WeakRef::new(),
            text_scale: 1.0,
            scale_provider,
            class_name,
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
                        row_box: row.downgrade(),
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
            let markup = compute_fragment_markup(
                fragment,
                state.epoch,
                index,
                &plan,
                state.find_matches.as_deref(),
                state.current_find_match,
                state.selection,
            );
            label.set_markup(&markup);
            if fragment.heading_start {
                label.update_property(&[gtk::accessible::Property::Level(fragment.heading as i32)]);
            }

            if fragment.is_table_row {
                let attrs = gtk::pango::AttrList::new();
                attrs.insert(gtk::pango::AttrString::new_family("monospace"));
                if fragment.is_table_header {
                    attrs.insert(gtk::pango::AttrInt::new_weight(gtk::pango::Weight::Bold));
                    row.add_css_class("table-header");
                } else if fragment.table_row_ordinal % 2 == 1 {
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
                            let bytes = glib::Bytes::from(data.as_ref());
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
                        } else {
                            let msg = ResourceError::Undecodable.message();
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
                view.scroll_to(index, gtk::ListScrollFlags::NONE, None);
            }
        });
        let widget = gtk::Paned::new(gtk::Orientation::Horizontal);
        widget.set_start_child(Some(
            &gtk::ScrolledWindow::builder()
                .min_content_width(180)
                .child(&outline)
                .build(),
        ));
        let scrolled = gtk::ScrolledWindow::builder()
            .hexpand(true)
            .vexpand(true)
            .child(&view)
            .build();
        widget.set_end_child(Some(&scrolled));
        widget.set_position(210);
        let this = Self {
            widget,
            state,
            outline,
        };
        this.attach_controllers(&scrolled);
        this
    }

    fn attach_controllers(&self, scrolled: &gtk::ScrolledWindow) {
        self.state.borrow_mut().scrolled = scrolled.downgrade();
        let view = self.state.borrow().view.clone();

        let drag = gtk::GestureDrag::new();
        drag.set_propagation_phase(gtk::PropagationPhase::Capture);
        drag.set_button(gdk::BUTTON_PRIMARY);

        let weak = Rc::downgrade(&self.state);
        drag.connect_drag_begin(move |gesture, x, y| {
            let Some(state) = weak.upgrade() else {
                return;
            };
            let extend = gesture
                .current_event_state()
                .contains(gdk::ModifierType::SHIFT_MASK);
            let view = {
                let mut s = state.borrow_mut();
                s.drag_start = (x, y);
                s.drag_last = (x, y);
                s.drag_active = false;
                s.drag_extend = extend;
                s.drag_anchor = None;
                if let Some(t) = s.drag_timer.take() {
                    t.remove();
                }
                s.view.clone()
            };
            view.grab_focus();
            // Sequence is NOT claimed here so plain clicks still reach child widgets (e.g. links).
        });

        let weak = Rc::downgrade(&self.state);
        drag.connect_drag_update(move |gesture, dx, dy| {
            let Some(state) = weak.upgrade() else {
                return;
            };
            let (start, extend, active) = {
                let s = state.borrow();
                (s.drag_start, s.drag_extend, s.drag_active)
            };

            if !active {
                if dx * dx + dy * dy < 16.0 {
                    return;
                }
                // Crossed 4 px drag threshold: claim sequence so ListView does not interpret as scroll
                gesture.set_state(gtk::EventSequenceState::Claimed);
                let anchor = if extend {
                    let s = state.borrow();
                    s.selection
                        .map(|sel| sel.anchor)
                        .or_else(|| s.hit_test(start.0, start.1))
                } else {
                    state.borrow().hit_test(start.0, start.1)
                };
                let Some(anchor) = anchor else {
                    return;
                };
                {
                    let mut s = state.borrow_mut();
                    s.drag_anchor = Some(anchor);
                    s.drag_active = true;
                }

                let auto_weak = Rc::downgrade(&state);
                let timer =
                    glib::timeout_add_local(std::time::Duration::from_millis(16), move || {
                        let Some(state) = auto_weak.upgrade() else {
                            return glib::ControlFlow::Break;
                        };
                        let (delta, adj_opt) = {
                            let s = state.borrow();
                            let y = s.drag_last.1;
                            let height = s.view.height() as f64;
                            let scrolled_opt = s.scrolled.upgrade();
                            if let Some(scrolled) = scrolled_opt {
                                let adj = scrolled.vadjustment();
                                let delta = if y < 24.0 {
                                    (y - 24.0).max(-150.0)
                                } else if y > height - 24.0 {
                                    (y - height + 24.0).min(150.0)
                                } else {
                                    0.0
                                };
                                (delta, Some(adj))
                            } else {
                                (0.0, None)
                            }
                        };
                        if delta != 0.0
                            && let Some(adj) = adj_opt
                        {
                            adj.set_value(adj.value() + delta);
                            if let Some(state) = auto_weak.upgrade() {
                                state.borrow_mut().drag_to_last();
                            }
                        }
                        glib::ControlFlow::Continue
                    });
                state.borrow_mut().drag_timer = Some(timer);
            }

            {
                let mut s = state.borrow_mut();
                s.drag_last = (start.0 + dx, start.1 + dy);
                s.drag_to_last();
            }
        });

        let weak = Rc::downgrade(&self.state);
        drag.connect_drag_end(move |_, dx, dy| {
            let Some(state) = weak.upgrade() else {
                return;
            };
            let (active, extend, start) = {
                let mut s = state.borrow_mut();
                if let Some(t) = s.drag_timer.take() {
                    t.remove();
                }
                let active = s.drag_active;
                let extend = s.drag_extend;
                let start = s.drag_start;
                s.drag_active = false;
                (active, extend, start)
            };

            if !active && !extend && (dx * dx + dy * dy < 16.0) {
                state.borrow_mut().clear_selection();
            } else if !active && extend {
                let pos = state.borrow().hit_test(start.0, start.1);
                if let Some(pos) = pos {
                    state.borrow_mut().extend_selection(pos);
                }
            }
        });
        view.add_controller(drag);

        let key_ctrl = gtk::EventControllerKey::new();
        key_ctrl.set_propagation_phase(gtk::PropagationPhase::Capture);
        let weak = Rc::downgrade(&self.state);
        key_ctrl.connect_key_pressed(move |_, key, _, state| {
            let Some(state_rc) = weak.upgrade() else {
                return glib::Propagation::Proceed;
            };
            let ctrl = state.contains(gdk::ModifierType::CONTROL_MASK);
            let shift = state.contains(gdk::ModifierType::SHIFT_MASK);
            match key {
                gdk::Key::a | gdk::Key::A if ctrl => {
                    state_rc.borrow_mut().select_all();
                    glib::Propagation::Stop
                }
                gdk::Key::c | gdk::Key::C | gdk::Key::Insert if ctrl => {
                    state_rc.borrow().copy_selection();
                    glib::Propagation::Stop
                }
                gdk::Key::Escape => {
                    if state_rc.borrow().has_selection() {
                        state_rc.borrow_mut().clear_selection();
                        glib::Propagation::Stop
                    } else {
                        glib::Propagation::Proceed
                    }
                }
                gdk::Key::End if ctrl && shift => {
                    let (target, view) = {
                        let mut s = state_rc.borrow_mut();
                        (s.extend_to_end(), s.view.clone())
                    };
                    view.scroll_to(
                        target.fragment_index as u32,
                        gtk::ListScrollFlags::FOCUS,
                        None,
                    );
                    glib::Propagation::Stop
                }
                gdk::Key::Home if ctrl && shift => {
                    let (target, view) = {
                        let mut s = state_rc.borrow_mut();
                        (s.extend_to_start(), s.view.clone())
                    };
                    view.scroll_to(
                        target.fragment_index as u32,
                        gtk::ListScrollFlags::FOCUS,
                        None,
                    );
                    glib::Propagation::Stop
                }
                gdk::Key::Right if shift => {
                    let (target, view) = {
                        let mut s = state_rc.borrow_mut();
                        (s.step_selection(1), s.view.clone())
                    };
                    if let Some(target) = target {
                        view.scroll_to(
                            target.fragment_index as u32,
                            gtk::ListScrollFlags::FOCUS,
                            None,
                        );
                    }
                    glib::Propagation::Stop
                }
                gdk::Key::Left if shift => {
                    let (target, view) = {
                        let mut s = state_rc.borrow_mut();
                        (s.step_selection(-1), s.view.clone())
                    };
                    if let Some(target) = target {
                        view.scroll_to(
                            target.fragment_index as u32,
                            gtk::ListScrollFlags::FOCUS,
                            None,
                        );
                    }
                    glib::Propagation::Stop
                }
                gdk::Key::Down if shift => {
                    let (target, view) = {
                        let mut s = state_rc.borrow_mut();
                        (s.step_fragment_selection(true), s.view.clone())
                    };
                    if let Some(target) = target {
                        view.scroll_to(
                            target.fragment_index as u32,
                            gtk::ListScrollFlags::FOCUS,
                            None,
                        );
                    }
                    glib::Propagation::Stop
                }
                gdk::Key::Up if shift => {
                    let (target, view) = {
                        let mut s = state_rc.borrow_mut();
                        (s.step_fragment_selection(false), s.view.clone())
                    };
                    if let Some(target) = target {
                        view.scroll_to(
                            target.fragment_index as u32,
                            gtk::ListScrollFlags::FOCUS,
                            None,
                        );
                    }
                    glib::Propagation::Stop
                }
                _ => glib::Propagation::Proceed,
            }
        });
        view.add_controller(key_ctrl);
    }

    pub fn hit_test(&self, x: f64, y: f64) -> Option<DocumentPosition> {
        self.state.borrow().hit_test(x, y)
    }

    // Native smoke observations: no clipboard or external URI dispatch.
    pub fn observed(&self) -> Vec<(u32, gtk::Label)> {
        self.state.borrow().observed()
    }

    pub fn row_count(&self) -> usize {
        self.state.borrow().rows.len()
    }

    pub fn plan(&self) -> Option<Arc<RenderPlan>> {
        self.state.borrow().plan.clone()
    }

    pub fn document(&self) -> Option<Arc<markdown_linux_core::CoreDocument>> {
        self.state.borrow().document.clone()
    }

    pub fn selection(&self) -> Option<DocumentSelection> {
        self.state.borrow().selection
    }

    pub fn has_selection(&self) -> bool {
        self.state.borrow().selection.is_some_and(|s| !s.is_empty())
    }

    pub fn set_selection(&self, selection: Option<DocumentSelection>) {
        self.state.borrow_mut().set_selection(selection);
    }

    pub fn clear_selection(&self) {
        self.state.borrow_mut().clear_selection();
    }

    pub fn select_all(&self) {
        self.state.borrow_mut().select_all();
    }

    pub fn selected_text(&self) -> Option<String> {
        self.state.borrow().selected_text()
    }

    pub fn selected_markdown(&self) -> Option<Vec<u8>> {
        self.state.borrow().selected_markdown()
    }

    pub fn copy_selection(&self) -> bool {
        self.state.borrow().copy_selection()
    }

    pub fn extend_selection(&self, target: DocumentPosition) {
        self.state.borrow_mut().extend_selection(target);
        self.scroll_to_fragment(target.fragment_index);
    }

    pub fn scroll_to_fragment(&self, fragment_index: usize) {
        let view = self.state.borrow().view.clone();
        view.scroll_to(fragment_index as u32, gtk::ListScrollFlags::NONE, None);
    }

    pub fn step_selection(&self, delta: isize) {
        let target = self.state.borrow_mut().step_selection(delta);
        if let Some(target) = target {
            self.scroll_to_fragment(target.fragment_index);
        }
    }

    pub fn step_fragment_selection(&self, forward: bool) {
        let target = self.state.borrow_mut().step_fragment_selection(forward);
        if let Some(target) = target {
            self.scroll_to_fragment(target.fragment_index);
        }
    }

    pub fn extend_to_start(&self) {
        let target = self.state.borrow_mut().extend_to_start();
        self.scroll_to_fragment(target.fragment_index);
    }

    pub fn extend_to_end(&self) {
        let target = self.state.borrow_mut().extend_to_end();
        self.scroll_to_fragment(target.fragment_index);
    }

    pub fn activate_outline(&self, index: u32) {
        self.outline.emit_by_name::<()>("activate", &[&index]);
    }

    pub fn is_outline_visible(&self) -> bool {
        self.widget.start_child().is_some_and(|w| w.is_visible())
    }

    pub fn set_outline_visible(&self, visible: bool) {
        if let Some(w) = self.widget.start_child() {
            w.set_visible(visible);
        }
    }

    pub fn save_position(
        &self,
        document: &markdown_linux_core::CoreDocument,
    ) -> Option<markdown_linux::restoration::SavedPosition> {
        let plan = self.plan()?;
        if plan.fragments.is_empty() {
            return None;
        }
        let top_frag = self.first_visible_fragment().unwrap_or(0) as usize;
        let top_offset = plan.source_offset_at(DocumentPosition::new(top_frag, 0), document);
        let top_anchor = document.anchor_at(top_offset).ok()?;

        if let Some(sel) = self.selection()
            && !sel.is_empty()
        {
            let start_offset = plan.source_offset_at(sel.start(), document);
            let end_offset = plan.source_offset_at(sel.end(), document);
            let start_anchor = document.anchor_at(start_offset).ok()?;
            let end_anchor = document.anchor_at(end_offset).ok()?;
            return Some(markdown_linux::restoration::SavedPosition {
                top: top_anchor,
                selection_start: start_anchor,
                selection_end: Some(end_anchor),
            });
        }

        Some(markdown_linux::restoration::SavedPosition {
            top: top_anchor.clone(),
            selection_start: top_anchor,
            selection_end: None,
        })
    }

    pub fn restore_position(
        &self,
        saved: &markdown_linux::restoration::SavedPosition,
        document: &markdown_linux_core::CoreDocument,
    ) {
        let Some(plan) = self.plan() else { return };
        if plan.fragments.is_empty() {
            return;
        }

        if let Ok(Some(top_res)) = document.resolve_anchor(&saved.top) {
            let top_pos = plan.position_at_source_offset(top_res.source_offset, document);
            self.scroll_to_fragment(top_pos.fragment_index);
        }

        if let Some(ref end_anchor) = saved.selection_end {
            if let (Ok(Some(start_res)), Ok(Some(end_res))) = (
                document.resolve_anchor(&saved.selection_start),
                document.resolve_anchor(end_anchor),
            ) {
                let start_pos = plan.position_at_source_offset(start_res.source_offset, document);
                let end_pos = plan.position_at_source_offset(end_res.source_offset, document);
                self.set_selection(Some(DocumentSelection::new(start_pos, end_pos)));
            } else {
                self.clear_selection();
            }
        } else {
            self.clear_selection();
        }
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
        self.install_document(plan, None);
    }

    pub fn install_document(
        &self,
        plan: Arc<RenderPlan>,
        document: Option<Arc<markdown_linux_core::CoreDocument>>,
    ) {
        self.clear();
        let fragments = plan.fragments.len() as u32;
        let entries = plan.outline.len() as u32;
        {
            let mut state = self.state.borrow_mut();
            state.plan = Some(plan);
            state.document = document;
        }
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
            state.find_matches = None;
            state.current_find_match = None;
            state.selection = None;
            state.document = None;
            if let Some(t) = state.drag_timer.take() {
                t.remove();
            }
            state.drag_active = false;
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

    pub fn text_scale(&self) -> f64 {
        self.state.borrow().text_scale
    }

    pub fn set_text_scale(&self, scale: f64) {
        let clamped = markdown_linux::preferences::clamped_scale(scale);
        let anchor = self.first_visible_fragment();
        {
            let mut state = self.state.borrow_mut();
            state.text_scale = clamped;
            let percent = (clamped * 100.0).round() as i32;
            let class = &state.class_name;
            let css = format!(".{class} label {{ font-size: {percent}%; }}\n");
            state.scale_provider.load_from_string(&css);
        }
        if let Some(anchor) = anchor {
            let view = self.state.borrow().view.clone();
            view.scroll_to(anchor, gtk::ListScrollFlags::NONE, None);
        }
    }

    pub fn first_visible_fragment(&self) -> Option<u32> {
        self.state.borrow().first_visible_fragment()
    }

    pub fn find_matches(&self) -> Option<Arc<markdown_linux::render::FindMatches>> {
        self.state.borrow().find_matches.clone()
    }

    pub fn current_find_match(&self) -> Option<usize> {
        self.state.borrow().current_find_match
    }

    pub fn set_find_matches(&self, matches: Option<Arc<markdown_linux::render::FindMatches>>) {
        {
            let mut state = self.state.borrow_mut();
            state.find_matches = matches;
            state.current_find_match = if state.find_matches.as_ref().is_none_or(|m| m.is_empty()) {
                None
            } else {
                Some(0)
            };
        }
        self.refresh_markup();
        if let Some(match_idx) = self.current_find_match() {
            self.scroll_to_match(match_idx);
        }
    }

    pub fn select_find_match(&self, match_idx: usize) {
        {
            let mut state = self.state.borrow_mut();
            if let Some(matches) = &state.find_matches {
                if match_idx < matches.count() {
                    state.current_find_match = Some(match_idx);
                } else {
                    return;
                }
            } else {
                return;
            }
        }
        self.refresh_markup();
        self.scroll_to_match(match_idx);
    }

    pub fn scroll_to_match(&self, match_idx: usize) {
        let (view, frag_idx) = {
            let state = self.state.borrow();
            let Some(matches) = &state.find_matches else {
                return;
            };
            let Some(m) = matches.matches.get(match_idx) else {
                return;
            };
            (state.view.clone(), m.primary_fragment_index as u32)
        };
        view.scroll_to(frag_idx, gtk::ListScrollFlags::FOCUS, None);
    }

    pub fn refresh_markup(&self) {
        self.state.borrow().refresh_markup();
    }
}

impl State {
    pub fn observed(&self) -> Vec<(u32, gtk::Label)> {
        self.rows
            .values()
            .filter_map(|row| {
                let (epoch, index) = row.binding.get()?;
                if epoch != self.epoch {
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

    pub fn first_visible_fragment(&self) -> Option<u32> {
        let observed = self.observed();
        if let Some(scrolled) = self.scrolled.upgrade() {
            let mut in_view: Vec<(u32, f32)> = observed
                .iter()
                .filter_map(|(idx, label)| {
                    let bounds = label.compute_bounds(&scrolled)?;
                    if bounds.height() > 0.0 && bounds.y() + bounds.height() > 0.0 {
                        Some((*idx, bounds.y()))
                    } else {
                        None
                    }
                })
                .collect();
            if !in_view.is_empty() {
                in_view.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
                return in_view.first().map(|(idx, _)| *idx);
            }
        }
        let mut fallback = observed;
        fallback.sort_by_key(|(idx, _)| *idx);
        fallback.first().map(|(idx, _)| *idx)
    }

    pub fn default_anchor(&self) -> DocumentPosition {
        let frag = self.first_visible_fragment().unwrap_or(0) as usize;
        DocumentPosition::new(frag, 0)
    }

    pub fn hit_test(&self, x: f64, y: f64) -> Option<DocumentPosition> {
        let plan = self.plan.as_ref()?;
        if plan.fragments.is_empty() {
            return None;
        }
        let height = self.view.height() as f64;
        let y_clamped = y.clamp(1.0, (height - 1.0).max(1.0));

        for dy in [0.0, -4.0, 4.0, -10.0, 10.0, -20.0, 20.0] {
            let test_y = (y_clamped + dy).clamp(1.0, (height - 1.0).max(1.0));
            let Some(widget) = self.view.pick(x, test_y, gtk::PickFlags::DEFAULT) else {
                continue;
            };

            let mut found_row = None;
            for row in self.rows.values() {
                let Some(row_box) = row.row_box.upgrade() else {
                    continue;
                };
                if (&widget == row_box.upcast_ref::<gtk::Widget>() || widget.is_ancestor(&row_box))
                    && let Some((epoch, index)) = row.binding.get()
                    && epoch == self.epoch
                    && let Some(label) = row
                        .labels
                        .iter()
                        .filter_map(|l| l.upgrade())
                        .find(|l| l.is_visible())
                {
                    found_row = Some((index, label));
                    break;
                }
            }

            let Some((index, label)) = found_row else {
                continue;
            };

            let Some(point) = self
                .view
                .compute_point(&label, &gtk::graphene::Point::new(x as f32, test_y as f32))
            else {
                continue;
            };

            let (ox, oy) = label.layout_offsets();
            let layout = label.layout();
            let lx = ((point.x() - ox as f32) * gtk::pango::SCALE as f32) as i32;
            let ly = ((point.y() - oy as f32) * gtk::pango::SCALE as f32) as i32;
            let text = label.text();
            let (_, byte, trailing) = layout.xy_to_index(lx, ly);
            let mut byte = byte.max(0) as usize;
            for _ in 0..trailing {
                byte += text[byte..].chars().next().map_or(0, char::len_utf8);
            }
            let byte = byte.min(text.len());
            let char_offset = text[..byte].chars().count();
            return Some(DocumentPosition::new(index as usize, char_offset));
        }

        if y < 1.0 {
            return Some(DocumentPosition::new(0, 0));
        }
        let last_frag = plan.fragments.len() - 1;
        let last_len = plan.fragments[last_frag].text().chars().count();
        Some(DocumentPosition::new(last_frag, last_len))
    }

    pub fn set_selection(&mut self, selection: Option<DocumentSelection>) {
        self.selection = selection.filter(|s| !s.is_empty());
        self.refresh_markup();
    }

    pub fn clear_selection(&mut self) {
        self.set_selection(None);
    }

    pub fn has_selection(&self) -> bool {
        self.selection.is_some_and(|s| !s.is_empty())
    }

    pub fn select_all(&mut self) {
        let (last_frag, last_len) = {
            let Some(plan) = &self.plan else {
                return;
            };
            if plan.fragments.is_empty() {
                return;
            }
            let last_idx = plan.fragments.len() - 1;
            let len = plan.fragments[last_idx].text().chars().count();
            (last_idx, len)
        };
        self.set_selection(Some(DocumentSelection::new(
            DocumentPosition::new(0, 0),
            DocumentPosition::new(last_frag, last_len),
        )));
    }

    pub fn selected_text(&self) -> Option<String> {
        let sel = self.selection?;
        if sel.is_empty() {
            return None;
        }
        let plan = self.plan.as_ref()?;
        Some(plan.text_between(sel.start(), sel.end()))
    }

    pub fn selected_markdown(&self) -> Option<Vec<u8>> {
        let sel = self.selection?;
        if sel.is_empty() {
            return None;
        }
        let plan = self.plan.as_ref()?;
        let start = sel.start();
        let end = sel.end();
        if let Some(doc) = &self.document
            && let Some(src) = plan.source_between(start, end, doc)
        {
            Some(src.to_vec())
        } else {
            Some(plan.text_between(start, end).into_bytes())
        }
    }

    pub fn copy_selection(&self) -> bool {
        let Some(sel) = self.selection else {
            return false;
        };
        if sel.is_empty() {
            return false;
        }
        let Some(plan) = &self.plan else {
            return false;
        };
        let start = sel.start();
        let end = sel.end();
        let plain = plan.text_between(start, end);
        let markdown: Vec<u8> = if let Some(doc) = &self.document
            && let Some(src) = plan.source_between(start, end, doc)
        {
            src.to_vec()
        } else {
            plain.as_bytes().to_vec()
        };

        let provider = gdk::ContentProvider::new_union(&[
            gdk::ContentProvider::for_value(&plain.to_value()),
            gdk::ContentProvider::for_bytes("text/markdown", &glib::Bytes::from(&markdown)),
        ]);

        if let Some(display) = gdk::Display::default() {
            display.clipboard().set_content(Some(&provider)).is_ok()
        } else {
            false
        }
    }

    pub fn extend_selection(&mut self, target: DocumentPosition) {
        let anchor = self
            .selection
            .map(|s| s.anchor)
            .unwrap_or_else(|| self.default_anchor());
        self.set_selection(Some(DocumentSelection::new(anchor, target)));
    }

    pub fn extend_to_start(&mut self) -> DocumentPosition {
        let target = DocumentPosition::new(0, 0);
        self.extend_selection(target);
        target
    }

    pub fn extend_to_end(&mut self) -> DocumentPosition {
        let (last_idx, last_len) = self.plan.as_ref().map_or((0, 0), |p| {
            if p.fragments.is_empty() {
                (0, 0)
            } else {
                let last = p.fragments.len() - 1;
                let len = p.fragments[last].text().chars().count();
                (last, len)
            }
        });
        let target = DocumentPosition::new(last_idx, last_len);
        self.extend_selection(target);
        target
    }

    pub fn step_selection(&mut self, delta: isize) -> Option<DocumentPosition> {
        let plan = self.plan.as_ref()?;
        let plan_len = plan.fragments.len();
        if plan_len == 0 {
            return None;
        }

        let (anchor, cur_pos) = if let Some(sel) = self.selection {
            (sel.anchor, sel.active)
        } else {
            let def = self.default_anchor();
            (def, def)
        };

        let frag_len = plan.fragments[cur_pos.fragment_index]
            .text()
            .chars()
            .count();

        let next_pos = if delta > 0 {
            if cur_pos.char_offset < frag_len {
                DocumentPosition::new(cur_pos.fragment_index, cur_pos.char_offset + 1)
            } else if cur_pos.fragment_index + 1 < plan_len {
                DocumentPosition::new(cur_pos.fragment_index + 1, 0)
            } else {
                cur_pos
            }
        } else if cur_pos.char_offset > 0 {
            DocumentPosition::new(cur_pos.fragment_index, cur_pos.char_offset - 1)
        } else if cur_pos.fragment_index > 0 {
            let prev_len = plan.fragments[cur_pos.fragment_index - 1]
                .text()
                .chars()
                .count();
            DocumentPosition::new(cur_pos.fragment_index - 1, prev_len)
        } else {
            cur_pos
        };

        self.set_selection(Some(DocumentSelection::new(anchor, next_pos)));
        Some(next_pos)
    }

    pub fn step_fragment_selection(&mut self, forward: bool) -> Option<DocumentPosition> {
        let plan = self.plan.as_ref()?;
        let plan_len = plan.fragments.len();
        if plan_len == 0 {
            return None;
        }

        let (anchor, cur_pos) = if let Some(sel) = self.selection {
            (sel.anchor, sel.active)
        } else {
            let def = self.default_anchor();
            (def, def)
        };

        let next_pos = if forward {
            if cur_pos.fragment_index + 1 < plan_len {
                let next_len = plan.fragments[cur_pos.fragment_index + 1]
                    .text()
                    .chars()
                    .count();
                DocumentPosition::new(
                    cur_pos.fragment_index + 1,
                    cur_pos.char_offset.min(next_len),
                )
            } else {
                let last_len = plan.fragments[plan_len - 1].text().chars().count();
                DocumentPosition::new(plan_len - 1, last_len)
            }
        } else if cur_pos.fragment_index > 0 {
            let prev_len = plan.fragments[cur_pos.fragment_index - 1]
                .text()
                .chars()
                .count();
            DocumentPosition::new(
                cur_pos.fragment_index - 1,
                cur_pos.char_offset.min(prev_len),
            )
        } else {
            DocumentPosition::new(0, 0)
        };

        self.set_selection(Some(DocumentSelection::new(anchor, next_pos)));
        Some(next_pos)
    }

    pub fn drag_to_last(&mut self) {
        let Some(anchor) = self.drag_anchor else {
            return;
        };
        let Some(active) = self.hit_test(self.drag_last.0, self.drag_last.1) else {
            return;
        };
        self.set_selection(Some(DocumentSelection::new(anchor, active)));
    }

    pub fn refresh_markup(&self) {
        let observed = self.observed();
        let Some(plan) = &self.plan else {
            return;
        };
        for (index, label) in observed {
            if let Some(fragment) = plan.fragments.get(index as usize) {
                let markup = compute_fragment_markup(
                    fragment,
                    self.epoch,
                    index,
                    plan,
                    self.find_matches.as_deref(),
                    self.current_find_match,
                    self.selection,
                );
                label.set_markup(&markup);
            }
        }
    }
}

fn compute_fragment_markup(
    fragment: &markdown_linux::render::Fragment,
    epoch: u64,
    index: u32,
    plan: &RenderPlan,
    find_matches: Option<&markdown_linux::render::FindMatches>,
    current_find_match: Option<usize>,
    selection: Option<DocumentSelection>,
) -> String {
    let sel = plan.selection_for_fragment(index as usize, selection);
    if let Some(matches) = find_matches {
        let highlights = matches.highlights_for_fragment(index as usize, current_find_match);
        if !highlights.is_empty() || sel.is_some() {
            return fragment.markup_with_highlights_and_selection(epoch, index, &highlights, sel);
        }
    } else if let Some(s) = sel {
        return fragment.markup_with_highlights_and_selection(epoch, index, &[], Some(s));
    }
    fragment.markup(epoch, index)
}
