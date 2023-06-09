//! A generic executable task implementation.

mod api;
mod core;
mod harness;
mod raw;
mod state;
mod vtable;

pub use self::api::{Adapter, DropHandle, Handle, Header, Task};
