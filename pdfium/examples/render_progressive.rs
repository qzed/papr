use nalgebra::{point, vector};

use pdfium::bitmap::{Bitmap, BitmapFormat, Color, ColorScheme};
use pdfium::doc::{PageRenderLayout, PageRotation, RenderFlags, ProgressiveRenderStatus};
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

        let width = size.x as _;
        let height = size.y as _;

        // Allocate a bitmap for rendering.
        let mut bmp = Bitmap::uninitialized(lib.clone(), width, height, BitmapFormat::Bgra)?;

        // Clear the bitmap / set background.
        bmp.fill_rect(0, 0, width, height, Color::WHITE);

        // Render the page. We need to set the reverse-byte-order flag because
        // pdfium renders as BGRA by default, whereas the 'image' crate expects
        // RGBA. The reverse-byte-order flag changes pdfium's rendering to
        // RGBA.
        let flags = RenderFlags::Annotations | RenderFlags::ReverseByteOrder;
        let layout = PageRenderLayout {
            start: point![0, 0],
            size: vector![size.x as _, size.y as _],
            rotate: PageRotation::None,
        };

        // The color-scheme lets us override colors for rendering.
        let colors = ColorScheme {
            path_fill_color: Color::BLACK,
            path_stroke_color: Color::BLACK,
            text_fill_color: Color::BLACK,
            text_stroke_color: Color::BLACK,
        };

        // Specify when to pause/interrupt rendering.
        let mut count = 0;
        let should_pause = || {
            count += 1;

            // Just pause every 10th invocation in this example. This could be
            // something more clever, like a time-based pause.
            count % 10 == 0
        };

        // The render-command borrows the bitmap, so we need to make sure it's
        // terminated before we can save it.
        {
            // Start rendering
            let mut cmd = page.render_progressive_with_colorscheme(
                &mut bmp,
                &layout,
                flags,
                &colors,
                should_pause,
            )?;

            // Continue rendering until we are done. Alternatively, call
            // cmd.render_finish() to complete the render without further
            // pauses.
            let mut n = 0;
            while cmd.status() != ProgressiveRenderStatus::Complete {
                // Save an early copy of the file. This is a bit silly for
                // rendering to files, but for rendering to a display/canvas,
                // the bitmap could be copied here.
                {
                    let buf = cmd.bitmap().buf().to_owned();
                    let img = image::ImageBuffer::from_raw(width, height, buf).unwrap();
                    let img = image::DynamicImage::ImageRgba8(img);
                    img.save(format!("out-{i}.png")).unwrap();
                }

                println!("  render has been paused, continuing now");
                cmd.render_continue()?;

                // We can also stop the rendering process by refusing to
                // continue at any time.
                n += 1;
                if n >= 5 {
                    break;
                }
            }

            // The render command will be automatically cleaned up when it is
            // being dropped. Alternatively, call cmd.render_close() to do that
            // early.
        }

        // Save the file
        let img = image::ImageBuffer::from_raw(width, height, bmp.buf().to_owned()).unwrap();
        let img = image::DynamicImage::ImageRgba8(img);
        img.save(format!("out-{i}.png")).unwrap();
    }

    Ok(())
}
