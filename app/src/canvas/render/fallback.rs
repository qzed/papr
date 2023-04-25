use std::collections::HashMap;
use std::ops::Range;

use nalgebra::{point, vector, Vector2};

use crate::canvas::PageData;
use crate::types::{Rect, Viewport};

use super::{TileHandle, TilePriority, TileSource};

#[derive(Clone, Copy, Debug)]
pub struct FallbackSpec {
    /// Number of pages around the visible range for which to render fallbacks
    pub halo: usize,

    /// Minimum width and/or height required for a fallback to be rendered
    pub render_threshold: Vector2<f64>,

    /// Maximum bitmap size for the rendered page
    pub render_limits: Vector2<i64>,
}

pub struct FallbackManager<H: TileHandle> {
    levels: Vec<Level<H>>,
}

struct Level<H: TileHandle> {
    spec: FallbackSpec,
    cache: HashMap<usize, CacheEntry<H>>,
    snapshot: Option<Snapshot>,
}

enum CacheEntry<H: TileHandle> {
    Empty,
    Cached(H::Data),
    Pending(H),
}

struct Snapshot {
    scale: f64,
    range: Range<usize>,
}

impl<H> FallbackManager<H>
where
    H: TileHandle,
{
    pub fn new(spec: &[FallbackSpec]) -> Self {
        let mut levels: Vec<_> = spec
            .iter()
            .map(|spec| Level {
                spec: *spec,
                cache: HashMap::new(),
                snapshot: None,
            })
            .collect();

        levels.sort_by_key(|x| (x.spec.render_limits.x, x.spec.render_limits.y));

        FallbackManager { levels }
    }

    pub fn update<F, S>(&mut self, source: &mut S, pages: &PageData<'_, F>, vp: &Viewport)
    where
        F: Fn(&Rect<f64>) -> Rect<f64>,
        S: TileSource<Handle = H>,
    {
        // process LoD levels from highest to lowest resolution
        for level in self.levels.iter_mut().rev() {
            // page range for which the fallbacks should be computed
            let range = level.spec.range(pages.layout.len(), pages.visible);

            // check if the level needs to be updated
            if !level.outdated(vp, &range) {
                continue;
            }

            // remove fallbacks for out-of-scope pages
            level.cache.retain(|i, _| range.contains(i));

            // request new fallbacks
            let mut complete = true;

            for (page_index, page_rect_pt) in range.clone().zip(&pages.layout[range.clone()]) {
                // transform page bounds to viewport
                let page_rect = (pages.transform)(page_rect_pt);

                // skip if the page is too small and remove any entries we have for it
                if page_rect.size.x < level.spec.render_threshold.x
                    && page_rect.size.y < level.spec.render_threshold.y
                {
                    level.cache.remove(&page_index);
                    continue;
                }

                let fallback = level.cache.entry(page_index).or_insert(CacheEntry::Empty);

                // if we already have a rendered result, skip
                if let CacheEntry::Cached(_) = fallback {
                    continue;
                }

                // check if a pending fallback has finished rendering and move it
                if fallback.is_render_finished() {
                    fallback.move_to_cached();
                    continue;
                }

                // if we have a pending fallback, update its priority
                if let CacheEntry::Pending(task) = fallback {
                    if pages.visible.contains(&page_index) {
                        task.set_priority(TilePriority::High);
                    } else {
                        task.set_priority(TilePriority::Low);
                    }

                    complete = false;
                    continue;
                }

                // compute page size for given limits
                let (page_size, rect) = {
                    let scale_x = level.spec.render_limits.x as f64 / page_rect_pt.size.x;
                    let scale_y = level.spec.render_limits.y as f64 / page_rect_pt.size.y;
                    let scale = scale_x.min(scale_y);

                    let page_size = page_rect_pt.size * scale;
                    let page_size = vector![page_size.x.round() as i64, page_size.y.round() as i64];
                    let rect = Rect::new(point![0, 0], page_size);

                    (page_size, rect)
                };

                // set priority based on visibility
                let priority = if pages.visible.contains(&page_index) {
                    TilePriority::High
                } else {
                    TilePriority::Low
                };

                // request tile
                let task = source.request(page_index, page_size, rect, priority);
                *fallback = CacheEntry::Pending(task);

                complete = false;
            }

            let snapshot = if complete {
                Some(Snapshot {
                    scale: vp.scale,
                    range,
                })
            } else {
                None
            };

            level.snapshot = snapshot
        }
    }

    pub fn fallback(&self, page_index: usize) -> Option<&H::Data> {
        // get the cached fallback with the highest resolution
        for level in self.levels.iter().rev() {
            if let Some(CacheEntry::Cached(tex)) = level.cache.get(&page_index) {
                return Some(tex);
            }
        }

        None
    }
}

impl FallbackSpec {
    fn range(&self, n: usize, base: &Range<usize>) -> Range<usize> {
        let start = base.start.saturating_sub(self.halo);
        let end = usize::min(base.end.saturating_add(self.halo), n);
        start..end
    }
}

impl<H> CacheEntry<H>
where
    H: TileHandle,
{
    fn is_render_finished(&self) -> bool {
        if let Self::Pending(task) = self {
            task.is_finished()
        } else {
            false
        }
    }

    fn move_to_cached(&mut self) {
        match std::mem::replace(self, CacheEntry::Empty) {
            CacheEntry::Empty => {}
            CacheEntry::Cached(tex) => *self = CacheEntry::Cached(tex),
            CacheEntry::Pending(task) => *self = CacheEntry::Cached(task.join()),
        }
    }
}

impl<H> Level<H>
where
    H: TileHandle,
{
    fn outdated(&self, vp: &Viewport, range: &Range<usize>) -> bool {
        // if no snapshot is available: level is incomplete
        let snap = match &self.snapshot {
            Some(snap) => snap,
            None => return true,
        };

        // if the page range is different: needs update
        if &snap.range != range {
            return true;
        }

        // if the fallback should always be rendered: no need to compare the scale
        if self.spec.render_threshold.x < 1.0 || self.spec.render_threshold.y < 1.0 {
            return false;
        }

        // otherwise: if the scale changed, we might need to update
        snap.scale != vp.scale
    }
}
