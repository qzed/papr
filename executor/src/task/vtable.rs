use std::ptr::NonNull;

use super::api::Adapter;
use super::core::Header;
use super::harness::Harness;

pub struct Vtable {
    pub execute: unsafe fn(NonNull<Header>),
    pub cancel: unsafe fn(NonNull<Header>) -> bool,
    pub read_result: unsafe fn(NonNull<Header>, *mut ()),
    pub dealloc: unsafe fn(NonNull<Header>),
    pub get_adapter: unsafe fn(NonNull<Header>) -> NonNull<()>,
}

pub fn vtable<T, F, R>() -> &'static Vtable
where
    F: FnOnce() -> R + Send + 'static,
    T: Adapter + Send + Sync + 'static,
{
    &Vtable {
        execute: execute::<T, F, R>,
        cancel: cancel::<T, F, R>,
        read_result: read_result::<T, F, R>,
        dealloc: dealloc::<T, F, R>,
        get_adapter: get_adapter::<T, F, R>,
    }
}

unsafe fn execute<T, F, R>(ptr: NonNull<Header>)
where
    F: FnOnce() -> R + Send + 'static,
    T: Adapter + Send + Sync + 'static,
{
    Harness::<T, F, R>::from_raw(ptr).execute();
}

unsafe fn read_result<T, F, R>(ptr: NonNull<Header>, out: *mut ())
where
    F: FnOnce() -> R + Send + 'static,
    T: Adapter + Send + Sync + 'static,
{
    let out = &mut *(out as *mut Option<R>);
    *out = Harness::<T, F, R>::from_raw(ptr).result();
}

unsafe fn cancel<T, F, R>(ptr: NonNull<Header>) -> bool
where
    F: FnOnce() -> R + Send + 'static,
    T: Adapter + Send + Sync + 'static,
{
    Harness::<T, F, R>::from_raw(ptr).cancel()
}

unsafe fn dealloc<T, F, R>(ptr: NonNull<Header>)
where
    F: FnOnce() -> R + Send + 'static,
    T: Adapter + Send + Sync + 'static,
{
    Harness::<T, F, R>::from_raw(ptr).dealloc();
}

unsafe fn get_adapter<T, F, R>(ptr: NonNull<Header>) -> NonNull<()>
where
    F: FnOnce() -> R + Send + 'static,
    T: Adapter + Send + Sync + 'static,
{
    Harness::<T, F, R>::get_adapter(ptr).cast::<()>()
}
