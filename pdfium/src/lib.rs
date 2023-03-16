mod bindings;
pub use bindings::{Bindings, FnTable};

mod error;
pub use error::{Error, ErrorCode, Result};

mod library;
pub use library::{Library, Config};

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_libpdfium_available() {
        let _lib = Bindings::load().unwrap();
    }
}
