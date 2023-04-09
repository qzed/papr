use std::ptr::NonNull;

use super::core::Header;
use super::harness::Harness;

pub struct Vtable {
    pub execute: unsafe fn(NonNull<Header>),
    pub cancel: unsafe fn(NonNull<Header>) -> bool,
    pub read_result: unsafe fn(NonNull<Header>, *mut ()),
    pub dealloc: unsafe fn(NonNull<Header>),
}

pub fn vtable<F, R>() -> &'static Vtable
where
    F: FnOnce() -> R,
{
    &Vtable {
        execute: execute::<F, R>,
        cancel: cancel::<F, R>,
        read_result: read_result::<F, R>,
        dealloc: dealloc::<F, R>,
    }
}

unsafe fn execute<F, R>(ptr: NonNull<Header>)
where
    F: FnOnce() -> R,
{
    Harness::<F, R>::from_raw(ptr).execute();
}

unsafe fn read_result<F, R>(ptr: NonNull<Header>, out: *mut ())
where
    F: FnOnce() -> R,
{
    let out = &mut *(out as *mut Option<R>);
    *out = Harness::<F, R>::from_raw(ptr).result();
}

unsafe fn cancel<F, R>(ptr: NonNull<Header>) -> bool
where
    F: FnOnce() -> R,
{
    Harness::<F, R>::from_raw(ptr).cancel()
}

unsafe fn dealloc<F, R>(ptr: NonNull<Header>)
where
    F: FnOnce() -> R,
{
    Harness::<F, R>::from_raw(ptr).dealloc();
}
