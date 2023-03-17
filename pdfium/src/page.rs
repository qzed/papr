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
        unsafe {
            self.lib
                .ftable()
                .FPDF_GetPageCount(self.doc.handle().as_ptr()) as u32
        }
    }

    pub fn get(&self, index: u32) -> Result<Page> {
        let handle = unsafe {
            self.lib
                .ftable()
                .FPDF_LoadPage(self.doc.handle().as_ptr(), index as _)
        };
        let handle = self.lib.assert_ptr(handle)?;

        let page = Page::new(self.lib.clone(), self.doc.clone(), handle);

        // TODO: FPDF_GetPageLabel depends on page index... which might change,
        // should we load and chache it here?

        Ok(page)
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
}

impl Drop for PageInner {
    fn drop(&mut self) {
        unsafe { self.lib.ftable().FPDF_ClosePage(self.handle.as_ptr()) };
    }
}
