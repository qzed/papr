use std::cell::{Cell, RefCell};

use gtk::{
    glib::{self, once_cell::sync::Lazy, ParamSpec, Value},
    graphene,
    prelude::{ObjectExt, ParamSpecBuilderExt, ToValue},
    subclass::{
        prelude::{ObjectImpl, ObjectSubclass, ObjectSubclassExt, ObjectSubclassIsExt},
        scrollable::ScrollableImpl,
        widget::WidgetImpl,
    },
    traits::{AdjustmentExt, ScrollableExt, SnapshotExt, WidgetExt},
    Adjustment, ScrollablePolicy,
};
use nalgebra::{point, vector, Point2};

use crate::canvas::Canvas;
use crate::types::{Bounds, Margin, Viewport};

pub struct CanvasWidget {
    // properties for scolling
    hscroll_policy: Cell<ScrollablePolicy>,
    vscroll_policy: Cell<ScrollablePolicy>,
    hadjustment: RefCell<Option<Adjustment>>,
    vadjustment: RefCell<Option<Adjustment>>,

    // handlers for scrolling
    hadjustment_handler: Cell<Option<glib::SignalHandlerId>>,
    vadjustment_handler: Cell<Option<glib::SignalHandlerId>>,

    // properties for canvas
    margin: RefCell<Margin>,

    // properties for viewport
    offset: RefCell<Point2<f64>>,
    scale: Cell<f64>,

    // render-state
    viewport: RefCell<Viewport>,

    // canvas to be rendered
    canvas: RefCell<Option<Canvas>>,
}

impl CanvasWidget {
    fn new() -> Self {
        let margin = Margin {
            left: 50.0,
            right: 50.0,
            top: 100.0,
            bottom: 100.0,
        };

        let offset = point![0.0, 0.0];
        let scale = 1.0;

        Self {
            hscroll_policy: Cell::new(ScrollablePolicy::Minimum),
            vscroll_policy: Cell::new(ScrollablePolicy::Minimum),
            hadjustment: RefCell::new(None),
            vadjustment: RefCell::new(None),

            hadjustment_handler: Cell::new(None),
            vadjustment_handler: Cell::new(None),

            margin: RefCell::new(margin),
            offset: RefCell::new(offset),
            scale: Cell::new(scale),

            viewport: RefCell::new(Viewport {
                size: vector![600.0, 800.0],
                offset,
                scale,
            }),

            canvas: RefCell::new(None),
        }
    }

    fn bounds(&self) -> Bounds {
        self.canvas
            .borrow()
            .as_ref()
            .map(|c| *c.bounds())
            .unwrap_or_else(Bounds::zero)
    }
}

impl Default for CanvasWidget {
    fn default() -> Self {
        Self::new()
    }
}

#[glib::object_subclass]
impl ObjectSubclass for CanvasWidget {
    const NAME: &'static str = "Canvas";
    type Type = super::CanvasWidget;
    type ParentType = gtk::Widget;
    type Interfaces = (gtk::Scrollable,);
}

