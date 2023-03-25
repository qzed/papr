use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::mpsc;

use gtk::traits::{SnapshotExt, WidgetExt};
use gtk::{gdk, glib};
use gtk::{Snapshot, Widget};

use na::{point, vector, Similarity2, Translation2, Vector2};
use nalgebra as na;

use pdfium::bitmap::{Bitmap, BitmapFormat};
use pdfium::doc::{Page, PageRenderLayout, PageRotation, RenderFlags};

use crate::pdf::Document;
use crate::types::{Bounds, Rect, Viewport};

mod layout;
pub use layout::{HorizontalLayout, Layout, LayoutProvider, VerticalLayout};

mod pool;
use pool::BufferPool;

mod tile;
use self::tile::TileId;
type Tile = self::tile::Tile<gdk::MemoryTexture>;

pub struct Canvas {
    widget: Rc<RefCell<Option<Widget>>>,
    pages: Vec<Page>,
    layout: Layout,
    render: TiledRenderer,
}

impl Canvas {
    pub fn create(doc: Document) -> Self {
        let pages: Vec<_> = (0..(doc.pdf.pages().count()))
            .map(|i| doc.pdf.pages().get(i).unwrap())
            .collect();

        let layout_provider = VerticalLayout;
        let layout = layout_provider.compute(&pages, 10.0);

        let widget: Rc<RefCell<Option<Widget>>> = Rc::new(RefCell::new(None));

        let (notif_sender, notif_receiver) = glib::MainContext::channel(glib::PRIORITY_DEFAULT);

        let w = widget.clone();
        notif_receiver.attach(None, move |_| {
            if let Some(w) = w.borrow().as_ref() {
                w.queue_draw();
            }

            glib::Continue(true)
        });

        let tile_size = vector![512, 512];
        let render = TiledRenderer::new(tile_size, notif_sender);

        Self {
            widget,
            pages,
            layout,
            render,
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

        self.render.render_pre();

        // transformation matrix: canvas to viewport
        let m_ctv = {
            let m_scale = Similarity2::from_scaling(vp.scale);
            let m_trans = Translation2::from(-vp.r.offs.coords);
            m_trans * m_scale
        };

        // page rendering
        let iter = self.pages.iter_mut().zip(&self.layout.rects);

        for (i, (page, page_rect)) in iter.enumerate() {
            // transformation matrix: page to canvas
            let m_ptc = Translation2::from(page_rect.offs);

            // transformation matrix: page to viewport/screen
            let m_ptv = m_ctv * m_ptc;

            // convert page bounds to screen coordinates
            let page_rect = Rect::new(m_ptv * point![0.0, 0.0], m_ptv * page_rect.size);

            // round coordinates for pixel-perfect rendering
            let page_rect = page_rect.round();
            let page_rect = Rect {
                offs: na::convert_unchecked(page_rect.offs),
                size: na::convert_unchecked(page_rect.size),
            };

            // clip page bounds to visible screen area (area on screen covered by page)
            let screen_rect = Rect::new(point![0, 0], na::convert_unchecked(vp.r.size));
            let page_clipped = page_rect.clip(&screen_rect);

            // check if page is in view, skip rendering if not
            if page_clipped.size.x < 1 || page_clipped.size.y < 1 {
                continue;
            }

            // draw background
            snapshot.append_color(&gdk::RGBA::new(1.0, 1.0, 1.0, 1.0), &page_clipped.into());

            // draw contents
            self.render.render_page(vp, i, page, &page_rect, &page_clipped, snapshot);
        }

        // free all invisible tiles
        self.render.render_post();
    }
}

pub struct TiledRenderer {
    tile_size: Vector2<i64>,
    cache: TileCache,
    queue: RenderQueue,
}

impl TiledRenderer {
    pub fn new(tile_size: Vector2<i64>, notif: glib::Sender<()>) -> Self {
        let (queue, mut thread) = RenderQueue::new(tile_size, notif);
        let cache = TileCache::new();

        // run the render thread
        std::thread::spawn(move || thread.run());

        Self {
            tile_size,
            cache,
            queue,
        }
    }

    pub fn render_pre(&mut self) {
        // get fresh tiles
        while let Some(tile) = self.queue.next() {
            self.cache.insert(tile);
        }

        // mark all tiles as invisible
        self.cache.mark();
    }

    pub fn render_post(&mut self) {
        self.cache.evict_invisible();
    }

    pub fn render_page(
        &mut self,
        vp: &Viewport,
        i_page: usize,
        page: &Page,
        page_rect: &Rect<i64>,
        page_clipped: &Rect<i64>,
        snapshot: &Snapshot,
    ) {
        // viewport bounds relative to the page in pixels (area of page visible on screen)
        let visible_page = Rect::new(-page_rect.offs, na::convert_unchecked(vp.r.size))
            .clip(&Rect::new(point![0, 0], page_rect.size))
            .bounds();

        // tile bounds
        let tiles = Bounds {
            x_min: visible_page.x_min / self.tile_size.x,
            y_min: visible_page.y_min / self.tile_size.y,
            x_max: (visible_page.x_max + self.tile_size.x - 1) / self.tile_size.x,
            y_max: (visible_page.y_max + self.tile_size.y - 1) / self.tile_size.y,
        };

        snapshot.push_clip(&(*page_clipped).into());

        for (ix, iy) in tiles.range_iter() {
            let tile_id = TileId::new(i_page, ix, iy, page_rect.size.x);

            // render cached texture or submit render task
            if let Some(tile) = self.cache.get(&tile_id) {
                // draw tile to screen
                let tile_offs = vector![ix, iy].component_mul(&self.tile_size);
                let tile_screen_rect = Rect::new(page_rect.offs + tile_offs, self.tile_size);
                snapshot.append_texture(&tile.data, &tile_screen_rect.into());
            } else {
                // submit render task
                self.queue.submit(page.clone(), *page_rect, tile_id);
            };
        }

        snapshot.pop();
    }
}

pub struct TileCache {
    storage: HashMap<TileId, TileCacheEntry>,
}

struct TileCacheEntry {
    visible: bool,
    tile: Tile,
}

impl TileCache {
    pub fn new() -> Self {
        Self {
            storage: HashMap::new(),
        }
    }

