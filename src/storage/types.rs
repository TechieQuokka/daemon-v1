use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Configuration for data layer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    /// Maximum number of keys before eviction
    pub max_keys: usize,
    /// Path to shared directory for large files
    pub data_layer_path: String,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            max_keys: 10000,
            data_layer_path: "/data_layer".to_string(),
        }
    }
}

/// Data entry (can be inline value or file reference)
#[derive(Debug, Clone)]
pub enum DataEntry {
    Inline(Value),
    File(String),
}
