use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Messages sent from Daemon to Module (stdin)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum DaemonToModule {
    /// Initialization message with module configuration
    Init {
        module_name: String,
        config: Value,
    },

    /// Command from Controller (free-form payload)
    Command {
        id: String,
        #[serde(flatten)]
        payload: Value,
    },

    /// Event from Message Bus
    Event {
        topic: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        data: Option<Value>,
        publisher: String,
        timestamp: u64,
    },

    /// Shutdown request
    Shutdown {
        #[serde(default)]
        force: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        timeout: Option<u64>,
    },

    /// Data read response
    DataResponse {
        key: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        value: Option<Value>,
        #[serde(skip_serializing_if = "Option::is_none")]
        path: Option<String>,
    },
}

/// Messages sent from Module to Daemon (stdout)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ModuleToDaemon {
    /// Acknowledgment of command receipt
    Ack {
        id: String,
    },

    /// Error response
    Error {
        id: String,
        code: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        message: Option<String>,
    },

    /// Publish event to bus
    Publish {
        topic: String,
        metadata: Value,
    },

    /// Subscribe to topics (wildcard support)
    SubscribeRequest {
        topic: String,
    },

    /// Unsubscribe from topics
    UnsubscribeRequest {
        topic: String,
    },

    /// Write data (inline)
    DataWrite {
        key: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        value: Option<Value>,
        #[serde(skip_serializing_if = "Option::is_none")]
        path: Option<String>,
    },

    /// Read data request
    DataRead {
        key: String,
    },

    /// Delete data
    DataDelete {
        key: String,
    },

    /// Log message
    Log {
        message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        level: Option<LogLevel>,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl Default for LogLevel {
    fn default() -> Self {
        Self::Info
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_daemon_to_module_serialization() {
        let init = DaemonToModule::Init {
            module_name: "calculator".to_string(),
            config: serde_json::json!({"data_layer_path": "/data"}),
        };
        let json = serde_json::to_string(&init).unwrap();
        assert!(json.contains(r#""cmd":"init""#));

        let deserialized: DaemonToModule = serde_json::from_str(&json).unwrap();
        assert!(matches!(deserialized, DaemonToModule::Init { .. }));
    }

    #[test]
    fn test_module_to_daemon_serialization() {
        let ack = ModuleToDaemon::Ack {
            id: "req-123".to_string(),
        };
        let json = serde_json::to_string(&ack).unwrap();
        assert!(json.contains(r#""type":"ack""#));

        let error = ModuleToDaemon::Error {
            id: "req-456".to_string(),
            code: 1002,
            message: Some("overflow".to_string()),
        };
        let json = serde_json::to_string(&error).unwrap();
        let deserialized: ModuleToDaemon = serde_json::from_str(&json).unwrap();
        assert!(matches!(deserialized, ModuleToDaemon::Error { code: 1002, .. }));
    }

    #[test]
    fn test_command_free_form() {
        let cmd = DaemonToModule::Command {
            id: "req-789".to_string(),
            payload: serde_json::json!({
                "action": "calculate",
                "n": 30
            }),
        };
        let json = serde_json::to_string(&cmd).unwrap();
        assert!(json.contains(r#""cmd":"command""#));
        assert!(json.contains(r#""action":"calculate""#));
    }
}
