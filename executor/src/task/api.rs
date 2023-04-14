use std::marker::PhantomData;
use std::ptr::NonNull;
use std::time::Duration;

pub use super::core::Header;
use super::raw::RawTask;

/// Direct handle to an executable task.
pub struct Task<T> {
    raw: RawTask,
    _p: PhantomData<T>,
}

/// Remote handle for a task.
pub struct Handle<R> {
    raw: RawTask,
    _p: PhantomData<R>,
}

/// Remote handle for a task, canceling the task when being dropped.
pub struct DropHandle<R> {
    raw: RawTask,
    _p: PhantomData<R>,
}

/// Execution adapter.
///
/// This trait allows hooking into specific stages of the task execution.  It
/// can be used to track the lifecycle of a taks from inside executors, for
/// example to clean up after a task has been canceled.
pub trait Adapter {
    type Data;

    /// Get the pointer to the adapter data.
    ///
    /// Note: This must not create any intermediate references.
    fn get_data_ptr(ptr: NonNull<Self>) -> NonNull<Self::Data>;

    /// Executed when the task starts executing its closure.
    fn on_execute(&self, _task: NonNull<Header>) {}

    /// Executed when the task finished executing its closure, either
    /// successfully or via a panic.
    fn on_complete(&self, _task: NonNull<Header>) {}

    /// Executed when the result of the task is being consumed.
    fn on_consume(&self, _task: NonNull<Header>) {}

    /// Executed when the task has been canceled successfully.
    fn on_cancel(&self, _task: NonNull<Header>) {}

    /// Executed right before the task is being deallocated.
    fn on_dealloc(&self, _task: NonNull<Header>) {}
}

impl<T> Task<T> {
    /// Create a new task.
    ///
    /// Create a new task for the given closure, returning its task- and
    /// join-handle.
    pub fn new<A, F, R>(adapter: A, closure: F) -> (Task<A::Data>, Handle<R>)
    where
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static,
        A: Adapter<Data = T> + Send + 'static,
        A::Data: Send + Sync + 'static,
    {
        let raw = RawTask::new(adapter, closure);

        let task = Task {
            raw: raw.clone(),
            _p: PhantomData,
        };

        let handle = Handle::new(raw);

        (task, handle)
    }

    /// Constructs a task from a raw pointer to its task header.
    ///
    /// After calling this function, the task reference from the provided
    /// pointer is owned by the resulting `Task` struct. Further access via the
    /// pointer should therefore be avoided.
    ///
    /// This function is akin to [`Box::from_raw`].
    ///
    /// # Safety
    ///
    /// The caller must ensure that the provided pointer points to a valid
    /// task, i.e., a task that has previously been turned into a pointer via
    /// [`Task::into_raw`].
    pub unsafe fn from_raw(ptr: NonNull<Header>) -> Self {
        Task {
            raw: RawTask::from_raw(ptr),
            _p: PhantomData,
        }
    }

    /// Consume this task handle, returning a raw pointer to its task header.
    ///
    /// This function is akin to [`Box::into_raw`].
    ///
    /// The reference held by this task is transferred over to the pointer.
    /// The caller is responsible for managing the memory of this task handle.
    /// The easiest way to do this is by using [`Task::from_raw`] to convert
    /// the pointer back to its original task representation.
    pub fn into_raw(self) -> NonNull<Header> {
        self.raw.into_raw()
    }

    /// Get the raw type-erased task pointer.
    pub fn as_raw(&self) -> NonNull<Header> {
        self.raw.as_raw()
    }

    /// Get the adapter data associated with the provided raw task.
    pub fn get_adapter_data(raw: NonNull<Header>) -> NonNull<T> {
        unsafe { RawTask::get_adapter_data(raw) }
    }

    /// Execute the task on the current thread, consuming this handle.
    pub fn execute(self) {
        self.raw.execute();
    }
}

