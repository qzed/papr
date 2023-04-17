use std::cell::RefCell;
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use gtk::traits::{SnapshotExt, WidgetExt};
use gtk::{gdk, glib};
use gtk::{Snapshot, Widget};

use na::{point, vector, Similarity2, Translation2, Vector2};
use nalgebra as na;

use num_enum::{IntoPrimitive, TryFromPrimitive};

use pdfium::bitmap::{Bitmap, BitmapFormat, Color};
use pdfium::doc::{Page, PageRenderLayout, PageRotation, RenderFlags};

use crate::pdf::Document;
use crate::types::{Bounds, Rect, Viewport};

mod layout;
pub use layout::{HorizontalLayout, Layout, LayoutProvider, VerticalLayout};

mod tile;
use self::tile::{HybridTilingScheme, TileId, TilingScheme};

type Tile = self::tile::Tile<gdk::MemoryTexture>;

pub struct Canvas {
    widget: Rc<RefCell<Option<Widget>>>,
    pages: Vec<Page>,
    fallbacks: Vec<gdk::MemoryTexture>,
    layout: Layout,
    manager: TileManager<HybridTilingScheme>,
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

        let scheme = HybridTilingScheme::new(vector![1024, 1024], 3072);
        let manager = TileManager::new(scheme, notif_sender);

        // pre-render some fallback images
        let mut fallbacks = Vec::with_capacity(pages.len());
        for page in &pages {
            let size = page.size();

            // compute page render size and rectangle in pixels
            let width: i64 = 256;
            let scale = width as f32 / size.x;
            let height = (scale * size.y).round() as i64;

            let page_size = vector![width, height];
            let rect = Rect::new(point![0, 0], page_size);

            // render page to GDK texture
            let flags = RenderFlags::LcdText;
            let background = Color::WHITE;
            let tex = render_page_rect_gdk(page, &page_size, &rect, background, flags).unwrap();

            fallbacks.push(tex);
        }

        Self {
            widget,
            pages,
            fallbacks,
            layout,
            manager,
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

        // page rendering
        let iter = self.pages.iter_mut().zip(&self.layout.rects);

        for (i, (page, page_rect_pt)) in iter.enumerate() {
            // transformation matrix: page to canvas
            let m_ptc = Translation2::from(page_rect_pt.offs);

            // transformation matrix: page to viewport/screen
            let m_ptv = m_ctv * m_ptc;

            // convert page bounds to screen coordinates
            let page_rect = Rect::new(m_ptv * point![0.0, 0.0], m_ptv * page_rect_pt.size);

            // round coordinates for pixel-perfect rendering
            let page_rect = page_rect.round();

            // clip page bounds to visible screen area (area on screen covered by page)
            let screen_rect = Rect::new(point![0.0, 0.0], vp.r.size);
            let page_clipped = page_rect.clip(&screen_rect);

            // check if page is in view, skip rendering if not
            if page_clipped.size.x <= 0.0 || page_clipped.size.y <= 0.0 {
                continue;
            }

            // recompute scale for rounded page
            let scale = page_rect.size.x / page_rect_pt.size.x;
            let vp_adj = Viewport { r: vp.r, scale };

            // draw background
            snapshot.append_color(&gdk::RGBA::new(1.0, 1.0, 1.0, 1.0), &page_clipped.into());

            // draw fallback
            snapshot.append_texture(&self.fallbacks[i], &page_rect.into());

            // draw tiles
            let rlist = self
                .manager
                .render_page(&vp_adj, i, page, page_rect_pt, &page_rect);

            snapshot.push_clip(&page_clipped.into());
            for (tile_rect, tile) in &rlist {
                snapshot.append_texture(&tile.data, &(*tile_rect).into());
            }
            snapshot.pop();
        }

        // free all invisible tiles
        self.manager.render_post();
    }
}

struct TileCache {
    cached: HashMap<TileId, Tile>,
    pending: HashMap<TileId, Option<Handle<Tile>>>,
}

impl TileCache {
    fn empty() -> Self {
        Self {
            cached: HashMap::new(),
            pending: HashMap::new(),
        }
    }
}

pub struct TileManager<S> {
    scheme: S,
    executor: Executor,
    monitor: TaskMonitor,

    visible: HashSet<usize>,
    cache: HashMap<usize, TileCache>,
}

impl<S: TilingScheme> TileManager<S> {
    pub fn new(scheme: S, notif: glib::Sender<()>) -> Self {
        let executor = Executor::new(1);
        let monitor = TaskMonitor::new(notif);

        let visible = HashSet::new();
        let cache = HashMap::new();

        Self {
            scheme,
            executor,
            monitor,
            visible,
            cache,
        }
    }

    pub fn render_post(&mut self) {
        // remove out-of-view pages from cache
        self.cache.retain(|page, _| self.visible.contains(page));
        self.visible.clear();
    }

