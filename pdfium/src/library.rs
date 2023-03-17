use crate::bindings::{Bindings, FnTable};
use crate::document::DocumentBacking;
use crate::{Document, Error, ErrorCode, Result};

use std::ffi::{c_void, CString};
use std::fs::File;
use std::io::{Read, Seek};
use std::path::{Path, PathBuf};
use std::ptr::NonNull;
use std::rc::Rc;

#[derive(Debug, Default, Clone)]
pub struct Config {
    pub user_font_paths: Option<Vec<PathBuf>>,
}

/// Initialized pdfium bindings.
#[derive(Clone)]
pub struct Library {
    inner: Rc<LibraryGuard>,
}

struct LibraryGuard {
    ftable: FnTable,
}

impl Library {
    pub fn init_with_bindings(bindings: Bindings, config: &Config) -> Result<Library> {
        // convert user font paths to null-terminated array of C-string pointers
        let paths = config
            .user_font_paths
            .as_ref()
            .map(|paths| -> Result<Vec<_>> { paths.iter().map(path_to_cstring).collect() })
            .transpose()?;

        let mut path_ptrs = paths.as_ref().map(|paths| -> Vec<*const std::ffi::c_char> {
            paths
                .iter()
                .map(|p| p.as_ptr())
                .chain(std::iter::once(std::ptr::null()))
                .collect()
        });

        let path_array_ptr = if let Some(ref mut path_ptrs) = path_ptrs {
            path_ptrs.as_mut_ptr()
        } else {
            std::ptr::null_mut()
        };

        // build config
        let config_sys = pdfium_sys::FPDF_LIBRARY_CONFIG {
            version: 2,
            m_pUserFontPaths: path_array_ptr,
            m_pIsolate: std::ptr::null_mut(),
            m_v8EmbedderSlot: 0,
            m_pPlatform: std::ptr::null_mut(),
            m_RendererType: 0,
        };

        // initialize library
        unsafe { bindings.ftable.FPDF_InitLibraryWithConfig(&config_sys) };

        // build library struct
        let inner = LibraryGuard {
            ftable: bindings.ftable,
        };

        let lib = Library {
            inner: Rc::new(inner),
        };

        // make sure everything is okay
        lib.assert_status()?;
        Ok(lib)
    }

    pub fn init_with_config(config: &Config) -> Result<Library> {
        Self::init_with_bindings(Bindings::load()?, config)
    }

    pub fn init() -> Result<Library> {
        Self::init_with_config(&Config::default())
    }

    pub fn ftable(&self) -> &FnTable {
        &self.inner.ftable
    }

    pub(crate) fn assert_status(&self) -> Result<()> {
        let err = unsafe { self.ftable().FPDF_GetLastError() };
        crate::error::error_code_to_result(err)?;
        Ok(())
    }

    pub(crate) fn assert_ptr<T>(&self, ptr: *mut T) -> Result<NonNull<T>> {
        match NonNull::new(ptr) {
            Some(ptr) => Ok(ptr),
            None => {
                self.assert_status()?;
                Err(ErrorCode::Unknown.into())
            }
        }
    }

    pub(crate) fn assert(&self, condition: bool) -> Result<()> {
        if condition {
            Ok(())
        } else {
            self.assert_status()?;
            Err(ErrorCode::Unknown.into())
        }
    }

    pub fn load_file<P>(&self, path: P, password: Option<&str>) -> Result<Document>
    where
        P: AsRef<Path>,
    {
        // Note: we go via a reader here because otherwise we'd have to convert
        // paths to C-strings... which works fine on UNIX type systems but not
        // so much on Windows.
        let file = File::open(path)?;
        self.load_reader(file, password)
    }

    pub fn load_reader<R>(&self, reader: R, password: Option<&str>) -> Result<Document>
    where
        R: Read + Seek + 'static,
    {
        // convert password to null-terminated C-string
        let password = password
            .map(CString::new)
            .transpose()
            .map_err(|_| Error::InvalidEncoding)?;

        let password = password
            .as_ref()
            .map(|p| p.as_ptr() as *const i8)
            .unwrap_or(std::ptr::null());

        // build custom file access
        let mut access = crate::fileaccess::ReaderAccess::from_reader(reader)?;

        // load document
        let handle = unsafe {
            self.ftable()
                .FPDF_LoadCustomDocument(access.sys_ptr(), password)
        };
        let handle = self.assert_ptr(handle)?;

        // FIXME: From pdfium docs:
        //   If PDFium is built with the XFA module, the application should
        //   call FPDF_LoadXFA() function after the PDF document loaded to
        //   support XFA fields defined in the fpdfformfill.h file.

        // set up our structs
        let backing = DocumentBacking::Reader { access };
        let document = Document::new(self.clone(), handle, backing);
        Ok(document)
    }

    pub fn load_buffer(&self, buffer: Vec<u8>, password: Option<&str>) -> Result<Document> {
        // convert password to null-terminated C-string
        let password = password
            .map(CString::new)
            .transpose()
            .map_err(|_| Error::InvalidEncoding)?;

        let password = password
            .as_ref()
            .map(|p| p.as_ptr() as *const i8)
            .unwrap_or(std::ptr::null());

        // load document
        let handle = unsafe {
            self.ftable().FPDF_LoadMemDocument64(
                buffer.as_ptr() as *const c_void,
                buffer.len(),
                password,
            )
        };
        let handle = self.assert_ptr(handle)?;

        // FIXME: From pdfium docs:
        //   If PDFium is built with the XFA module, the application should
        //   call FPDF_LoadXFA() function after the PDF document loaded to
        //   support XFA fields defined in the fpdfformfill.h file.

        // set up our structs
        let backing = DocumentBacking::Buffer { buffer };
        let document = Document::new(self.clone(), handle, backing);
        Ok(document)
    }
}

impl Drop for LibraryGuard {
    fn drop(&mut self) {
        unsafe { self.ftable.FPDF_DestroyLibrary() };
    }
}

#[cfg(target_family = "unix")]
fn path_to_cstring(path: impl AsRef<Path>) -> Result<CString> {
    use std::os::unix::ffi::OsStrExt;

    CString::new(path.as_ref().as_os_str().as_bytes()).map_err(|_| Error::InvalidEncoding)
}

#[cfg(not(target_family = "unix"))]
fn path_to_cstring(path: impl AsRef<Path>) -> Result<CString> {
    // FIXME: This assumes paths are always valid unicode, which might not be true

    let unicode = path.as_ref().to_str().ok_or(Error::InvalidEncoding)?;
    CString::new(unicode).map_err(|_| Error::InvalidEncoding)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_init() {
        let _lib = Library::init().unwrap();
    }
}
