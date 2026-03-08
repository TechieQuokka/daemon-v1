use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Message sent through the bus
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusMessage {
    pub id: u64,
    pub topic: String,
    pub payload: Value,
    pub timestamp: u64,
    pub source: MessageSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MessageSource {
    Module { id: String },
    Controller,
    System,
}

impl BusMessage {
    pub fn new(topic: String, payload: Value, source: MessageSource) -> Self {
        use std::time::{SystemTime, UNIX_EPOCH};

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let id = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);

        Self {
            id,
            topic,
            payload,
            timestamp,
            source,
        }
    }
}

/// Bus configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusConfig {
    pub max_events: usize,
}

impl Default for BusConfig {
    fn default() -> Self {
        Self { max_events: 10000 }
    }
}
