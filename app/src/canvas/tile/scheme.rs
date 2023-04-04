use nalgebra as na;
use nalgebra::{vector, Vector2};

use crate::types::{Bounds, Rect, Viewport};

use super::{TileId, TileRect};

pub trait TilingScheme {
    fn tile_size(&self) -> Vector2<i64>;

    fn tiles(&self, vp: &Viewport, page: &Rect<f64>, rect: &Bounds<f64>) -> TileRect;

    /// Area covered by this tile in pixels adjusted for the specified z-level,
    /// aligned at the page origin.
    fn screen_rect(&self, vp: &Viewport, page: &Rect<f64>, id: &TileId) -> Rect<f64>;

    fn render_rect(
        &self,
        page_size_pt: &Vector2<f64>,
        page_size_vp: &Vector2<f64>,
        id: &TileId,
    ) -> (Vector2<i64>, Rect<i64>);
}

#[derive(Debug, Clone)]
pub struct ExactLevelTilingScheme {
    tile_size: Vector2<i64>,
}

impl ExactLevelTilingScheme {
    pub fn new(tile_size: Vector2<i64>) -> Self {
        Self { tile_size }
    }
}

impl TilingScheme for ExactLevelTilingScheme {
    #[inline]
    fn tile_size(&self) -> Vector2<i64> {
        self.tile_size
    }

    #[inline]
    fn tiles(&self, _vp: &Viewport, page: &Rect<f64>, rect: &Bounds<f64>) -> TileRect {
        let rect = rect.cast_unchecked().tiled(&self.tile_size);
        let z = page.size.x as i64;

        TileRect { rect, z }
    }

    #[inline]
    fn screen_rect(&self, _vp: &Viewport, page: &Rect<f64>, id: &TileId) -> Rect<f64> {
        let tile_size: Vector2<f64> = na::convert(self.tile_size);
        let xy: Vector2<f64> = na::convert(vector![id.x, id.y]);
        let z = page.size.x;

        Rect::new(xy.component_mul(&tile_size).into(), tile_size).scale(z / id.z as f64)
    }

    #[inline]
    fn render_rect(
        &self,
        _page_size_pt: &Vector2<f64>,
        page_size_vp: &Vector2<f64>,
        id: &TileId,
    ) -> (Vector2<i64>, Rect<i64>) {
        let page_size = na::convert_unchecked(*page_size_vp);
        let tile_offs = vector![id.x, id.y].component_mul(&self.tile_size);
        let tile_rect = Rect::new(tile_offs.into(), self.tile_size);

        (page_size, tile_rect)
    }
}

#[derive(Debug, Clone)]
pub struct QuadTreeTilingScheme {
    tile_size: Vector2<i64>,
}

#[allow(unused)]
impl QuadTreeTilingScheme {
    pub fn new(tile_size: Vector2<i64>) -> Self {
        Self { tile_size }
    }
}

impl TilingScheme for QuadTreeTilingScheme {
    #[inline]
    fn tile_size(&self) -> Vector2<i64> {
        self.tile_size
    }

    #[inline]
    fn tiles(&self, vp: &Viewport, _page: &Rect<f64>, rect: &Bounds<f64>) -> TileRect {
        let z = vp.scale.log2().ceil();
        let level = z.exp2();

        let rect = rect.scale(level / vp.scale).round_outwards();
        let rect = rect.cast_unchecked().tiled(&self.tile_size);

        TileRect { rect, z: z as i64 }
    }

    #[inline]
    fn screen_rect(&self, vp: &Viewport, _page: &Rect<f64>, id: &TileId) -> Rect<f64> {
        let tile_size: Vector2<f64> = na::convert(self.tile_size);
        let xy: Vector2<f64> = na::convert(vector![id.x, id.y]);

        Rect::new(xy.component_mul(&tile_size).into(), tile_size)
            .scale(vp.scale / (id.z as f64).exp2())
    }

    #[inline]
    fn render_rect(
        &self,
        page_size_pt: &Vector2<f64>,
        _page_size_vp: &Vector2<f64>,
        id: &TileId,
    ) -> (Vector2<i64>, Rect<i64>) {
        let scale = (id.z as f64).exp2();

        let page_size = page_size_pt * scale;
        let page_size = vector![page_size.x.ceil() as _, page_size.y.ceil() as _];

        let tile_offs = vector![id.x, id.y].component_mul(&self.tile_size);
        let tile_rect = Rect::new(tile_offs.into(), self.tile_size);

        (page_size, tile_rect)
    }
}
