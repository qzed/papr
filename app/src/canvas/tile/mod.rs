mod scheme;
pub use scheme::{ExactLevelTilingScheme, HybridTilingScheme, QuadTreeTilingScheme, TilingScheme};

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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Tile<T> {
    pub id: TileId,
    pub data: T,
}

impl<T> Tile<T> {
    pub fn new(id: TileId, data: T) -> Self {
        Self { id, data }
    }
}

pub struct TileRect {
    pub rect: Bounds<i64>,
    pub z: i64,
}
