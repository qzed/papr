use gtk::traits::SnapshotExt;
use gtk::Snapshot;
use gtk::{gdk, glib};

use na::{point, vector, Point2, Similarity2, Translation2, Vector2};
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
            let page_rect = Rect::<i64> {
                offs: na::convert_unchecked(page_rect.offs),
                size: na::convert_unchecked(page_rect.size),
            };

            // clip page bounds to visible screen area (area on screen covered by page)
            let screen_rect = Rect::<i64>::new(point![0, 0], na::convert_unchecked(vp.r.size));
            let page_clipped = page_rect.clip(&screen_rect);

            // check if page is in view
            if page_clipped.size.x < 1 || page_clipped.size.y < 1 {
                continue;
            }

            // page offset in display pixels
            let page_offs_d = page_rect.offs - page_clipped.offs;

            // allocate buffer to which the PDF is being rendered
            let stride = page_clipped.size.x as usize * 4;
            let mut buffer = vec![0; stride * page_clipped.size.y as usize];

            // render the PDF page
            {
                // wrap buffer in bitmap
                let mut bmp = Bitmap::from_buf(
                    page.library().clone(),
                    page_clipped.size.x as _,
                    page_clipped.size.y as _,
                    BitmapFormat::Bgra,
                    &mut buffer[..],
                    stride as _,
                )
                .unwrap();

                // set up render layout
                let layout = PageRenderLayout {
                    start: na::convert::<_, Vector2<i32>>(page_offs_d).into(),
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
                page_clipped.size.x as _,
                page_clipped.size.y as _,
                gdk::MemoryFormat::B8g8r8a8,
                &bytes,
                stride as _,
            );

            // draw background and page contents
            snapshot.append_color(&gdk::RGBA::new(1.0, 1.0, 1.0, 1.0), &page_clipped.into());
            snapshot.append_texture(&texture, &page_clipped.into());
        }
    }
}
