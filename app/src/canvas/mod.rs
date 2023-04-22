use std::cell::RefCell;
use std::ops::Range;
use std::rc::Rc;

use gtk::traits::{SnapshotExt, WidgetExt};
use gtk::{gdk, glib};
use gtk::{Snapshot, Widget};

use na::{point, vector, Similarity2, Translation2, Vector2};
use nalgebra as na;

use pdfium::bitmap::{Bitmap, BitmapFormat, Color};
use pdfium::doc::{Page, PageRenderLayout, PageRotation, RenderFlags};

use crate::pdf::Document;
use crate::types::{Bounds, Rect, Viewport};

mod layout;
pub use layout::{HorizontalLayout, Layout, LayoutProvider, VerticalLayout};

mod render;
use self::render::{FallbackManager, FallbackSpec};
use self::render::{HybridTilingScheme, TileManager, TilePriority, TileSource};

pub struct PageData<'a, F>
where
    F: Fn(&Rect<f64>) -> Rect<f64>,
{
    pub layout: &'a [Rect<f64>],
    pub visible: &'a Range<usize>,
    pub transform: &'a F,
}

impl<'a, F> PageData<'a, F>
where
    F: Fn(&Rect<f64>) -> Rect<f64>,
{
    pub fn new(layout: &'a [Rect<f64>], visible: &'a Range<usize>, transform: &'a F) -> Self {
        Self {
            layout,
            visible,
            transform,
        }
    }
}

pub struct Canvas {
    widget: Rc<RefCell<Option<Widget>>>,
    layout: Layout,
    source: PdfTileSource,
    tile_manager: TileManager<HybridTilingScheme, Handle<gdk::MemoryTexture>>,
    fbck_manager: FallbackManager<Handle<gdk::MemoryTexture>>,
}

impl Canvas {
    pub fn create(doc: Document) -> Self {
        // load pages
        let pages: Vec<_> = (0..(doc.pdf.pages().count()))
            .map(|i| doc.pdf.pages().get(i).unwrap())
            .collect();

        // compute layout
        let layout_provider = VerticalLayout;
        let layout = layout_provider.compute(&pages, 10.0);

        // set up tile-manager
        let scheme = HybridTilingScheme::new(vector![1024, 1024], 3072);
        let tile_manager = TileManager::new(scheme, vector![1, 1], vector![25.0, 25.0]);

        // set up fallback-manager
        let fbck_spec = [
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
        ];
        let fbck_manager = FallbackManager::new(&fbck_spec);

        // set up render task execution
        let (notif_sender, notif_receiver) = glib::MainContext::channel(glib::PRIORITY_DEFAULT);

        let widget: Rc<RefCell<Option<Widget>>> = Rc::new(RefCell::new(None));
        let w = widget.clone();
        notif_receiver.attach(None, move |_| {
            if let Some(w) = w.borrow().as_ref() {
                w.queue_draw();
            }

            glib::Continue(true)
        });

        let executor = Executor::new(1);
        let monitor = TaskMonitor::new(notif_sender);
        let source = PdfTileSource::new(executor, monitor, pages);

        Self {
            widget,
            layout,
            source,
            tile_manager,
            fbck_manager,
        }
    }

    pub fn set_widget(&mut self, widget: Option<Widget>) {
        *self.widget.borrow_mut() = widget;
    }

    pub fn bounds(&self) -> &Bounds<f64> {
        &self.layout.bounds
    }

    pub fn scale_bounds(&self) -> (f64, f64) {
        (1e-2, 1e4)
    }

    pub fn render(&mut self, vp: &Viewport, snapshot: &Snapshot) {
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

        for (i, page_rect_pt) in self.layout.rects.iter().enumerate() {
            // transform page bounds to viewport
            let page_rect = transform(page_rect_pt);

            // check if the page is visible
            if page_rect.intersects(&screen_rect) {
                visible.start = usize::min(visible.start, i);
                visible.end = usize::max(visible.end, i + 1);
            }
        }

        // update fallback- and tile-caches
        let pages = PageData::new(&self.layout.rects, &visible, &transform);
        self.fbck_manager.update(&self.source, &pages, vp);
        self.tile_manager.update(&self.source, &pages, vp);

        // render pages
        let iter = visible.clone().zip(&self.layout.rects[visible]);

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
            if let Some(tex) = self.fbck_manager.fallback(i) {
                snapshot.append_texture(tex, &page_rect.into());
            }

            // draw tiles
            let tile_list = self.tile_manager.tiles(&vp_adj, i, &page_rect);

            snapshot.push_clip(&page_clipped.into());
            for (tile_rect, tex) in &tile_list {
                snapshot.append_texture(*tex, &(*tile_rect).into());
            }
            snapshot.pop();
        }
    }
}

