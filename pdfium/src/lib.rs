#![allow(clippy::module_inception)]

mod error;
mod library;

pub mod bindings;
pub mod doc;
pub mod bitmap;
pub mod types;

pub(crate) mod io;
pub(crate) mod utils;

pub use error::{Error, ErrorCode, Result};
pub use library::{Config, Library};

#[cfg(test)]
mod test {
    use super::bindings::Bindings;

    #[test]
    fn test_libpdfium_available() {
        let _lib = Bindings::load().unwrap();
    }
}
