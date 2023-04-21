use nalgebra as na;
use nalgebra::{point, vector, Vector2};

use crate::types::{Bounds, Rect, Viewport};

use super::{TileId, TileRect};

/// A tiling scheme, describing how a page can be divided into specific tiles.
///
/// Describes which tiles are needed to cover a specific area of a page at a
/// specific resolution, and how these tiles look like (i.e., their size and
/// positions).
pub trait TilingScheme {
    /// Return the preferred set of tiles to cover the given area (`rect`) of
    /// the `page` using the specified viewport for rendering.
    ///
    /// Note that there are many combinations of tiles that can cover the
    /// specified area, even more so when mixing different z-levels. This
    /// function returns the required tiles for the z-level that best fits the
    /// specified viewport.
    ///
    /// # Arguments
    /// - `vp`: The [`Viewport`] used for rendering.
    /// - `page`: The page bounds in viewport coordinates.
    /// - `rect`: The area for which the required tiles should be returned, in
    ///    viewport coordinates aligned at the page origin.
    fn tiles(&self, vp: &Viewport, page: &Rect<f64>, rect: &Bounds<f64>) -> TileRect;

    /// Area on screen covered by the given tile in pixels, adjusted for the
    /// specified z-level and aligned at the page origin.
    ///
    /// # Arguments
    /// - `vp`: The [`Viewport`] used for rendering.
    /// - `page`: The page bounds in viewport coordinates.
    /// - `id`: The tile ID.
    fn screen_rect(&self, vp: &Viewport, page: &Rect<f64>, id: &TileId) -> Rect<f64>;

    /// Return the page size and rectangle describing how the given tile
    /// relates to a full-sized bitmap of the page.
    ///
    /// This function essentially describes how a tile is rendered: It returns
    /// `(page_size, tile_rect)`, describing that a page should be rendered
    /// with size `page_size` (in pixels), where the tile is the result of that
    /// operation if one would crop out only the returned `tile_rect`.
    ///
    /// # Arguments
    /// - `page_size_pt`: The page size in PDF points.
    /// - `page_size_vp`: The page size in viewport coordinates.
    /// - `id`: The tile ID.
    fn render_rect(
        &self,
        page_size_pt: &Vector2<f64>,
        page_size_vp: &Vector2<f64>,
        id: &TileId,
    ) -> (Vector2<i64>, Rect<i64>);
}

/// A hybrid tiling-scheme.
///
/// Divides a page into tiles if it is larger than a specified threshold and
/// renders the page as a single tile if not. Follows the
/// [`ExactLevelTilingScheme`] approach for tiling, rendering tiles at the
/// specific output resolution to bypass the need for interpolation and provide
/// visually better results.
#[derive(Debug, Clone)]
pub struct HybridTilingScheme {
    tile_size: Vector2<i64>,
    min_tile_z: i64,
}

impl HybridTilingScheme {
    /// Create a new hybrid tiling-scheme.
    ///
    /// # Arguments
    /// - `tile_size`: The size of the tiles when the page is being tiled.
    /// - `min_size`: The minimum page size for when a page should be tiled.
    ///
    ///    If the maximum dimension (i.e., maximum of width and height) of a
    ///    page in viewport coordinates is larger than this threshold, the page
    ///    will be divided into (multiple) tiles. Otherwise, it will be
    ///    rendered as a single tile (with size equals to the page size in
    ///    viewport coordinates).
    pub fn new(tile_size: Vector2<i64>, min_size: i64) -> Self {
        Self {
            tile_size,
            min_tile_z: min_size,
        }
    }
}

impl TilingScheme for HybridTilingScheme {
    #[inline]
    fn tiles(&self, _vp: &Viewport, page: &Rect<f64>, rect: &Bounds<f64>) -> TileRect {
        let z = f64::max(page.size.x, page.size.y) as i64;

        let rect = if z > self.min_tile_z {
            rect.cast_unchecked().tiled(&self.tile_size)
        } else {
            Rect::new(point![0, 0], vector![1, 1]).bounds()
        };

        TileRect { rect, z }
    }

    #[inline]
    fn screen_rect(&self, _vp: &Viewport, page: &Rect<f64>, id: &TileId) -> Rect<f64> {
        if id.z > self.min_tile_z {
            let z = f64::max(page.size.x, page.size.y);
            let tile_size: Vector2<f64> = na::convert(self.tile_size);
            let xy: Vector2<f64> = na::convert(vector![id.x, id.y]);

            Rect::new(xy.component_mul(&tile_size).into(), tile_size).scale(z / id.z as f64)
        } else {
            Rect::new(point![0.0, 0.0], page.size)
        }
    }

    #[inline]
    fn render_rect(
        &self,
        _page_size_pt: &Vector2<f64>,
        page_size_vp: &Vector2<f64>,
        id: &TileId,
    ) -> (Vector2<i64>, Rect<i64>) {
        let page_size: Vector2<i64> = na::convert_unchecked(*page_size_vp);

        let z = f64::max(page_size_vp.x, page_size_vp.y) as i64;

        let tile_rect = if z > self.min_tile_z {
            Rect::new(
                vector![id.x, id.y].component_mul(&self.tile_size).into(),
                self.tile_size,
            )
        } else {
            Rect::new(point![0, 0], page_size)
        };

        (page_size, tile_rect)
    }
}

/// A tiling-scheme using tiles at the exact resolution.
///
/// Uses tiles at the exact viewport resolution/z-level. This avoids the need
/// for interpolation and provides visually more crisp results (especially for
/// text, improving readability), however, means that tiles need to be rendered
/// specifically for each zoom level.
#[derive(Debug, Clone)]
pub struct ExactLevelTilingScheme {
    tile_size: Vector2<i64>,
}

#[allow(unused)]
impl ExactLevelTilingScheme {
    /// Creates a new exact-level tiling-scheme with the specified tile size.
    pub fn new(tile_size: Vector2<i64>) -> Self {
        Self { tile_size }
    }
}

impl TilingScheme for ExactLevelTilingScheme {
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

/// A basic quad-tree-based tiling scheme.
///
/// Tiles are rendered at discrete power-of-two zoom levels and interpolated to
/// the desired output resolution.
#[derive(Debug, Clone)]
pub struct QuadTreeTilingScheme {
    tile_size: Vector2<i64>,
}

#[allow(unused)]
impl QuadTreeTilingScheme {
    /// Creates a new quad-tree tiling-scheme with the specified tile size.
    pub fn new(tile_size: Vector2<i64>) -> Self {
        Self { tile_size }
    }
}

impl TilingScheme for QuadTreeTilingScheme {
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
