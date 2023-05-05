use std::cell::{Cell, RefCell};

use executor::exec::Monitor;

use gtk::{
    gdk,
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

use nalgebra::{point, vector, Point2, Similarity2, Translation2};

use pdfium::bitmap::Color;
use pdfium::doc::{Document, RenderFlags};

use crate::core::render::core::{FallbackManager, FallbackSpec, HybridTilingScheme, TileManager};
use crate::core::render::interop::{Bitmap, TileFactory};
use crate::core::render::layout::Layout;
use crate::core::render::pdfium::{Executor, Handle, PdfTileProvider, RenderOptions};
use crate::types::{Bounds, Margin, Rect, Viewport};

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
    margin: RefCell<Margin<f64>>,

    // properties for viewport
    offset: RefCell<Point2<f64>>,
    scale: Cell<f64>,

    // render options
    fallback_specs: Vec<FallbackSpec>,
    render_opts_main: RenderOptions,
    render_opts_fallback: RenderOptions,

    // render state
    viewport: RefCell<Viewport>,

    // document data
    data: RefCell<Option<DocumentData>>,
}

struct DocumentData {
    layout: Layout,
    tile_provider: PdfTileProvider<TaskMonitor, TextureFactory>,
    tile_manager: TileManager<HybridTilingScheme, Handle<gdk::MemoryTexture>>,
    fallback_manager: FallbackManager<Handle<gdk::MemoryTexture>>,
}

impl CanvasWidget {
    fn new() -> Self {
        Self {
            hscroll_policy: Cell::new(ScrollablePolicy::Minimum),
            vscroll_policy: Cell::new(ScrollablePolicy::Minimum),
            hadjustment: RefCell::new(None),
            vadjustment: RefCell::new(None),

            hadjustment_handler: Cell::new(None),
            vadjustment_handler: Cell::new(None),

            margin: RefCell::new(Margin {
                left: 50.0,
                right: 50.0,
                top: 100.0,
                bottom: 100.0,
            }),
            offset: RefCell::new(point![0.0, 0.0]),
            scale: Cell::new(1.0),

            viewport: RefCell::new(Viewport {
                r: Rect {
                    offs: point![0.0, 0.0],
                    size: vector![600.0, 800.0],
                },
                scale: 1.0,
            }),

            fallback_specs: vec![
                FallbackSpec {
                    halo: usize::MAX,
                    render_threshold: vector![0.0, 0.0],
                    render_limits: vector![128, 128],
                },
                FallbackSpec {
                    halo: 24,
                    render_threshold: vector![256.0, 256.0],
                    render_limits: vector![256, 256],
                },
                FallbackSpec {
                    halo: 1,
                    render_threshold: vector![1024.0, 1024.0],
                    render_limits: vector![1024, 1024],
                },
                FallbackSpec {
                    halo: 0,
                    render_threshold: vector![2048.0, 2048.0],
                    render_limits: vector![2048, 2048],
                },
                FallbackSpec {
                    halo: 0,
                    render_threshold: vector![3072.0, 3072.0],
                    render_limits: vector![3072, 3072],
                },
            ],
            render_opts_main: RenderOptions {
                flags: RenderFlags::LcdText | RenderFlags::Annotations,
                background: Color::WHITE,
            },
            render_opts_fallback: RenderOptions {
                flags: RenderFlags::Annotations,
                background: Color::WHITE,
            },

            data: RefCell::new(None),
        }
    }

    fn bounds(&self) -> Bounds<f64> {
        self.data
            .borrow()
            .as_ref()
            .map(|d| d.layout.bounds)
            .unwrap_or_else(Bounds::zero)
    }

    fn scale_bounds(&self) -> (f64, f64) {
        (1e-2, 1e4)
    }

    pub fn set_document(&self, doc: Document) {
        use crate::core::render::layout::{LayoutProvider, VerticalLayout};

        // compute layout
        let page_sizes = (0..(doc.pages().count())).map(|i| doc.pages().get_size(i).unwrap());
        let layout = VerticalLayout.compute(page_sizes, 10.0);

        // set up tile-manager
        let scheme = HybridTilingScheme::new(vector![1024, 1024], 3072);
        let tile_manager = TileManager::new(scheme, vector![1, 1], vector![25.0, 25.0]);

        // set up fallback-manager
        let fallback_manager = FallbackManager::new(&self.fallback_specs);

        // set up render task execution
        let executor = Executor::new(1);
        let monitor = TaskMonitor::new(self.obj().clone());
        let factory = TextureFactory;
        let tile_provider = PdfTileProvider::new(executor, monitor, factory, doc);

        let data = DocumentData {
            layout,
            tile_provider,
            tile_manager,
            fallback_manager,
        };

        *self.data.borrow_mut() = Some(data);
        self.obj().queue_allocate();
    }

    pub fn clear(&self) {
        *self.data.borrow_mut() = None;
        self.obj().queue_allocate();
    }

