//! Adapted from spikes/feas008/native/src/model.rs at 5bc81c4 (Apache-2.0).
//! A list model that reports a count and creates position objects lazily, so
//! startup cost does not grow with the number of display fragments.

use gtk::{gio, glib, prelude::*, subclass::prelude::*};

mod imp {
    use super::*;
    use std::cell::Cell;

    #[derive(Default)]
    pub struct CountModel {
        pub count: Cell<u32>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for CountModel {
        const NAME: &'static str = "MarkdownReaderCountModel";
        type Type = super::CountModel;
        type Interfaces = (gio::ListModel,);
    }

    impl ObjectImpl for CountModel {}

    impl ListModelImpl for CountModel {
        fn item_type(&self) -> glib::Type {
            glib::BoxedAnyObject::static_type()
        }

        fn n_items(&self) -> u32 {
            self.count.get()
        }

        fn item(&self, position: u32) -> Option<glib::Object> {
            (position < self.count.get()).then(|| glib::BoxedAnyObject::new(position).upcast())
        }
    }
}

glib::wrapper! {
    pub struct CountModel(ObjectSubclass<imp::CountModel>) @implements gio::ListModel;
}

impl CountModel {
    pub fn new(count: u32) -> CountModel {
        let model: CountModel = glib::Object::new();
        model.imp().count.set(count);
        model
    }
}
