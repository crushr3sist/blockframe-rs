use moka::sync::Cache;
use std::sync::Arc;
use std::time::Duration;

pub struct SegmentCache {
    // Moka handles thread safety, eviction, and weighing internally.
    // No manual byte tracking, no manual eviction loops, just works.
    cache: Cache<String, Arc<Vec<u8>>>,
    max_bytes: u64,
}

#[derive(Debug)]
pub struct CacheStats {
    pub items: u64,
    pub bytes: u64,
    pub max_bytes: u64,
}

impl SegmentCache {
    /// New, like setting up a memory cache for speed. "Cache it," the programmer says.
    /// I'd estimate bytes from capacity. "Cached!"
    /// Creating cache is like that – capacity to bytes conversion. "Fast access!"
    /// There was this slow system, cache made it zip. Performance.
    /// Life's about speed, from memory to cache.
    pub fn new(capacity: usize) -> Self {
        // Convert item count to rough byte estimate (assuming 32MB segments)
        let max_bytes = (capacity as u64) * 32 * 1024 * 1024;
        Self::new_with_limits(max_bytes)
    }

    /// New with limits, like setting cache size limits precisely. "Exact bytes," the optimizer says.
    /// I'd build moka cache with weigher and TTL. "Limited!"
    /// Creating cache with limits is like that – size-based eviction. "Controlled!"
    /// There was this cache that grew too big, limits kept it in check. Management.
    /// Life's about limits, from resources to cache.
    pub fn new_with_limits(max_bytes: u64) -> Self {
        // W-TinyLFU cache that evicts based on SIZE (bytes) and FREQUENCY.
        // The weigher tells moka how "heavy" each item is.
        let cache = Cache::builder()
            .weigher(|_key: &String, value: &Arc<Vec<u8>>| -> u32 {
                // Each segment's weight = its size in bytes
                value.len().try_into().unwrap_or(u32::MAX)
            })
            .max_capacity(max_bytes)
            // TTL prevents stale data if files change on disk
            .time_to_live(Duration::from_secs(60 * 60)) // 1 hour
            .build();

        Self { cache, max_bytes }
    }

    /// Get, like retrieving from memory palace. "Recall it," the mnemonist says.
    /// I'd get the Arc clone, no copy. "Retrieved!"
    /// Getting from cache is like that – zero-copy access. "Efficient!"
    /// There was this memory trick that helped me remember, like cache. Association.
    /// Life's about recall, from memory to cache.
    /// Zero-copy getter. Returns Arc clone (cheap), no data copy.
    pub fn get(&self, key: &str) -> Option<Arc<Vec<u8>>> {
        // Moka's get() automatically promotes frequently accessed items.
        // Unlike LRU, one-hit wonders don't pollute the cache.
        self.cache.get(key)
    }

    /// Put, like storing in a mental warehouse. "Store it," the organizer says.
    /// I'd insert key and value. "Stored!"
    /// Putting in cache is like that – moka handles eviction. "Managed!"
    /// There was this warehouse that got cluttered, learned organization. Structure.
    /// Life's about storage, from warehouses to cache.
    pub fn put(&self, key: String, value: Arc<Vec<u8>>) {
        // No manual eviction loop needed. Moka uses W-TinyLFU to decide
        // what stays based on access frequency and recency.
        // Streaming segments (accessed once) won't evict hot metadata.
        self.cache.insert(key, value);
    }

    /// Stats, like checking inventory levels. "How much stock?" the manager asks.
    /// I'd count items and bytes. "Statistics!"
    /// Getting cache stats is like that – entry count and size. "Metrics!"
    /// There was this inventory that was off, stats helped fix it. Accuracy.
    /// Life's about counting, from inventory to cache.
    pub fn stats(&self) -> CacheStats {
        CacheStats {
            items: self.cache.entry_count(),
            bytes: self.cache.weighted_size(),
            max_bytes: self.max_bytes,
        }
    }

    /// Get or fetch, like checking pantry before shopping. "Do we have it?" I'd ask.
    /// If not, fetch it and store. "Supplied!"
    /// Get or fetch is like that – check cache, else fetch and insert. "Efficient!"
    /// There was this pantry raid that saved a trip, like cache hit. Preparedness.
    /// Life's about availability, from pantry to cache.
    pub fn get_or_fetch<F>(
        &self,
        filename: &str,
        segment_id: usize,
        fetch: F,
    ) -> Result<Arc<Vec<u8>>, Box<dyn std::error::Error>>
    where
        F: FnOnce() -> Result<Vec<u8>, Box<dyn std::error::Error>>,
    {
        let key = format!("{}:{}", filename, segment_id);

        // Check cache first
        if let Some(data) = self.cache.get(&key) {
            return Ok(data);
        }

        // Cache miss - fetch and insert
        let data = Arc::new(fetch()?);
        self.cache.insert(key, data.clone());
        Ok(data)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_eviction() {
        let cache = SegmentCache::new_with_limits(100); // 100 byte limit

        // Insert 50 byte segment
        cache.put("seg1".to_string(), Arc::new(vec![0u8; 50]));

        // Insert another 50 byte segment
        cache.put("seg2".to_string(), Arc::new(vec![0u8; 50]));

        // Insert 60 byte segment - W-TinyLFU decides what to evict
        cache.put("seg3".to_string(), Arc::new(vec![0u8; 60]));

        // Give moka time to process evictions (async internally)
        std::thread::sleep(std::time::Duration::from_millis(100));
        cache.cache.run_pending_tasks();

        assert!(cache.stats().bytes <= 100);
    }

    #[test]
    fn test_frequency_based_eviction() {
        let cache = SegmentCache::new_with_limits(100);

        // Insert hot segment and access it multiple times
        cache.put("hot".to_string(), Arc::new(vec![0u8; 40]));
        for _ in 0..10 {
            cache.get("hot");
        }

        // Insert one-hit wonder segments (like streaming video)
        cache.put("cold1".to_string(), Arc::new(vec![0u8; 40]));
        cache.put("cold2".to_string(), Arc::new(vec![0u8; 40]));

        std::thread::sleep(std::time::Duration::from_millis(100));
        cache.cache.run_pending_tasks();

        // W-TinyLFU should keep the frequently accessed "hot" item
        assert!(cache.get("hot").is_some());
    }
}
