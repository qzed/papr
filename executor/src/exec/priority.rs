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

/// A basic thread-pool executor with a fixed number of threads and cancellable
/// tasks.
pub struct Executor {
    inner: Arc<ExecutorStruct>,

    /// Handles to the execution threads
    threads: Vec<JoinHandle<()>>,
}

/// Remote handle for a task.
pub struct Handle<R> {
    base: BaseHandle<R>,
}

/// Remote handle for a task, canceling the task when being dropped.
pub struct DropHandle<R> {
    base: BaseDropHandle<R>,
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

impl Executor {
    pub fn new(num_priority: u8, num_threads: u32) -> Self {
        let queues = (0..num_priority).map(|_| TaskList::new()).collect();

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

        Executor { inner, threads }
    }

    pub fn submit<F, R>(&self, priority: u8, closure: F) -> Handle<R>
    where
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static,
    {
        self.submit_with((), priority, closure)
    }

    pub fn submit_with<F, R, M>(&self, monitor: M, priority: u8, closure: F) -> Handle<R>
    where
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static,
        M: Monitor + Send + 'static,
    {
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

impl Drop for Executor {
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

impl<R> Handle<R> {
    fn new(base: BaseHandle<R>) -> Self {
        Self { base }
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
    pub fn cancel_on_drop(self) -> DropHandle<R> {
        DropHandle::new(self.base.cancel_on_drop())
    }

    /// Update the priority of this task.
    pub fn set_priority(&self, priority: u8) {
        use std::sync::atomic::Ordering;

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

    /// Return a pointer to the raw underlying task header.
    ///
    /// To be used with care.
    pub fn as_raw_task(&self) -> NonNull<Header> {
        self.base.as_raw_task()
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

impl<R> DropHandle<R> {
    fn new(base: BaseDropHandle<R>) -> Self {
        Self { base }
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

    /// Update the priority of this task.
    pub fn set_priority(&self, priority: u8) {
        use std::sync::atomic::Ordering;

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

    /// Return a pointer to the raw underlying task header.
    ///
    /// To be used with care.
    pub fn as_raw_task(&self) -> NonNull<Header> {
        self.base.as_raw_task()
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

    #[test]
    fn basic() {
        use std::thread;
        use std::time::Duration;

        let mut exec = Executor::new(3, 2);

        let val_a = 123;
        let a = exec.submit(0, move || {
            thread::sleep(Duration::from_millis(100));
            val_a
        });

        let val_b = 456;
        let b = exec.submit(1, move || {
            thread::sleep(Duration::from_millis(50));
            val_b
        });

        let val_c = 789;
        let c = exec.submit(2, move || {
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

        let mut exec = Executor::new(3, 1);

        let completion = Arc::new(Completion::new());
        let order = Arc::new(Mutex::new(Vec::new()));

        // Create a first task to block the worker thread until we have
        // finished submitting and modifying our tasks below.
        let compl = completion.clone();
        let a = exec.submit(2, move || {
            compl.wait();
        });

        // Create a second task with a medium priority.
        let ord = order.clone();
        let b = exec.submit(1, move || {
            ord.lock().unwrap().push(2);
        });

        // Create a third task with a low initial priority.
        let ord = order.clone();
        let c = exec.submit(0, move || {
            ord.lock().unwrap().push(3);
        });

        // Update the priority of the third task to "high". Since the worker
        // thread is blocked, the second task has not been started yet.
        // Therefore, the third task should be executed before the second task.
        c.set_priority(2);

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
