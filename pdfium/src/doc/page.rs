use crate::bitmap::{Bitmap, ColorScheme};
use crate::doc::Document;
use crate::types::{Point2, Rect, Vector2};
use crate::{Library, Result};

use std::ffi::{c_double, c_int, c_void};
use std::ptr::NonNull;
use std::rc::Rc;

use nalgebra::{matrix, vector, Affine2, RealField};
use simba::scalar::SupersetOf;

pub struct Pages<'a> {
    lib: &'a Library,
    doc: &'a Document,
}

impl<'a> Pages<'a> {
    pub(crate) fn new(lib: &'a Library, doc: &'a Document) -> Self {
        Pages { lib, doc }
    }

    pub fn count(&self) -> u32 {
        let doc = self.doc.handle().as_ptr();
        unsafe { self.lib.ftable().FPDF_GetPageCount(doc) as u32 }
    }

    pub fn get(&self, index: u32) -> Result<Page> {
        let doc = self.doc.handle().as_ptr();

        let page = unsafe { self.lib.ftable().FPDF_LoadPage(doc, index as _) };
        let page = self.lib.assert_ptr(page)?;

        let page = Page::new(self.lib.clone(), self.doc.clone(), page);

        // TODO: FPDF_GetPageLabel depends on page index... which might change,
        // should we load and chache it here?

        Ok(page)
    }

    pub fn get_label(&self, index: u32) -> Result<Option<String>> {
        let doc = self.doc.handle().as_ptr();

        // get length, including trailing zeros
        let len = unsafe {
            self.lib
                .ftable()
                .FPDF_GetPageLabel(doc, index as _, std::ptr::null_mut(), 0)
        };

        // zero-length: return empty string
        if len == 0 {
            return Ok(None);
        }

        // get actual string as bytes
        let mut buffer: Vec<u8> = vec![0; len as usize];
        let buffer_p = buffer.as_mut_ptr() as *mut c_void;

        let res = unsafe {
            self.lib
                .ftable()
                .FPDF_GetPageLabel(doc, index as _, buffer_p, buffer.len() as _)
        };

        assert_eq!(res, len);

        // convert bytes to string
        let value = crate::utils::utf16le::from_bytes(&buffer)?;
        Ok(Some(value))
    }
}

pub type PageHandle = NonNull<pdfium_sys::fpdf_page_t__>;

#[derive(Clone)]
pub struct Page {
    inner: Rc<PageInner>,
}

struct PageInner {
    lib: Library,
    doc: Document,
    handle: PageHandle,
}

impl Page {
    pub(crate) fn new(lib: Library, doc: Document, handle: PageHandle) -> Self {
        let inner = PageInner { lib, doc, handle };

        Self {
            inner: Rc::new(inner),
        }
    }

    pub fn handle(&self) -> PageHandle {
        self.inner.handle
    }

    pub fn document(&self) -> &Document {
        &self.inner.doc
    }

    pub fn library(&self) -> &Library {
        &self.inner.lib
    }

    pub fn width(&self) -> f32 {
        unsafe {
            self.library()
                .ftable()
                .FPDF_GetPageWidthF(self.handle().as_ptr())
        }
    }

    pub fn height(&self) -> f32 {
        unsafe {
            self.library()
                .ftable()
                .FPDF_GetPageHeightF(self.handle().as_ptr())
        }
    }

    pub fn size(&self) -> Vector2<f32> {
        Vector2::new(self.width(), self.height())
    }

    pub fn bounding_box(&self) -> Result<Rect> {
        let page = self.handle().as_ptr();

        let mut rect = pdfium_sys::FS_RECTF {
            left: 0.0,
            top: 0.0,
            right: 0.0,
            bottom: 0.0,
        };

        let status = unsafe {
            self.library()
                .ftable()
                .FPDF_GetPageBoundingBox(page, &mut rect)
        };
        self.library().assert(status != 0)?;

        Ok(Rect::from(rect))
    }

    pub fn transform_device_to_page(
        &self,
        layout: &PageRenderLayout,
        device: Point2<i32>,
    ) -> Result<Point2<f32>> {
        let handle = self.handle().as_ptr();

        let mut page_x: c_double = 0.0;
        let mut page_y: c_double = 0.0;

        let status = unsafe {
            self.library().ftable().FPDF_DeviceToPage(
                handle,
                layout.start.x,
                layout.start.y,
                layout.size.x,
                layout.size.y,
                layout.rotate.as_i32(),
                device.x,
                device.y,
                &mut page_x,
                &mut page_y,
            )
        };
        self.library().assert(status != 0)?;

        Ok(Point2::new(page_x as _, page_y as _))
    }

