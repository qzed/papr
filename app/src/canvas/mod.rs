use gtk::traits::SnapshotExt;
use gtk::Snapshot;
use gtk::{gdk, glib};

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

pub struct Canvas {
    pages: Vec<PageData>,
    layout: Layout,
    tile_size: Vector2<i64>,
    pool: BufferPool,
}

impl Canvas {
    pub fn create(doc: Document) -> Self {
        let pages: Vec<_> = (0..(doc.pdf.pages().count()))
            .map(|i| PageData::new(doc.pdf.pages().get(i).unwrap()))
            .collect();

        let layout_provider = VerticalLayout;
        let layout = layout_provider.compute(pages.iter().map(|d| &d.page), 10.0);

        let tile_size = vector![512, 512];
        let pool = BufferPool::new(Some(64), (tile_size.x * tile_size.y * 4) as _);

        Self {
            pages,
            layout,
            tile_size,
            pool,
        }
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

        // TODO:
        //   - page shadow

        // transformation matrix: canvas to viewport
        let m_ctv = {
            let m_scale = Similarity2::from_scaling(vp.scale);
            let m_trans = Translation2::from(-vp.r.offs.coords);
            m_trans * m_scale
        };

        // page rendering
        let iter = self.pages.iter_mut().zip(&self.layout.rects);

        for (page, page_rect) in iter {
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

            // check if page is in view
            if page_clipped.size.x < 1 || page_clipped.size.y < 1 {
                // evict cached tiles for invisible pages
                page.tiles.storage.clear();

                // skip rendering
                continue;
            }

            // draw background
            snapshot.append_color(&gdk::RGBA::new(1.0, 1.0, 1.0, 1.0), &page_clipped.into());

            // tiled rendering (for now: very inefficient)

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

            // mark all tiles as invisible
            for tile in &mut page.tiles.storage {
                tile.visible = false;
            }

            snapshot.push_clip(&screen_rect.into());

            for (ix, iy) in tiles.range_iter() {
                let tile_id = TileId::new(ix, iy, page_rect.size.x);
                let tile_offs = vector![ix, iy].component_mul(&self.tile_size);

                // look for current tile
                let tile = page
                    .tiles
                    .storage
                    .iter_mut()
                    .find(|tile| tile.id == tile_id);

                // get cached texture or render tile
                let texture = if let Some(tile) = tile {
                    // mark tile as visible
                    tile.visible = true;

                    // return cached texture
                    tile.texture.clone()
                } else {
                    // allocate tile bitmap buffer
                    let stride = self.tile_size.x as usize * 4;
                    let mut buffer = self.pool.alloc();

                    // render to tile
                    {
                        // wrap buffer in bitmap
                        let mut bmp = Bitmap::from_buf(
                            page.page.library().clone(),
                            self.tile_size.x as _,
                            self.tile_size.y as _,
                            BitmapFormat::Bgra,
                            &mut buffer[..],
                            stride as _,
                        )
                        .unwrap();

                        // set up render layout
                        let layout = PageRenderLayout {
                            start: na::convert::<_, Vector2<i32>>(-tile_offs).into(),
                            size: na::convert(page_rect.size),
                            rotate: PageRotation::None,
                        };

                        // render page to bitmap
                        let flags = RenderFlags::LcdText | RenderFlags::Annotations;
                        page.page.render(&mut bmp, &layout, flags).unwrap();
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

                    // insert new tile
                    let tile = Tile {
                        id: tile_id,
                        visible: true,
                        texture: texture.clone(),
                    };
                    page.tiles.storage.push(tile);

                    texture
                };

                // draw tile to screen
                let tile_screen_rect = Rect {
                    offs: page_rect.offs + tile_offs,
                    size: self.tile_size,
                };
                snapshot.append_texture(&texture, &tile_screen_rect.into());
            }

            snapshot.pop();

            // free all invisible tiles
            page.tiles.storage.retain(|t| t.visible);
        }
    }
}

struct PageData {
    page: Page,
    tiles: TileCache,
}

impl PageData {
    fn new(page: Page) -> PageData {
        let tiles = TileCache::new();

        Self { page, tiles }
    }
}

pub struct TileCache {
    storage: Vec<Tile>,
}

impl TileCache {
    pub fn new() -> Self {
        Self {
            storage: Vec::new(),
        }
    }
}

#[derive(Debug)]
pub struct Tile {
    id: TileId,
    visible: bool,
    texture: gdk::MemoryTexture,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TileId {
    pub x: i64,
    pub y: i64,
    pub z: i64,
}

impl TileId {
    pub fn new(x: i64, y: i64, z: i64) -> Self {
        Self { x, y, z }
    }
}
