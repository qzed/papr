use adw::subclass::prelude::AdwApplicationWindowImpl;
use gtk::{
    glib,
    glib::subclass::InitializingObject,
    subclass::prelude::{
        ApplicationWindowImpl, CompositeTemplateClass, CompositeTemplateInitializingExt,
        ObjectImpl, ObjectSubclass, WidgetImpl, WindowImpl,
    },
    CompositeTemplate,
};

#[derive(CompositeTemplate, Default)]
#[template(resource = "/io/mxnluz/paper/ui/appwindow.ui")]
pub struct AppWindow {}

#[glib::object_subclass]
impl ObjectSubclass for AppWindow {
    const NAME: &'static str = "AppWindow";
    type Type = super::AppWindow;
    type ParentType = adw::ApplicationWindow;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
    }

    fn instance_init(obj: &InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for AppWindow {}
impl WidgetImpl for AppWindow {}
impl WindowImpl for AppWindow {}
impl ApplicationWindowImpl for AppWindow {}
impl AdwApplicationWindowImpl for AppWindow {}
