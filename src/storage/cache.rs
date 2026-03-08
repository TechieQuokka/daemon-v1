use super::sieve::SieveCache;
use super::types::{DataEntry, StorageConfig};
use crate::error::{DaemonError, Result};
use serde_json::Value;
use std::sync::{Arc, RwLock};

/// Thread-safe data layer with SIEVE eviction
#[derive(Clone)]
pub struct DataLayer {
    cache: Arc<RwLock<SieveCache>>,
    config: Arc<StorageConfig>,
}

impl DataLayer {
    pub fn new(config: StorageConfig) -> Self {
        let cache = SieveCache::new(config.max_keys);
        Self {
            cache: Arc::new(RwLock::new(cache)),
            config: Arc::new(config),
        }
    }

    /// Get value (inline or file reference)
    pub fn get(&self, key: &str) -> Result<Option<DataEntry>> {
        let mut cache = self
            .cache
            .write()
            .map_err(|e| DaemonError::Storage(format!("Lock error: {}", e)))?;

        Ok(cache.get(key).map(|value| {
            // Check if it's a file reference
            if let Some(path) = value.as_str() {
                if path.starts_with(&self.config.data_layer_path) {
                    return DataEntry::File(path.to_string());
                }
            }
            DataEntry::Inline(value.clone())
        }))
    }

    /// Set value (inline)
    pub fn set(&self, key: String, value: Value) -> Result<()> {
        let mut cache = self
            .cache
            .write()
            .map_err(|e| DaemonError::Storage(format!("Lock error: {}", e)))?;

        cache.insert(key, value);
        Ok(())
    }

    /// Set file reference
    pub fn set_file(&self, key: String, path: String) -> Result<()> {
        let mut cache = self
            .cache
            .write()
            .map_err(|e| DaemonError::Storage(format!("Lock error: {}", e)))?;

        cache.insert(key, Value::String(path));
        Ok(())
    }

    /// Delete key
    pub fn delete(&self, key: &str) -> Result<Option<Value>> {
        let mut cache = self
            .cache
            .write()
            .map_err(|e| DaemonError::Storage(format!("Lock error: {}", e)))?;

        Ok(cache.remove(key))
    }

    /// List all keys
    pub fn list_keys(&self) -> Result<Vec<String>> {
        let cache = self
            .cache
            .read()
            .map_err(|e| DaemonError::Storage(format!("Lock error: {}", e)))?;

        Ok(cache.keys().cloned().collect())
    }

    /// Get cache size
    pub fn len(&self) -> Result<usize> {
        let cache = self
            .cache
            .read()
            .map_err(|e| DaemonError::Storage(format!("Lock error: {}", e)))?;

        Ok(cache.len())
    }

    /// Check if cache is empty
    pub fn is_empty(&self) -> Result<bool> {
        let cache = self
            .cache
            .read()
            .map_err(|e| DaemonError::Storage(format!("Lock error: {}", e)))?;

        Ok(cache.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_data_layer_operations() {
        let config = StorageConfig {
            max_keys: 3,
            data_layer_path: "/data".to_string(),
        };
        let layer = DataLayer::new(config);

        // Set and get
        layer
            .set("count".to_string(), serde_json::json!(123))
            .unwrap();
        let entry = layer.get("count").unwrap().unwrap();
        match entry {
            DataEntry::Inline(val) => assert_eq!(val, serde_json::json!(123)),
            _ => panic!("Expected inline value"),
        }

        // Delete
        let deleted = layer.delete("count").unwrap();
        assert!(deleted.is_some());
        assert!(layer.get("count").unwrap().is_none());
    }

    #[test]
    fn test_file_reference() {
        let config = StorageConfig {
            max_keys: 10,
            data_layer_path: "/data".to_string(),
        };
        let layer = DataLayer::new(config);

        layer
            .set_file("bigfile".to_string(), "/data/file123.dat".to_string())
            .unwrap();

        let entry = layer.get("bigfile").unwrap().unwrap();
        match entry {
            DataEntry::File(path) => assert_eq!(path, "/data/file123.dat"),
            _ => panic!("Expected file reference"),
        }
    }

    #[test]
    fn test_eviction() {
        let config = StorageConfig {
            max_keys: 2,
            data_layer_path: "/data".to_string(),
        };
        let layer = DataLayer::new(config);

        layer.set("a".to_string(), serde_json::json!(1)).unwrap();
        layer.set("b".to_string(), serde_json::json!(2)).unwrap();

        // Access 'a' to mark as visited
        layer.get("a").unwrap();

        // Insert 'c' - should evict 'b'
        layer.set("c".to_string(), serde_json::json!(3)).unwrap();

        assert_eq!(layer.len().unwrap(), 2);
        assert!(layer.get("a").unwrap().is_some());
        assert!(layer.get("c").unwrap().is_some());
    }
}
