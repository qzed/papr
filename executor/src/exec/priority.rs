//! A thread-pool based executor with support for task priorities.

use std::ptr::NonNull;
use std::sync::atomic::{AtomicBool, AtomicU8};
use std::sync::{Arc, Condvar, Mutex, Weak};
use std::thread::JoinHandle;
use std::time::Duration;

use crate::task::{self, Header};
use crate::utils::linked_list;

use super::Monitor;

use task::{DropHandle as BaseDropHandle, Handle as BaseHandle};

type Task = task::Task<Data>;
type TaskList = linked_list::List<Task>;

/// A priority enum.
///
/// Priority values are arranged from `0` (lowest, inclusively) to
/// `Self::count() - 1` (highest).
pub trait Priority: Sized + Copy {
    /// The maximum number of supported priorities.
    fn count() -> u8;

    /// Returns the priority instance for the given value.
    fn from_value(value: u8) -> Option<Self>;

    /// The priority value of this instance.
    fn as_value(&self) -> u8;
}

/// A basic thread-pool executor with a fixed number of threads and cancellable
/// tasks.
pub struct Executor<P> {
    inner: Arc<ExecutorStruct>,

    /// Handles to the execution threads
    threads: Vec<JoinHandle<()>>,

    /// Marker for priority.
    _marker: std::marker::PhantomData<P>,
}

/// Remote handle for a task.
pub struct Handle<P, R> {
    base: BaseHandle<R>,
    _marker: std::marker::PhantomData<P>,
}

/// Remote handle for a task, canceling the task when being dropped.
pub struct DropHandle<P, R> {
    base: BaseDropHandle<R>,
    _marker: std::marker::PhantomData<P>,
}

struct ExecutorStruct {
    /// Linked list heads for the task queue, one per priority
    queues: Mutex<Vec<TaskList>>,

    /// Condition variable for signaling arrival of new work items
    signal: Condvar,

    /// Whether to keep the queue running
    running: AtomicBool,
}

struct Data {
    node: linked_list::Pointers<task::Header>,
    exec: Weak<ExecutorStruct>,
    priority: AtomicU8,
}

struct Adapter<M> {
    data: Data,
    monitor: M,
}

impl<P: Priority> Executor<P> {
    pub fn new(num_threads: u32) -> Self {
        let queues = (0..P::count()).map(|_| TaskList::new()).collect();

        let inner = ExecutorStruct {
            queues: Mutex::new(queues),
            signal: Condvar::new(),
            running: AtomicBool::new(true),
        };
        let inner = Arc::new(inner);

        let threads = (0..num_threads)
            .map(|_| {
                let exec = inner.clone();
                std::thread::spawn(move || exec.process())
            })
            .collect();

        Executor {
            inner,
            threads,
            _marker: std::marker::PhantomData,
        }
    }

    pub fn submit<F, R>(&self, priority: P, closure: F) -> Handle<P, R>
    where
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static,
    {
        self.submit_with((), priority, closure)
    }

    pub fn submit_with<F, R, M>(&self, monitor: M, priority: P, closure: F) -> Handle<P, R>
    where
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static,
        M: Monitor + Send + 'static,
    {
        let priority = priority.as_value();

        let adapter = Adapter::new(Arc::downgrade(&self.inner), monitor, priority);
        let (task, handle) = Task::new(adapter, closure);

        self.inner.push(task, priority);

        Handle::new(handle)
    }

    pub fn shutdown(&mut self) {
        use std::sync::atomic::Ordering;

        // tell all threads to shut down
        self.inner.running.store(false, Ordering::SeqCst);
        self.inner.signal.notify_all();

        // wait for all threads to finish, ignore any panics
        let threads = std::mem::take(&mut self.threads);
        for handle in threads {
            let _ = handle.join();
        }
    }
}

impl<P> Drop for Executor<P> {
    fn drop(&mut self) {
        use std::sync::atomic::Ordering;

        // tell all threads to shut down
        self.inner.running.store(false, Ordering::Release);
        self.inner.signal.notify_all();
    }
}

impl ExecutorStruct {
    fn push(&self, task: Task, priority: u8) {
        let mut queues = self.queues.lock().unwrap();

        queues[priority as usize].push_front(task);
        self.signal.notify_one();
    }

    fn pop(&self) -> Option<Task> {
        use std::sync::atomic::Ordering;

        let mut queues = self.queues.lock().unwrap();

        while self.running.load(Ordering::SeqCst) {
            for queue in queues.iter_mut().rev() {
                match queue.pop_back() {
                    Some(task) => return Some(task),
                    None => (),
                }
            }

            queues = self.signal.wait(queues).unwrap();
        }

        None
    }

    fn process(&self) {
        while let Some(task) = self.pop() {
            task.execute()
        }
    }
}

impl<P, R> Handle<P, R> {
    fn new(base: BaseHandle<R>) -> Self {
        Self {
            base,
            _marker: std::marker::PhantomData,
        }
    }

    /// Check if the ssociated task has been completed.
    pub fn is_finished(&self) -> bool {
        self.base.is_finished()
    }

