use crate::{Bindings, Error, ErrorCode, FnTable, Result};
use std::path::PathBuf;
use std::{ffi::CString, path::Path, rc::Rc};

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

    fn assert_status(&self) -> std::result::Result<(), ErrorCode> {
        let err = unsafe { self.ftable().FPDF_GetLastError() };
        crate::error::error_code_to_result(err)
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
