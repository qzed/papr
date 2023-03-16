use std::path::Path;

/// Raw pdfium function pointer table.
pub type FnTable = pdfium_sys::libpdfium;

/// Pdfium library function bindings.
pub struct Bindings {
    pub(crate) ftable: FnTable,
}

impl Bindings {
    const LIBRARY_NAME: &'static str = pdfium_sys::LIBRARY_NAME;

    pub fn load() -> Result<Bindings, libloading::Error> {
        Self::load_from_path(Self::LIBRARY_NAME)
    }

    pub fn load_from_path(path: impl AsRef<Path>) -> Result<Bindings, libloading::Error> {
        let ftable = unsafe { pdfium_sys::libpdfium::new(path.as_ref()) }?;

        let library = Bindings { ftable };
        Ok(library)
    }

    pub fn load_from_library(lib: libloading::Library) -> Result<Bindings, libloading::Error> {
        let ftable = unsafe { pdfium_sys::libpdfium::from_library(lib) }?;

        let library = Bindings { ftable };
        Ok(library)
    }

    pub fn ftable(&self) -> &FnTable {
        &self.ftable
    }
}