    pub fn transform_page_to_device(
        &self,
        layout: &PageRenderLayout,
        page: Point2<f32>,
    ) -> Result<Point2<i32>> {
        let handle = self.handle().as_ptr();

        let mut device_x: c_int = 0;
        let mut device_y: c_int = 0;

        let status = unsafe {
            self.library().ftable().FPDF_PageToDevice(
                handle,
                layout.start.x,
                layout.start.y,
                layout.size.x,
                layout.size.y,
                layout.rotate.as_i32(),
                page.x as _,
                page.y as _,
                &mut device_x,
                &mut device_y,
            )
        };
        self.library().assert(status != 0)?;

        Ok(Point2::new(device_x, device_x))
    }

    /// Get the display matrix, transforming page coordinates to display/device
    /// coordinates.
    pub fn display_transform<T>(
        &self,
        start: Point2<T>,
        size: Vector2<T>,
        rotate: PageRotation,
    ) -> Affine2<T>
    where
        T: RealField + Copy + SupersetOf<f32>,
    {
        let page_size = self.size().cast::<T>();

        let left = start.x;
        let top = start.y;
        let right = start.x + size.x;
        let bottom = start.y + size.y;

        let (v0, v1, v2) = match rotate {
            PageRotation::None => {
                let v0 = vector![left, bottom];
                let v1 = vector![left, top];
                let v2 = vector![right, bottom];
                (v0, v1, v2)
            }
            PageRotation::Deg90 => {
                let v0 = vector![left, top];
                let v1 = vector![right, top];
                let v2 = vector![left, bottom];
                (v0, v1, v2)
            }
            PageRotation::Deg180 => {
                let v0 = vector![right, top];
                let v1 = vector![right, bottom];
                let v2 = vector![left, top];
                (v0, v1, v2)
            }
            PageRotation::Deg270 => {
                let v0 = vector![right, bottom];
                let v1 = vector![left, bottom];
                let v2 = vector![right, top];
                (v0, v1, v2)
            }
        };

        let m = matrix! {
            (v2.x - v0.x) / page_size.x, (v1.x - v0.x) / page_size.y, v0.x;
            (v2.y - v0.y) / page_size.x, (v1.y - v0.y) / page_size.y, v0.y;
            T::zero(), T::zero(), T::one();
        };

        nalgebra::try_convert(m).unwrap()
    }

    /// Render this page to a bitmap, using the specified layout and options.
    ///
    /// Translation, scaling, and rotation (90° steps) can be specified via
    /// `layout`. Note that the size of the provided bitmap does not have to be
    /// equal to the the (scaled) page size. This allows rendering only the
    /// parts relevant to the current viewport.
    pub fn render<C>(
        &self,
        bitmap: &mut Bitmap<C>,
        layout: &PageRenderLayout,
        flags: RenderFlags,
    ) -> Result<()> {
        let page = self.handle().as_ptr();
        let bitmap = bitmap.handle().as_ptr();

        unsafe {
            self.library().ftable().FPDF_RenderPageBitmap(
                bitmap,
                page,
                layout.start.x,
                layout.start.y,
                layout.size.x,
                layout.size.y,
                layout.rotate.as_i32(),
                flags.bits() as _,
            )
        };
        self.library().assert_status()
    }

    /// Render this page to a bitmap, using the specified transformation and options.
    ///
    /// The provided matrix is applied to the display-transformed page, i.e., a
    /// point `a` on the page is transformed to a point `b` on the rendered
    /// output in the following way:
    /// ```txt
    /// b = transform * display_transform * a
    /// ```
    /// where `transform` is the provided transform and `display_transform` is
    /// the default display transform, which can be obtained by calling
    /// [`Self::display_transform()`] via
    /// ```no_run
    /// # use nalgebra::point;
    /// let display_transform = page.display_transform(
    ///         point![0.0, 0.0], page.size(), PageRotation::None);
    /// ```
    ///
    /// Note that `display_transform` is a mapping that flips the y-coordinate
    /// and positions the origin at the top of the page. It essentially
    /// transfers from the PDF page coordinate system (y goes from bottom to
    /// top with the origin at the bottom left corner of the page) to the
    /// standard display coordinate system (y goes from top to bottom with the
    /// origin at the top left corner of the page). It does not do any scaling
    /// or rotations.
    ///
    /// Note that the size of the provided bitmap does not have to be equal to
    /// the the (scaled) page or clip size. This allows rendering only the
    /// parts relevant to the current viewport.
    ///
    /// Clipping is performed after applying both transforms, meaning that clip
    /// coordinates are given as pixel coordinates in the output image.
    pub fn render_with_transform<C>(
        &self,
        bitmap: &mut Bitmap<C>,
        transform: &Affine2<f32>,
        clip: &Rect,
        flags: RenderFlags,
    ) -> Result<()> {
        let page = self.handle().as_ptr();
        let bitmap = bitmap.handle().as_ptr();
        let matrix = crate::types::affine_to_pdfmatrix(transform);
        let clip = pdfium_sys::FS_RECTF::from(clip);

        unsafe {
            self.library().ftable().FPDF_RenderPageBitmapWithMatrix(
                bitmap,
                page,
                &matrix,
                &clip,
                flags.bits() as _,
            )
        };
        self.library().assert_status()
    }

