#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(rustdoc::bare_urls)]
#![allow(rustdoc::broken_intra_doc_links)]
#![allow(clippy::all)]

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

#[cfg(all(target_family = "unix", not(target_os = "macos")))]
pub const LIBRARY_NAME: &'static str = "libpdfium.so";

#[cfg(target_os = "macos")]
pub const LIBRARY_NAME: &'static str = "libpdfium.dylib";

#[cfg(target_os = "windows")]
pub const LIBRARY_NAME: &'static str = "pdfium.dll";

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_libpdfium_available() {
        let _lib = unsafe { libpdfium::new(LIBRARY_NAME) }.unwrap();
    }
}
