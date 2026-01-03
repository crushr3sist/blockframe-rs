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
    pub fn new(capacity: usize) -> Self {
        // Convert item count to rough byte estimate (assuming 32MB segments)
        let max_bytes = (capacity as u64) * 32 * 1024 * 1024;
        Self::new_with_limits(max_bytes)
    }

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

    /// Zero-copy getter. Returns Arc clone (cheap), no data copy.
    pub fn get(&self, key: &str) -> Option<Arc<Vec<u8>>> {
        // Moka's get() automatically promotes frequently accessed items.
        // Unlike LRU, one-hit wonders don't pollute the cache.
        self.cache.get(key)
    }

    pub fn put(&self, key: String, value: Arc<Vec<u8>>) {
        // No manual eviction loop needed. Moka uses W-TinyLFU to decide
        // what stays based on access frequency and recency.
        // Streaming segments (accessed once) won't evict hot metadata.
        self.cache.insert(key, value);
    }

    pub fn stats(&self) -> CacheStats {
        CacheStats {
            items: self.cache.entry_count(),
            bytes: self.cache.weighted_size(),
            max_bytes: self.max_bytes,
        }
    }

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