    /// Cancel the associated task.
    ///
    /// Cancels the associated task. Returns `Ok(())` if the task has been
    /// canceled successfully, `Err(self)` if the task could not be canceled or
    /// has already been completed successfully.
    pub fn cancel(self) -> Result<(), Self> {
        self.base.cancel().map_err(Self::new)
    }

    /// Transform into a handle that cancels the task when dropped.
    pub fn cancel_on_drop(self) -> DropHandle<P, R> {
        DropHandle::new(self.base.cancel_on_drop())
    }

    /// Return a pointer to the raw underlying task header.
    ///
    /// To be used with care.
    pub fn as_raw_task(&self) -> NonNull<Header> {
        self.base.as_raw_task()
    }
}

impl<P: Priority, R> Handle<P, R> {
    /// Update the priority of this task.
    pub fn set_priority(&self, priority: P) {
        use std::sync::atomic::Ordering;

        let priority = priority.as_value();

        // Get the executor-specific task data
        let task = self.base.as_raw_task();
        let data = unsafe { Task::get_adapter_data(task).as_ref() };

        let exec = data.exec.upgrade().unwrap();
        let mut queues = exec.queues.lock().unwrap();

        // Update the stored task priority
        let old_priority = data.priority.swap(priority, Ordering::SeqCst);

        // Try to remove the task from the queue. This may return None in case
        // the task is executing or has been completed
        let task = unsafe { queues[old_priority as usize].remove(task) };

        // Add task to the new queue
        if let Some(task) = task {
            queues[priority as usize].push_front(task);
        }
    }

    /// Returns the current priority of this task.
    pub fn priority(&self) -> P {
        use std::sync::atomic::Ordering;

        // get the executor-specific task data
        let task = self.base.as_raw_task();
        let data = unsafe { Task::get_adapter_data(task).as_ref() };

        let value = data.priority.load(Ordering::SeqCst);
        P::from_value(value).unwrap()
    }
}

impl<P, R: Send> Handle<P, R> {
    /// Wait for the task to complete and return its result.
    ///
    /// This function will return immediately if the associated task has
    /// already been completed. Non-blocking operations are supported by
    /// checking [`is_finished()`][Self::is_finished()] and calling
    /// [`join()`][Self::join()] only if that returns `true`.
    ///
    /// # Panics
    ///
    /// This function will panic if the associated task function panicked
    /// during its execution.
    pub fn join(self) -> R {
        self.base.join()
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
    /// This function will panic if the associated task function panicked
    /// during its execution.
    pub fn join_timeout(self, duration: Duration) -> Result<R, Self> {
        self.base.join_timeout(duration).map_err(Self::new)
    }
}

impl<P, R> DropHandle<P, R> {
    fn new(base: BaseDropHandle<R>) -> Self {
        Self {
            base,
            _marker: std::marker::PhantomData,
        }
    }

    /// Check if the ssociated task has been completed.
    pub fn is_finished(&self) -> bool {
        self.base.is_finished()
    }

    /// Cancel the associated task.
    ///
    /// Cancels the associated task. Returns `Ok(())` if the task has been
    /// canceled successfully, `Err(self)` if the task could not be canceled or
    /// has already been completed successfully.
    pub fn cancel(self) -> Result<(), Self> {
        self.base.cancel().map_err(Self::new)
    }

    /// Return a pointer to the raw underlying task header.
    ///
    /// To be used with care.
    pub fn as_raw_task(&self) -> NonNull<Header> {
        self.base.as_raw_task()
    }
}

impl<P: Priority, R> DropHandle<P, R> {
    /// Update the priority of this task.
    pub fn set_priority(&self, priority: P) {
        use std::sync::atomic::Ordering;

        let priority = priority.as_value();

        // Get the executor-specific task data
        let task = self.base.as_raw_task();
        let data = unsafe { Task::get_adapter_data(task).as_ref() };

        let exec = data.exec.upgrade().unwrap();
        let mut queues = exec.queues.lock().unwrap();

        // Update the stored task priority
        let old_priority = data.priority.swap(priority, Ordering::SeqCst);

        // Try to remove the task from the queue. This may return None in case
        // the task is executing or has been completed
        let task = unsafe { queues[old_priority as usize].remove(task) };

        // Add task to the new queue
        if let Some(task) = task {
            queues[priority as usize].push_front(task);
        }
    }

