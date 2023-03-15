use gtk::glib;

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
}

impl Default for CanvasWidget {
    fn default() -> Self {
        Self::new()
    }
}