    pub fn render_page(
        &mut self,
        vp: &Viewport,
        i_page: usize,
        page: &Page,
        page_rect_pt: &Rect<f64>,
        page_rect: &Rect<f64>,
    ) -> Vec<(Rect<f64>, &Tile)> {
        // viewport bounds relative to the page in pixels (area of page visible on screen)
        let visible_page = Rect::new(-page_rect.offs, vp.r.size)
            .clip(&Rect::new(point![0.0, 0.0], page_rect.size))
            .bounds();

        // tile bounds
        let tiles = self.scheme.tiles(vp, page_rect, &visible_page);

        // get cached tiles for this page
        let entry = self.cache.entry(i_page).or_insert_with(TileCache::empty);

        // mark page as visible
        self.visible.insert(i_page);

        // request new tiles if not cached or pending
        for (ix, iy) in tiles.rect.range_iter() {
            let tile_id = TileId::new(i_page, ix, iy, tiles.z);

            // check if we already have the tile (or requested it)
            if entry.cached.contains_key(&tile_id) || entry.pending.contains_key(&tile_id) {
                continue;
            }

            // compute page size and tile bounds
            let (page_size, rect) =
                self.scheme
                    .render_rect(&page_rect_pt.size, &page_rect.size, &tile_id);

            // offload rendering to dedicated thread
            let monitor = self.monitor.clone();
            let page = page.clone();
            let priority = TaskPriority::Medium;

            let handle = self.executor.submit_with(monitor, priority, move || {
                let flags = RenderFlags::LcdText | RenderFlags::Annotations;
                let color = Color::WHITE;

                let texture = render_page_rect_gdk(&page, &page_size, &rect, color, flags).unwrap();

                Tile::new(tile_id, texture)
            });
            let handle = handle.cancel_on_drop();

            // store handle to the render task
            entry.pending.insert(tile_id, Some(handle));
        }

        // move newly rendered tiles to cached map
        for (id, task) in &mut entry.pending {
            if task.is_some() && task.as_ref().unwrap().is_finished() {
                entry
                    .cached
                    .insert(*id, std::mem::take(task).unwrap().join());
            }
        }

        // find unused/occluded pending tiles and remove them
        entry.pending.retain(|id, task| {
            // remove any tasks that have already been completed
            if task.is_none() {
                return false;
            }

            // stop loading anything that is not on the current zoom level
            if id.z != tiles.z {
                return false;
            }

            // stop loading tiles if not in view
            let tile_rect = self.scheme.screen_rect(vp, page_rect, id);
            let tile_rect = tile_rect.translate(&page_rect.offs.coords);
            let vpz_rect = Rect::new(point![0.0, 0.0], vp.r.size);

            if !tile_rect.intersects(&vpz_rect) {
                return false;
            }

            // otherwise: keep loading
            true
        });

        // find unused/occluded cached tiles and remove them
        let cached_keys: HashSet<_> = entry.cached.keys().cloned().collect();

        entry.cached.retain(|id, _tile| {
            // compute tile bounds
            let tile_rect = self.scheme.screen_rect(vp, page_rect, id);
            let tile_rect = tile_rect.bounds().round_outwards();
            let tile_rect_screen = tile_rect.translate(&page_rect.offs.coords);

            // check if tile is in view, drop it if it is not
            let vpz_rect = Rect::new(point![0.0, 0.0], vp.r.size).bounds();
            if !tile_rect_screen.intersects(&vpz_rect) {
                return false;
            }

            // if the tile is on the current level: keep it
            if id.z == tiles.z {
                return true;
            }

            // otherwise: check if the tile is replaced by ones with the
            // current z-level
            //
            // note: this does not check if e.g. a lower-z tile is occluded
            // by higher-z tiles, only if a tile is fully occluded by tiles
            // on the current z-level

            // compute tile IDs on current z-level required to fully cover the
            // original one
            let tiles_req = self.scheme.tiles(vp, page_rect, &tile_rect);
            let tiles_req = tiles_req.rect.clip(&tiles.rect);

            // check if all required tiles are present
            !tiles_req
                .range_iter()
                .all(|(x, y)| cached_keys.contains(&TileId::new(i_page, x, y, tiles.z)))
        });

        // build ordered render list
        let mut rlist: Vec<_> = entry.cached.values().collect();

        rlist.sort_unstable_by(|a, b| {
            // sort by z-level:
            // - put all tiles with current z-level last
            // - sort rest in descending order (i.e., coarser tiles first)

            if a.id.z == b.id.z {
                // same z-levels are always equal
                Ordering::Equal
            } else if a.id.z == tiles.z {
                // put current z-level last
                Ordering::Greater
            } else if b.id.z == tiles.z {
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

        rlist
            .into_iter()
            .map(|tile| {
                let tile_rect = self.scheme.screen_rect(vp, page_rect, &tile.id);
                let tile_rect = tile_rect.translate(&page_rect.offs.coords);

                (tile_rect, tile)
            })
            .collect()
    }
}

type Executor = executor::exec::priority::Executor<TaskPriority>;
type Handle<R> = executor::exec::priority::DropHandle<TaskPriority, R>;

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, IntoPrimitive, TryFromPrimitive)]
enum TaskPriority {
    Low = 0,
    Medium = 1,
    High = 2,
}

impl executor::exec::priority::Priority for TaskPriority {
    fn count() -> u8 {
        3
    }

    fn from_value(value: u8) -> Option<Self> {
        Self::try_from_primitive(value).ok()
    }

    fn as_value(&self) -> u8 {
        *self as _
    }
}

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

fn render_page_rect(
    page: &Page,
    page_size: &Vector2<i64>,
    rect: &Rect<i64>,
    background: Color,
    flags: RenderFlags,
) -> pdfium::Result<Box<[u8]>> {
    // allocate tile bitmap buffer
    let stride = rect.size.x as usize * 4;
    let mut buffer = vec![0; stride * rect.size.y as usize];

    // wrap buffer in bitmap
    let mut bmp = Bitmap::from_buf(
        page.library().clone(),
        rect.size.x as _,
        rect.size.y as _,
        BitmapFormat::Bgra,
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
    let stride = rect.size.x as usize * 4;
    let bytes = glib::Bytes::from_owned(buf);
    let texture = gdk::MemoryTexture::new(
        rect.size.x as _,
        rect.size.y as _,
        gdk::MemoryFormat::B8g8r8a8,
        &bytes,
        stride as _,
    );

    Ok(texture)
}