impl ObjectImpl for CanvasWidget {
    fn properties() -> &'static [ParamSpec] {
        static PROPERTIES: Lazy<Vec<ParamSpec>> = Lazy::new(|| {
            vec![
                glib::ParamSpecOverride::for_interface::<gtk::Scrollable>("hscroll-policy"),
                glib::ParamSpecOverride::for_interface::<gtk::Scrollable>("vscroll-policy"),
                glib::ParamSpecOverride::for_interface::<gtk::Scrollable>("hadjustment"),
                glib::ParamSpecOverride::for_interface::<gtk::Scrollable>("vadjustment"),
                glib::ParamSpecDouble::builder("bounds-x-min")
                    .read_only()
                    .build(),
                glib::ParamSpecDouble::builder("bounds-x-max")
                    .read_only()
                    .build(),
                glib::ParamSpecDouble::builder("bounds-y-min")
                    .read_only()
                    .build(),
                glib::ParamSpecDouble::builder("bounds-y-max")
                    .read_only()
                    .build(),
                glib::ParamSpecDouble::builder("margin-left").build(),
                glib::ParamSpecDouble::builder("margin-right").build(),
                glib::ParamSpecDouble::builder("margin-top").build(),
                glib::ParamSpecDouble::builder("margin-bottom").build(),
                glib::ParamSpecDouble::builder("offset-x").build(),
                glib::ParamSpecDouble::builder("offset-y").build(),
                glib::ParamSpecDouble::builder("scale").build(),
            ]
        });
        PROPERTIES.as_ref()
    }

    fn set_property(&self, _id: usize, value: &Value, pspec: &ParamSpec) {
        match pspec.name() {
            "hscroll-policy" => {
                let hscroll_policy = value.get().unwrap();

                let old = self.hscroll_policy.replace(hscroll_policy);

                if old != hscroll_policy {
                    let obj = self.obj();

                    obj.queue_resize();
                    obj.notify_by_pspec(pspec);
                }
            }
            "vscroll-policy" => {
                let vscroll_policy = value.get().unwrap();

                let old = self.vscroll_policy.replace(vscroll_policy);

                if old != vscroll_policy {
                    let obj = self.obj();

                    obj.queue_resize();
                    obj.notify_by_pspec(pspec);
                }
            }
            "hadjustment" => {
                let adj: Option<Adjustment> = value.get().unwrap();
                let obj = self.obj();

                // disconnect old adjustment
                if let Some(id) = self.hadjustment_handler.take() {
                    self.hadjustment.borrow().as_ref().unwrap().disconnect(id);
                }

                if let Some(ref adj) = adj {
                    adj.connect_value_changed(glib::clone!(@weak obj => move |adj| {
                        // update offset from adjustment
                        obj.imp().offset.borrow_mut().x = adj.value();

                        obj.queue_allocate();
                        obj.notify("offset-x");
                    }));
                }

                self.hadjustment.replace(adj);

                // request an update
                obj.queue_allocate();
                obj.notify_by_pspec(pspec);
            }
            "vadjustment" => {
                let adj: Option<Adjustment> = value.get().unwrap();
                let obj = self.obj();

                // disconnect old adjustment
                if let Some(id) = self.vadjustment_handler.take() {
                    self.vadjustment.borrow().as_ref().unwrap().disconnect(id);
                }

                // connect new adjustment
                if let Some(ref adj) = adj {
                    adj.connect_value_changed(glib::clone!(@weak obj => move |adj| {
                        // update offset from adjustment
                        obj.imp().offset.borrow_mut().y = adj.value();

                        obj.queue_allocate();
                        obj.notify("offset-y");
                    }));
                }

                self.vadjustment.replace(adj);

                // request an update
                obj.queue_allocate();
                obj.notify_by_pspec(pspec);
            }
            "margin-left" => {
                self.margin.borrow_mut().left = value.get().unwrap();

                // request an update
                let obj = self.obj();
                obj.queue_resize();
                obj.notify_by_pspec(pspec);
            }
            "margin-right" => {
                self.margin.borrow_mut().right = value.get().unwrap();

                // request an update
                let obj = self.obj();
                obj.queue_resize();
                obj.notify_by_pspec(pspec);
            }
            "margin-top" => {
                self.margin.borrow_mut().top = value.get().unwrap();

                // request an update
                let obj = self.obj();
                obj.queue_resize();
                obj.notify_by_pspec(pspec);
            }
            "margin-bottom" => {
                self.margin.borrow_mut().bottom = value.get().unwrap();

                // request an update
                let obj = self.obj();
                obj.queue_resize();
                obj.notify_by_pspec(pspec);
            }
            "offset-x" => {
                self.offset.borrow_mut().x = value.get().unwrap();

                // request an update
                let obj = self.obj();
                obj.queue_allocate();
                obj.notify_by_pspec(pspec);
            }
            "offset-y" => {
                self.offset.borrow_mut().y = value.get().unwrap();

                // request an update
                let obj = self.obj();
                obj.queue_allocate();
                obj.notify_by_pspec(pspec);
            }
            "scale" => {
                self.scale.set(value.get().unwrap());

                // request an update
                let obj = self.obj();
                obj.queue_resize();
                obj.notify_by_pspec(pspec);
            }
            _ => unimplemented!(),
        }
    }

    fn property(&self, _id: usize, pspec: &ParamSpec) -> Value {
        match pspec.name() {
            "hscroll-policy" => self.hscroll_policy.get().to_value(),
            "vscroll-policy" => self.vscroll_policy.get().to_value(),
            "hadjustment" => self.hadjustment.borrow().to_value(),
            "vadjustment" => self.vadjustment.borrow().to_value(),
            "bounds-x-min" => self.bounds().x_min.to_value(),
            "bounds-x-max" => self.bounds().x_max.to_value(),
            "bounds-y-min" => self.bounds().y_min.to_value(),
            "bounds-y-max" => self.bounds().y_max.to_value(),
            "margin-left" => self.margin.borrow().left.to_value(),
            "margin-right" => self.margin.borrow().right.to_value(),
            "margin-top" => self.margin.borrow().top.to_value(),
            "margin-bottom" => self.margin.borrow().bottom.to_value(),
            "offset-x" => self.offset.borrow().x.to_value(),
            "offset-y" => self.offset.borrow().y.to_value(),
            "scale" => self.scale.get().to_value(),
            _ => unimplemented!(),
        }
    }
}