type Executor = executor::exec::priority::Executor<TilePriority>;
type Handle<R> = executor::exec::priority::DropHandle<TilePriority, R>;

#[derive(Clone)]
struct TaskMonitor {
    sender: glib::Sender<()>,
}

impl TaskMonitor {
    fn new(sender: glib::Sender<()>) -> Self {
        Self { sender }
    }
}

impl executor::exec::Monitor for TaskMonitor {
    fn on_complete(&self) {
        self.sender.send(()).unwrap()
    }
}

struct PdfTileSource {
    executor: Executor,
    monitor: TaskMonitor,
    pages: Vec<Page>,
}

impl PdfTileSource {
    fn new(executor: Executor, monitor: TaskMonitor, pages: Vec<Page>) -> Self {
        Self {
            executor,
            monitor,
            pages,
        }
    }
}

impl TileSource for PdfTileSource {
    type Data = gdk::MemoryTexture;
    type Handle = Handle<gdk::MemoryTexture>;

    fn request(
        &self,
        page_index: usize,
        page_size: Vector2<i64>,
        rect: Rect<i64>,
        priority: TilePriority,
    ) -> Self::Handle {
        let page = self.pages[page_index].clone();
        let task = move || {
            // TODO: struct for rendering (with some sort of factory pattern for GDK?)

            let flags = RenderFlags::LcdText | RenderFlags::Annotations;
            let color = Color::WHITE;

            render_page_rect_gdk(&page, &page_size, &rect, color, flags).unwrap()
        };

        self.executor
            .submit_with(self.monitor.clone(), priority, task)
            .cancel_on_drop()
    }
}

fn render_page_rect(
    page: &Page,
    page_size: &Vector2<i64>,
    rect: &Rect<i64>,
    background: Color,
    flags: RenderFlags,
) -> pdfium::Result<Box<[u8]>> {
    // allocate tile bitmap buffer
    let stride = rect.size.x as usize * 3;
    let mut buffer = vec![0; stride * rect.size.y as usize];

    // wrap buffer in bitmap
    let mut bmp = Bitmap::from_buf(
        page.library().clone(),
        rect.size.x as _,
        rect.size.y as _,
        BitmapFormat::Bgr,
        &mut buffer[..],
        stride as _,
    )?;

    // clear bitmap with background color
    bmp.fill_rect(0, 0, rect.size.x as _, rect.size.y as _, background);

    // set up render layout
    let layout = PageRenderLayout {
        start: na::convert::<_, Vector2<i32>>(-rect.offs.coords).into(),
        size: na::convert(*page_size),
        rotate: PageRotation::None,
    };

    // render page to bitmap
    page.render(&mut bmp, &layout, flags)?;

    // drop the wrapping bitmap and return the buffer
    drop(bmp);
    Ok(buffer.into_boxed_slice())
}

fn render_page_rect_gdk(
    page: &Page,
    page_size: &Vector2<i64>,
    rect: &Rect<i64>,
    background: Color,
    flags: RenderFlags,
) -> pdfium::Result<gdk::MemoryTexture> {
    // render page to byte buffer
    let buf = render_page_rect(page, page_size, rect, background, flags)?;

    // create GTK/GDK texture
    let stride = rect.size.x as usize * 3;
    let bytes = glib::Bytes::from_owned(buf);
    let texture = gdk::MemoryTexture::new(
        rect.size.x as _,
        rect.size.y as _,
        gdk::MemoryFormat::B8g8r8,
        &bytes,
        stride as _,
    );

    Ok(texture)
}