    pub fn get(&mut self, id: &TileId) -> Option<&Tile> {
        if let Some(entry) = self.storage.get_mut(id) {
            entry.visible = true;
            Some(&entry.tile)
        } else {
            None
        }
    }

    pub fn insert(&mut self, tile: Tile) {
        let id = tile.id;

        let entry = TileCacheEntry {
            visible: true,
            tile,
        };

        self.storage.insert(id, entry);
    }

    pub fn mark(&mut self) {
        for entry in self.storage.values_mut() {
            entry.visible = false;
        }
    }

    pub fn evict_invisible(&mut self) {
        self.storage.retain(|_, e| e.visible);
    }
}

struct TileRenderer {
    tile_size: Vector2<i64>,
    pool: BufferPool,
}

impl TileRenderer {
    fn new(tile_size: Vector2<i64>) -> Self {
        let pool = BufferPool::new(Some(64), (tile_size.x * tile_size.y * 4) as _);

        Self { tile_size, pool }
    }

    fn render_tile(&self, page: &Page, page_rect: &Rect<i64>, id: &TileId) -> pdfium::Result<Tile> {
        // allocate tile bitmap buffer
        let stride = self.tile_size.x as usize * 4;
        let mut buffer = self.pool.alloc();

        // render to tile
        {
            let tile_offs = vector![id.x, id.y].component_mul(&self.tile_size);

            // wrap buffer in bitmap
            let mut bmp = Bitmap::from_buf(
                page.library().clone(),
                self.tile_size.x as _,
                self.tile_size.y as _,
                BitmapFormat::Bgra,
                &mut buffer[..],
                stride as _,
            )?;

            // set up render layout
            let layout = PageRenderLayout {
                start: na::convert::<_, Vector2<i32>>(-tile_offs).into(),
                size: na::convert(page_rect.size),
                rotate: PageRotation::None,
            };

            // render page to bitmap
            let flags = RenderFlags::LcdText | RenderFlags::Annotations;
            page.render(&mut bmp, &layout, flags)?;
        }

        // create GTK/GDK texture
        let bytes = glib::Bytes::from_owned(buffer);
        let texture = gdk::MemoryTexture::new(
            self.tile_size.x as _,
            self.tile_size.y as _,
            gdk::MemoryFormat::B8g8r8a8,
            &bytes,
            stride as _,
        );

        // create tile
        Ok(Tile::new(*id, texture))
    }
}

// TODO:
// - we should remove the task from the queue once it starts being rendered
// - add support for canceling tasks and cancel task when out of view

pub struct RenderQueue {
    sender: mpsc::Sender<RenderTask>,
    receiver: mpsc::Receiver<Tile>,
    pending: HashSet<TileId>,
}

pub struct RenderThread {
    renderer: TileRenderer,
    receiver: mpsc::Receiver<RenderTask>,
    sender: mpsc::Sender<Tile>,
    notif: glib::Sender<()>,
}

struct RenderTask {
    page: Page,
    page_rect: Rect<i64>,
    tile_id: TileId,
}

impl RenderQueue {
    pub fn new(tile_size: Vector2<i64>, notif: glib::Sender<()>) -> (RenderQueue, RenderThread) {
        let (task_sender, task_receiver) = mpsc::channel();
        let (tile_sender, tile_receiver) = mpsc::channel();
        let renderer = TileRenderer::new(tile_size);

        let queue = RenderQueue {
            sender: task_sender,
            receiver: tile_receiver,
            pending: HashSet::new(),
        };

        let thread = RenderThread {
            renderer,
            receiver: task_receiver,
            sender: tile_sender,
            notif,
        };

        (queue, thread)
    }

    pub fn is_pending(&self, tile_id: &TileId) -> bool {
        self.pending.contains(tile_id)
    }

    pub fn submit(&mut self, page: Page, page_rect: Rect<i64>, tile_id: TileId) {
        if self.is_pending(&tile_id) {
            return;
        }

        let task = RenderTask {
            page,
            page_rect,
            tile_id,
        };

        self.pending.insert(tile_id);
        self.sender.send(task).unwrap();
    }

    pub fn next(&mut self) -> Option<Tile> {
        match self.receiver.try_recv() {
            Ok(tile) => {
                self.pending.remove(&tile.id);
                Some(tile)
            }
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => panic!(),
        }
    }
}

impl RenderThread {
    pub fn run(&mut self) {
        while let Ok(task) = self.receiver.recv() {
            log::trace!(
                "rendering tile: ({}, {}, {}, {})",
                task.tile_id.page,
                task.tile_id.x,
                task.tile_id.y,
                task.tile_id.z
            );

            let tile = self
                .renderer
                .render_tile(&task.page, &task.page_rect, &task.tile_id)
                .unwrap();

            self.sender.send(tile).unwrap();
            self.notif.send(()).unwrap();
        }
    }
}