    pub fn render_progressive<'a, 'b, C, F>(
        &'a self,
        bitmap: &'b mut Bitmap<C>,
        layout: &PageRenderLayout,
        flags: RenderFlags,
        should_pause: F,
    ) -> Result<ProgressiveRender<'a, 'b, C, F>>
    where
        F: FnMut() -> bool,
    {
        let mut should_pause = should_pause;

        let status = progressive::render_start(self, bitmap, layout, flags, &mut should_pause)?;

        let command = ProgressiveRender::new(self, bitmap, status, should_pause);
        Ok(command)
    }

    pub fn render_progressive_with_colorscheme<'a, 'b, C, F>(
        &'a self,
        bitmap: &'b mut Bitmap<C>,
        layout: &PageRenderLayout,
        flags: RenderFlags,
        colors: &ColorScheme,
        should_pause: F,
    ) -> Result<ProgressiveRender<'a, 'b, C, F>>
    where
        F: FnMut() -> bool,
    {
        let mut should_pause = should_pause;

        let status = progressive::render_with_colorscheme_start(
            self,
            bitmap,
            layout,
            flags,
            colors,
            &mut should_pause,
        )?;

        let command = ProgressiveRender::new(self, bitmap, status, should_pause);
        Ok(command)
    }
}

impl Drop for PageInner {
    fn drop(&mut self) {
        unsafe { self.lib.ftable().FPDF_ClosePage(self.handle.as_ptr()) };
    }
}

/// Page rotation used for rendering.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum PageRotation {
    /// Do not rotate.
    None,

    /// Rotate 90 degrees clockwise.
    Deg90,

    /// Rotate 180 degrees clockwise.
    Deg180,

    /// Rotate 270 degrees clockwise.
    Deg270,
}

impl PageRotation {
    fn as_i32(&self) -> i32 {
        match self {
            PageRotation::None => 0,
            PageRotation::Deg90 => 1,
            PageRotation::Deg180 => 2,
            PageRotation::Deg270 => 3,
        }
    }
}

/// Descriptor for the page/viewport layout used for rendering.
pub struct PageRenderLayout {
    /// Offset of the display/viewport on the page, in pixels.
    pub start: Point2<i32>,

    /// Size of the full page to be rendered, in pixels.
    pub size: Vector2<i32>,

    /// Rotation of the page.
    pub rotate: PageRotation,
}

bitflags::bitflags! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    pub struct RenderFlags: u32 {
        /// Render annotations
        const Annotations = pdfium_sys::FPDF_ANNOT;

        /// Use text rendering optimized for LCD displays. This flag will only
        /// take effect if anti-aliasing is enabled for text.
        const LcdText = pdfium_sys::FPDF_LCD_TEXT;

        /// Don't use the native text output available on some platforms.
        const NoNativeText = pdfium_sys::FPDF_NO_NATIVETEXT;

        /// Grayscale output.
        const Grayscale = pdfium_sys::FPDF_GRAYSCALE;

        /// Limit image cache size.
        const LimitImageCache = pdfium_sys::FPDF_RENDER_LIMITEDIMAGECACHE;

        /// Always use halftone for image stretching.
        const ForceHalftone = pdfium_sys::FPDF_RENDER_FORCEHALFTONE;

        /// Render for printing.
        const Print = pdfium_sys::FPDF_PRINTING;

        /// Disable anti-aliasing on text. This flag will also disable LCD
        /// optimization for text rendering.
        const NoSmoothText = pdfium_sys::FPDF_RENDER_NO_SMOOTHTEXT;

        /// Disable anti-aliasing on images.
        const NoSmoothImage = pdfium_sys::FPDF_RENDER_NO_SMOOTHIMAGE;

        /// Set to disable anti-aliasing on paths.
        const NoSmoothPath = pdfium_sys::FPDF_RENDER_NO_SMOOTHPATH;

        /// Set whether to render in a reverse Byte order, this flag is only
        /// used when rendering to a bitmap.
        const ReverseByteOrder = pdfium_sys::FPDF_REVERSE_BYTE_ORDER;

        /// Whether fill paths need to be stroked. This flag is only used when
        /// a color scheme is passed in, since with a single fill color for
        /// paths the boundaries of adjacent fill paths are less visible.
        const ConvertFillToStroke = pdfium_sys::FPDF_CONVERT_FILL_TO_STROKE;
    }
}

