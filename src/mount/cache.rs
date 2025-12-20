use lru::LruCache;
use std::num::NonZeroUsize;
use std::sync::Arc;

pub struct SegmentCache {
    cache: LruCache<String, Arc<Vec<u8>>>,
}

impl SegmentCache {
    pub fn new(capacity: usize) -> Self {
        Self {
            cache: LruCache::new(NonZeroUsize::new(capacity).unwrap()),
        }
    }

    pub fn get(&mut self, key: &str) -> Option<Arc<Vec<u8>>> {
        self.cache.get(key).cloned()
    }

    pub fn put(&mut self, key: String, value: Arc<Vec<u8>>) {
        self.cache.put(key, value);
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
