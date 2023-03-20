use adw::subclass::prelude::AdwApplicationWindowImpl;
use gtk::glib::subclass::InitializingObject;
use gtk::subclass::prelude::{
    ApplicationWindowImpl, CompositeTemplateClass, CompositeTemplateInitializingExt, ObjectImpl,
    ObjectSubclass, WidgetImpl, WindowImpl,
};
use gtk::subclass::widget::WidgetClassSubclassExt;
use gtk::{glib, CompositeTemplate, TemplateChild};

use crate::ui::canvas::CanvasWidget;
use crate::ui::viewport::ViewportWidget;

#[derive(CompositeTemplate, Default)]
#[template(resource = "/io/mxnluz/paper/ui/appwindow.ui")]
pub struct AppWindow {
    #[template_child]
    viewport: TemplateChild<ViewportWidget>,

    #[template_child]
    canvas: TemplateChild<CanvasWidget>,
}

impl AppWindow {
    pub fn viewport(&self) -> &ViewportWidget {
        &self.viewport
    }

    pub fn canvas(&self) -> &CanvasWidget {
        &self.canvas
    }
}

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
