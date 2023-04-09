use std::sync::atomic::{AtomicUsize, Ordering};

const TASK_EXECUTING_BIT: usize = 1 << 0;
const TASK_COMPLETE_BIT: usize = 1 << 1;
const TASK_CONSUMED_BIT: usize = 1 << 2;

const TASK_CANCELED_BIT: usize = 1 << 3;

const STATE_MASK: usize =
    TASK_EXECUTING_BIT | TASK_COMPLETE_BIT | TASK_CONSUMED_BIT | TASK_CANCELED_BIT;

const REF_MASK: usize = !STATE_MASK;
const REF_SHIFT: usize = REF_MASK.count_zeros() as usize;
const REF_ONE: usize = 1 << REF_SHIFT;

pub struct State {
    value: AtomicUsize,
}

pub struct Snapshot {
    value: usize,
}

impl State {
    pub fn initial() -> Self {
        let init = REF_ONE;

        Self {
            value: AtomicUsize::new(init),
        }
    }

    pub fn ref_inc(&self) {
        let prev = self.value.fetch_add(REF_ONE, Ordering::Relaxed);

        // check for overflow
        if prev > isize::MAX as usize {
            std::process::abort();
        }
    }

    pub fn ref_dec(&self) -> bool {
        let prev = self.value.fetch_sub(REF_ONE, Ordering::AcqRel);
        let prev = (prev & REF_MASK) >> REF_SHIFT;

        // check for underflow
        if prev < 1 {
            std::process::abort();
        }

        prev == 1
    }

    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            value: self.value.load(Ordering::Acquire),
        }
    }

    pub fn transition_init_to_exec(&self) -> Result<Snapshot, Snapshot> {
        self.fetch_update(|value| {
            // if already completed: abort transition
            if (value & TASK_COMPLETE_BIT) != 0 {
                return None;
            }

            // if currently executing: abort transition
            if (value & TASK_EXECUTING_BIT) != 0 {
                return None;
            }

            // mark task as executing
            let value = value | TASK_EXECUTING_BIT;

            Some(value)
        })
    }

    pub fn transition_exec_to_complete(&self) -> Result<Snapshot, Snapshot> {
        self.fetch_update(|value| {
            // Note: we should have exclusive access due to the "executing" bit
            // being set.

            // remove the "executing" bit and set the "completed" bit
            let value = value & !TASK_EXECUTING_BIT;
            let value = value | TASK_COMPLETE_BIT;

            Some(value)
        })
    }

    pub fn transition_complete_to_consumed(&self) -> Result<Snapshot, Snapshot> {
        self.fetch_update(|value| {
            // if the task has not been completed yet: abort transition
            if (value & TASK_COMPLETE_BIT) == 0 {
                return None;
            }

            // if the task already been consumed: abort transition
            if (value & TASK_CONSUMED_BIT) != 0 {
                return None;
            }

            // mark task as consumed
            let value = value | TASK_CONSUMED_BIT;

            Some(value)
        })
    }

    pub fn transition_to_canceled(&self) -> Result<Snapshot, Snapshot> {
        self.fetch_update(|value| {
            // if the task has already been completed: abort transition
            if (value & TASK_COMPLETE_BIT) != 0 {
                return None;
            }

            // if the task is currently executing: abort transition
            if (value & TASK_EXECUTING_BIT) != 0 {
                return None;
            }

            // mark task as "completed", "consumed", and "canceled"
            let value = value | TASK_COMPLETE_BIT; // prevent execution
            let value = value | TASK_CONSUMED_BIT; // prevent access to result
            let value = value | TASK_CANCELED_BIT; // let everyone know it's canceled

            Some(value)
        })
    }

    fn fetch_update<F>(&self, mut f: F) -> Result<Snapshot, Snapshot>
    where
        F: FnMut(usize) -> Option<usize>,
    {
        let mut curr = self.value.load(Ordering::Acquire);

        loop {
            let next = match f(curr) {
                Some(next) => next,
                None => return Err(Snapshot { value: curr }),
            };

            let res = self
                .value
                .compare_exchange(curr, next, Ordering::AcqRel, Ordering::Acquire);

            match res {
                Ok(_) => return Ok(Snapshot { value: next }),
                Err(actual) => curr = actual,
            }
        }
    }
}

impl Snapshot {
    pub fn refcount(&self) -> usize {
        (self.value & REF_MASK) >> REF_SHIFT
    }

    pub fn is_complete(&self) -> bool {
        (self.value & TASK_COMPLETE_BIT) != 0
    }

    pub fn is_canceled(&self) -> bool {
        (self.value & TASK_CANCELED_BIT) != 0
    }

    pub fn is_consumed(&self) -> bool {
        (self.value & TASK_CONSUMED_BIT) != 0
    }
}
