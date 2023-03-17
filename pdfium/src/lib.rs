mod bindings;
mod document;
mod error;
mod fileaccess;
mod library;
mod metadata;
mod utils;
mod version;

pub use document::Document;
pub use error::{Error, ErrorCode, Result};
pub use library::{Config, Library};
pub use metadata::{Metadata, MetadataTag};
pub use version::Version;

pub mod lowlevel {
    pub use crate::bindings::{Bindings, FnTable};
    pub use crate::document::DocumentHandle;
}

#[cfg(test)]
mod test {
    use super::lowlevel::Bindings;

    #[test]
    fn test_libpdfium_available() {
        let _lib = Bindings::load().unwrap();
    }
}