impl<R> Handle<R> {
    fn new(raw: RawTask) -> Self {
        Handle {
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

    pub fn cancel_on_drop(self) -> DropHandle<R> {
        DropHandle::new(self.raw)
    }
}

impl<R: Send> Handle<R> {
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

impl<R> DropHandle<R> {
    fn new(raw: RawTask) -> Self {
        Self {
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
}

impl<R: Send> DropHandle<R> {
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

impl<R> Drop for DropHandle<R> {
    fn drop(&mut self) {
        self.raw.cancel();
    }
}

impl Adapter for () {
    type Data = ();

    fn get_data_ptr(ptr: NonNull<Self>) -> NonNull<Self::Data> {
        ptr.cast()
    }
}

#[cfg(test)]
mod test {
    use std::sync::{Arc, Mutex, Weak};

    use super::*;

    use crate::utils::linked_list::{Link, List, Pointers};

    #[derive(Clone)]
    struct Queue {
        list: Arc<Mutex<List<Task<Data>>>>,
    }

    #[derive(Clone)]
    struct QueueRef {
        list: Weak<Mutex<List<Task<Data>>>>,
    }

    impl Queue {
        fn new() -> Self {
            Queue {
                list: Arc::new(Mutex::new(List::new())),
            }
        }

        fn weak(&self) -> QueueRef {
            QueueRef {
                list: Arc::downgrade(&self.list),
            }
        }

        fn push(&self, task: Task<Data>) {
            self.list.lock().unwrap().push_front(task)
        }

        fn pop(&self) -> Option<Task<Data>> {
            self.list.lock().unwrap().pop_back()
        }
    }

    impl QueueRef {
        unsafe fn remove(&self, task: NonNull<Header>) {
            if let Some(list) = self.list.upgrade() {
                list.lock().unwrap().remove(task);
            }
        }
    }

    struct Data {
        node: Pointers<Header>,
        test: u32,
    }

    struct Adapter {
        data: Data,
        queue: QueueRef,
    }

    impl Adapter {
        fn new(queue: QueueRef, test: u32) -> Self {
            Adapter {
                data: Data {
                    node: Pointers::new(),
                    test,
                },
                queue,
            }
        }
    }

    impl super::Adapter for Adapter {
        type Data = Data;

        fn get_data_ptr(ptr: NonNull<Self>) -> NonNull<Self::Data> {
            unsafe { NonNull::new_unchecked(std::ptr::addr_of_mut!((*ptr.as_ptr()).data)) }
        }

        fn on_cancel(&self, task: NonNull<Header>) {
            // remove ourselves from the task queue
            unsafe { self.queue.remove(task) }
        }
    }

    // Safety: Tasks are always pinned.
    unsafe impl Link for Task<Data> {
        type Node = Header;
        type Pointer = Task<Data>;

        fn into_raw(task: Self::Pointer) -> NonNull<Self::Node> {
            task.into_raw()
        }

        unsafe fn from_raw(ptr: NonNull<Self::Node>) -> Self::Pointer {
            Task::from_raw(ptr)
        }

        unsafe fn pointers(target: NonNull<Self::Node>) -> NonNull<Pointers<Self::Node>> {
            let ptr = Self::Pointer::get_adapter_data(target);
            let ptr = std::ptr::addr_of_mut!((*ptr.as_ptr()).node);

            NonNull::new_unchecked(ptr)
        }
    }

    #[test]
    fn adapter_access() {
        let queue = Queue::new();

        // create a new task with the specified adapter, storing a value inside it
        let value = 42;
        let adapter = Adapter::new(queue.weak(), value);
        let (task, _handle) = Task::new(adapter, || 123);

        // get a pointer to the adapter
        let adapter = Task::<Data>::get_adapter_data(task.as_raw());

        // read back the value we stored in the adapter and make sure it matches
        assert_eq!(unsafe { adapter.as_ref().test }, value);
    }

    #[test]
    fn adapter_queue() {
        let queue = Queue::new();

        // create a task
        let value_a = 123;
        let adapter = Adapter::new(queue.weak(), 0);
        let (task_a, handle_a) = Task::new(adapter, move || value_a);

        // create another task
        let value_b = 456;
        let adapter = Adapter::new(queue.weak(), 0);
        let (task_b, handle_b) = Task::new(adapter, move || value_b);

        // push both tasks to the queue
        queue.push(task_a);
        queue.push(task_b);

        // pop the first task from the queue and execute it
        let task_a = queue.pop().unwrap();
        task_a.execute();

        // pop the second task from the queue and execute it
        let task_b = queue.pop().unwrap();
        task_b.execute();

        // make sure the results are as we expect
        assert_eq!(handle_a.join(), value_a);
        assert_eq!(handle_b.join(), value_b);

        // the queue should be empty now
        assert!(queue.pop().is_none());
    }

    #[test]
    fn adapter_cancel_queue() {
        let queue = Queue::new();

        // create a task
        let value_a = 123;
        let adapter = Adapter::new(queue.weak(), 0);
        let (task_a, handle_a) = Task::new(adapter, move || value_a);

        // create another task
        let value_b = 456;
        let adapter = Adapter::new(queue.weak(), 1);
        let (task_b, handle_b) = Task::new(adapter, move || value_b);

        // push both tasks to the queue
        queue.push(task_a);
        queue.push(task_b);

        // cancel the first task: this should remove it from the queue
        let res = handle_a.cancel();
        assert!(res.is_ok());

        // pop the remaining task from the queue and execute it
        let task_b = queue.pop().unwrap();

        // the queue should be empty now
        assert!(queue.pop().is_none());

        // get a pointer to the adapter of this task
        let adapter = Task::<Data>::get_adapter_data(task_b.as_raw());

        // read back the value we stored in the adapter and make sure it
        // matches the second task
        assert_eq!(unsafe { adapter.as_ref().test }, 1);

        // execute the task
        task_b.execute();

        // make sure the results are as we expect
        assert_eq!(handle_b.join(), value_b);
    }

    /// This test is intended to be run via `cargo miri test` for leak testing.
    #[test]
    fn drop_queue() {
        let queue = Queue::new();

        // create a task
        let value_a = 123;
        let adapter = Adapter::new(queue.weak(), 0);
        let (task_a, handle_a) = Task::new(adapter, move || value_a);

        // create another task
        let value_b = 456;
        let adapter = Adapter::new(queue.weak(), 1);
        let (task_b, handle_b) = Task::new(adapter, move || value_b);

        // drop handles
        drop(handle_a);
        drop(handle_b);

        // push both tasks to the queue
        queue.push(task_a);
        queue.push(task_b);

        // drop queue with tasks
        drop(queue);
    }
}