impl WidgetImpl for CanvasWidget {
    fn request_mode(&self) -> gtk::SizeRequestMode {
        gtk::SizeRequestMode::ConstantSize
    }

    fn measure(&self, orientation: gtk::Orientation, _for_size: i32) -> (i32, i32, i32, i32) {
        let bounds = self.bounds();

        let margin = self.margin.borrow();
        let margin_lower = vector![margin.left, margin.top];
        let margin_upper = vector![margin.right, margin.bottom];

        let scale = self.scale.get();
        let canvas_size = vector![bounds.x_max - bounds.x_min, bounds.y_max - bounds.y_min];
        let natural_size = canvas_size * scale + margin_lower + margin_upper;

        match orientation {
            gtk::Orientation::Horizontal => (0, natural_size.x.ceil() as _, -1, -1),
            gtk::Orientation::Vertical => (0, natural_size.y.ceil() as _, -1, -1),
            _ => unimplemented!(),
        }
    }

    fn size_allocate(&self, width: i32, height: i32, _baseline: i32) {
        // Note: The general idea is that we primarily use the "offset-x",
        // "offset-y", and "scale" properties to determine our viewport. More
        // specifically, we use those to update the horizontal and vertical
        // adjustments (scrollbars), i.e., we do _not_ use the adjustments to
        // determine the position directly.
        //
        // The reason for this is that setting the value on an adjustment will
        // clip it to the range defined by that adjustment. This makes
        // implementing certain transforms (zoom-in on specific coordinate)
        // difficult, because the adjustment ranges only get updated here.
        // Note, that we still want to "clip" our viewport position to some
        // area though.
        //
        // Therefore, the procedure is as follows: Any positional movement
        // (drag gestures, scrollbar movement) will update "offset-x" and
        // "offset-y" and queue an allocation, which brings us here. In the
        // allocation, we then update the viewport, adjustments, and clip the
        // position and offsets.

        let hadj = self.obj().hadjustment().unwrap();
        let vadj = self.obj().vadjustment().unwrap();

        let viewport_size = vector![width as f64, height as f64];
        let scale = self.scale.get();

        let bounds = self.bounds();
        let bounds_min = vector![bounds.x_min, bounds.y_min];
        let bounds_max = vector![bounds.x_max, bounds.y_max];

        let margin = self.margin.borrow();
        let margin_lower = vector![margin.left, margin.top];
        let margin_upper = vector![margin.right, margin.bottom];

        let mut lower = bounds_min * scale - margin_lower;
        let mut upper = bounds_max * scale + margin_upper;

        let offset = *self.offset.borrow();
        let mut offset = point![
            offset.x.min(upper.x - viewport_size.x).max(lower.x),
            offset.y.min(upper.y - viewport_size.y).max(lower.y)
        ];

        // if we zoom out to see the full document: center the view
        if upper.x - lower.x < viewport_size.x {
            let margin = viewport_size.x - (upper.x - lower.x);

            lower.x -= margin / 2.0;
            upper.x = lower.x + viewport_size.x;

            offset.x = lower.x;
        }

        if upper.y - lower.y < viewport_size.y {
            let margin = viewport_size.y - (upper.y - lower.y);

            lower.y -= margin / 2.0;
            upper.y = lower.y + viewport_size.y;

            offset.y = lower.y;
        }

        // update adjustments and properties
        hadj.configure(
            offset.x,
            lower.x,
            upper.x,
            0.1 * viewport_size.x,
            0.9 * viewport_size.x,
            viewport_size.x,
        );

        vadj.configure(
            offset.y,
            lower.y,
            upper.y,
            0.1 * viewport_size.y,
            0.9 * viewport_size.y,
            viewport_size.y,
        );

        self.offset.replace(offset);
        self.obj().notify("offset-x");
        self.obj().notify("offset-y");

        // update render state
        let mut viewport = self.viewport.borrow_mut();
        viewport.offset = offset;
        viewport.size = viewport_size;
        viewport.scale = scale;
    }

    fn snapshot(&self, snapshot: &gtk::Snapshot) {
        let canvas = self.canvas.borrow();

        if let Some(canvas) = canvas.as_ref() {
            let obj = self.obj();

            // clip drawing to widget area
            let bounds = graphene::Rect::new(0.0, 0.0, obj.width() as _, obj.height() as _);
            snapshot.push_clip(&bounds);

            // draw actual canvas
            let viewport = self.viewport.borrow();
            canvas.render(&viewport, snapshot);

            // pop the clip
            snapshot.pop();
        }
    }
}

impl ScrollableImpl for CanvasWidget {}
