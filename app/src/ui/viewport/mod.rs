use gtk::{glib, subclass::prelude::ObjectSubclassIsExt, prelude::IsA, Widget};

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
}
