//! A generic executable task implementation.

mod api;
mod completion;
mod core;
mod harness;
mod raw;
mod state;
mod vtable;

pub use self::api::{Adapter, Handle, Header, Task};
