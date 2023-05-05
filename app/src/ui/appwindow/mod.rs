use gtk::subclass::prelude::ObjectSubclassIsExt;
use gtk::{gio, glib};

mod imp;

glib::wrapper! {
    pub struct AppWindow(ObjectSubclass<imp::AppWindow>)
        @extends gtk::ApplicationWindow, adw::ApplicationWindow, gtk::Window, adw::Window, gtk::Widget,
        @implements gio::ActionGroup, gio::ActionMap, gtk::Accessible, gtk::Buildable,
                gtk::ConstraintTarget, gtk::Native, gtk::Root, gtk::ShortcutManager;
}

impl AppWindow {
    pub fn new(app: &adw::Application) -> Self {
        glib::Object::builder().property("application", app).build()
    }

    pub fn open_file(&self, file: gio::File) {
        self.imp().open_file(file)
    }
}
