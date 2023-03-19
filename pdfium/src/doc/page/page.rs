use crate::bitmap::{Bitmap, ColorScheme};
use crate::doc::Document;
use crate::types::{Point2, Rect, Vector2};
use crate::{Library, Result};

use super::render;
use super::{PageRenderLayout, PageRotation, ProgressiveRender, RenderFlags};

use std::ffi::{c_double, c_int};
use std::ptr::NonNull;
use std::rc::Rc;

use nalgebra::{matrix, vector, Affine2, RealField};
use simba::scalar::SupersetOf;

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

    /// Render this page to a bitmap, progressively.
    ///
    /// This render call initiates a progressive render operation. Rendering is
    /// started immediately and interrupted upon request by (repeatedly)
    /// checking the provided closure. If this closure returns `true`,
    /// rendering will be paused and control will return to the caller.
    ///
    /// This function returns a [`ProgressiveRender`] object after either the
    /// first interrupt or render completion (whichever happens first). This
    /// object can be used to assess whether the operation has completed or
    /// whether it has been paused, as well as continuing or aborting it.
    ///
    /// See [`Self::render()`] for more information.
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

        let status =
            render::progressive::render_start(self, bitmap, layout, flags, &mut should_pause)?;

        let command = ProgressiveRender::new(self, bitmap, status, should_pause);
        Ok(command)
    }

    /// Render this page to a bitmap using the provided color scheme, progressively.
    ///
    /// See [`Self::render_progressive()`] for more information.
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

        let status = render::progressive::render_with_colorscheme_start(
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
