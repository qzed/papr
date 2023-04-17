use std::sync::{Condvar, Mutex};
use std::time::Duration;

pub struct Completion {
    flag: Mutex<bool>,
    cvar: Condvar,
}

impl Completion {
    pub fn new() -> Self {
        Completion {
            flag: Mutex::new(false),
            cvar: Condvar::new(),
        }
    }

    pub fn set_completed(&self) {
        *self.flag.lock().unwrap() = true;
        self.cvar.notify_all();
    }

    pub fn wait(&self) {
        let _guard = self
            .cvar
            .wait_while(self.flag.lock().unwrap(), |f| !*f)
            .unwrap();
    }

    #[must_use]
    pub fn wait_timeout(&self, duration: Duration) -> bool {
        let (_guard, result) = self
            .cvar
            .wait_timeout_while(self.flag.lock().unwrap(), duration, |f| !*f)
            .unwrap();

        !result.timed_out()
    }
}

impl Default for Completion {
    fn default() -> Self {
        Self::new()
    }
}
