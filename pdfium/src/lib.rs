mod bindings;
pub use bindings::{Bindings, FnTable};

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_libpdfium_available() {
        let _lib = Bindings::load().unwrap();
    }
}
