// poopdf - a local-first PDF viewer that reads documents aloud.
// Copyright (C) 2026  poopdf contributors <https://github.com/MAKROFREEK/poopdf>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! The page-bitmap cache: an LRU keyed by (page index, zoom), bounded by a
//! byte budget. Cached bitmaps drop on zoom change, so paging stays fast on
//! large Documents without unbounded memory growth.
//!

use std::collections::{HashMap, VecDeque};

/// A cache key: which page, at which zoom (encoded as a milli-factor so the
/// key is hashable).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CacheKey {
    pub page: usize,
    zoom_milli: u32,
}

impl CacheKey {
    pub fn new(page: usize, zoom_factor: f32) -> Self {
        Self {
            page,
            zoom_milli: (zoom_factor * 1000.0).round() as u32,
        }
    }
}

/// A default byte budget: ~1 GiB of bitmap pixels (page height/width are
/// known at insert time; the caller reports the bitmap's byte cost).
pub const DEFAULT_BUDGET_BYTES: u64 = 1024 * 1024 * 1024;

#[derive(Debug)]
pub struct PageCache<V> {
    max_bytes: u64,
    byte_count: u64,
    entries: HashMap<CacheKey, (V, u64)>,
    order: VecDeque<CacheKey>,
}

impl<V> PageCache<V> {
    pub fn new(max_bytes: u64) -> Self {
        Self {
            max_bytes,
            byte_count: 0,
            entries: HashMap::new(),
            order: VecDeque::new(),
        }
    }

    /// Number of cached bitmaps.
    #[cfg_attr(not(test), allow(dead_code))] // seam surface; exercised by tests
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the cache holds no bitmaps.
    #[cfg_attr(not(test), allow(dead_code))] // seam surface; exercised by tests
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Total cost of cached entries in bytes.
    #[cfg_attr(not(test), allow(dead_code))] // seam surface; exercised by tests
    pub fn total_bytes(&self) -> u64 {
        self.byte_count
    }

    /// Promote `key` to most-recently-used and return its value.
    pub fn get(&mut self, key: CacheKey) -> Option<&V> {
        if self.entries.contains_key(&key) {
            self.promote(key);
        }
        self.entries.get(&key).map(|(v, _)| v)
    }

    /// Insert a value with its byte cost, evicting least-recently-used
    /// entries until the budget holds. An entry that alone exceeds the
    /// budget is not stored.
    pub fn insert(&mut self, key: CacheKey, value: V, cost_bytes: u64) {
        self.remove(&key);
        if cost_bytes > self.max_bytes {
            return;
        }
        self.entries.insert(key, (value, cost_bytes));
        self.order.push_back(key);
        self.byte_count += cost_bytes;
        self.evict_until_within_budget();
    }

