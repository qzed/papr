use std::any::Any;
use std::cell::UnsafeCell;

use super::completion::Completion;
use super::state::State;
use super::vtable;
use super::vtable::Vtable;

pub struct Cell<F, R> {
    /// Common task state and data without any specific type references.
    pub(super) header: Header,

    /// Closure or output, depending on the current execution stage.
    pub(super) core: Core<F, R>,
}

pub struct Header {
    /// Current state of this task.
    pub(super) state: State,

    /// Synchronization primitive for waiting for and signalling task
    /// completion.
    pub(super) complete: Completion,

    /// Function pointers for dealing with this task in a type-erased context.
    pub(super) vtable: &'static Vtable,
}

pub struct Core<F, R> {
    /// Stage specific data.
    pub(super) data: UnsafeCell<Data<F, R>>,
}

#[derive(Default)]
pub enum Data<F, R> {
    /// Empty variant, storing no stage-specific data.
    #[default]
    Empty,

    /// Stores the closure to be executed at a later time.
    Closure(F),

    /// Stores the result obtained by executing the closure of the task.
    Result(R),

    /// Stores a panic that occurred when running the closure of the task.
    Panic(Box<dyn Any + Send + 'static>),
}

impl<F, R> Cell<F, R>
where
    F: FnOnce() -> R,
    F: Send,
    R: Send,
{
    pub fn new(closure: F) -> Box<Cell<F, R>> {
        Box::new(Cell {
            header: Header {
                state: State::initial(),
                complete: Completion::new(),
                vtable: vtable::vtable::<F, R>(),
            },
            core: Core {
                data: UnsafeCell::new(Data::Closure(closure)),
            },
        })
    }
}

impl<F, R> Core<F, R>
where
    F: FnOnce() -> R,
{
    pub unsafe fn take_data(&self) -> Data<F, R> {
        std::mem::take(&mut *self.data.get())
    }

    pub unsafe fn set_result(&self, result: R) {
        *self.data.get() = Data::Result(result);
    }

    pub unsafe fn set_panic(&self, panic: Box<dyn Any + Send + 'static>) {
        *self.data.get() = Data::Panic(panic);
    }
}
