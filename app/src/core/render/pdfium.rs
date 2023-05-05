use std::collections::HashMap;
use std::ops::Range;
use std::sync::{Arc, Mutex};

use executor::exec::Monitor;

use nalgebra as na;
use nalgebra::Vector2;

use pdfium::bitmap::{BitmapFormat, Color};
use pdfium::doc::{Document, Page, PageRenderLayout, PageRotation, RenderFlags};

use crate::types::Rect;

use super::interop::{Bitmap, TileFactory};
use super::core::{TilePriority, TileProvider, TileSource};

pub type Executor = executor::exec::priority::Executor<TilePriority>;
pub type Handle<R> = executor::exec::priority::DropHandle<TilePriority, R>;

pub struct PdfTileProvider<M, F> {
    executor: Executor,
    monitor: M,
    factory: F,
    document: Document,
    page_cache: Arc<Mutex<HashMap<usize, Page>>>,
}

pub struct PdfTileSource<'a, M, F> {
    provider: &'a mut PdfTileProvider<M, F>,
    pages: Range<usize>,
}

#[derive(Debug, Clone)]
pub struct RenderOptions {
    pub flags: RenderFlags,
    pub background: Color,
}

impl<M, F> PdfTileProvider<M, F> {
    pub fn new(executor: Executor, monitor: M, factory: F, document: Document) -> Self {
        Self {
            executor,
            monitor,
            factory,
            document,
            page_cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

impl<M, T> TileProvider for PdfTileProvider<M, T>
where
    M: Monitor + Send + Clone + 'static,
    T: TileFactory + Send + Clone + 'static,
    T::Data: Send,
{
    type Source<'a> = PdfTileSource<'a, M, T>;

    fn request<F, R>(&mut self, pages: &Range<usize>, f: F) -> R
    where
        F: FnOnce(&mut Self::Source<'_>) -> R,
    {
        f(&mut PdfTileSource::new(self, pages.clone()))
    }
}

impl<'a, M, F> PdfTileSource<'a, M, F> {
    fn new(provider: &'a mut PdfTileProvider<M, F>, pages: Range<usize>) -> Self {
        let mut source = Self { provider, pages };
        source.prepare();
        source
    }

    fn prepare(&mut self) {
        // remove any cached pages that are no longer visible
        let cache = self.provider.page_cache.clone();
        let pages = self.pages.clone();

        self.provider.executor.submit(TilePriority::High, move || {
            cache.lock().unwrap().retain(|i, _| pages.contains(i));
        });
    }

    fn release(&mut self) {
        // remove any cached pages that are no longer visible
        let cache = self.provider.page_cache.clone();
        let pages = self.pages.clone();

        self.provider.executor.submit(TilePriority::Low, move || {
            cache.lock().unwrap().retain(|i, _| pages.contains(i));
        });
    }
}

impl<'a, M, F> Drop for PdfTileSource<'a, M, F> {
    fn drop(&mut self) {
        self.release()
    }
}

impl<'a, M, F> TileSource for PdfTileSource<'a, M, F>
where
    M: Monitor + Send + Clone + 'static,
    F: TileFactory + Send + Clone + 'static,
    F::Data: Send,
{
    type Data = F::Data;
    type Handle = Handle<F::Data>;
    type RequestOptions = RenderOptions;

    fn request(
        &mut self,
        page_index: usize,
        page_size: Vector2<i64>,
        rect: Rect<i64>,
        opts: &Self::RequestOptions,
        priority: TilePriority,
    ) -> Self::Handle {
        let factory = self.provider.factory.clone();
        let doc = self.provider.document.clone();
        let cache = self.provider.page_cache.clone();
        let visible = self.pages.clone();
        let opts = opts.clone();

        let task = move || {
            let mut cache = cache.lock().unwrap();

            // look up page in cache, storing it if visible
            let page = if visible.contains(&page_index) {
                cache
                    .entry(page_index)
                    .or_insert_with(|| doc.pages().get(page_index as _).unwrap())
                    .clone()
            } else {
                cache
                    .get(&page_index)
                    .cloned()
                    .unwrap_or_else(|| doc.pages().get(page_index as _).unwrap())
            };

            // render page to buffer
            let bmp = render_page_rect(&page, &page_size, &rect, &opts).unwrap();

            // create return value
            factory.create(bmp)
        };

        self.provider
            .executor
            .submit_with(self.provider.monitor.clone(), priority, task)
            .cancel_on_drop()
    }
}

fn render_page_rect(
    page: &Page,
    page_size: &Vector2<i64>,
    rect: &Rect<i64>,
    opts: &RenderOptions,
) -> pdfium::Result<Bitmap> {
    // allocate tile bitmap buffer
    let stride = rect.size.x as usize * 3;
    let mut buffer = vec![0; stride * rect.size.y as usize];

    // wrap buffer in bitmap
    let mut bmp = pdfium::bitmap::Bitmap::from_buf(
        page.library().clone(),
        rect.size.x as _,
        rect.size.y as _,
        BitmapFormat::Bgr,
        &mut buffer[..],
        stride as _,
    )?;

    // clear bitmap with background color
    bmp.fill_rect(0, 0, rect.size.x as _, rect.size.y as _, opts.background);

    // set up render layout
    let layout = PageRenderLayout {
        start: na::convert::<_, Vector2<i32>>(-rect.offs.coords).into(),
        size: na::convert(*page_size),
        rotate: PageRotation::None,
    };

    // render page to bitmap
    page.render(&mut bmp, &layout, opts.flags)?;

    // drop the wrapping bitmap
    drop(bmp);

    // construct bitmap
    let bmp = Bitmap {
        buffer: buffer.into_boxed_slice(),
        size: na::convert_unchecked(rect.size),
        stride: stride as _,
    };

    Ok(bmp)
}
