use std::collections::{HashMap, HashSet};

use nalgebra::{point, Vector2};

use crate::types::{Bounds, Rect, Viewport};

use super::{TileHandle, TileId, TilePriority, TileSource, TilingScheme, PageData};

pub struct TileManager<S, H: TileHandle> {
    scheme: S,
    cache: HashMap<usize, Cache<H>>,
    halo: Vector2<i64>,
    min_retain_size: Vector2<f64>,
}

struct Cache<H: TileHandle> {
    cached: HashMap<TileId, H::Data>,
    pending: HashMap<TileId, Option<H>>,
}

impl<S, H> TileManager<S, H>
where
    S: TilingScheme,
    H: TileHandle,
{
    pub fn new(scheme: S, halo: Vector2<i64>, min_retain_size: Vector2<f64>) -> Self {
        Self {
            scheme,
            cache: HashMap::new(),
            halo,
            min_retain_size,
        }
    }

    pub fn update<F, T, O>(
        &mut self,
        source: &mut T,
        pages: &PageData<'_, F>,
        vp: &Viewport,
        request_opts: &O,
    ) where
        F: Fn(&Rect<f64>) -> Rect<f64>,
        T: TileSource<Handle = H, RequestOptions = O>,
    {
        // remove out-of-view pages from cache
        self.cache.retain(|page, _| pages.visible.contains(page));

        // update tiles for all visible pages
        let iter = pages
            .visible
            .clone()
            .zip(&pages.layout[pages.visible.clone()]);

        for (page_index, page_rect_pt) in iter {
            // transform page bounds to viewport
            let page_rect = (pages.transform)(page_rect_pt);

            // recompute scale for rounded page
            let scale = page_rect.size.x / page_rect_pt.size.x;
            let vp_adj = Viewport { r: vp.r, scale };

            // update tiles for page
            self.update_page(
                source,
                &vp_adj,
                page_index,
                &page_rect,
                page_rect_pt,
                request_opts,
            );
        }
    }

    fn update_page<T, O>(
        &mut self,
        source: &mut T,
        vp: &Viewport,
        page_index: usize,
        page_rect: &Rect<f64>,
        page_rect_pt: &Rect<f64>,
        request_opts: &O,
    ) where
        T: TileSource<Handle = H, RequestOptions = O>,
    {
        // viewport bounds relative to the page in pixels (area of page visible on screen)
        let visible_page = Rect::new(-page_rect.offs, vp.r.size)
            .clip(&Rect::new(point![0.0, 0.0], page_rect.size))
            .bounds();

        // tile bounds for the visible part of the page
        let tiles = self.scheme.tiles(vp, page_rect, &visible_page);

        // tile bounds for the full page
        let tiles_page = {
            let page_bounds = Rect::new(point![0.0, 0.0], page_rect.size).bounds();
            self.scheme.tiles(vp, page_rect, &page_bounds).rect
        };

        // tile bounds for the extended viewport (with cached halo tiles)
        let tiles_vp = {
            let tiles_vp = Bounds {
                x_min: tiles.rect.x_min - self.halo.x,
                x_max: tiles.rect.x_max + self.halo.x,
                y_min: tiles.rect.y_min - self.halo.y,
                y_max: tiles.rect.y_max + self.halo.y,
            };

            tiles_vp.clip(&tiles_page)
        };

        // get cached tiles for this page
        let entry = self.cache.entry(page_index).or_insert_with(Cache::empty);

        // helper for requesting tiles
        let mut request_tiles = |tile_rect: &Bounds<i64>, priority| {
            for (x, y) in tile_rect.range_iter() {
                let id = TileId::new(page_index, x, y, tiles.z);

                // check if we already have the tile
                if entry.cached.contains_key(&id) {
                    continue;
                }

                // check if we already requested the tile and update the priority
                if let Some(entry) = entry.pending.get(&id) {
                    if let Some(task) = entry {
                        task.set_priority(priority);
                    }
                    continue;
                }

                // compute page size and tile bounds
                let (page_size, rect) =
                    self.scheme
                        .render_rect(&page_rect_pt.size, &page_rect.size, &id);

                // request tile
                let handle = source.request(page_index, page_size, rect, request_opts, priority);

                // store handle to the render task
                entry.pending.insert(id, Some(handle));
            }
        };

        // request new tiles in view if not cached or pending
        request_tiles(&tiles.rect, TilePriority::Medium);

        // pre-request new tiles around view with lower priority
        {
            let top = Bounds {
                x_min: tiles.rect.x_min,
                x_max: tiles.rect.x_max,
                y_min: (tiles.rect.y_min - self.halo.y).max(tiles_page.y_min),
                y_max: tiles.rect.y_min,
            };

            let bottom = Bounds {
                x_min: tiles.rect.x_min,
                x_max: tiles.rect.x_max,
                y_min: tiles.rect.y_max,
                y_max: (tiles.rect.y_max + self.halo.y).min(tiles_page.y_max),
            };

            let left = Bounds {
                x_min: (tiles.rect.x_min - self.halo.x).max(tiles_page.x_min),
                x_max: tiles.rect.x_min,
                y_min: (tiles.rect.y_min - self.halo.y).max(tiles_page.y_min),
                y_max: (tiles.rect.y_max + self.halo.y).min(tiles_page.y_max),
            };

            let right = Bounds {
                x_min: tiles.rect.x_max,
                x_max: (tiles.rect.x_max + self.halo.x).min(tiles_page.x_max),
                y_min: (tiles.rect.y_min - self.halo.y).max(tiles_page.y_min),
                y_max: (tiles.rect.y_max + self.halo.y).min(tiles_page.y_max),
            };

            request_tiles(&bottom, TilePriority::Low);
            request_tiles(&top, TilePriority::Low);
            request_tiles(&left, TilePriority::Low);
            request_tiles(&right, TilePriority::Low);
        }

        // move newly rendered tiles to cached map
        for (id, task) in &mut entry.pending {
            if task.is_some() && task.as_ref().unwrap().is_finished() {
                entry
                    .cached
                    .insert(*id, std::mem::take(task).unwrap().join());
            }
        }

        // find unused/occluded pending tiles and remove them
        entry.pending.retain(|id, task| {
            // remove any tasks that have already been completed
            if task.is_none() {
                return false;
            }

            // stop loading anything that is not on the current zoom level
            if id.z != tiles.z {
                return false;
            }

            // otherwise: check if tile is in the extended viewport
            tiles_vp.contains_point(&id.xy())
        });

        // find unused/occluded cached tiles and remove them
        let cached_keys: HashSet<_> = entry.cached.keys().cloned().collect();

        entry.cached.retain(|id, _tile| {
            // if the tile is on the current level: keep it if it is in the
            // extended viewport, drop it if not
            if id.z == tiles.z {
                return tiles_vp.contains_point(&id.xy());
            }

            // compute tile bounds
            let tile_rect = self.scheme.screen_rect(vp, page_rect, id);
            let tile_rect = tile_rect.bounds().round_outwards();
            let tile_rect_screen = tile_rect.translate(&page_rect.offs.coords);

            // check if tile is in view, drop it if it is not
            let vpz_rect = Rect::new(point![0.0, 0.0], vp.r.size).bounds();
            if !tile_rect_screen.intersects(&vpz_rect) {
                return false;
            }

            // if the tile is sufficently small, remove it
            let size = tile_rect_screen.rect().size;
            if size.x < self.min_retain_size.x && size.y < self.min_retain_size.y {
                return false;
            }

            // otherwise: check if the tile is replaced by ones with the
            // current z-level
            //
            // note: this does not check if e.g. a lower-z tile is occluded
            // by higher-z tiles, only if a tile is fully occluded by tiles
            // on the current z-level

            // compute tile IDs on current z-level required to fully cover the
            // original one
            let tiles_req = self.scheme.tiles(vp, page_rect, &tile_rect);
            let tiles_req = tiles_req.rect.clip(&tiles.rect);

            // check if all required tiles are present
            !tiles_req
                .range_iter()
                .all(|(x, y)| cached_keys.contains(&TileId::new(page_index, x, y, tiles.z)))
        });
    }

    pub fn tiles(
        &self,
        vp: &Viewport,
        page_index: usize,
        page_rect: &Rect<f64>,
    ) -> Vec<(Rect<f64>, &H::Data)> {
        // viewport bounds relative to the page in pixels (area of page visible on screen)
        let visible_page = Rect::new(-page_rect.offs, vp.r.size)
            .clip(&Rect::new(point![0.0, 0.0], page_rect.size))
            .bounds();

        // tile bounds for viewport
        let tiles = self.scheme.tiles(vp, page_rect, &visible_page);

        // get cache entry
        let entry = if let Some(entry) = self.cache.get(&page_index) {
            entry
        } else {
            return Vec::new();
        };

        // build ordered render list
        let mut rlist: Vec<_> = entry
            .cached
            .iter()
            .filter(|(id, _)| {
                // if the tile has a different z-level we assume that it is
                // required (otherwise, it should have been removed in the
                // update)
                id.z != tiles.z ||
                // if z-levels match, check if the tile is inside the viewport
                tiles.rect.contains_point(&id.xy())
            })
            .collect();

        rlist.sort_unstable_by(|(id_a, _), (id_b, _)| {
            use std::cmp::Ordering;

            // sort by z-level:
            // - put all tiles with current z-level last
            // - sort rest in descending order (i.e., coarser tiles first)

            if id_a.z == id_b.z {
                // same z-levels are always equal
                Ordering::Equal
            } else if id_a.z == tiles.z {
                // put current z-level last
                Ordering::Greater
            } else if id_b.z == tiles.z {
                // put current z-level last
                Ordering::Less
            } else {
                // sort by z-level, descending
                if id_a.z < id_b.z {
                    Ordering::Greater
                } else {
                    Ordering::Less
                }
            }
        });

        rlist
            .into_iter()
            .map(|(id, data)| {
                let tile_rect = self.scheme.screen_rect(vp, page_rect, id);
                let tile_rect = tile_rect.translate(&page_rect.offs.coords);

                (tile_rect, data)
            })
            .collect()
    }
}

impl<T: TileHandle> Cache<T> {
    fn empty() -> Self {
        Self {
            cached: HashMap::new(),
            pending: HashMap::new(),
        }
    }
}
