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

type Task = task::Task<Adapter>;
type TaskList = linked_list::List<Task>;

pub use task::Handle;

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

struct Adapter {
    node: linked_list::Pointers<task::Header>,
    exec: Weak<ExecutorStruct>,
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
        let adapter = Adapter::new(Arc::downgrade(&self.inner));
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

impl Adapter {
    fn new(exec: Weak<ExecutorStruct>) -> Self {
        Adapter {
            node: linked_list::Pointers::new(),
            exec,
        }
    }
}

impl task::Adapter for Adapter {
    fn on_cancel(&self, task: NonNull<task::Header>) {
        // try to get a strong reference to the executor
        if let Some(exec) = self.exec.upgrade() {
            let mut queue = exec.queue.lock().unwrap();

            // try to remove ourselves from the queue
            unsafe { queue.remove(task) };
        }
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
        let ptr = Self::Pointer::get_adapter(target);
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
