use std::cell::RefCell;
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::atomic::AtomicBool;
use std::sync::{mpsc, Arc};

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

mod tile;
use self::tile::TileId;

type Tile = self::tile::Tile<gdk::MemoryTexture>;

pub struct Canvas {
    widget: Rc<RefCell<Option<Widget>>>,
    pages: Vec<Page>,
    fallbacks: Vec<gdk::MemoryTexture>,
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

        let tile_size = vector![1024, 1024];
        let render = TiledRenderer::new(tile_size, notif_sender);

        // pre-render some fallback images
        let mut fallbacks = Vec::with_capacity(pages.len());
        for page in &pages {
            let lib = page.library().clone();
            let size = page.size();

            let width: u32 = 512;
            let scale = width as f32 / size.x as f32;
            let height = (scale * size.y as f32).round() as u32;

            let format = BitmapFormat::Bgra;
            let mut bmp = Bitmap::uninitialized(lib, width, height, format).unwrap();
            bmp.fill_rect(0, 0, width, height, pdfium::bitmap::Color::WHITE);

            // set up render layout
            let layout = PageRenderLayout {
                start: point![0, 0],
                size: vector![width as i32, height as i32],
                rotate: PageRotation::None,
            };

            // render page to bitmap
            let flags = RenderFlags::LcdText | RenderFlags::Annotations;
            page.render(&mut bmp, &layout, flags).unwrap();

            // create GTK/GDK texture
            let bytes = glib::Bytes::from(bmp.buf());
            let texture = gdk::MemoryTexture::new(
                width as _,
                height as _,
                gdk::MemoryFormat::B8g8r8a8,
                &bytes,
                bmp.stride() as _,
            );

            fallbacks.push(texture);
        }

        Self {
            widget,
            pages,
            fallbacks,
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

            // draw fallback
            snapshot.append_texture(&self.fallbacks[i], &page_rect.into());

            // draw contents
            self.render
                .render_page(vp, i, page, &page_rect, &page_clipped, snapshot);
        }

        // free all invisible tiles
        self.render.render_post();
    }
}

pub struct TiledRenderer {
    tile_size: Vector2<i64>,
    cached: HashMap<usize, HashMap<TileId, Tile>>,
    pending: HashMap<usize, HashSet<TileId>>,
    visible: HashSet<usize>,
    queue: RenderQueue,
}

impl TiledRenderer {
    pub fn new(tile_size: Vector2<i64>, notif: glib::Sender<()>) -> Self {
        let (queue, mut thread) = RenderQueue::new(tile_size, notif);
        let cached = HashMap::new();
        let pending = HashMap::new();
        let visible = HashSet::new();

        // run the render thread
        std::thread::spawn(move || thread.run());

        Self {
            tile_size,
            cached,
            pending,
            visible,
            queue,
        }
    }

    pub fn render_pre(&mut self) {
        // get fresh tiles
        while let Some(tile) = self.queue.next() {
            // remove from pending
            let pending = self.pending.get_mut(&tile.id.page).unwrap();
            pending.remove(&tile.id);
            if pending.is_empty() {
                self.pending.remove(&tile.id.page);
            }

            // add to cached
            self.cached
                .entry(tile.id.page)
                .or_insert_with(HashMap::new)
                .insert(tile.id, tile);
        }
    }

