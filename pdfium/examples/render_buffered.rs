use nalgebra::{point, vector};

use pdfium::bitmap::{self, Bitmap, BitmapFormat};
use pdfium::doc::{PageRenderLayout, PageRotation, RenderFlags};
use pdfium::{Library, Result};

fn main() -> Result<()> {
    let file = std::env::args_os().nth(1).unwrap();

    let lib = Library::init()?;
    let doc = lib.load_file(file, None)?;

    let pages = doc.pages();

    for i in 0..pages.count() {
        let page = pages.get(i)?;
        let size = page.size();

        println!("render page {i} to file 'out-{i}.png'");

        // Allocate a buffer for rendering
        let width = size.x as _;
        let height = size.y as _;
        let stride = width * 4;

        let mut buf = vec![0; stride as usize * height as usize];

        {
            // Create a bitmap with our buffer as backing
            let mut bmp = Bitmap::from_buf(
                lib.clone(),
                width,
                height,
                BitmapFormat::Bgra,
                &mut buf[..],
                stride
            )?;

            // Clear the bitmap / set background
            bmp.fill_rect(0, 0, width, height, bitmap::Color::WHITE);

            // Render the page. We need to set the reverse-byte-order flag
            // because pdfium renders as BGRA by default, whereas the 'image'
            // crate expects RGBA. The reverse-byte-order flag changes pdfium's
            // rendering to RGBA.
            let flags = RenderFlags::Annotations | RenderFlags::ReverseByteOrder;
            let layout = PageRenderLayout {
                start: point![0, 0],
                size: vector![size.x as _, size.y as _],
                rotate: PageRotation::None,
            };

            page.render(&mut bmp, &layout, flags);
        }

        // Save the file
        let img = image::ImageBuffer::from_raw(width, height, buf).unwrap();
        let img = image::DynamicImage::ImageRgba8(img);
        img.save(format!("out-{i}.png")).unwrap();
    }

    Ok(())
}