    /// Returns the current priority of this task.
    pub fn priority(&self) -> u8 {
        use std::sync::atomic::Ordering;

        // get the executor-specific task data
        let task = self.base.as_raw_task();
        let data = unsafe { Task::get_adapter_data(task).as_ref() };

        data.priority.load(Ordering::SeqCst)
    }
}

impl<P, R: Send> DropHandle<P, R> {
    /// Wait for the task to complete and return its result.
    ///
    /// This function will return immediately if the associated task has
    /// already been completed. Non-blocking operations are supported by
    /// checking [`is_finished()`][Self::is_finished()] and calling
    /// [`join()`][Self::join()] only if that returns `true`.
    ///
    /// # Panics
    ///
    /// This function will panic if the associated task function panicked
    /// during its execution.
    pub fn join(self) -> R {
        self.base.join()
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
    /// This function will panic if the associated task function panicked
    /// during its execution.
    pub fn join_timeout(self, duration: Duration) -> Result<R, Self> {
        self.base.join_timeout(duration).map_err(Self::new)
    }
}

impl<M> Adapter<M>
where
    M: Monitor + Send + 'static,
{
    fn new(exec: Weak<ExecutorStruct>, monitor: M, priority: u8) -> Self {
        Adapter {
            data: Data {
                node: linked_list::Pointers::new(),
                exec,
                priority: AtomicU8::new(priority),
            },
            monitor,
        }
    }
}

impl<M> task::Adapter for Adapter<M>
where
    M: Monitor + Send + 'static,
{
    type Data = Data;

    fn get_data_ptr(ptr: NonNull<Self>) -> NonNull<Self::Data> {
        unsafe { NonNull::new_unchecked(std::ptr::addr_of_mut!((*ptr.as_ptr()).data)) }
    }

    fn on_cancel(&self, task: NonNull<task::Header>) {
        // try to get a strong reference to the executor
        if let Some(exec) = self.data.exec.upgrade() {
            use std::sync::atomic::Ordering;

            let mut queues = exec.queues.lock().unwrap();

            // note: priority may only be accessed when we have the queue lock
            let priority = self.data.priority.load(Ordering::Acquire);

            // try to remove ourselves from the queue
            unsafe { queues[priority as usize].remove(task) };
        }

        self.monitor.on_canceled();
    }

    fn on_complete(&self, _task: NonNull<task::Header>) {
        self.monitor.on_complete();
    }

    fn on_execute(&self, _task: NonNull<task::Header>) {
        self.monitor.on_execute();
    }
}

// Safety: Tasks are always pinned.
unsafe impl linked_list::Link for Task {
    type Node = task::Header;
    type Pointer = Task;

    fn into_raw(task: Self::Pointer) -> NonNull<Self::Node> {
        task.into_raw()
    }

    unsafe fn from_raw(ptr: NonNull<Self::Node>) -> Self::Pointer {
        Task::from_raw(ptr)
    }

    unsafe fn pointers(target: NonNull<Self::Node>) -> NonNull<linked_list::Pointers<Self::Node>> {
        let ptr = Self::Pointer::get_adapter_data(target);
        let ptr = std::ptr::addr_of_mut!((*ptr.as_ptr()).node);

        NonNull::new_unchecked(ptr)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum TaskPriority {
        Low,
        Medium,
        High,
    }

    impl Priority for TaskPriority {
        fn count() -> u8 {
            3
        }

        fn from_value(value: u8) -> Option<Self> {
            match value {
                0 => Some(Self::Low),
                1 => Some(Self::Medium),
                2 => Some(Self::High),
                _ => None,
            }
        }

        fn as_value(&self) -> u8 {
            match self {
                Self::Low => 0,
                Self::Medium => 1,
                Self::High => 2,
            }
        }
    }

    type Executor = super::Executor<TaskPriority>;

    #[test]
    fn basic() {
        use std::thread;
        use std::time::Duration;

        let mut exec = Executor::new(2);

        let val_a = 123;
        let a = exec.submit(TaskPriority::Low, move || {
            thread::sleep(Duration::from_millis(100));
            val_a
        });

        let val_b = 456;
        let b = exec.submit(TaskPriority::Medium, move || {
            thread::sleep(Duration::from_millis(50));
            val_b
        });

        let val_c = 789;
        let c = exec.submit(TaskPriority::High, move || {
            thread::sleep(Duration::from_millis(150));
            val_c
        });

        assert_eq!(a.join(), val_a);
        assert_eq!(b.join(), val_b);
        assert_eq!(c.join(), val_c);

        exec.shutdown();
    }

    #[test]
    fn priority() {
        use crate::utils::sync::Completion;

        let mut exec = Executor::new(1);

        let completion = Arc::new(Completion::new());
        let order = Arc::new(Mutex::new(Vec::new()));

        // Create a first task to block the worker thread until we have
        // finished submitting and modifying our tasks below.
        let compl = completion.clone();
        let a = exec.submit(TaskPriority::High, move || {
            compl.wait();
        });

        // Create a second task with a medium priority.
        let ord = order.clone();
        let b = exec.submit(TaskPriority::Medium, move || {
            ord.lock().unwrap().push(2);
        });

        // Create a third task with a low initial priority.
        let ord = order.clone();
        let c = exec.submit(TaskPriority::Low, move || {
            ord.lock().unwrap().push(3);
        });

        // Update the priority of the third task to "high". Since the worker
        // thread is blocked, the second task has not been started yet.
        // Therefore, the third task should be executed before the second task.
        c.set_priority(TaskPriority::High);

        // Unblock the worker thread so that the remaining two tasks can run.
        completion.set_completed();

        // Wait for the threads to finish execution.
        a.join();
        b.join();
        c.join();

        // Verify the execution order.
        let order = order.lock().unwrap();
        assert_eq!(*order, [3, 2]);

        exec.shutdown();
    }
}
