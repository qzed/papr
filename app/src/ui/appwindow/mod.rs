use crate::canvas::Canvas;
use crate::types::Bounds;

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

            // TODO
            println!("{:?}", data.len());

            let bounds = Bounds {
                x_min: 0.0,
                y_min: 0.0,
                x_max: 1000.0,
                y_max: 1500.0,
            };

            let canvas = Canvas::new(bounds);
            appwindow.canvas().set_canvas(Some(canvas));
            appwindow.viewport().set_offset_and_scale(vector![0.0, 0.0], 1.0);
            appwindow.viewport().fit_width();
        }));
    }
}
