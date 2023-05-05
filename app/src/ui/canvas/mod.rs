use gtk::glib;
use gtk::subclass::prelude::ObjectSubclassIsExt;

use pdfium::doc::Document;

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

    pub fn set_document(&self, document: Document) {
        self.imp().set_document(document)
    }

    pub fn clear(&self) {
        self.imp().clear()
    }
}

impl Default for CanvasWidget {
    fn default() -> Self {
        Self::new()
    }
}
