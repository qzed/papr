use crate::bitmap::Bitmap;
use crate::doc::Document;
use crate::types::{Point2, Rect, Vector2};
use crate::{Library, Result};

use std::ffi::{c_double, c_int, c_void};
use std::ptr::NonNull;
use std::rc::Rc;

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
        if len <= 0 {
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

pub struct PageRenderLayout {
    pub start: Point2<i32>,
    pub size: Vector2<i32>,
    pub rotate: PageRotation,
}
