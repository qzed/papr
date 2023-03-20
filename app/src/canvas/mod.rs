use gtk::graphene::{Point, Rect};
use gtk::traits::SnapshotExt;
use gtk::Snapshot;
use gtk::{gdk, glib};

use nalgebra::{point, vector, Affine2, Matrix3, Vector2};

use pdfium::bitmap::{Bitmap, BitmapFormat};
use pdfium::doc::{Page, PageRenderLayout, PageRotation, RenderFlags};

use crate::pdf::Document;
use crate::types::{Bounds, Viewport};

pub struct Canvas {
    page: Page,
    bounds: Bounds,
}

impl Canvas {
    pub fn create(doc: Document) -> Self {
        let page = doc.pdf.pages().get(0).unwrap();
        let size = page.size();

        let bounds = Bounds {
            x_min: 0.0,
            y_min: 0.0,
            x_max: size.x as _,
            y_max: size.y as _,
        };

        Self { page, bounds }
    }

    pub fn bounds(&self) -> &Bounds {
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
        // - render more than one page
        // - further optimizations (tiling?)

        // do the drawing in canvas coordinates
        snapshot.translate(&Point::new(-vp.offset.x as f32, -vp.offset.y as f32));
        snapshot.scale(vp.scale as f32, vp.scale as f32);

        // transformation matrix: canvas to viewport
        let m_ctv = {
            let m_scale = Matrix3::new_scaling(vp.scale);
            let m_scale = Affine2::from_matrix_unchecked(m_scale);

            let m_trans = Matrix3::new_translation(&-vp.offset.coords);
            let m_trans = Affine2::from_matrix_unchecked(m_trans);

            m_trans * m_scale
        };

        // transformation matrix: viewport to canvas
        let m_vtc = m_ctv.inverse();

        // convert viewport bounds (0, 0, width, height) to canvas coordinates
        let vp_offs_c = m_vtc * point![0.0, 0.0];
        let vp_size_c = m_vtc * vp.size;

        // page rendering

        // transformation matrix: page to canvas
        let m_ptc = {
            let m_trans = Matrix3::new_translation(&vector![0.0, 0.0]);
            let m_trans = Affine2::from_matrix_unchecked(m_trans);

            m_trans
        };

        // transformation matrix: canvas to page
        let m_ctp = m_ptc.inverse();

        // transformation matrix: page to viewport
        let m_ptv = m_ctv * m_ptc;

        // convert viewport bounds to page-local coordinates
        let vp_offs_p = m_ctp * vp_offs_c;
        let vp_size_p = m_ctp * vp_size_c;

        // clip page-local viewport to page bounds (0, 0, width, height)
        let page_size: Vector2<f64> = nalgebra::convert(self.page.size());

        let vp_offs_p_clipped = point![vp_offs_p.x.max(0.0), vp_offs_p.y.max(0.0)];
        let vp_size_p_clipped = vector![
            (vp_offs_p.x + vp_size_p.x).min(page_size.x) - vp_offs_p_clipped.x,
            (vp_offs_p.y + vp_size_p.y).min(page_size.y) - vp_offs_p_clipped.y
        ];

        // convert clipped viewport back to canvas coordinates
        let vp_offs_c_clipped = m_ptc * vp_offs_p_clipped;
        let vp_size_c_clipped = m_ptc * vp_size_p_clipped;

        // offset into page in render pixels
        let page_px_offs = m_ptv * vp_offs_p_clipped.coords;

        // full page size in render pixels
        let page_px_size = m_ptv * page_size;

        // viewport size in render pixels
        let page_px_vpsize = m_ptv * vp_size_p_clipped;

        // allocate bitmap
        let mut bmp = Bitmap::uninitialized(
            self.page.library().clone(),
            page_px_vpsize.x as _,
            page_px_vpsize.y as _,
            BitmapFormat::Bgra,
        )
        .unwrap();

        // set up render layout
        let layout = PageRenderLayout {
            start: nalgebra::convert_unchecked::<_, Vector2<i32>>(-page_px_offs).into(),
            size: nalgebra::convert_unchecked::<_, Vector2<i32>>(page_px_size),
            rotate: PageRotation::None,
        };

        // render page to bitmap
        let flags = RenderFlags::Annotations;
        self.page.render(&mut bmp, &layout, flags).unwrap();

        // convert rendered bitmap to GTK texture
        let bytes = glib::Bytes::from(bmp.buf());

        let texture = gdk::MemoryTexture::new(
            page_px_vpsize.x as i32,
            page_px_vpsize.y as i32,
            gdk::MemoryFormat::B8g8r8a8,
            &bytes,
            bmp.stride() as _,
        );

        // draw background
        snapshot.append_color(
            &gtk::gdk::RGBA::new(1.0, 1.0, 1.0, 1.0),
            &Rect::new(
                vp_offs_c_clipped.x as _,
                vp_offs_c_clipped.y as _,
                vp_size_c_clipped.x as _,
                vp_size_c_clipped.y as _,
            ),
        );

        // draw page contents
        snapshot.append_texture(
            &texture,
            &Rect::new(
                vp_offs_c_clipped.x as _,
                vp_offs_c_clipped.y as _,
                vp_size_c_clipped.x as _,
                vp_size_c_clipped.y as _,
            ),
        );
    }
}
