use std::cell::RefCell;
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::ops::Range;
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

pub struct Canvas {
    widget: Rc<RefCell<Option<Widget>>>,
    pages: Vec<Page>,
    layout: Layout,
    exec: ExecutionContext,
    tile_manager: TileManager<HybridTilingScheme>,
    fbck_manager: FallbackManager,
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
        let tile_manager = TileManager::new(scheme);

        // set up fallback-manager
        let fbck_spec = [
            FallbackSpec {
                halo: usize::MAX,
                min_width: 0.0,
                tex_width: 128,
            },
            FallbackSpec {
                halo: 24,
                min_width: 256.0,
                tex_width: 256,
            },
            FallbackSpec {
                halo: 1,
                min_width: 1024.0,
                tex_width: 1024,
            },
            FallbackSpec {
                halo: 0,
                min_width: 2048.0,
                tex_width: 2048,
            },
            FallbackSpec {
                halo: 0,
                min_width: 3072.0,
                tex_width: 3072,
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
        let exec = ExecutionContext::new(executor, monitor);

        Self {
            widget,
            pages,
            layout,
            exec,
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
        let pages = PageData::new(&self.pages, &self.layout.rects, &visible);
        self.fbck_manager.update(&self.exec, &pages, transform);
        self.tile_manager.update(&self.exec, &pages, transform, vp);

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

struct PageData<'a> {
    pub pages: &'a [Page],
    pub layout: &'a [Rect<f64>],
    pub visible: &'a Range<usize>,
}

impl<'a> PageData<'a> {
    pub fn new(pages: &'a [Page], layout: &'a [Rect<f64>], visible: &'a Range<usize>) -> Self {
        Self {
            pages,
            layout,
            visible,
        }
    }
}

struct FallbackManager {
    levels: Vec<FallbackLevel>,
}

enum FallbackCacheEntry<T> {
    Empty,
    Cached(T),
    Pending(Handle<T>),
}

#[derive(Clone, Copy)]
struct FallbackSpec {
    pub halo: usize,
    pub min_width: f64,
    pub tex_width: i64,
}

struct FallbackLevel {
    spec: FallbackSpec,
    cache: HashMap<usize, FallbackCacheEntry<gdk::MemoryTexture>>,
}

impl FallbackManager {
    pub fn new(spec: &[FallbackSpec]) -> Self {
        let mut levels: Vec<_> = spec
            .iter()
            .map(|spec| FallbackLevel {
                spec: *spec,
                cache: HashMap::new(),
            })
            .collect();

        levels.sort_by_key(|x| x.spec.tex_width);

        FallbackManager { levels }
    }

    pub fn update<F>(&mut self, exec: &ExecutionContext, pages: &PageData<'_>, page_transform: F)
    where
        F: Fn(&Rect<f64>) -> Rect<f64>,
    {
        // process LoD levels from lowest to highest resolution
        for level in &mut self.levels {
            // page range for which the fallbacks should be computed
            let range = level.spec.range(pages.pages.len(), pages.visible);

            // remove fallbacks for out-of-scope pages
            level.cache.retain(|i, _| range.contains(i));

            // request new fallbacks
            let iter = range
                .clone()
                .zip(&pages.pages[range.clone()])
                .zip(&pages.layout[range]);

            for ((i, page), page_rect_pt) in iter {
                // transform page bounds to viewport
                let page_rect = page_transform(page_rect_pt);

                // skip if the page is too small and remove any entries we have for it
                if page_rect.size.x < level.spec.min_width {
                    level.cache.remove(&i);
                    continue;
                }

                let fallback = level.cache.entry(i).or_insert(FallbackCacheEntry::Empty);

                // if we already have a rendered result, skip
                if let FallbackCacheEntry::Cached(_) = fallback {
                    continue;
                }

                // check if a pending fallback has finished rendering and move it
                if fallback.is_render_finished() {
                    fallback.move_to_cached();
                    continue;
                }

                // if we have a pending fallback, update its priority
                if let FallbackCacheEntry::Pending(task) = fallback {
                    if pages.visible.contains(&i) {
                        task.set_priority(TaskPriority::High);
                    } else {
                        task.set_priority(TaskPriority::Low);
                    }
                    continue;
                }

                // compute page size for given width
                let scale = level.spec.tex_width as f64 / page_rect_pt.size.x;
                let page_size = page_rect_pt.size * scale;
                let page_size = vector![page_size.x.round() as i64, page_size.y.round() as i64];
                let rect = Rect::new(point![0, 0], page_size);

                // offload rendering to dedicated thread
                let priority = if pages.visible.contains(&i) {
                    TaskPriority::High
                } else {
                    TaskPriority::Low
                };

                let page = page.clone();
                let task = exec.submit(priority, move || {
                    let flags = RenderFlags::LcdText | RenderFlags::Annotations;
                    let color = Color::WHITE;

                    render_page_rect_gdk(&page, &page_size, &rect, color, flags).unwrap()
                });

                *fallback = FallbackCacheEntry::Pending(task);
            }
        }
    }

    pub fn fallback(&self, page_index: usize) -> Option<&gdk::MemoryTexture> {
        // get the cached fallback with the highest resolution
        for level in self.levels.iter().rev() {
            if let Some(FallbackCacheEntry::Cached(tex)) = level.cache.get(&page_index) {
                return Some(tex);
            }
        }

        None
    }
}

impl<T> FallbackCacheEntry<T>
where
    T: Send,
{
    pub fn is_render_finished(&self) -> bool {
        if let Self::Pending(task) = self {
            task.is_finished()
        } else {
            false
        }
    }

    pub fn move_to_cached(&mut self) {
        match std::mem::replace(self, FallbackCacheEntry::Empty) {
            FallbackCacheEntry::Empty => {}
            FallbackCacheEntry::Cached(tex) => *self = FallbackCacheEntry::Cached(tex),
            FallbackCacheEntry::Pending(task) => *self = FallbackCacheEntry::Cached(task.join()),
        }
    }
}

impl FallbackSpec {
    fn range(self, n: usize, base: &Range<usize>) -> Range<usize> {
        let start = base.start.saturating_sub(self.halo);
        let end = usize::min(base.end.saturating_add(self.halo), n);
        start..end
    }
}

struct TileManager<S> {
    scheme: S,
    cache: HashMap<usize, TileCache<gdk::MemoryTexture>>,
    halo: i64,
    min_retain_size: Vector2<f64>,
}

struct TileCache<T> {
    cached: HashMap<TileId, T>,
    pending: HashMap<TileId, Option<Handle<T>>>,
}

impl<S: TilingScheme> TileManager<S> {
    pub fn new(scheme: S) -> Self {
        Self {
            scheme,
            cache: HashMap::new(),
            halo: 1,
            min_retain_size: vector![25.0, 25.0],
        }
    }

    pub fn update<F>(
        &mut self,
        exec: &ExecutionContext,
        pages: &PageData<'_>,
        page_transform: F,
        vp: &Viewport,
    ) where
        F: Fn(&Rect<f64>) -> Rect<f64>,
    {
        // remove out-of-view pages from cache
        self.cache.retain(|page, _| pages.visible.contains(page));

        // update tiles for all visible pages
        let iter = pages
            .visible
            .clone()
            .zip(&pages.pages[pages.visible.clone()])
            .zip(&pages.layout[pages.visible.clone()]);

        for ((i, page), page_rect_pt) in iter {
            // transform page bounds to viewport
            let page_rect = page_transform(page_rect_pt);

            // recompute scale for rounded page
            let scale = page_rect.size.x / page_rect_pt.size.x;
            let vp_adj = Viewport { r: vp.r, scale };

            // update tiles for page
            self.update_page(exec, &vp_adj, page, i, &page_rect, page_rect_pt);
        }
    }

    fn update_page(
        &mut self,
        exec: &ExecutionContext,
        vp: &Viewport,
        page: &Page,
        page_index: usize,
        page_rect: &Rect<f64>,
        page_rect_pt: &Rect<f64>,
    ) {
        // viewport bounds relative to the page in pixels (area of page visible on screen)
        let visible_page = Rect::new(-page_rect.offs, vp.r.size)
            .clip(&Rect::new(point![0.0, 0.0], page_rect.size))
            .bounds();

        // tile bounds for the visible part of the page
        let tiles = self.scheme.tiles(vp, page_rect, &visible_page);

        // tile bounds for the full page
        let tiles_page = {
            let page_bounds = Rect::new(point![0.0, 0.0], page_rect.size).bounds();
            self.scheme.tiles(vp, page_rect, &page_bounds).rect
        };

        // tile bounds for the extended viewport (with cached halo tiles)
        let tiles_vp = {
            let tiles_vp = Bounds {
                x_min: tiles.rect.x_min - self.halo,
                x_max: tiles.rect.x_max + self.halo,
                y_min: tiles.rect.y_min - self.halo,
                y_max: tiles.rect.y_max + self.halo,
            };

            tiles_vp.clip(&tiles_page)
        };

        // get cached tiles for this page
        let entry = self
            .cache
            .entry(page_index)
            .or_insert_with(TileCache::empty);

        // helper for requesting tiles
        let mut request_tiles = |tile_rect: &Bounds<i64>, priority| {
            for (x, y) in tile_rect.range_iter() {
                let id = TileId::new(page_index, x, y, tiles.z);

                // check if we already have the tile
                if entry.cached.contains_key(&id) {
                    return;
                }

                // check if we already requested the tile and update the priority
                if let Some(entry) = entry.pending.get(&id) {
                    if let Some(task) = entry {
                        task.set_priority(priority);
                    }
                    return;
                }

                // compute page size and tile bounds
                let (page_size, rect) =
                    self.scheme
                        .render_rect(&page_rect_pt.size, &page_rect.size, &id);

                // offload rendering to dedicated thread
                let page = page.clone();
                let task = exec.submit(priority, move || {
                    let flags = RenderFlags::LcdText | RenderFlags::Annotations;
                    let color = Color::WHITE;

                    render_page_rect_gdk(&page, &page_size, &rect, color, flags).unwrap()
                });

                // store handle to the render task
                entry.pending.insert(id, Some(task));
            }
        };

        // request new tiles in view if not cached or pending
        request_tiles(&tiles.rect, TaskPriority::Medium);

        // pre-request new tiles around view with lower priority
        {
            let top = Bounds {
                x_min: tiles.rect.x_min,
                x_max: tiles.rect.x_max,
                y_min: (tiles.rect.y_min - self.halo).max(tiles_page.y_min),
                y_max: tiles.rect.y_min,
            };

            let bottom = Bounds {
                x_min: tiles.rect.x_min,
                x_max: tiles.rect.x_max,
                y_min: tiles.rect.y_max,
                y_max: (tiles.rect.y_max + self.halo).min(tiles_page.y_max),
            };

            let left = Bounds {
                x_min: (tiles.rect.x_min - self.halo).max(tiles_page.x_min),
                x_max: tiles.rect.x_min,
                y_min: (tiles.rect.y_min - self.halo).max(tiles_page.y_min),
                y_max: (tiles.rect.y_max + self.halo).min(tiles_page.y_max),
            };

            let right = Bounds {
                x_min: tiles.rect.x_max,
                x_max: (tiles.rect.x_max + self.halo).min(tiles_page.x_max),
                y_min: (tiles.rect.y_min - self.halo).max(tiles_page.y_min),
                y_max: (tiles.rect.y_max + self.halo).min(tiles_page.y_max),
            };

            request_tiles(&bottom, TaskPriority::Low);
            request_tiles(&top, TaskPriority::Low);
            request_tiles(&left, TaskPriority::Low);
            request_tiles(&right, TaskPriority::Low);
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

            // otherwise: check if tile is in the extended viewport
            tiles_vp.contains_point(&id.xy())
        });

        // find unused/occluded cached tiles and remove them
        let cached_keys: HashSet<_> = entry.cached.keys().cloned().collect();

        entry.cached.retain(|id, _tile| {
            // if the tile is on the current level: keep it if it is in the
            // extended viewport, drop it if not
            if id.z == tiles.z {
                return tiles_vp.contains_point(&id.xy());
            }

            // compute tile bounds
            let tile_rect = self.scheme.screen_rect(vp, page_rect, id);
            let tile_rect = tile_rect.bounds().round_outwards();
            let tile_rect_screen = tile_rect.translate(&page_rect.offs.coords);

            // check if tile is in view, drop it if it is not
            let vpz_rect = Rect::new(point![0.0, 0.0], vp.r.size).bounds();
            if !tile_rect_screen.intersects(&vpz_rect) {
                return false;
            }

            // if the tile is sufficently small, remove it
            let size = tile_rect_screen.rect().size;
            if size.x < self.min_retain_size.x && size.y < self.min_retain_size.y {
                return false;
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
                .all(|(x, y)| cached_keys.contains(&TileId::new(page_index, x, y, tiles.z)))
        });
    }

    pub fn tiles(
        &self,
        vp: &Viewport,
        page_index: usize,
        page_rect: &Rect<f64>,
    ) -> Vec<(Rect<f64>, &gdk::MemoryTexture)> {
        // viewport bounds relative to the page in pixels (area of page visible on screen)
        let visible_page = Rect::new(-page_rect.offs, vp.r.size)
            .clip(&Rect::new(point![0.0, 0.0], page_rect.size))
            .bounds();

        // tile bounds for viewport
        let tiles = self.scheme.tiles(vp, page_rect, &visible_page);

        // get cache entry
        let entry = if let Some(entry) = self.cache.get(&page_index) {
            entry
        } else {
            return Vec::new();
        };

        // build ordered render list
        let mut rlist: Vec<_> = entry
            .cached
            .iter()
            .filter(|(id, _)| {
                // if the tile has a different z-level we assume that it is
                // required (otherwise, it should have been removed in the
                // update)
                id.z != tiles.z ||
                // if z-levels match, check if the tile is inside the viewport
                tiles.rect.contains_point(&id.xy())
            })
            .collect();

        rlist.sort_unstable_by(|(id_a, _), (id_b, _)| {
            // sort by z-level:
            // - put all tiles with current z-level last
            // - sort rest in descending order (i.e., coarser tiles first)

            if id_a.z == id_b.z {
                // same z-levels are always equal
                Ordering::Equal
            } else if id_a.z == tiles.z {
                // put current z-level last
                Ordering::Greater
            } else if id_b.z == tiles.z {
                // put current z-level last
                Ordering::Less
            } else {
                // sort by z-level, descending
                if id_a.z < id_b.z {
                    Ordering::Greater
                } else {
                    Ordering::Less
                }
            }
        });

        rlist
            .into_iter()
            .map(|(id, data)| {
                let tile_rect = self.scheme.screen_rect(vp, page_rect, &id);
                let tile_rect = tile_rect.translate(&page_rect.offs.coords);

                (tile_rect, data)
            })
            .collect()
    }
}

impl<T> TileCache<T> {
    fn empty() -> Self {
        Self {
            cached: HashMap::new(),
            pending: HashMap::new(),
        }
    }
}

type Executor = executor::exec::priority::Executor<TaskPriority>;
type Handle<R> = executor::exec::priority::DropHandle<TaskPriority, R>;

struct ExecutionContext {
    executor: Executor,
    monitor: TaskMonitor,
}

impl ExecutionContext {
    pub fn new(executor: Executor, monitor: TaskMonitor) -> Self {
        Self { executor, monitor }
    }

    pub fn submit<F, R>(&self, priority: TaskPriority, closure: F) -> Handle<R>
    where
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static,
    {
        self.executor
            .submit_with(self.monitor.clone(), priority, closure)
            .cancel_on_drop()
    }
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, IntoPrimitive, TryFromPrimitive)]
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
