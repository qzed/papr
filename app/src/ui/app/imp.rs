use adw::subclass::prelude::AdwApplicationImpl;
use gtk::{
    gio, glib,
    prelude::{Cast, StaticType},
    subclass::prelude::{
        ApplicationImpl, ApplicationImplExt, GtkApplicationImpl, ObjectImpl, ObjectSubclass,
        ObjectSubclassExt,
    },
    traits::{GtkApplicationExt, WidgetExt},
};

use crate::ui::{appwindow::AppWindow, canvas::CanvasWidget, viewport::ViewportWidget};

#[derive(Debug, Default)]
pub struct App {}

impl App {
    fn new_appwindow(&self) -> AppWindow {
        AppWindow::new(self.obj().upcast_ref::<adw::Application>())
    }
}

#[glib::object_subclass]
impl ObjectSubclass for App {
    const NAME: &'static str = "App";
    type Type = super::App;
    type ParentType = adw::Application;
}

impl ObjectImpl for App {}

impl ApplicationImpl for App {
    fn startup(&self) {
        self.parent_startup();

        gio::resources_register_include!("papr.gresource").expect("Failed to register resources.");

        // register custom widgets
        AppWindow::static_type();
        CanvasWidget::static_type();
        ViewportWidget::static_type();
    }

    fn activate(&self) {
        self.parent_activate();
        self.new_appwindow().show();
    }

    fn open(&self, files: &[gio::File], hint: &str) {
        self.parent_open(files, hint);

        // get active window or create new one if we don't have one
        let window = if let Some(window) = self.obj().active_window() {
            window.downcast().unwrap()
        } else {
            let window = self.new_appwindow();
            window.show();
            window
        };

        // open file, if we have one
        let file = files.first().cloned();
        if let Some(file) = file {
            window.open_file(file);
        }
    }
}

impl GtkApplicationImpl for App {}
impl AdwApplicationImpl for App {}
