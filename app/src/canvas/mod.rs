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

pub struct Canvas {
    pages: Vec<Page>,
    layout: Layout,
}

impl Canvas {
    pub fn create(doc: Document) -> Self {
        let pages: Vec<_> = (0..(doc.pdf.pages().count()))
            .map(|i| doc.pdf.pages().get(i).unwrap())
            .collect();

        let layout_provider = VerticalLayout;
        let layout = layout_provider.compute(&pages, 10.0);

        Self { pages, layout }
    }

    pub fn bounds(&self) -> &Bounds<f64> {
        &self.layout.bounds
    }

    pub fn scale_bounds(&self) -> (f64, f64) {
        (1e-2, 1e4)
    }

    pub fn render(&self, vp: &Viewport, snapshot: &Snapshot) {
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
        for (page, offs) in self.pages.iter().zip(&self.layout.offsets) {
            let page_size: Vector2<f64> = na::convert(page.size());

            // transformation matrix: page to canvas
            let m_ptc = Translation2::from(*offs);

            // transformation matrix: page to viewport/screen
            let m_ptv = m_ctv * m_ptc;

            // convert page bounds to screen coordinates
            let page_rect = Rect::new(m_ptv * point![0.0, 0.0], m_ptv * page_size);

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
                continue;
            }

            // draw background
            snapshot.append_color(&gdk::RGBA::new(1.0, 1.0, 1.0, 1.0), &page_clipped.into());

            // tiled rendering (for now: very inefficient)
            let tile_size = vector![512, 512];

            // viewport bounds relative to the page in pixels (area of page visible on screen)
            let visible_page = Rect::new(-page_rect.offs, na::convert_unchecked(vp.r.size))
                .clip(&Rect::new(point![0, 0], page_rect.size))
                .bounds();

            // tile bounds
            let tiles = Bounds {
                x_min: visible_page.x_min / tile_size.x,
                y_min: visible_page.y_min / tile_size.y,
                x_max: (visible_page.x_max + tile_size.x - 1) / tile_size.x,
                y_max: (visible_page.y_max + tile_size.y - 1) / tile_size.y,
            };

            snapshot.push_clip(&screen_rect.into());

            for ix in tiles.range_x() {
                for iy in tiles.range_y() {
                    let tile_offs = vector![ix, iy].component_mul(&tile_size);

                    // allocate tile bitmap buffer
                    let stride = tile_size.x as usize * 4;
                    let mut buffer = vec![0; stride * tile_size.y as usize];

                    // render to tile
                    {
                        // wrap buffer in bitmap
                        let mut bmp = Bitmap::from_buf(
                            page.library().clone(),
                            tile_size.x as _,
                            tile_size.y as _,
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
                        page.render(&mut bmp, &layout, flags).unwrap();
                    }

                    // transfer buffer ownership to GTK/GDK
                    let bytes = glib::Bytes::from_owned(buffer);
                    let texture = gdk::MemoryTexture::new(
                        tile_size.x as _,
                        tile_size.y as _,
                        gdk::MemoryFormat::B8g8r8a8,
                        &bytes,
                        stride as _,
                    );

                    // draw background and page contents
                    let tile_screen_rect = Rect {
                        offs: page_rect.offs + tile_offs,
                        size: tile_size,
                    };
                    snapshot.append_texture(&texture, &tile_screen_rect.into());
                }
            }

            snapshot.pop();
        }
    }
}
