use std::ops::Range;

use executor::exec::priority::DropHandle;
use nalgebra::Vector2;

use crate::types::Rect;

pub trait TileProvider {
    type Source<'a>: TileSource + 'a;

    fn request<F, R>(&mut self, pages: &Range<usize>, f: F) -> R
    where
        F: FnOnce(&mut Self::Source<'_>) -> R;
}

pub trait TileSource {
    type Data;
    type Handle: TileHandle<Data = Self::Data>;

    fn request(
        &mut self,
        page_index: usize,
        page_size: Vector2<i64>,
        rect: Rect<i64>,
        priority: TilePriority,
    ) -> Self::Handle;
}

pub trait TileHandle {
    type Data;

    fn is_finished(&self) -> bool;
    fn set_priority(&self, priority: TilePriority);
    fn join(self) -> Self::Data;
}

impl<T: Send> TileHandle for DropHandle<TilePriority, T> {
    type Data = T;

    fn is_finished(&self) -> bool {
        DropHandle::is_finished(self)
    }

    fn set_priority(&self, priority: TilePriority) {
        DropHandle::set_priority(self, priority)
    }

    fn join(self) -> T {
        DropHandle::join(self)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum TilePriority {
    Low,
    Medium,
    High,
}

impl executor::exec::priority::Priority for TilePriority {
    fn count() -> u8 {
        3
    }

    fn from_value(value: u8) -> Option<Self> {
        match value {
            0 => Some(TilePriority::Low),
            1 => Some(TilePriority::Medium),
            2 => Some(TilePriority::High),
            _ => None,
        }
    }

    fn as_value(&self) -> u8 {
        match self {
            TilePriority::Low => 0,
            TilePriority::Medium => 1,
            TilePriority::High => 2,
        }
    }
}
