use std::marker::PhantomData;
use std::ptr::NonNull;
use std::time::Duration;

pub use super::core::Header;
use super::raw::RawTask;

/// Handle to an executable task.
pub struct Task<T> {
    raw: RawTask,
    _p: PhantomData<T>,
}

/// Join handle for a task.
pub struct JoinHandle<R> {
    raw: RawTask,
    _p: PhantomData<R>,
}

impl<T> Task<T> {
    /// Create a new task.
    ///
    /// Create a new task for the given closure, returning its task- and
    /// join-handle.
    pub fn new<F, R>(adapter: T, closure: F) -> (Task<T>, JoinHandle<R>)
    where
        F: FnOnce() -> R + 'static,
        F: Send,
        R: Send,
    {
        let raw = RawTask::new(adapter, closure);

        let task = Task {
            raw: raw.clone(),
            _p: PhantomData,
        };

        let join = JoinHandle::new(raw);

        (task, join)
    }

    pub unsafe fn from_raw(ptr: NonNull<Header>) -> Self {
        Task {
            raw: RawTask::from_raw(ptr),
            _p: PhantomData,
        }
    }

    pub fn into_raw(self) -> NonNull<Header> {
        self.raw.into_raw()
    }

    pub fn as_raw(self) -> NonNull<Header> {
        self.raw.as_raw()
    }

    pub fn get_adapter(raw: NonNull<Header>) -> NonNull<T> {
        unsafe { RawTask::get_adapter(raw) }
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

#[cfg(test)]
mod test {
    use super::*;

    use crate::utils::linked_list::{Link, List, Pointers};

    struct Adapter {
        node: Pointers<Header>,
        test: u32,
    }

    impl Adapter {
        fn new(test: u32) -> Self {
            Adapter {
                node: Pointers::new(),
                test,
            }
        }
    }

    // Safety: Tasks are always pinned.
    unsafe impl Link for Task<Adapter> {
        type Node = Header;
        type Pointer = Task<Adapter>;

        fn into_raw(task: Self::Pointer) -> NonNull<Self::Node> {
            task.into_raw()
        }

        unsafe fn from_raw(ptr: NonNull<Self::Node>) -> Self::Pointer {
            Task::from_raw(ptr)
        }

        unsafe fn pointers(target: NonNull<Self::Node>) -> NonNull<Pointers<Self::Node>> {
            let ptr = Self::Pointer::get_adapter(target);
            let ptr = std::ptr::addr_of_mut!((*ptr.as_ptr()).node);

            NonNull::new_unchecked(ptr)
        }
    }

    #[test]
    fn adapter_access() {
        let value = 42;

        let adapter = Adapter::new(value);
        let (task, _handle) = Task::new(adapter, || 123);

        let adapter = Task::<Adapter>::get_adapter(task.as_raw());

        assert_eq!(unsafe { adapter.as_ref().test }, value);
    }

    #[test]
    fn adapter_queue() {
        let mut list: List<Task<Adapter>> = List::new();

        let value_a = 123;
        let adapter = Adapter::new(0);
        let (task_a, handle_a) = Task::new(adapter, move || value_a);

        let value_b = 456;
        let adapter = Adapter::new(1);
        let (task_b, handle_b) = Task::new(adapter, move || value_b);

        list.push_front(task_a);
        list.push_front(task_b);

        let task_a = list.pop_back().unwrap();
        task_a.execute();

        let task_b = list.pop_back().unwrap();
        task_b.execute();

        assert_eq!(handle_a.join(), value_a);
        assert_eq!(handle_b.join(), value_b);
    }
}
