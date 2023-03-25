use std::collections::HashMap;

use super::{Tile, TileId};

pub struct TileCache<T> {
    storage: HashMap<TileId, TileCacheEntry<T>>,
}

struct TileCacheEntry<T> {
    in_use: bool,
    tile: Tile<T>,
}

impl<T> TileCache<T> {
    pub fn new() -> Self {
        Self {
            storage: HashMap::new(),
        }
    }

    pub fn get(&mut self, id: &TileId) -> Option<&Tile<T>> {
        if let Some(entry) = self.storage.get_mut(id) {
            entry.in_use = true;
            Some(&entry.tile)
        } else {
            None
        }
    }

    pub fn insert(&mut self, tile: Tile<T>) {
        let id = tile.id;
        let entry = TileCacheEntry { in_use: true, tile };

        self.storage.insert(id, entry);
    }

    pub fn mark(&mut self) {
        for entry in self.storage.values_mut() {
            entry.in_use = false;
        }
    }

    pub fn evict_unused(&mut self) {
        self.storage.retain(|_, e| e.in_use);
    }
}
