use std::collections::HashMap;
use std::ops::Range;

use nalgebra::{point, vector};

use crate::canvas::PageData;
use crate::types::Rect;

use super::{TileHandle, TilePriority, TileSource};

#[derive(Clone, Copy)]
pub struct FallbackSpec {
    pub halo: usize,
    pub min_width: f64,
    pub tex_width: i64,
}

pub struct FallbackManager<H: TileHandle> {
    levels: Vec<Level<H>>,
}

struct Level<H: TileHandle> {
    spec: FallbackSpec,
    cache: HashMap<usize, CacheEntry<H>>,
}

enum CacheEntry<H: TileHandle> {
    Empty,
    Cached(H::Data),
    Pending(H),
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
            })
            .collect();

        levels.sort_by_key(|x| x.spec.tex_width);

        FallbackManager { levels }
    }

    pub fn update<F, S>(&mut self, source: &S, pages: &PageData<'_, F>)
    where
        F: Fn(&Rect<f64>) -> Rect<f64>,
        S: TileSource<Handle = H>,
    {
        // process LoD levels from lowest to highest resolution
        for level in &mut self.levels {
            // page range for which the fallbacks should be computed
            let range = level.spec.range(pages.layout.len(), pages.visible);

            // remove fallbacks for out-of-scope pages
            level.cache.retain(|i, _| range.contains(i));

            // request new fallbacks
            for (page_index, page_rect_pt) in range.clone().zip(&pages.layout[range]) {
                // transform page bounds to viewport
                let page_rect = (pages.transform)(page_rect_pt);

                // skip if the page is too small and remove any entries we have for it
                if page_rect.size.x < level.spec.min_width {
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
                    continue;
                }

                // compute page size for given width
                let scale = level.spec.tex_width as f64 / page_rect_pt.size.x;
                let page_size = page_rect_pt.size * scale;
                let page_size = vector![page_size.x.round() as i64, page_size.y.round() as i64];
                let rect = Rect::new(point![0, 0], page_size);

                // set priority based on visibility
                let priority = if pages.visible.contains(&page_index) {
                    TilePriority::High
                } else {
                    TilePriority::Low
                };

                // request tile
                let task = source.request(page_index, page_size, rect, priority);
                *fallback = CacheEntry::Pending(task);
            }
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

impl FallbackSpec {
    fn range(self, n: usize, base: &Range<usize>) -> Range<usize> {
        let start = base.start.saturating_sub(self.halo);
        let end = usize::min(base.end.saturating_add(self.halo), n);
        start..end
    }
}
