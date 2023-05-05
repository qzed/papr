use gtk::glib;
use gtk::subclass::prelude::ObjectSubclassIsExt;

use crate::core::Canvas;

mod imp;

glib::wrapper! {
    pub struct CanvasWidget(ObjectSubclass<imp::CanvasWidget>)
        @extends gtk::Widget,
        @implements gtk::Scrollable, gtk::Buildable;
}

impl CanvasWidget {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_canvas(&self, canvas: Option<Canvas>) {
        self.imp().set_canvas(canvas)
    }
}

impl Default for CanvasWidget {
    fn default() -> Self {
        Self::new()
    }
}
