use gtk::graphene::Rect;
use gtk::traits::SnapshotExt;
use gtk::Snapshot;
use gtk::{gdk, glib};

use nalgebra::{point, vector, Affine2, Matrix3, Vector2};

use pdfium::bitmap::{Bitmap, BitmapFormat};
use pdfium::doc::{Page, PageRenderLayout, PageRotation, RenderFlags};

use crate::pdf::Document;
use crate::types::{Bounds, Viewport};

pub struct Canvas {
    pages: Vec<Page>,
    bounds: Bounds<f64>,
    page_space: f64,
}

impl Canvas {
    pub fn create(doc: Document) -> Self {
        let mut pages = Vec::new();

        let page_space = 10.0;
        let mut x: f64 = 0.0;
        let mut y: f64 = 0.0;

        for i in 0..(doc.pdf.pages().count()) {
            let page = doc.pdf.pages().get(i).unwrap();
            let size = page.size();

            x = x.max(size.x as f64);
            y += size.y as f64;

            if i > 0 {
                y += page_space;
            }

            pages.push(page);
        }

        let bounds = Bounds {
            x_min: 0.0,
            y_min: 0.0,
            x_max: x,
            y_max: y,
        };

        Self {
            pages,
            bounds,
            page_space,
        }
    }

    pub fn bounds(&self) -> &Bounds<f64> {
        &self.bounds
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
        // - further optimizations (tiling?)

        // transformation matrix: canvas to viewport
        let m_ctv = {
            let m_scale = Matrix3::new_scaling(vp.scale);
            let m_scale = Affine2::from_matrix_unchecked(m_scale);

            let m_trans = Matrix3::new_translation(&-vp.offset.coords);
            let m_trans = Affine2::from_matrix_unchecked(m_trans);

            m_trans * m_scale
        };

        // page rendering
        let mut offs_y = 0.0;

        for page in &self.pages {
            let page_size: Vector2<f64> = nalgebra::convert(page.size());

            // transformation matrix: page to canvas
            let m_ptc = {
                let m = Matrix3::new_translation(&vector![0.0, offs_y]);
                Affine2::from_matrix_unchecked(m)
            };

            // transformation matrix: page to viewport
            let m_ptv = m_ctv * m_ptc;

            // convert page bounds to viewport coordinates
            let page_offs_v = m_ptv * point![0.0, 0.0];
            let page_size_v = m_ptv * page_size;

            // round coordinates for pixel-perfect rendering
            let page_offs_v = point![page_offs_v.x.round() as i64, page_offs_v.y.round() as i64];
            let page_size_v = vector![page_size_v.x.round() as i64, page_size_v.y.round() as i64];

            // update page offset
            offs_y += page_size.y + self.page_space;

            // clip page bounds to viewport
            let page_offs_v_clipped = point![page_offs_v.x.max(0), page_offs_v.y.max(0)];
            let page_size_v_clipped = vector![
                (page_offs_v.x + page_size_v.x).min(vp.size.x as i64) - page_offs_v_clipped.x,
                (page_offs_v.y + page_size_v.y).min(vp.size.y as i64) - page_offs_v_clipped.y
            ];

            // check if page is in view
            if page_size_v_clipped.x < 1 || page_size_v_clipped.y < 1 {
                continue;
            }

            // page offset in display pixels
            let page_offs_d = page_offs_v - page_offs_v_clipped;

            // allocate buffer to which the PDF is being rendered
            let stride = page_size_v_clipped.x as usize * 4;
            let mut buffer = vec![0; stride * page_size_v_clipped.y as usize];

            // render the PDF page
            {
                // wrap buffer in bitmap
                let mut bmp = Bitmap::from_buf(
                    page.library().clone(),
                    page_size_v_clipped.x as _,
                    page_size_v_clipped.y as _,
                    BitmapFormat::Bgra,
                    &mut buffer[..],
                    stride as _,
                )
                .unwrap();

                // set up render layout
                let layout = PageRenderLayout {
                    start: nalgebra::convert::<_, Vector2<i32>>(page_offs_d).into(),
                    size: nalgebra::convert(page_size_v),
                    rotate: PageRotation::None,
                };

                // render page to bitmap
                let flags = RenderFlags::LcdText | RenderFlags::Annotations;
                page.render(&mut bmp, &layout, flags).unwrap();
            }

            // transfer buffer ownership to GTK/GDK
            let bytes = glib::Bytes::from_owned(buffer);
            let texture = gdk::MemoryTexture::new(
                page_size_v_clipped.x as i32,
                page_size_v_clipped.y as i32,
                gdk::MemoryFormat::B8g8r8a8,
                &bytes,
                stride as _,
            );

            // draw background
            snapshot.append_color(
                &gtk::gdk::RGBA::new(1.0, 1.0, 1.0, 1.0),
                &Rect::new(
                    page_offs_v_clipped.x as _,
                    page_offs_v_clipped.y as _,
                    page_size_v_clipped.x as _,
                    page_size_v_clipped.y as _,
                ),
            );

            // draw page contents
            snapshot.append_texture(
                &texture,
                &Rect::new(
                    page_offs_v_clipped.x as _,
                    page_offs_v_clipped.y as _,
                    page_size_v_clipped.x as _,
                    page_size_v_clipped.y as _,
                ),
            );
        }
    }
}