    pub fn render(&self, vp: &Viewport, snapshot: &gtk::Snapshot) {
        use crate::core::render::core::{PageData, TileProvider};

        let mut data = self.data.borrow_mut();
        let data = match data.as_mut() {
            Some(data) => data,
            None => return,
        };

        // We have 3 coordinate systems:
        //
        // - Viewport coordinates, in pixels relative to the screen with origin
        //   (0, 0) as upper left corner of the widget.
        //
        // - Canvas coordinates, in PDF points. The relation between viewport
        //   and canvas coordinates is defined by the scale and viewport
        //   offset.
        //
        // - Page coordinates, in PDF points, relative to the page. The origin
        //   (0, 0) is defined as the upper left corner of the respective page.
        //   The relation between page coordinates and canvas coordinates is
        //   defined by the page offset in the canvas.

        // transformation matrix: canvas to viewport
        let m_ctv = {
            let m_scale = Similarity2::from_scaling(vp.scale);
            let m_trans = Translation2::from(-vp.r.offs.coords);
            m_trans * m_scale
        };

        // transformation: page (bounds) from canvas to viewport
        let transform = move |page_rect: &Rect<f64>| {
            // transformation matrix: page to canvas
            let m_ptc = Translation2::from(page_rect.offs);

            // transformation matrix: page to viewport/screen
            let m_ptv = m_ctv * m_ptc;

            // convert page bounds to screen coordinates
            let page_rect = Rect::new(m_ptv * point![0.0, 0.0], m_ptv * page_rect.size);

            // round coordinates for pixel-perfect rendering
            page_rect.round()
        };

        // origin-aligned viewport
        let screen_rect = Rect::new(point![0.0, 0.0], vp.r.size);

        // find visible pages
        #[allow(clippy::reversed_empty_ranges)]
        let mut visible = usize::MAX..0;

        for (i, page_rect_pt) in data.layout.rects.iter().enumerate() {
            // transform page bounds to viewport
            let page_rect = transform(page_rect_pt);

            // check if the page is visible
            if page_rect.intersects(&screen_rect) {
                visible.start = usize::min(visible.start, i);
                visible.end = usize::max(visible.end, i + 1);
            }
        }

        // ensure that we have a valid range if there are no visible pages
        if visible.start > visible.end {
            visible = 0..0;
        }

        // update fallback- and tile-caches
        data.tile_provider.request(&visible, |source| {
            let pages = PageData::new(&data.layout.rects, &visible, &transform);

            data.fallback_manager
                .update(source, &pages, vp, &self.render_opts_fallback);

            data.tile_manager
                .update(source, &pages, vp, &self.render_opts_main);
        });

        // render pages
        let iter = visible.clone().zip(&data.layout.rects[visible]);

        for (i, page_rect_pt) in iter {
            // transform page bounds to viewport
            let page_rect = transform(page_rect_pt);

            // clip page bounds to visible screen area (area on screen covered by page)
            let page_clipped = page_rect.clip(&screen_rect);

            // recompute scale for rounded page
            let scale = page_rect.size.x / page_rect_pt.size.x;
            let vp_adj = Viewport { r: vp.r, scale };

            // draw page shadow
            {
                let bounds = page_rect.into();
                let radius = gtk::gsk::graphene::Size::new(0.0, 0.0);
                let outline = gtk::gsk::RoundedRect::new(bounds, radius, radius, radius, radius);

                let color = gdk::RGBA::new(0.0, 0.0, 0.0, 0.5);

                let shift = vector![0.0, 1.0];
                let spread = 0.0;
                let blur = 3.5;

                snapshot.append_outset_shadow(&outline, &color, shift.x, shift.y, spread, blur)
            }

            // draw page background
            snapshot.append_color(&gdk::RGBA::new(1.0, 1.0, 1.0, 1.0), &page_clipped.into());

            // draw fallback
            if let Some(tex) = data.fallback_manager.fallback(i) {
                snapshot.append_texture(tex, &page_rect.into());
            }

            // draw tiles
            let tile_list = data.tile_manager.tiles(&vp_adj, i, &page_rect);

            snapshot.push_clip(&page_clipped.into());
            for (tile_rect, tex) in &tile_list {
                snapshot.append_texture(*tex, &(*tile_rect).into());
            }
            snapshot.pop();
        }
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
                glib::ParamSpecDouble::builder("scale-min")
                    .read_only()
                    .build(),
                glib::ParamSpecDouble::builder("scale-max")
                    .read_only()
                    .build(),
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
                let scale: f64 = value.get().unwrap();

                let (min_scale, max_scale) = self.scale_bounds();
                let scale = scale.clamp(min_scale, max_scale);

                self.scale.set(scale);

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
            "scale-min" => self.scale_bounds().0.to_value(),
            "scale-max" => self.scale_bounds().1.to_value(),
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
        viewport.r.offs = offset;
        viewport.r.size = viewport_size;
        viewport.scale = scale;
    }

    fn snapshot(&self, snapshot: &gtk::Snapshot) {
        let obj = self.obj();

        // clip drawing to widget area
        let bounds = graphene::Rect::new(0.0, 0.0, obj.width() as _, obj.height() as _);
        snapshot.push_clip(&bounds);

        // draw actual canvas
        let viewport = self.viewport.borrow();
        self.render(&viewport, snapshot);

        // pop the clip
        snapshot.pop();
    }
}

impl ScrollableImpl for CanvasWidget {}

#[derive(Clone)]
struct TaskMonitor {
    sender: glib::Sender<()>,
}

impl TaskMonitor {
    fn new(widget: super::CanvasWidget) -> Self {
        let (sender, receiver) = glib::MainContext::channel(glib::PRIORITY_DEFAULT);

        receiver.attach(None, move |_| {
            widget.queue_draw();
            glib::Continue(true)
        });

        Self { sender }
    }
}

impl Monitor for TaskMonitor {
    fn on_complete(&self) {
        self.sender.send(()).unwrap()
    }
}

#[derive(Debug, Clone)]
struct TextureFactory;

impl TileFactory for TextureFactory {
    type Data = gdk::MemoryTexture;

    fn create(&self, bmp: Bitmap) -> gdk::MemoryTexture {
        let bytes = glib::Bytes::from_owned(bmp.buffer);

        gdk::MemoryTexture::new(
            bmp.size.x as _,
            bmp.size.y as _,
            gdk::MemoryFormat::B8g8r8,
            &bytes,
            bmp.stride as _,
        )
    }
}
