use std::ptr::NonNull;

use super::api::Adapter;
use super::core::Header;
use super::harness::Harness;

pub struct Vtable {
    pub execute: unsafe fn(NonNull<Header>),
    pub cancel: unsafe fn(NonNull<Header>) -> bool,
    pub read_result: unsafe fn(NonNull<Header>, *mut ()),
    pub dealloc: unsafe fn(NonNull<Header>),
    pub get_adapter_data: unsafe fn(NonNull<Header>) -> NonNull<()>,
}

pub fn vtable<A, F, R>() -> &'static Vtable
where
    F: FnOnce() -> R + Send + 'static,
    A: Adapter + Send + 'static,
    A::Data: Send + Sync + 'static,
{
    &Vtable {
        execute: execute::<A, F, R>,
        cancel: cancel::<A, F, R>,
        read_result: read_result::<A, F, R>,
        dealloc: dealloc::<A, F, R>,
        get_adapter_data: get_adapter_data::<A, F, R>,
    }
}

unsafe fn execute<A, F, R>(ptr: NonNull<Header>)
where
    F: FnOnce() -> R + Send + 'static,
    A: Adapter + Send + 'static,
    A::Data: Send + Sync + 'static,
{
    Harness::<A, F, R>::from_raw(ptr).execute();
}

unsafe fn read_result<A, F, R>(ptr: NonNull<Header>, out: *mut ())
where
    F: FnOnce() -> R + Send + 'static,
    A: Adapter + Send + 'static,
    A::Data: Send + Sync + 'static,
{
    let out = &mut *(out as *mut Option<R>);
    *out = Harness::<A, F, R>::from_raw(ptr).result();
}

unsafe fn cancel<A, F, R>(ptr: NonNull<Header>) -> bool
where
    F: FnOnce() -> R + Send + 'static,
    A: Adapter + Send + 'static,
    A::Data: Send + Sync + 'static,
{
    Harness::<A, F, R>::from_raw(ptr).cancel()
}

unsafe fn dealloc<A, F, R>(ptr: NonNull<Header>)
where
    F: FnOnce() -> R + Send + 'static,
    A: Adapter + Send + 'static,
    A::Data: Send + Sync + 'static,
{
    Harness::<A, F, R>::from_raw(ptr).dealloc();
}

unsafe fn get_adapter_data<A, F, R>(ptr: NonNull<Header>) -> NonNull<()>
where
    F: FnOnce() -> R + Send + 'static,
    A: Adapter + Send + 'static,
    A::Data: Send + Sync + 'static,
{
    Harness::<A, F, R>::get_adapter_data(ptr).cast::<()>()
}
