use std::ffi::c_void;
use std::ptr::NonNull;
use std::rc::Rc;

use crate::{Document, Library, Result};

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
        let value = crate::utils::utf16le_from_bytes(&buffer)?;
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
}

impl Drop for PageInner {
    fn drop(&mut self) {
        unsafe { self.lib.ftable().FPDF_ClosePage(self.handle.as_ptr()) };
    }
}
