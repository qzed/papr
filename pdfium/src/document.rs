use std::ptr::NonNull;

use crate::{fileaccess::ReaderAccess, Library, Version};

pub type DocumentHandle = NonNull<pdfium_sys::fpdf_document_t__>;

pub struct Document {
    lib: Library,
    handle: DocumentHandle,

    // This is the underlying document storage. It needs to be kept alive for
    // the lifetime of the whole document and must not be modified.
    #[allow(unused)]
    backing: DocumentBacking,
}

impl Document {
    pub(crate) fn new(lib: Library, handle: DocumentHandle, backing: DocumentBacking) -> Self {
        Self {
            lib,
            handle,
            backing,
        }
    }

    pub fn handle(&self) -> DocumentHandle {
        self.handle
    }

    pub fn version(&self) -> Version {
        let mut version: i32 = 0;

        let success = unsafe {
            self.lib
                .ftable()
                .FPDF_GetFileVersion(self.handle.as_ptr(), &mut version)
        };

        // if this fails, the document was created with pdfium, but the version
        // has not been set yet
        if success != 0 {
            Version::from_i32(version)
        } else {
            Version::Unset
        }
    }
}

impl Drop for Document {
    fn drop(&mut self) {
        unsafe { self.lib.ftable().FPDF_CloseDocument(self.handle.as_ptr()) };
    }
}

#[allow(unused)]
pub(crate) enum DocumentBacking {
    Buffer { buffer: Vec<u8> },
    Reader { access: ReaderAccess },
}
