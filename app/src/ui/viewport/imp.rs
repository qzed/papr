use std::{cell::Cell, rc::Rc};

use gtk::{
    gdk,
    glib::{self, clone},
    prelude::{Cast, DisplayExt, ObjectExt, SeatExt, SurfaceExt},
    subclass::{
        prelude::{
            BuildableImpl, BuildableImplExt, ObjectImpl, ObjectImplExt, ObjectSubclass,
            ObjectSubclassExt, ObjectSubclassIsExt,
        },
        widget::{
            CompositeTemplateClass, CompositeTemplateDisposeExt, CompositeTemplateInitializingExt,
            WidgetClassSubclassExt, WidgetImpl,
        },
    },
    traits::{EventControllerExt, GestureDragExt, GestureExt, NativeExt, WidgetExt},
    CompositeTemplate, EventControllerScroll, EventControllerScrollFlags, EventSequenceState,
    GestureDrag, GestureZoom, Inhibit, PropagationPhase, TemplateChild,
};
use nalgebra::{vector, Vector2};

use crate::types::{Margin, Aabb};

#[derive(Debug, CompositeTemplate)]
#[template(resource = "/io/mxnluz/paper/ui/viewport.ui")]
pub struct ViewportWidget {
    scale_step: f64,

    #[template_child]
    scroller: TemplateChild<gtk::ScrolledWindow>,
}

impl ViewportWidget {
    pub fn new() -> Self {
        Self {
            scale_step: 0.1,
            scroller: Default::default(),
        }
    }

    pub fn scroller(&self) -> gtk::ScrolledWindow {
        self.scroller.get()
    }

    pub fn canvas_offset(&self) -> Option<Vector2<f64>> {
        self.scroller
            .child()
            .map(|c| vector![c.property("offset-x"), c.property("offset-y")])
    }

    pub fn set_canvas_offset(&self, offset: Vector2<f64>) {
        if let Some(child) = self.scroller.child() {
            child.set_property("offset-x", offset.x);
            child.set_property("offset-y", offset.y);
        }
    }

    pub fn canvas_scale(&self) -> Option<f64> {
        self.scroller.child().map(|c| c.property("scale"))
    }

    pub fn set_canvas_scale(&self, scale: f64) {
        if let Some(child) = self.scroller.child() {
            child.set_property("scale", scale);
        }
    }

    pub fn set_canvas_offset_and_scale(&self, offset: Vector2<f64>, scale: f64) {
        if let Some(child) = self.scroller.child() {
            child.set_property("offset-x", offset.x);
            child.set_property("offset-y", offset.y);
            child.set_property("scale", scale);
        }
    }

    pub fn canvas_margin(&self) -> Option<Margin> {
        self.scroller.child().map(|c|
            Margin {
                left: c.property("margin-left"),
                right: c.property("margin-right"),
                top: c.property("margin-top"),
                bottom: c.property("margin-bottom"),
            }
        )
    }

    pub fn canvas_bounds(&self) -> Option<Aabb> {
        self.scroller.child().map(|c|
            Aabb {
                x_min: c.property("bounds-x-min"),
                x_max: c.property("bounds-x-max"),
                y_min: c.property("bounds-y-min"),
                y_max: c.property("bounds-y-max"),
            }
        )
    }

    pub fn canvas_fit_width(&self) {
        if self.scroller.child().is_none() {
            return;
        }

        let mut offset = self.canvas_offset().unwrap();
        let margin = self.canvas_margin().unwrap();
        let bounds = self.canvas_bounds().unwrap();

        let canvas_width = bounds.x_max - bounds.x_min;
        let viewport_width = self.scroller.width() as f64 - margin.left - margin.right;
        let scale = viewport_width / canvas_width;

        offset.x = bounds.x_min - margin.left;

        self.set_canvas_offset_and_scale(offset, scale);
    }
}

impl Default for ViewportWidget {
    fn default() -> Self {
        Self::new()
    }
}

