use gtk::{gio, glib, prelude::FileExt};

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
        glib::MainContext::default().spawn_local(async move {
            // TODO: handle errors
            let (data, _etag) = file.load_bytes_future().await.expect("failed to load file");
            let data = data.to_vec();

            // TODO
            println!("{:?}", data.len());
        });
    }
}
