use nalgebra::{vector, Vector2};

use crate::types::Rect;

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

    /// Area covered by this tile in pixels, aligned at the page origin.
    #[inline]
    pub fn rect(&self, tile_size: &Vector2<i64>) -> Rect<i64> {
        Rect::new(
            vector![self.x, self.y].component_mul(tile_size).into(),
            *tile_size,
        )
    }

    /// Area covered by this tile in pixels for different z-level, aligned at
    /// the page origin.
    #[inline]
    pub fn rect_for_z(&self, tile_size: &Vector2<i64>, z: i64) -> Rect<f64> {
        self.rect(tile_size)
            .cast::<f64>()
            .scale(z as f64 / self.z as f64)
    }

    /// Area covered by this tile in pixels for different z-level, aligned at
    /// the page origin and rounded outwards to full pixels.
    #[inline]
    pub fn rect_for_z_rounded(&self, tile_size: &Vector2<i64>, z: i64) -> Rect<i64> {
        self.rect_for_z(tile_size, z)
            .bounds()
            .round_outwards()
            .cast_unchecked::<i64>()
            .rect()
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
