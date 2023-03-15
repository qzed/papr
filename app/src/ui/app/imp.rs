use adw::subclass::prelude::AdwApplicationImpl;
use gtk::{
    gio, glib,
    prelude::{Cast, StaticType},
    subclass::prelude::{
        ApplicationImpl, ApplicationImplExt, GtkApplicationImpl, ObjectImpl, ObjectSubclass,
        ObjectSubclassExt,
    },
    traits::WidgetExt,
};

use crate::ui::{appwindow::AppWindow, canvas::CanvasWidget, viewport::ViewportWidget};

#[derive(Debug, Default)]
pub struct PaperApp {}

#[glib::object_subclass]
impl ObjectSubclass for PaperApp {
    const NAME: &'static str = "PaperApp";
    type Type = super::PaperApp;
    type ParentType = adw::Application;
}

impl ObjectImpl for PaperApp {}

impl ApplicationImpl for PaperApp {
    fn startup(&self) {
        self.parent_startup();

        gio::resources_register_include!("paper.gresource").expect("Failed to register resources.");

        // register custom widgets
        AppWindow::static_type();
        CanvasWidget::static_type();
        ViewportWidget::static_type();
    }

    fn activate(&self) {
        self.parent_activate();

        let window = AppWindow::new(self.obj().upcast_ref::<adw::Application>());
        window.show();
    }
}

impl GtkApplicationImpl for PaperApp {}
impl AdwApplicationImpl for PaperApp {}
