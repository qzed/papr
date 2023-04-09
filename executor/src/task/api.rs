use std::marker::PhantomData;
use std::ptr::NonNull;
use std::time::Duration;

use crate::utils::linked_list;

pub use super::core::Header;
use super::raw::RawTask;

/// Handle to an executable task.
pub struct Task {
    raw: RawTask,
}

/// Join handle for a task.
pub struct JoinHandle<R> {
    raw: RawTask,
    _p: PhantomData<R>,
}

impl Task {
    /// Create a new task.
    ///
    /// Create a new task for the given closure, returning its task- and
    /// join-handle.
    pub fn new<F, R>(closure: F) -> (Task, JoinHandle<R>)
    where
        F: FnOnce() -> R,
        F: Send,
        R: Send,
    {
        let raw = RawTask::new(closure);

        let task = Task { raw: raw.clone() };
        let join = JoinHandle::new(raw);

        (task, join)
    }

    fn from_raw(ptr: NonNull<Header>) -> Self {
        Task {
            raw: RawTask::from_raw(ptr),
        }
    }

    fn into_raw(self) -> NonNull<Header> {
        self.raw.into_raw()
    }

    /// Execute the task on the current thread, consuming this handle.
    pub fn execute(self) {
        self.raw.execute();
    }
}

impl<R: Send> JoinHandle<R> {
    fn new(raw: RawTask) -> Self {
        JoinHandle {
            raw,
            _p: PhantomData,
        }
    }

    /// Check if the ssociated task has been completed.
    pub fn is_finished(&self) -> bool {
        self.raw.is_complete()
    }

    /// Cancel the associated task.
    ///
    /// Cancels the associated task. Returns `Ok(())` if the task has been
    /// canceled successfully, `Err(self)` if the task could not be canceled or
    /// has already been completed successfully.
    pub fn cancel(self) -> Result<(), Self> {
        if self.raw.cancel() {
            Ok(())
        } else {
            Err(self)
        }
    }

    /// Wait for the task to complete and return its result.
    ///
    /// This function will return immediately if the associated task has
    /// already been completed. Non-blocking operations are supported by
    /// checking [`is_finished()`][Self::is_finished()] and calling
    /// [`join()`][Self::join()] only if that returns `true`.
    ///
    /// # Panics
    ///
    /// This function will panic if the associated task function panicked.
    pub fn join(self) -> R {
        // Wait for completion. This will return immediately if the task has
        // already been completed.
        self.raw.wait();

        // Take the result. We should be the only one to access this.
        self.raw.result().expect("result already taken")
    }

    /// Wait for the task to complete with a timeout and return its result if
    /// successful.
    ///
    /// Returns `Ok(result)` if the task completed within the timeout,
    /// `Err(self)` if this operation timed out.
    ///
    /// If the associated task has already been completed, this function will
    /// return its result with `Ok` immediately.
    ///
    /// # Panics
    ///
    /// This function will panic if the associated task function panicked.
    pub fn join_timeout(self, duration: Duration) -> Result<R, Self> {
        // Wait for completion. This will return immediately if the task has
        // already been completed.
        if self.raw.wait_timeout(duration) {
            // Take the result. We should be the only one to access this.
            Ok(self.raw.result().expect("result already taken"))
        } else {
            Err(self)
        }
    }
}

// Safety: Tasks are always pinned.
unsafe impl linked_list::Link for Task {
    type Node = Header;
    type Pointer = Task;

    fn into_raw(task: Self::Pointer) -> NonNull<Self::Node> {
        task.into_raw()
    }

    unsafe fn from_raw(ptr: NonNull<Self::Node>) -> Self::Pointer {
        Task::from_raw(ptr)
    }

    unsafe fn pointers(target: NonNull<Self::Node>) -> NonNull<linked_list::Pointers<Self::Node>> {
        NonNull::new_unchecked(std::ptr::addr_of_mut!((*target.as_ptr()).node))
    }
}
