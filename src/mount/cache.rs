use lru::LruCache;
use std::num::NonZeroUsize;

pub struct SegmentCache {
    cache: LruCache<String, Vec<u8>>,
}

impl SegmentCache {
    pub fn new(capacity: usize) -> Self {
        Self {
            cache: LruCache::new(NonZeroUsize::new(capacity).unwrap()),
        }
    }
    pub fn get_or_fetch<F>(
        &mut self,
        filename: &str,
        segment_id: usize,
        fetch: F,
    ) -> Result<Vec<u8>, Box<dyn std::error::Error>>
    where
        F: FnOnce() -> Result<Vec<u8>, Box<dyn std::error::Error>>,
    {
        let key = format!("{}:{}", filename, segment_id);
        if let Some(data) = self.cache.get(&key) {
            return Ok(data.clone());
        }
        let data = fetch()?;
        self.cache.put(key, data.clone());
        Ok(data)
    }
}
