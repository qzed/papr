use super::{Metadata, Pages, Version};

use crate::bindings::Handle;
use crate::io::fileaccess::ReaderAccess;
use crate::utils::sync::{Rc, Unused};
use crate::Library;

pub type DocumentHandle = Handle<pdfium_sys::fpdf_document_t__>;

#[derive(Clone)]
pub struct Document {
    inner: Rc<DocumentInner>,
}

struct DocumentInner {
    lib: Library,
    handle: DocumentHandle,

    // This is the underlying document storage. It needs to be kept alive for
    // the lifetime of the whole document and must not be modified.
    #[allow(unused)]
    backing: Unused<DocumentBacking>,
}

#[allow(unused)]
pub(crate) enum DocumentBacking {
    Buffer { buffer: Vec<u8> },
    Reader { access: ReaderAccess },
}

impl Document {
    pub(crate) fn new(lib: Library, handle: DocumentHandle, backing: DocumentBacking) -> Self {
        let inner = DocumentInner {
            lib,
            handle,
            backing: Unused::new(backing),
        };

        Self {
            inner: Rc::new(inner),
        }
    }

    pub fn handle(&self) -> &DocumentHandle {
        &self.inner.handle
    }

    pub fn library(&self) -> &Library {
        &self.inner.lib
    }

    pub fn version(&self) -> Version {
        let lib = self.handle().get();

        let mut version: i32 = 0;
        let success = unsafe {
            self.library()
                .ftable()
                .FPDF_GetFileVersion(lib, &mut version)
        };

        // if this fails, the document was created with pdfium, but the version
        // has not been set yet
        if success != 0 {
            Version::from_i32(version)
        } else {
            Version::Unset
        }
    }

    pub fn metadata(&self) -> Metadata {
        Metadata::new(self.library(), self)
    }

    pub fn pages(&self) -> Pages {
        Pages::new(self.library(), self)
    }
}

impl Drop for DocumentInner {
    fn drop(&mut self) {
        unsafe { self.lib.ftable().FPDF_CloseDocument(self.handle.get()) };
    }
}
