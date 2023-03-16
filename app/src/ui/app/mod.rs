use gtk::{gio, glib};

mod imp;

glib::wrapper! {
    pub struct App(ObjectSubclass<imp::App>)
        @extends gio::Application, gtk::Application, adw::Application,
        @implements gio::ActionGroup, gio::ActionMap;
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    pub fn new() -> Self {
        let flags = gio::ApplicationFlags::HANDLES_OPEN;

        glib::Object::builder()
            .property("application-id", "io.mxnluz.Paper")
            .property("flags", flags)
            .build()
    }
}
