use std::panic::AssertUnwindSafe;
use std::ptr::NonNull;

use crate::utils::ptr::container_of;

use super::api::Adapter;
use super::core::{Cell, Core, Data, Header};

pub struct Harness<T, F, R> {
    ptr: NonNull<Cell<T, F, R>>,
}

impl<A, F, R> Harness<A, F, R>
where
    F: FnOnce() -> R + Send + 'static,
    A: Adapter + Send + 'static,
    A::Data: Send + Sync + 'static,
{
    pub fn from_raw(ptr: NonNull<Header>) -> Self {
        let ptr = container_of!(ptr.as_ptr(), Cell<A, F, R>, header);
        let ptr = unsafe { NonNull::new_unchecked(ptr as *mut _) };

        Self { ptr }
    }

    fn header_ptr(&self) -> NonNull<Header> {
        unsafe { NonNull::new_unchecked(std::ptr::addr_of_mut!((*self.ptr.as_ptr()).header)) }
    }

    fn header(&self) -> &Header {
        unsafe { &self.ptr.as_ref().header }
    }

    fn core(&self) -> &Core<A, F, R> {
        unsafe { &self.ptr.as_ref().core }
    }

    pub fn get_adapter(ptr: NonNull<Header>) -> NonNull<A> {
        let ptr = container_of!(ptr.as_ptr(), Cell<A, F, R>, header);
        let ptr = unsafe { std::ptr::addr_of!((*ptr).core.adapter) };

        unsafe { NonNull::new_unchecked(ptr as *mut A) }
    }

    pub fn get_adapter_data(ptr: NonNull<Header>) -> NonNull<A::Data> {
        A::get_data_ptr(Self::get_adapter(ptr))
    }

    pub fn execute(&self) {
        let header = self.header();
        let core = self.core();

        // Check if the closure is present and whether we're allowed to take
        // it. If we are, update the state to "executing" to give us exclusive
        // access to task data.
        if header.state.transition_init_to_exec().is_err() {
            return;
        }

        // Run the adapter callback for execution.
        core.adapter.on_execute(self.header_ptr());

        // Take the closure from the task data.
        //
        // Safety: By checking the state and successfully marking the task as
        // "executing", we have gained exclusive access to the task data. We
        // are therefore free to take and consume the closure.
        let closure = match unsafe { core.take_data() } {
            Data::Closure(closure) => closure,
            _ => unreachable!("invalid state"),
        };

        // Run the closure and catch any panic.
        let result = std::panic::catch_unwind(AssertUnwindSafe(closure));

        // Store the result.
        //
        // Safety: The exclusive access guarantees from the previous unsafe
        // block still hold as the task is still marked as "executing". We can
        // therefor safely store the result.
        match result {
            Ok(result) => unsafe { core.set_result(result) },
            Err(panic) => unsafe { core.set_panic(panic) },
        }

        // Mark task as complete.
        let _ = header.state.transition_exec_to_complete();

        // Signal completion to wake up all waiting threads.
        header.complete.set_completed();

        // Run the adapter callback for completion.
        core.adapter.on_complete(self.header_ptr());
    }

    pub fn result(&self) -> Option<R> {
        let header = self.header();
        let core = self.core();

        // Check if a result (or panic) is present and whether we're allowed to
        // take it. If we are, update the state to let everyone know that we
        // have claimed the result.
        if header.state.transition_complete_to_consumed().is_err() {
            return None;
        }

        // Run the adapter callback.
        core.adapter.on_consume(self.header_ptr());

        // Take the result from the task data.
        //
        // Safety: By checking the state and successfully marking the task as
        // "consumed", we have gained exclusive access to its result. We are
        // therefore free to take and consume it.
        let res = match unsafe { core.take_data() } {
            Data::Result(res) => res,
            Data::Panic(panic) => std::panic::resume_unwind(panic),
            _ => unreachable!("invalid state"),
        };

        Some(res)
    }

    pub fn cancel(&self) -> bool {
        let header = self.header();
        let core = self.core();

        // Try to mark us as canceled. If task is already running or has been
        // completed successfully (or with panic), return false. I.e., only
        // return true if the task has truly been canceled.
        if let Err(state) = header.state.transition_to_canceled() {
            return state.is_canceled();
        }

        // Run the adapter callback.
        core.adapter.on_cancel(self.header_ptr());

        // Drop the closure, mark ourselves as completed, and return "success".
        drop(unsafe { core.take_data() });
        header.complete.set_completed();
        true
    }

    pub fn dealloc(self) {
        // Verify that we're actually the last reference.
        debug_assert_eq!(self.header().state.snapshot().refcount(), 0);

        // Run the adapter callback.
        self.core().adapter.on_dealloc(self.header_ptr());

        // Drop the stage-specific data. If we get to here and the user cares
        // about the result, it should already have been taken, so we
        // deliberately ignore any panics here and carry on as if nothing
        // happened to avoid (potentially) messing up the execution thread.
        let _ = std::panic::catch_unwind(AssertUnwindSafe(|| {
            // Safety: We have the last reference to this task. So it's safe to
            // mutate it as we please.
            drop(unsafe { self.core().take_data() });
        }));

        // Drop the entire task cell we're pointing to.
        unsafe { drop(Box::from_raw(self.ptr.as_ptr())) };
    }
}
