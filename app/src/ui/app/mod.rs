use gtk::{gio, glib};

mod imp;

glib::wrapper! {
    pub struct PaperApp(ObjectSubclass<imp::PaperApp>)
        @extends gio::Application, gtk::Application, adw::Application,
        @implements gio::ActionGroup, gio::ActionMap;
}

impl Default for PaperApp {
    fn default() -> Self {
        Self::new()
    }
}

impl PaperApp {
    pub fn new() -> Self {
        glib::Object::builder()
            .property("application-id", "io.mxnluz.Paper")
            .property("flags", gio::ApplicationFlags::empty())
            .build()
    }
}
