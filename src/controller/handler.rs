use crate::bus::{BusMessage, MessageBus, MessageSource};
use crate::module::ModuleRegistry;
use crate::protocol::{actions, ControllerRequest, ControllerResponse};
use crate::storage::DataLayer;
use serde_json::{json, Value};
use std::path::PathBuf;

/// Command handler for Controller requests
pub struct CommandHandler {
    bus: MessageBus,
    data_layer: DataLayer,
    module_registry: ModuleRegistry,
}

impl CommandHandler {
    pub fn new(bus: MessageBus, data_layer: DataLayer, module_registry: ModuleRegistry) -> Self {
        Self {
            bus,
            data_layer,
            module_registry,
        }
    }

    /// Handle a controller request
    pub async fn handle(&self, request: ControllerRequest) -> ControllerResponse {
        let result = match request.action.as_str() {
            actions::MODULE_START => self.handle_module_start(request.params).await,
            actions::MODULE_STOP => self.handle_module_stop(request.params).await,
            actions::MODULE_LIST => self.handle_module_list().await,
            actions::HEALTH_CHECK => self.handle_health_check(request.params).await,
            actions::DATA_GET => self.handle_data_get(request.params).await,
            actions::DATA_SET => self.handle_data_set(request.params).await,
            actions::DATA_DELETE => self.handle_data_delete(request.params).await,
            actions::DATA_LIST => self.handle_data_list().await,
            actions::BUS_PUBLISH => self.handle_bus_publish(request.params).await,
            actions::DAEMON_STATUS => self.handle_daemon_status().await,
            _ => Err(format!("Unknown action: {}", request.action)),
        };

        match result {
            Ok(value) => ControllerResponse::success(request.id, value),
            Err(error) => ControllerResponse::error(request.id, error),
        }
    }

    async fn handle_module_start(&self, params: Option<Value>) -> Result<Value, String> {
        let params = params.ok_or("Missing parameters")?;
        let name = params["name"]
            .as_str()
            .ok_or("Missing 'name' field")?
            .to_string();
        let path = params["path"]
            .as_str()
            .ok_or("Missing 'path' field")?;
        let config = params["config"].clone();

        let module_id = self
            .module_registry
            .start_module(name.clone(), PathBuf::from(path), config)
            .await
            .map_err(|e| e.to_string())?;

        Ok(json!({ "module_id": module_id }))
    }

    async fn handle_module_stop(&self, params: Option<Value>) -> Result<Value, String> {
        let params = params.ok_or("Missing parameters")?;
        let id = params["id"].as_str().ok_or("Missing 'id' field")?;
        let timeout = params["timeout"].as_u64().unwrap_or(5000);

        self.module_registry
            .stop_module(id, timeout)
            .await
            .map_err(|e| e.to_string())?;

        Ok(json!({ "status": "stopped" }))
    }

    async fn handle_module_list(&self) -> Result<Value, String> {
        let modules = self.module_registry.list_modules().await;
        Ok(json!({ "modules": modules }))
    }

    async fn handle_health_check(&self, params: Option<Value>) -> Result<Value, String> {
        let params = params.ok_or("Missing parameters")?;
        let module_id = params["module"].as_str().ok_or("Missing 'module' field")?;

        let info = self
            .module_registry
            .get_info(module_id)
            .await
            .ok_or_else(|| format!("Module '{}' not found", module_id))?;

        Ok(json!({
            "module_id": module_id,
            "status": info.status,
            "pid": info.pid
        }))
    }

    async fn handle_data_get(&self, params: Option<Value>) -> Result<Value, String> {
        let params = params.ok_or("Missing parameters")?;
        let key = params["key"].as_str().ok_or("Missing 'key' field")?;

        let entry = self.data_layer.get(key).map_err(|e| e.to_string())?;

        match entry {
            Some(crate::storage::DataEntry::Inline(value)) => {
                Ok(json!({ "key": key, "value": value }))
            }
            Some(crate::storage::DataEntry::File(path)) => {
                Ok(json!({ "key": key, "path": path }))
            }
            None => Ok(json!({ "key": key, "value": null })),
        }
    }

    async fn handle_data_set(&self, params: Option<Value>) -> Result<Value, String> {
        let params = params.ok_or("Missing parameters")?;
        let key = params["key"]
            .as_str()
            .ok_or("Missing 'key' field")?
            .to_string();

        if let Some(path) = params.get("path").and_then(|v| v.as_str()) {
            self.data_layer
                .set_file(key.clone(), path.to_string())
                .map_err(|e| e.to_string())?;
        } else if let Some(value) = params.get("value") {
            self.data_layer
                .set(key.clone(), value.clone())
                .map_err(|e| e.to_string())?;
        } else {
            return Err("Missing 'value' or 'path' field".to_string());
        }

        Ok(json!({ "key": key, "status": "set" }))
    }

    async fn handle_data_delete(&self, params: Option<Value>) -> Result<Value, String> {
        let params = params.ok_or("Missing parameters")?;
        let key = params["key"].as_str().ok_or("Missing 'key' field")?;

        let deleted = self.data_layer.delete(key).map_err(|e| e.to_string())?;

        Ok(json!({
            "key": key,
            "deleted": deleted.is_some()
        }))
    }

    async fn handle_data_list(&self) -> Result<Value, String> {
        let keys = self.data_layer.list_keys().map_err(|e| e.to_string())?;
        Ok(json!({ "keys": keys }))
    }

    async fn handle_bus_publish(&self, params: Option<Value>) -> Result<Value, String> {
        let params = params.ok_or("Missing parameters")?;
        let topic = params["topic"]
            .as_str()
            .ok_or("Missing 'topic' field")?
            .to_string();
        let data = params.get("data").cloned().unwrap_or(json!({}));

        let message = BusMessage::new(topic, data, MessageSource::Controller);
        self.bus.publish(message).await.map_err(|e| e.to_string())?;

        Ok(json!({ "status": "published" }))
    }

    async fn handle_daemon_status(&self) -> Result<Value, String> {
        let module_count = self.module_registry.count().await;
        let subscriber_count = self.bus.subscriber_count().await;
        let data_keys = self.data_layer.len().map_err(|e| e.to_string())?;

        Ok(json!({
            "modules": module_count,
            "subscribers": subscriber_count,
            "data_keys": data_keys,
            "status": "running"
        }))
    }
}