    pub fn render_post(&mut self) {
        // remove out-of-view pages from cache
        self.cached.retain(|page, _| self.visible.contains(page));
        self.visible.clear();
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
        let tiles = visible_page.tiled(&self.tile_size);

        // mark page as visible
        self.visible.insert(i_page);

        // get cached tiles for page
        let mut fallback = HashMap::new();
        let cached = self.cached.get_mut(&i_page).unwrap_or(&mut fallback);
        let pending = self.pending.entry(i_page).or_insert_with(HashSet::new);
        let iz = page_rect.size.x;

        // request new tiles if not cached or pending
        for (ix, iy) in tiles.range_iter() {
            let tile_id = TileId::new(i_page, ix, iy, iz);

            if !cached.contains_key(&tile_id) && !pending.contains(&tile_id) {
                pending.insert(tile_id);
                self.queue.submit(page.clone(), *page_rect, tile_id);
            }
        }

        // find unused/occluded tiles and remove them
        let keys: HashSet<_> = cached.keys().cloned().collect();

        cached.retain(|_, t| {
            // compute tile bounds
            let tile_rect =
                t.id.rect_for_z_rounded(&self.tile_size, iz)
                    .translate(&page_rect.offs.coords);

            // check if tile is in view, drop it if it is not
            let vpz_rect = Rect::new(point![0, 0], na::convert_unchecked(vp.r.size));
            if !tile_rect.intersects(&vpz_rect) {
                return false;
            }

            // if the tile is on the current level: keep it
            if t.id.z == iz {
                return true;
            }

            // otherwise: check if the tile is replaced by ones with the
            // current z-level
            //
            // note: this does not check if e.g. a lower-z tile is occluded
            // by higher-z tiles, only if a tile is fully occluded by tiles
            // on the current z-level

            // compute pixel coordinates for current z-level, round to
            // ensure proper visibility testing, compute tile IDs required
            // to fully cover this, and check if we have all of them
            return !t
                .id
                .rect_for_z_rounded(&self.tile_size, iz)
                .bounds()
                .tiled(&self.tile_size)
                .clip(&tiles)
                .range_iter()
                .all(|(x, y)| keys.contains(&TileId::new(i_page, x, y, iz)));
        });

        pending.retain(|id| {
            // stop loading anything that is not on the current zoom level
            if id.z != iz {
                self.queue.cancel(id);
                return false;
            }

            // stop loading tiles if not in view
            let tile_rect = id.rect(&self.tile_size).translate(&page_rect.offs.coords);
            let vpz_rect = Rect::new(point![0, 0], na::convert_unchecked(vp.r.size));

            if !tile_rect.intersects(&vpz_rect) {
                self.queue.cancel(id);
                return false;
            }

            // otherwise: keep loading
            true
        });

        // build ordered render list
        let mut rlist: Vec<_> = cached.values().collect();

        rlist.sort_unstable_by(|a, b| {
            // sort by z-level:
            // - put all tiles with current z-level last
            // - sort rest in descending order (i.e., coarser tiles first)

            if a.id.z == b.id.z {
                // same z-levels are always equal
                Ordering::Equal
            } else if a.id.z == iz {
                // put current z-level last
                Ordering::Greater
            } else if b.id.z == iz {
                // put current z-level last
                Ordering::Less
            } else {
                // sort by z-level, descending
                if a.id.z < b.id.z {
                    Ordering::Greater
                } else {
                    Ordering::Less
                }
            }
        });

        // render tiles
        snapshot.push_clip(&(*page_clipped).into());

        for tile in &rlist {
            let tile_rect = tile
                .id
                .rect_for_z(&self.tile_size, iz)
                .translate(&na::convert(page_rect.offs.coords))
                .into();

            snapshot.append_texture(&tile.data, &tile_rect);
        }

        snapshot.pop();
    }
}

struct TileRenderer {
    tile_size: Vector2<i64>,
}

impl TileRenderer {
    fn new(tile_size: Vector2<i64>) -> Self {
        Self { tile_size }
    }

    fn render_tile(&self, page: &Page, page_rect: &Rect<i64>, id: &TileId) -> pdfium::Result<Tile> {
        // allocate tile bitmap buffer
        let stride = self.tile_size.x as usize * 4;
        let mut buffer = vec![0; stride * self.tile_size.y as usize];

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
            bmp.fill_rect(
                0,
                0,
                self.tile_size.x as _,
                self.tile_size.y as _,
                pdfium::bitmap::Color::WHITE,
            );

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
    pending: HashMap<TileId, Arc<AtomicBool>>,
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
    canceled: Arc<AtomicBool>,
}

impl RenderQueue {
    pub fn new(tile_size: Vector2<i64>, notif: glib::Sender<()>) -> (RenderQueue, RenderThread) {
        let (task_sender, task_receiver) = mpsc::channel();
        let (tile_sender, tile_receiver) = mpsc::channel();
        let renderer = TileRenderer::new(tile_size);

        let queue = RenderQueue {
            sender: task_sender,
            receiver: tile_receiver,
            pending: HashMap::new(),
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
        self.pending.contains_key(tile_id)
    }

    pub fn submit(&mut self, page: Page, page_rect: Rect<i64>, tile_id: TileId) {
        if self.is_pending(&tile_id) {
            return;
        }

        let canceled = Arc::new(AtomicBool::new(false));

        let task = RenderTask {
            page,
            page_rect,
            tile_id,
            canceled: canceled.clone(),
        };

        self.pending.insert(tile_id, canceled);
        self.sender.send(task).unwrap();
    }

    pub fn cancel(&mut self, tile_id: &TileId) {
        if let Some(flag) = self.pending.remove(tile_id) {
            flag.store(true, std::sync::atomic::Ordering::Release)
        }
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
            if task.canceled.load(std::sync::atomic::Ordering::Acquire) {
                continue;
            }

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
