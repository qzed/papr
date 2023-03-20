use gtk::{graphene, glib, gdk};
use gtk::traits::SnapshotExt;
use gtk::Snapshot;
use nalgebra::{point, vector};
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

    pub fn render(&self, viewport: &Viewport, snapshot: &Snapshot) {
        snapshot.translate(&graphene::Point::new(
            -viewport.offset.x as f32,
            -viewport.offset.y as f32,
        ));
        snapshot.scale(viewport.scale as f32, viewport.scale as f32);

        // clip drawing to canvas area
        snapshot.push_clip(&self.bounds.into());

        // draw background
        snapshot.append_color(
            &gtk::gdk::RGBA::new(1.0, 1.0, 1.0, 1.0),
            &graphene::Rect::from(self.bounds),
        );

        // TODO:
        // - render only what's on screen
        // - render more than one page
        // - further optimizations (tiling?)

        // convert offset to PDF coordinates (pt)
        let page_size = self.page.size();
        let page_size_scaled = page_size * viewport.scale as f32;

        let flags = RenderFlags::Annotations;
        let layout = PageRenderLayout {
            start: point![0, 0],
            size: vector![page_size_scaled.x as _, page_size_scaled.y as _],
            rotate: PageRotation::None,
        };

        let lib = self.page.library().clone();
        let mut bmp = Bitmap::uninitialized(lib, page_size_scaled.x as _, page_size_scaled.y as _, BitmapFormat::Bgra).unwrap();

        self.page.render(&mut bmp, &layout, flags).unwrap();

        let bytes = glib::Bytes::from(bmp.buf());

        let texture = gdk::MemoryTexture::new(
            bmp.width() as i32,
            bmp.height() as i32,
            gdk::MemoryFormat::B8g8r8a8,
            &bytes,
            bmp.stride() as _,
        );

        snapshot.append_texture(
            &texture,
            &graphene::Rect::new(0.0, 0.0, page_size.x as _, page_size.y as _),
        );

        // pop the clip
        snapshot.pop();
    }
}
