use std::ptr::NonNull;
use std::time::Duration;

use super::core::{Cell, Header};
use super::vtable::Vtable;

pub struct RawTask {
    ptr: NonNull<Header>,
}

impl RawTask {
    pub fn new<F, R>(closure: F) -> Self
    where
        F: FnOnce() -> R,
    {
        let cell = Cell::new(closure);

        let ptr = Box::into_raw(cell);
        let ptr = unsafe { std::ptr::addr_of_mut!((*ptr).header) };
        let ptr = unsafe { NonNull::new_unchecked(ptr) };

        Self { ptr }
    }

    pub fn from_raw(ptr: NonNull<Header>) -> Self {
        Self { ptr }
    }

    pub fn into_raw(self) -> NonNull<Header> {
        std::mem::ManuallyDrop::new(self).header_ptr()
    }

    fn header_ptr(&self) -> NonNull<Header> {
        self.ptr
    }

    fn header(&self) -> &Header {
        unsafe { self.header_ptr().as_ref() }
    }

    fn vtable(&self) -> &'static Vtable {
        self.header().vtable
    }

    pub fn execute(&self) {
        unsafe { (self.vtable().execute)(self.ptr) }
    }

    pub fn result<R>(&self) -> Option<R> {
        let mut out = None;

        let out_ptr = &mut out as *mut _ as *mut ();
        unsafe {
            (self.vtable().read_result)(self.ptr, out_ptr);
        }

        out
    }

    pub fn cancel(&self) -> bool {
        // Shortcut: Don't attempt to cancel if we're already marked as
        // complete. Return "true" to indicate that the task is done.
        if self.is_complete() {
            return true;
        }

        unsafe { (self.vtable().cancel)(self.ptr) }
    }

    pub fn wait(&self) {
        // Shortcut: Don't attempt to wait if we're already marked as complete.
        if self.is_complete() {
            return;
        }

        self.header().complete.wait()
    }

    #[must_use]
    pub fn wait_timeout(&self, duration: Duration) -> bool {
        // Shortcut: Don't attempt to wait if we're already marked as complete.
        if self.is_complete() {
            return true;
        }

        self.header().complete.wait_timeout(duration)
    }

    pub fn is_complete(&self) -> bool {
        self.header().state.snapshot().is_complete()
    }

    pub fn is_canceled(&self) -> bool {
        self.header().state.snapshot().is_canceled()
    }

    pub fn is_consumed(&self) -> bool {
        self.header().state.snapshot().is_consumed()
    }
}

impl Clone for RawTask {
    fn clone(&self) -> Self {
        self.header().state.ref_inc();
        RawTask { ptr: self.ptr }
    }
}

impl Drop for RawTask {
    fn drop(&mut self) {
        if self.header().state.ref_dec() {
            unsafe { (self.vtable().dealloc)(self.ptr) }
        }
    }
}

unsafe impl Send for RawTask {}
unsafe impl Sync for RawTask {}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn execute_local() {
        let value: i32 = 42;
        let closure = || value;

        // create new task
        let task = RawTask::new(closure);

        assert!(!task.is_complete());
        assert!(!task.is_canceled());
        assert!(!task.is_consumed());

        // execute task immediately on this thread
        task.execute();

        assert!(task.is_complete());
        assert!(!task.is_canceled());
        assert!(!task.is_consumed());

        // "wait" for the task to complete. As we just executed it, this should
        // be a no-op.
        let duration = Duration::from_millis(1);
        let success = task.wait_timeout(duration);
        assert!(success);

        assert!(task.is_complete());
        assert!(!task.is_canceled());
        assert!(!task.is_consumed());

        // get the result of the task
        assert_eq!(task.result(), Some(value));
        assert!(task.is_complete());
        assert!(!task.is_canceled());
        assert!(task.is_consumed());
    }

    #[test]
    fn execute_local_cancel() {
        let value: i32 = 42;
        let closure = || {
            // this should never be reached
            assert!(false);
            value
        };

        // create new task
        let task = RawTask::new(closure);

        assert!(!task.is_complete());
        assert!(!task.is_canceled());
        assert!(!task.is_consumed());

        task.cancel();
        assert!(task.is_complete());
        assert!(task.is_canceled());

        // try to execute task immediately on this thread
        task.execute();
        assert!(task.is_complete());
        assert!(task.is_canceled());

        // "wait" for the task to complete. As we just executed it, this should
        // be a no-op.
        let duration = Duration::from_millis(1);
        let success = task.wait_timeout(duration);
        assert!(success);

        // get the result of the task
        assert_eq!(task.result(), None::<i32>);
        assert!(task.is_complete());
        assert!(task.is_canceled());
        assert!(task.is_consumed());
    }

    #[test]
    fn execute_remote() {
        let value: i32 = 42;

        let closure = || {
            // wait a bit to make it look like we're doing some work
            std::thread::sleep(Duration::from_millis(100));

            value
        };

        // create a new task
        let task = RawTask::new(closure);

        assert!(!task.is_complete());
        assert!(!task.is_canceled());
        assert!(!task.is_consumed());

        let result = std::thread::scope(|s| {
            // execute the task on a new thread
            s.spawn(|| {
                task.execute();
            });

            // wait for the task to complete
            let duration = Duration::from_millis(500);
            let success = task.wait_timeout(duration);
            assert!(success);

            assert!(task.is_complete());
            assert!(!task.is_canceled());
            assert!(!task.is_consumed());

            // get the result of the task
            task.result()
        });

        assert_eq!(result, Some(value));
        assert!(task.is_complete());
        assert!(!task.is_canceled());
        assert!(task.is_consumed());
    }

    #[test]
    fn execute_remote_panic() {
        use std::panic::AssertUnwindSafe;

        // a closure that panics
        let closure = || -> () {
            panic!("foo");
        };

        // create a new task
        let task = RawTask::new(closure);

        // execute the task on a new thread and wait for it to finish
        std::thread::scope(|s| {
            s.spawn(|| {
                task.execute();
            });
        });

        // get the result: this should panic
        let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
            let _result: Option<()> = task.result();
        }));

        // make sure we got the right panic
        assert!(result.is_err());
        assert_eq!(
            *result.unwrap_err().downcast_ref::<&'static str>().unwrap(),
            "foo"
        );
    }
}