#[glib::object_subclass]
impl ObjectSubclass for ViewportWidget {
    const NAME: &'static str = "Viewport";
    type Type = super::ViewportWidget;
    type ParentType = gtk::Widget;
    type Interfaces = (gtk::Buildable,);

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
        klass.set_layout_manager_type::<gtk::BinLayout>();
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for ViewportWidget {
    fn constructed(&self) {
        self.parent_constructed();

        let obj = self.obj();

        // pan with middle mouse button + drag
        {
            let ctrl = GestureDrag::builder()
                .name("canvas_drag_middle_mouse_controller")
                .button(gdk::BUTTON_MIDDLE)
                .exclusive(true)
                .propagation_phase(PropagationPhase::Bubble)
                .build();

            let drag_start = Rc::new(Cell::new(vector![0.0, 0.0]));

            ctrl.connect_drag_begin(clone!(@strong drag_start, @weak obj => move |_, _, _| {
                let vp = obj.imp();
                drag_start.set(vp.canvas_offset().unwrap_or_default());
            }));

            ctrl.connect_drag_update(clone!(@strong drag_start, @weak obj => move |_, dx, dy| {
                let vp = obj.imp();
                vp.set_canvas_offset(drag_start.get() - vector![dx, dy]);
            }));

            self.scroller.add_controller(ctrl);
        }

        // zoom with ctrl + scroll-wheel
        {
            let ctrl = EventControllerScroll::builder()
                .name("canvas_zoom_scroll_controller")
                .propagation_phase(PropagationPhase::Bubble)
                .flags(EventControllerScrollFlags::VERTICAL)
                .build();

            ctrl.connect_scroll(clone!(@weak obj => @default-return Inhibit(false),
                move |ctrl, _, dy| {
                    if ctrl.current_event_state() == gdk::ModifierType::CONTROL_MASK {
                        let vp = obj.imp();

                        // get mouse position on surface
                        //
                        // Note: Ideally, we would get the device from the
                        // event via event.device() but that somehow causes a
                        // crash on when dealing with touchpads. So get the
                        // default pointer instead.
                        let event = ctrl.current_event().unwrap();
                        let seat = event.display().unwrap().default_seat().unwrap();
                        let device = seat.pointer().unwrap();

                        let native = obj.native().unwrap();
                        let surface = native.surface();

                        let pos_surface = surface.device_position(&device).unwrap();
                        let pos_surface = vector![pos_surface.0, pos_surface.1];

                        // translate mouse position from surface to root widget
                        let margin_surface = native.surface_transform();
                        let margin_surface = vector![margin_surface.0, margin_surface.1];

                        let pos_root = pos_surface - margin_surface;

                        // translate mouse position from root widget to canvas widget
                        let root = obj.root().unwrap();
                        let pos_wdg = root.translate_coordinates(&obj, pos_root.x, pos_root.y)
                            .unwrap();

                        // fixpoint in screen units: this is what we zoom in/out on
                        let fixp_screen = vector![pos_wdg.0, pos_wdg.1];

                        // offset of the viewport in screen units
                        let offset = vp.canvas_offset().unwrap_or_default();
                        let scale = vp.canvas_scale().unwrap_or(1.0);

                        // calculate fixpoint in document coordinates
                        let fixp_doc = (offset + fixp_screen) / scale;

                        // calculate new scale value
                        let scale = scale * (1.0 - dy * vp.scale_step);

                        // calculate new viewport offset from fixpoint document coordinates
                        let offset = fixp_doc * scale - fixp_screen;

                        // update properties
                        vp.set_canvas_offset_and_scale(offset, scale);

                        Inhibit(true)
                    } else {
                        Inhibit(false)
                    }
                }
            ));

            self.scroller.add_controller(ctrl);
        }

        // zoom + move with touch gesture
        {
            let ctrl = GestureZoom::builder()
                .name("canvas_zoom_touch_controller")
                .propagation_phase(PropagationPhase::Capture)
                .build();

            let fixpoint = Rc::new(Cell::new(vector![0.0, 0.0]));
            let scale_start = Rc::new(Cell::new(1.0));

            ctrl.connect_begin(clone!(
                    @strong fixpoint,
                    @strong scale_start,
                    @weak obj
                => move |ctrl, _seq| {
                    ctrl.set_state(EventSequenceState::Claimed);

                    let vp = obj.imp();

                    // initial fixpoint in screen coordinates (gesture center)
                    let center = ctrl
                        .bounding_box_center()
                        .map(|c| vector![c.0, c.1])
                        .unwrap_or_else(|| {
                            vector![
                                vp.scroller.width() as f64 / 2.0,
                                vp.scroller.height() as f64 / 2.0
                            ]
                        });

                    // initial viewport offset
                    let offset = vp
                        .canvas_offset()
                        .unwrap_or_default();

                    // initial viewport scale
                    let scale = vp
                        .canvas_scale()
                        .unwrap_or(1.0);

                    // calculate fixpoint in document coordinates
                    let center = (offset + center) / scale;

                    // remember initial values
                    fixpoint.set(center);
                    scale_start.set(scale);
                }
            ));

            ctrl.connect_scale_changed(clone!(
                    @strong fixpoint,
                    @strong scale_start,
                    @weak obj
                => move |ctrl, gesture_scale| {
                    let vp = obj.imp();

                    let scale = scale_start.get() * gesture_scale;

                    // new fixpoint position in screen coordinates (gesture center)
                    let center = ctrl
                        .bounding_box_center()
                        .map(|c| vector![c.0, c.1])
                        .unwrap_or_else(|| {
                            vector![
                                vp.scroller.width() as f64 / 2.0,
                                vp.scroller.height() as f64 / 2.0
                            ]
                        });

                    // calculate viewport offset from fixpoint for new scale
                    let offset = fixpoint.get() * scale - center;

                    // set properties
                    vp.set_canvas_offset_and_scale(offset, scale);
                }
            ));

            ctrl.connect_cancel(move |ctrl, _seq| {
                ctrl.set_state(EventSequenceState::Denied);
            });

            ctrl.connect_end(move |ctrl, _seq| {
                ctrl.set_state(EventSequenceState::Denied);
            });

            self.scroller.add_controller(ctrl);
        }
    }

    fn dispose(&self) {
        self.dispose_template();
    }
}

impl WidgetImpl for ViewportWidget {}

impl BuildableImpl for ViewportWidget {
    fn add_child(&self, builder: &gtk::Builder, child: &glib::Object, type_: Option<&str>) {
        if !self.scroller.is_bound() {
            self.parent_add_child(builder, child, type_);
        } else {
            self.obj()
                .set_child(Some(child.downcast_ref::<gtk::Widget>().unwrap()));
        }
    }
}
