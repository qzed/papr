//! Common structs and traits across executors.

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

impl Monitor for () {}