pub use progressive::{ProgressiveRender, ProgressiveRenderStatus};

mod progressive {
    use crate::bitmap::{Bitmap, ColorScheme};
    use crate::doc::{Page, PageRenderLayout, RenderFlags};
    use crate::{Error, Result};

    use std::ffi::{c_int, c_void};
    use std::panic::AssertUnwindSafe;

    pub struct ProgressiveRender<'a, 'b, C, F> {
        page: &'a Page,
        bitmap: &'b Bitmap<C>,
        status: ProgressiveRenderStatus,
        should_pause: F,
        closed: bool,
    }

    impl<'a, 'b, C, F> ProgressiveRender<'a, 'b, C, F> {
        pub(crate) fn new(
            page: &'a Page,
            bitmap: &'b Bitmap<C>,
            status: ProgressiveRenderStatus,
            should_pause: F,
        ) -> ProgressiveRender<'a, 'b, C, F> {
            ProgressiveRender {
                page,
                bitmap,
                status,
                should_pause,
                closed: false,
            }
        }

        pub fn page(&self) -> &'a Page {
            self.page
        }

        pub fn bitmap(&self) -> &'b Bitmap<C> {
            self.bitmap
        }

        pub fn status(&self) -> ProgressiveRenderStatus {
            self.status
        }

        pub fn render_continue(&mut self) -> Result<ProgressiveRenderStatus>
        where
            F: FnMut() -> bool,
        {
            self.status = render_continue(self.page, &mut self.should_pause)?;
            Ok(self.status)
        }

        pub fn render_finish(&mut self) -> Result<()> {
            self.status = render_finish(self.page)?;
            assert_eq!(self.status, ProgressiveRenderStatus::Complete);

            Ok(())
        }

        pub fn render_close(&mut self) {
            if !self.closed {
                render_close(self.page);
                self.closed = true;
            }
        }
    }

    impl<'a, 'b, C, F> Drop for ProgressiveRender<'a, 'b, C, F> {
        fn drop(&mut self) {
            self.render_close()
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum ProgressiveRenderStatus {
        Incomplete,
        Complete,
    }

    impl ProgressiveRenderStatus {
        fn try_from_raw(status: c_int) -> Result<Self> {
            match status as _ {
                pdfium_sys::FPDF_RENDER_READY => {
                    // This state is generally not exposed. We cannot directly
                    // query the status (only get it as return value of a
                    // render call), at which point it will be one of the
                    // values below.
                    panic!("unexpected progressive render state: ready")
                }
                pdfium_sys::FPDF_RENDER_FAILED => {
                    // This status generally does not indicate an underlying
                    // library failure, but rather an invalid invocation of one
                    // of the render functions. For example, calling
                    // render_continue() without a previous render_start()
                    // call.
                    Err(Error::InvalidOperation)
                }
                pdfium_sys::FPDF_RENDER_TOBECONTINUED => Ok(ProgressiveRenderStatus::Incomplete),
                pdfium_sys::FPDF_RENDER_DONE => Ok(ProgressiveRenderStatus::Complete),
                _ => {
                    // There should not be any other status.
                    panic!("progressive render returned unexpected status code: {status}")
                }
            }
        }
    }

    pub fn render_start<C, F>(
        page: &Page,
        bitmap: &mut Bitmap<C>,
        layout: &PageRenderLayout,
        flags: RenderFlags,
        should_pause: F,
    ) -> Result<ProgressiveRenderStatus>
    where
        F: FnMut() -> bool,
    {
        // set up callback trampoline
        let mut cbdata = CallbackData::new(should_pause);

        let mut pause = pdfium_sys::IFSDK_PAUSE {
            version: 1,
            NeedToPauseNow: Some(cbdata.trampoline()),
            user: &mut cbdata as *mut CallbackData<_> as *mut c_void,
        };

        // start render
        let status = unsafe {
            page.library().ftable().FPDF_RenderPageBitmap_Start(
                bitmap.handle().as_ptr(),
                page.handle().as_ptr(),
                layout.start.x,
                layout.start.y,
                layout.size.x,
                layout.size.y,
                layout.rotate.as_i32(),
                flags.bits() as _,
                &mut pause,
            )
        };

        // check for panic in callback
        if let Err(err) = cbdata.panic {
            std::panic::resume_unwind(err)
        }

        // check for error in render call
        page.library().assert_status()?;

        ProgressiveRenderStatus::try_from_raw(status)
    }

    pub fn render_with_colorscheme_start<C, F>(
        page: &Page,
        bitmap: &mut Bitmap<C>,
        layout: &PageRenderLayout,
        flags: RenderFlags,
        colors: &ColorScheme,
        should_pause: F,
    ) -> Result<ProgressiveRenderStatus>
    where
        F: FnMut() -> bool,
    {
        let colors = (*colors).into();

        // set up callback trampoline
        let mut cbdata = CallbackData::new(should_pause);

        let mut pause = pdfium_sys::IFSDK_PAUSE {
            version: 1,
            NeedToPauseNow: Some(cbdata.trampoline()),
            user: &mut cbdata as *mut CallbackData<_> as *mut c_void,
        };

        // start render
        let status = unsafe {
            page.library()
                .ftable()
                .FPDF_RenderPageBitmapWithColorScheme_Start(
                    bitmap.handle().as_ptr(),
                    page.handle().as_ptr(),
                    layout.start.x,
                    layout.start.y,
                    layout.size.x,
                    layout.size.y,
                    layout.rotate.as_i32(),
                    flags.bits() as _,
                    &colors,
                    &mut pause,
                )
        };

        // check for panic in callback
        if let Err(err) = cbdata.panic {
            std::panic::resume_unwind(err)
        }

        // check for error in render call
        page.library().assert_status()?;

        ProgressiveRenderStatus::try_from_raw(status)
    }

    pub fn render_continue<F>(page: &Page, should_pause: F) -> Result<ProgressiveRenderStatus>
    where
        F: FnMut() -> bool,
    {
        // set up callback trampoline
        let mut cbdata = CallbackData::new(should_pause);

        let mut pause = pdfium_sys::IFSDK_PAUSE {
            version: 1,
            NeedToPauseNow: Some(cbdata.trampoline()),
            user: &mut cbdata as *mut CallbackData<_> as *mut c_void,
        };

        // continue render
        let status = unsafe {
            page.library()
                .ftable()
                .FPDF_RenderPage_Continue(page.handle().as_ptr(), &mut pause)
        };

        // check for panic in callback
        if let Err(err) = cbdata.panic {
            std::panic::resume_unwind(err)
        }

        // check for error in render call
        page.library().assert_status()?;

        ProgressiveRenderStatus::try_from_raw(status)
    }

    pub fn render_finish(page: &Page) -> Result<ProgressiveRenderStatus> {
        // continue render
        let status = unsafe {
            page.library()
                .ftable()
                .FPDF_RenderPage_Continue(page.handle().as_ptr(), std::ptr::null_mut())
        };

        // check for error in render call
        page.library().assert_status()?;

        ProgressiveRenderStatus::try_from_raw(status)
    }

    pub fn render_close(page: &Page) {
        unsafe {
            page.library()
                .ftable()
                .FPDF_RenderPage_Close(page.handle().as_ptr());
        }
    }

    struct CallbackData<C> {
        closure: C,
        panic: std::thread::Result<()>,
    }

    impl<C> CallbackData<C>
    where
        C: FnMut() -> bool,
    {
        fn new(closure: C) -> Self {
            Self {
                closure,
                panic: Ok(()),
            }
        }

        fn trampoline(&self) -> Trampoline {
            trampoline::<C>
        }
    }

    type Trampoline = unsafe extern "C" fn(*mut pdfium_sys::IFSDK_PAUSE) -> i32;

    unsafe extern "C" fn trampoline<C>(param: *mut pdfium_sys::IFSDK_PAUSE) -> c_int
    where
        C: FnMut() -> bool,
    {
        let data: &mut CallbackData<C> = &mut *((*param).user as *mut CallbackData<C>);

        let result = std::panic::catch_unwind(AssertUnwindSafe(&mut data.closure));
        let should_pause = match result {
            Ok(should_pause) => {
                data.panic = Ok(());
                should_pause
            }
            Err(err) => {
                data.panic = Err(err);
                true
            }
        };

        should_pause as c_int
    }
}
