use serde_json::Value;
use std::collections::{HashMap, VecDeque};

/// SIEVE cache eviction algorithm (NSDI'24)
///
/// SIEVE is a simple and efficient eviction algorithm that improves upon LRU.
/// It uses a single "visited" bit per entry and a hand pointer that sweeps
/// through entries, evicting those with visited=false.
pub struct SieveCache {
    /// Data storage
    data: HashMap<String, CacheEntry>,
    /// Queue of keys (insertion order)
    queue: VecDeque<String>,
    /// Current eviction pointer
    hand: usize,
    /// Maximum capacity
    capacity: usize,
}

struct CacheEntry {
    value: Value,
    visited: bool,
}

impl SieveCache {
    pub fn new(capacity: usize) -> Self {
        Self {
            data: HashMap::with_capacity(capacity),
            queue: VecDeque::with_capacity(capacity),
            hand: 0,
            capacity,
        }
    }

    /// Get value and mark as visited
    pub fn get(&mut self, key: &str) -> Option<&Value> {
        self.data.get_mut(key).map(|entry| {
            entry.visited = true;
            &entry.value
        })
    }

    /// Insert or update value
    pub fn insert(&mut self, key: String, value: Value) {
        if self.data.contains_key(&key) {
            // Update existing
            if let Some(entry) = self.data.get_mut(&key) {
                entry.value = value;
                entry.visited = true;
            }
        } else {
            // New insertion - evict if at capacity
            if self.data.len() >= self.capacity {
                self.evict_one();
            }

            self.data.insert(
                key.clone(),
                CacheEntry {
                    value,
                    visited: false,
                },
            );
            self.queue.push_back(key);
        }
    }

    /// Remove specific key
    pub fn remove(&mut self, key: &str) -> Option<Value> {
        self.data.remove(key).map(|entry| {
            // Remove from queue (linear scan)
            if let Some(pos) = self.queue.iter().position(|k| k == key) {
                self.queue.remove(pos);
                // Adjust hand if needed
                if self.hand > pos {
                    self.hand = self.hand.saturating_sub(1);
                }
            }
            entry.value
        })
    }

    /// SIEVE eviction: sweep through queue, evict first non-visited entry
    fn evict_one(&mut self) {
        if self.queue.is_empty() {
            return;
        }

        // Wrap hand around queue
        if self.hand >= self.queue.len() {
            self.hand = 0;
        }

        // Sweep until we find an entry with visited=false
        let mut swept = 0;
        let queue_len = self.queue.len();

        while swept < queue_len {
            let key = &self.queue[self.hand];

            if let Some(entry) = self.data.get_mut(key) {
                if entry.visited {
                    // Reset visited bit and move hand
                    entry.visited = false;
                    self.hand = (self.hand + 1) % self.queue.len();
                } else {
                    // Evict this entry
                    let key_to_remove = self.queue.remove(self.hand).unwrap();
                    self.data.remove(&key_to_remove);
                    // Hand stays at same position (points to next entry)
                    if self.hand >= self.queue.len() && !self.queue.is_empty() {
                        self.hand = 0;
                    }
                    return;
                }
            } else {
                // Entry not in data (shouldn't happen)
                self.queue.remove(self.hand);
                if self.hand >= self.queue.len() && !self.queue.is_empty() {
                    self.hand = 0;
                }
                return;
            }

            swept += 1;
        }

        // All entries are visited - evict at hand
        if !self.queue.is_empty() {
            let key_to_remove = self.queue.remove(self.hand).unwrap();
            self.data.remove(&key_to_remove);
            if self.hand >= self.queue.len() && !self.queue.is_empty() {
                self.hand = 0;
            }
        }
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    pub fn contains_key(&self, key: &str) -> bool {
        self.data.contains_key(key)
    }

    pub fn keys(&self) -> impl Iterator<Item = &String> {
        self.data.keys()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_operations() {
        let mut cache = SieveCache::new(3);

        cache.insert("a".to_string(), serde_json::json!(1));
        cache.insert("b".to_string(), serde_json::json!(2));
        cache.insert("c".to_string(), serde_json::json!(3));

        assert_eq!(cache.len(), 3);
        assert_eq!(cache.get("a"), Some(&serde_json::json!(1)));
        assert_eq!(cache.get("b"), Some(&serde_json::json!(2)));
    }

    #[test]
    fn test_eviction() {
        let mut cache = SieveCache::new(3);

        // Fill cache
        cache.insert("a".to_string(), serde_json::json!(1));
        cache.insert("b".to_string(), serde_json::json!(2));
        cache.insert("c".to_string(), serde_json::json!(3));

        // Access 'a' and 'b' (mark as visited)
        cache.get("a");
        cache.get("b");

        // Insert 'd' - should evict 'c' (not visited)
        cache.insert("d".to_string(), serde_json::json!(4));

        assert_eq!(cache.len(), 3);
        assert!(cache.contains_key("a"));
        assert!(cache.contains_key("b"));
        assert!(cache.contains_key("d"));
        assert!(!cache.contains_key("c"));
    }

    #[test]
    fn test_update_existing() {
        let mut cache = SieveCache::new(2);

        cache.insert("a".to_string(), serde_json::json!(1));
        cache.insert("a".to_string(), serde_json::json!(100));

        assert_eq!(cache.len(), 1);
        assert_eq!(cache.get("a"), Some(&serde_json::json!(100)));
    }

    #[test]
    fn test_remove() {
        let mut cache = SieveCache::new(3);

        cache.insert("a".to_string(), serde_json::json!(1));
        cache.insert("b".to_string(), serde_json::json!(2));

        let removed = cache.remove("a");
        assert_eq!(removed, Some(serde_json::json!(1)));
        assert_eq!(cache.len(), 1);
        assert!(!cache.contains_key("a"));
    }

    #[test]
    fn test_all_visited_eviction() {
        let mut cache = SieveCache::new(2);

        cache.insert("a".to_string(), serde_json::json!(1));
        cache.insert("b".to_string(), serde_json::json!(2));

        // Access both (mark visited)
        cache.get("a");
        cache.get("b");

        // Insert 'c' - should still evict one (after resetting visited bits)
        cache.insert("c".to_string(), serde_json::json!(3));

        assert_eq!(cache.len(), 2);
        assert!(cache.contains_key("c"));
    }
}
