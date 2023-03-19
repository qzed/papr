use crate::doc::{Document, Page};
use crate::{Library, Result};

use std::ffi::c_void;

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
