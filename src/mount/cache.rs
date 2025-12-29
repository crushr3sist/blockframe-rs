use lru::LruCache;
use std::num::NonZeroUsize;
use std::sync::Arc;
use tracing::error;

pub struct SegmentCache {
    cache: LruCache<String, Arc<Vec<u8>>>,
    max_bytes: usize,
    current_bytes: usize,
}

#[derive(Debug)]
pub struct CacheStats {
    pub items: usize,
    pub bytes: usize,
    pub max_bytes: usize,
}

impl SegmentCache {
    pub fn new(capacity: usize) -> Self {
        let item_capacity = NonZeroUsize::new(capacity).expect("Cache capacity cannot be zero");
        Self {
            cache: LruCache::new(item_capacity),
            max_bytes: usize::MAX,
            current_bytes: 0,
        }
    }

    pub fn new_with_limits(capacity: usize, max_bytes: usize) -> Self {
        let item_capacity = NonZeroUsize::new(capacity).expect("Cache capacity cannot be zero");
        Self {
            cache: LruCache::new(item_capacity),
            max_bytes,
            current_bytes: 0,
        }
    }
    /// Zero-Copy and eviction safe getter for cache.
    pub fn get(&mut self, key: &str) -> Option<Arc<Vec<u8>>> {
        // when the cache is being accessed, we're actually returning an arc.
        // this is done so that the data which is returned is a reference to the data inside of the arc lrucache store.
        // since our lru-cache is a complex data structure (hashmap + linked list).
        self.cache.get(key).cloned()
    }

    pub fn put(&mut self, key: String, value: Arc<Vec<u8>>) {
        let value_size = value.len();

        // evict old entries until we have space for a new segment
        while self.current_bytes + value_size > self.max_bytes && !self.cache.is_empty() {
            if let Some((_, evicted_value)) = self.cache.pop_lru() {
                self.current_bytes -= evicted_value.len();
                // we cant actually free the memory if other arcs exist,
                // but we can just get rid of them from out accounting
            }
        }
        // if in some insane case we have a set size thats really small,
        // then just limit putting it in
        if value_size > self.max_bytes {
            error!(
                "Warning: segment size ({} bytes) exceeds cache limit ({} bytes)",
                value_size, self.max_bytes
            )
        }
        self.cache.put(key, value);
        self.current_bytes += value_size;
    }
    // NEW: Get current cache stats

    pub fn stats(&self) -> CacheStats {
        CacheStats {
            items: self.cache.len(),
            bytes: self.current_bytes,
            max_bytes: self.max_bytes,
        }
    }

    pub fn get_or_fetch<F>(
        &mut self,
        filename: &str,
        segment_id: usize,
        fetch: F,
    ) -> Result<Arc<Vec<u8>>, Box<dyn std::error::Error>>
    where
        F: FnOnce() -> Result<Vec<u8>, Box<dyn std::error::Error>>,
    {
        let key = format!("{}:{}", filename, segment_id);
        if let Some(data) = self.cache.get(&key) {
            return Ok(data.clone());
        }
        let data = Arc::new(fetch()?);
        self.cache.put(key, data.clone());
        Ok(data)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_eviction() {
        let mut cache = SegmentCache::new_with_limits(10, 100); // 10 items, 100 byte limit

        // Insert 50 byte segment
        cache.put("seg1".to_string(), Arc::new(vec![0u8; 50]));
        assert_eq!(cache.stats().bytes, 50);

        // Insert another 50 byte segment
        cache.put("seg2".to_string(), Arc::new(vec![0u8; 50]));
        assert_eq!(cache.stats().bytes, 100);

        // Insert 60 byte segment - should evict seg1
        cache.put("seg3".to_string(), Arc::new(vec![0u8; 60]));
        assert!(cache.stats().bytes <= 100);
        assert!(cache.get("seg1").is_none()); // seg1 was evicted
    }
}
