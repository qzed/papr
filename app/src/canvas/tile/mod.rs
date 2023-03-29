use nalgebra::{Vector2, vector};

use crate::types::Rect;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TileId {
    pub page: usize,
    pub x: i64,
    pub y: i64,
    pub z: i64,
}

impl TileId {
    pub fn new(page: usize, x: i64, y: i64, z: i64) -> Self {
        Self { page, x, y, z }
    }

    /// Area covered by this tile in pixels, aligned at the page origin. 
    pub fn rect(&self, tile_size: &Vector2<i64>) -> Rect<i64> {
        Rect::new(vector![self.x, self.y].component_mul(tile_size).into(), *tile_size)
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
