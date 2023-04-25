use crate::canvas::Canvas;

use gtk::glib::clone;
use gtk::prelude::FileExt;
use gtk::subclass::prelude::ObjectSubclassIsExt;
use gtk::{gio, glib};

use nalgebra::vector;

use super::canvas::CanvasWidget;
use super::viewport::ViewportWidget;

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

    fn canvas(&self) -> &CanvasWidget {
        self.imp().canvas()
    }

    fn viewport(&self) -> &ViewportWidget {
        self.imp().viewport()
    }

    pub fn open_file(&self, file: gio::File) {
        glib::MainContext::default().spawn_local(clone!(@weak self as appwindow => async move {
            println!("loading file: {:?}", file.path().unwrap());

            // TODO: handle errors
            let (data, _etag) = file.load_bytes_future().await.expect("failed to load file");
            let data = data.to_vec();

            let pdflib = pdfium::Library::init().unwrap();
            let doc = pdflib.load_buffer(data, None).unwrap();

            let canvas = Canvas::create(doc);

            println!("file loaded, creating canvas");
            appwindow.canvas().set_canvas(Some(canvas));
            appwindow.viewport().set_offset_and_scale(vector![0.0, 0.0], 1.0);
            appwindow.viewport().fit_width();
        }));
    }
}