    /// Drop every cached bitmap, freeing its bytes. Dropping a texture
    /// handle also frees the GPU-side copy.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.order.clear();
        self.byte_count = 0;
    }

    /// Drop every bitmap that was not rendered at `zoom_factor` (zoom
    /// changed, so all of them are stale at the new scale).
    pub fn invalidate_zoom(&mut self, zoom_factor: f32) {
        let keep = CacheKey::new(0, zoom_factor).zoom_milli;
        let stale: Vec<CacheKey> = self
            .order
            .iter()
            .copied()
            .filter(|k| k.zoom_milli != keep)
            .collect();
        for key in stale {
            self.remove(&key);
        }
    }

    /// Evict least-recently-used entries until the byte budget holds.
    fn evict_until_within_budget(&mut self) {
        while self.byte_count > self.max_bytes {
            let Some(oldest) = self.order.front().copied() else {
                break;
            };
            self.remove(&oldest);
        }
    }

    fn remove(&mut self, key: &CacheKey) {
        if let Some((_, cost)) = self.entries.remove(key) {
            self.byte_count -= cost;
            self.order.retain(|k| k != key);
        }
    }

    fn promote(&mut self, key: CacheKey) {
        self.order.retain(|k| k != &key);
        self.order.push_back(key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(page: usize) -> CacheKey {
        CacheKey::new(page, 1.0)
    }

    #[test]
    fn insert_then_get_returns_value() {
        let mut cache = PageCache::new(1000);
        cache.insert(key(0), 42, 10);
        assert_eq!(cache.get(key(0)), Some(&42));
    }

    #[test]
    fn get_unknown_page_returns_none() {
        let mut cache = PageCache::<i32>::new(1000);
        assert_eq!(cache.get(key(3)), None);
    }

    #[test]
    fn same_page_at_different_zoom_is_a_different_entry() {
        let mut cache = PageCache::new(1000);
        cache.insert(key(0), 1, 10);
        cache.insert(CacheKey::new(0, 2.0), 2, 10);
        assert_eq!(cache.get(key(0)), Some(&1));
        assert_eq!(cache.get(CacheKey::new(0, 2.0)), Some(&2));
    }

    #[test]
    fn eviction_drops_least_recently_used_when_budget_exceeded() {
        let mut cache = PageCache::new(25);
        // Insert page 0..=3 with cost 10 each; budget 25 fits two entries + spare.
        cache.insert(key(0), 0, 10);
        cache.insert(key(1), 1, 10);
        cache.insert(key(2), 2, 10); // evicts page 0 (oldest)
        assert_eq!(cache.get(key(0)), None);
        assert_eq!(cache.get(key(1)), Some(&1));
        assert_eq!(cache.get(key(2)), Some(&2));
    }

    #[test]
    fn getting_an_entry_promotes_it_out_of_eviction() {
        let mut cache = PageCache::new(25);
        cache.insert(key(0), 0, 10);
        cache.insert(key(1), 1, 10);
        let _ = cache.get(key(0)); // page 0 is now most recently used
        cache.insert(key(2), 2, 10); // evicts page 1
        assert_eq!(cache.get(key(0)), Some(&0));
        assert_eq!(cache.get(key(1)), None);
        assert_eq!(cache.get(key(2)), Some(&2));
    }

    #[test]
    fn invalidating_the_current_zoom_keeps_its_bitmaps() {
        let mut cache = PageCache::new(1000);
        cache.insert(key(0), 0, 10);
        cache.insert(CacheKey::new(0, 2.0), 2, 10);
        cache.invalidate_zoom(2.0);
        assert_eq!(cache.get(CacheKey::new(0, 2.0)), Some(&2));
        assert_eq!(cache.get(key(0)), None);
    }

    #[test]
    fn clearing_drops_every_entry() {
        let mut cache = PageCache::new(1000);
        cache.insert(key(0), 0, 10);
        cache.insert(CacheKey::new(0, 2.0), 2, 10);
        cache.clear();
        assert_eq!(cache.len(), 0);
        assert_eq!(cache.total_bytes(), 0);
        assert_eq!(cache.get(key(0)), None);
        assert_eq!(cache.get(CacheKey::new(0, 2.0)), None);
    }

    #[test]
    fn oversized_entry_is_evicted_immediately() {
        let mut cache = PageCache::new(25);
        cache.insert(key(0), 0, 100); // bigger than the whole budget
        assert_eq!(cache.get(key(0)), None);
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn invalidating_a_zoom_drops_bitmaps_rendered_at_other_zooms() {
        let mut cache = PageCache::new(1000);
        cache.insert(key(0), 0, 10);
        cache.insert(key(1), 1, 10);
        cache.insert(CacheKey::new(2, 2.0), 2, 10);
        cache.invalidate_zoom(1.0);
        assert_eq!(cache.get(key(0)), Some(&0));
        assert_eq!(cache.get(key(1)), Some(&1));
        assert_eq!(cache.get(CacheKey::new(2, 2.0)), None);
    }

    #[test]
    fn invalidating_a_zoom_nothing_was_rendered_at_empties_the_cache() {
        let mut cache = PageCache::new(1000);
        cache.insert(key(0), 0, 10);
        cache.insert(key(1), 1, 10);
        cache.invalidate_zoom(3.0); // nothing was ever rendered at 3.0
        assert_eq!(cache.len(), 0);
        assert_eq!(cache.total_bytes(), 0);
    }

    #[test]
    fn total_bytes_tracks_the_budget_consumed() {
        let mut cache = PageCache::new(1000);
        cache.insert(key(0), 0, 10);
        cache.insert(key(1), 1, 15);
        assert_eq!(cache.total_bytes(), 25);
    }

    #[test]
    fn evicted_entries_free_their_bytes() {
        let mut cache = PageCache::new(25);
        cache.insert(key(0), 0, 10);
        cache.insert(key(1), 1, 10);
        cache.insert(key(2), 2, 10); // evicts page 0
        assert_eq!(cache.total_bytes(), 20);
    }
}
