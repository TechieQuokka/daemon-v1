use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Controller → Daemon request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControllerRequest {
    pub action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
    pub id: String,
}

/// Daemon → Controller response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControllerResponse {
    pub id: String,
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl ControllerResponse {
    pub fn success(id: String, result: Value) -> Self {
        Self {
            id,
            success: true,
            result: Some(result),
            error: None,
        }
    }

    pub fn error(id: String, error: String) -> Self {
        Self {
            id,
            success: false,
            result: None,
            error: Some(error),
        }
    }
}

/// Standard actions
pub mod actions {
    // Module management
    pub const MODULE_START: &str = "module.start";
    pub const MODULE_STOP: &str = "module.stop";
    pub const MODULE_LIST: &str = "module.list";
    pub const HEALTH_CHECK: &str = "health_check";

    // Module command
    pub const MODULE_COMMAND: &str = "module.command";

    // Data layer
    pub const DATA_GET: &str = "data.get";
    pub const DATA_SET: &str = "data.set";
    pub const DATA_DELETE: &str = "data.delete";
    pub const DATA_LIST: &str = "data.list";

    // Bus
    pub const BUS_PUBLISH: &str = "bus.publish";

    // Daemon
    pub const DAEMON_STATUS: &str = "daemon.status";
    pub const DAEMON_SHUTDOWN: &str = "daemon.shutdown";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_controller_request_serialization() {
        let req = ControllerRequest {
            action: "module.start".to_string(),
            params: Some(serde_json::json!({
                "name": "calculator",
                "config": {"port": 8000}
            })),
            id: "req-1".to_string(),
        };

        let json = serde_json::to_string(&req).unwrap();
        let deserialized: ControllerRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.action, "module.start");
    }

    #[test]
    fn test_controller_response() {
        let resp = ControllerResponse::success(
            "req-1".to_string(),
            serde_json::json!({"module_id": "calc-123"}),
        );
        assert!(resp.success);
        assert!(resp.error.is_none());

        let err_resp = ControllerResponse::error(
            "req-2".to_string(),
            "Module not found".to_string(),
        );
        assert!(!err_resp.success);
        assert!(err_resp.result.is_none());
    }
}
