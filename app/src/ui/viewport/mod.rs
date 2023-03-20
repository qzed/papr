use gtk::{glib, subclass::prelude::ObjectSubclassIsExt, prelude::IsA, Widget};
use nalgebra::Vector2;

mod imp;

glib::wrapper! {
    pub struct ViewportWidget(ObjectSubclass<imp::ViewportWidget>)
        @extends gtk::Widget,
        @implements gtk::Buildable;
}

impl Default for ViewportWidget {
    fn default() -> Self {
        Self::new()
    }
}

impl ViewportWidget {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_child(&self, child: Option<&impl IsA<Widget>>) {
        self.imp().scroller().set_child(child);
    }

    pub fn fit_width(&self) {
        self.imp().canvas_fit_width()
    }

    pub fn set_offset(&self, offset: Vector2<f64>) {
        self.imp().set_canvas_offset(offset)
    }

    pub fn set_scale(&self, scale: f64) {
        self.imp().set_canvas_scale(scale)
    }

    pub fn set_offset_and_scale(&self, offset: Vector2<f64>, scale: f64) {
        self.imp().set_canvas_offset_and_scale(offset, scale)
    }
}
