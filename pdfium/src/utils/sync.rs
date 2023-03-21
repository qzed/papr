use std::{cell::UnsafeCell, ptr::NonNull};

#[cfg(not(feature = "sync"))]
pub type Rc<T> = std::rc::Rc<T>;

#[cfg(feature = "sync")]
pub type Rc<T> = std::sync::Arc<T>;

/// A wrapper type to store an unused value.
///
/// This is mainly to derive Send and Sync. The internal value cannot be
/// accessed, and is therefore safe to share and move across threads.
pub struct Unused<T: Sized> {
    #[allow(unused)]
    value: T,
}

impl<T> Unused<T> {
    pub fn new(value: T) -> Self {
        Self { value }
    }
}

unsafe impl<T> Send for Unused<T> {}
unsafe impl<T> Sync for Unused<T> {}

/// A handle for a pdfium object.
///
/// _Implementation notes:_ This type does a couple of things:
/// - It wraps the underlying pointer in `NonNull` as we make explicitly sure
///   that handles are always valid.
/// - It marks the handle as `Send` and `Sync` if compiled with the `sync`
///   feature. Note that handles can only be used with the respective pdfium
///   library functions, which are guarded by a mutex if `sync` is enabled.
///   Therefore, any state being modified is guarded by that mutex as well.
/// - Lastly, it wraps the underlying pointer in `UnsafeCell`. This is because
///   the handle appears to rust as an immutable and clonable object, whereas
///   in reality calling library functions can modify the state. Note that
///   because of the locking guarantees, reads and writes from the underlying
///   library-managed state can never happen concurrently.
#[derive(Debug)]
pub struct Handle<T> {
    ptr: UnsafeCell<NonNull<T>>,
}

impl<T> Handle<T> {
    pub(crate) fn new(ptr: NonNull<T>) -> Self {
        Self {
            ptr: UnsafeCell::new(ptr),
        }
    }

    pub fn get(&self) -> *mut T {
        unsafe { (*self.ptr.get()).as_ptr() }
    }
}

impl<T> Clone for Handle<T> {
    fn clone(&self) -> Self {
        let ptr = unsafe { NonNull::new_unchecked(self.get()) };

        Self {
            ptr: UnsafeCell::new(ptr),
        }
    }
}

#[cfg(feature = "sync")]
unsafe impl<T> Send for Handle<T> {}

#[cfg(feature = "sync")]
unsafe impl<T> Sync for Handle<T> {}
