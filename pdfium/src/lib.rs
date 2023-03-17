mod bindings;
mod error;
mod fileaccess;
mod library;
mod metadata;
mod page;
mod utils;
mod version;

pub mod document;

pub use document::Document;
pub use error::{Error, ErrorCode, Result};
pub use library::{Config, Library};
pub use metadata::MetadataTag;
pub use version::Version;

pub mod lowlevel {
    pub use crate::bindings::{Bindings, FnTable};
    pub use crate::document::DocumentHandle;
}

pub mod accessor {
    pub use crate::metadata::Metadata;
    pub use crate::page::Pages;
}

#[cfg(test)]
mod test {
    use super::lowlevel::Bindings;

    #[test]
    fn test_libpdfium_available() {
        let _lib = Bindings::load().unwrap();
    }
}
