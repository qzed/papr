mod scheme;
pub use scheme::{ExactLevelTilingScheme, HybridTilingScheme, QuadTreeTilingScheme, TilingScheme};

mod source;
pub use source::{TileHandle, TilePriority, TileSource};

use nalgebra::{point, Point2};

use crate::types::Bounds;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TileId {
    pub page: usize,
    pub x: i64,
    pub y: i64,
    pub z: i64,
}

impl TileId {
    #[inline]
    pub fn new(page: usize, x: i64, y: i64, z: i64) -> Self {
        Self { page, x, y, z }
    }

    #[inline]
    pub fn xy(&self) -> Point2<i64> {
        point![self.x, self.y]
    }
}

pub struct TileRect {
    pub rect: Bounds<i64>,
    pub z: i64,
}
