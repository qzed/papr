//! Work-queue with cancellable work items, inspired by the Linux kernel
//! worqueue but adapted for this project.
//!
//! The idea of this queue is to provide a mechanism with time complexity of
//! O(1) for push, pop, and cancel operations.

use std::ptr::NonNull;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Condvar, Mutex, Weak};
use std::thread::JoinHandle;

use crate::task;
use crate::utils::linked_list;

type Task = task::Task<Data>;
type TaskList = linked_list::List<Task>;

pub use task::Handle;

/// Monitor trait to monitor the progress of a task.
pub trait Monitor {
    /// Executed when the task starts executing its closure.
    fn on_execute(&self) {}

    /// Executed when the task finished executing its closure, either
    /// successfully or via a panic.
    fn on_complete(&self) {}

    /// Executed when the task has been canceled successfully.
    fn on_canceled(&self) {}
}

/// A basic thread-pool executor with a fixed number of threads and cancellable
/// tasks.
pub struct Executor {
    inner: Arc<ExecutorStruct>,

    /// Handles to the execution threads
    threads: Vec<JoinHandle<()>>,
}

struct ExecutorStruct {
    /// Linked list head for the task queue
    queue: Mutex<TaskList>,

    /// Condition variable for signaling arrival of new work items
    signal: Condvar,

    /// Whether to keep the queue running
    running: AtomicBool,
}

struct Data {
    node: linked_list::Pointers<task::Header>,
}

struct Adapter<M> {
    data: Data,
    exec: Weak<ExecutorStruct>,
    monitor: M,
}

impl Executor {
    pub fn new(num_threads: u32) -> Self {
        let inner = ExecutorStruct {
            queue: Mutex::new(TaskList::new()),
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

    pub fn submit<F, R>(&self, closure: F) -> Handle<R>
    where
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static,
    {
        self.submit_with((), closure)
    }

    pub fn submit_with<F, R, M>(&self, monitor: M, closure: F) -> Handle<R>
    where
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static,
        M: Monitor + Send + 'static,
    {
        let adapter = Adapter::new(Arc::downgrade(&self.inner), monitor);
        let (task, handle) = Task::new(adapter, closure);

        self.inner.push(task);

        handle
    }

    pub fn shutdown(&mut self) {
        use std::sync::atomic::Ordering;

        // tell all threads to shut down
        self.inner.running.store(false, Ordering::Release);
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
    fn push(&self, task: Task) {
        let mut queue = self.queue.lock().unwrap();

        queue.push_front(task);
        self.signal.notify_one();
    }

    fn pop(&self) -> Option<Task> {
        use std::sync::atomic::Ordering;

        let mut queue = self.queue.lock().unwrap();

        while self.running.load(Ordering::Acquire) {
            match queue.pop_back() {
                Some(task) => return Some(task),
                None => {
                    queue = self.signal.wait(queue).unwrap();
                    continue;
                }
            }
        }

        None
    }

    fn process(&self) {
        while let Some(task) = self.pop() {
            task.execute()
        }
    }
}

impl<M> Adapter<M>
where
    M: Monitor + Send + 'static,
{
    fn new(exec: Weak<ExecutorStruct>, monitor: M) -> Self {
        Adapter {
            data: Data {
                node: linked_list::Pointers::new(),
            },
            exec,
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
        if let Some(exec) = self.exec.upgrade() {
            let mut queue = exec.queue.lock().unwrap();

            // try to remove ourselves from the queue
            unsafe { queue.remove(task) };
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

impl Monitor for () {}

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
    fn basic_thread_pool() {
        use std::thread;
        use std::time::Duration;

        let mut exec = Executor::new(2);

        let val_a = 123;
        let a = exec.submit(move || {
            thread::sleep(Duration::from_millis(100));
            val_a
        });

        let val_b = 456;
        let b = exec.submit(move || {
            thread::sleep(Duration::from_millis(50));
            val_b
        });

        let val_c = 789;
        let c = exec.submit(move || {
            thread::sleep(Duration::from_millis(150));
            val_c
        });

        assert_eq!(a.join(), val_a);
        assert_eq!(b.join(), val_b);
        assert_eq!(c.join(), val_c);

        exec.shutdown();
    }
}
