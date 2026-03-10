use super::process::ModuleProcess;
use super::registry::{ModuleInfo, ModuleRegistry};
use crate::bus::{BusMessage, MessageBus, MessageSource};
use crate::error::{DaemonError, Result};
use crate::protocol::{DaemonToModule, ModuleToDaemon};
use crate::storage::{DataEntry, DataLayer};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Module manager that handles module lifecycle and message processing
pub struct ModuleManager {
    registry: ModuleRegistry,
    bus: MessageBus,
    data_layer: DataLayer,
    // Track message handler tasks
    handlers: Arc<RwLock<HashMap<String, tokio::task::JoinHandle<()>>>>,
}

impl ModuleManager {
    pub fn new(bus: MessageBus, data_layer: DataLayer) -> Self {
        Self {
            registry: ModuleRegistry::new(),
            bus,
            data_layer,
            handlers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Start a new module and begin processing its messages
    pub async fn start_module(
        &self,
        id: String,
        path: PathBuf,
        config: serde_json::Value,
    ) -> Result<String> {
        // Start module process via registry
        self.registry.start_module(id.clone(), path, config).await?;

        // Spawn message handler task
        self.spawn_message_handler(id.clone()).await?;

        Ok(id)
    }

    /// Spawn a task to handle messages from a module
    async fn spawn_message_handler(&self, module_id: String) -> Result<()> {
        let bus = self.bus.clone();
        let data_layer = self.data_layer.clone();
        let registry = self.registry.clone();

        // Get process Arc from registry
        let process_arc = {
            let modules = self.registry.modules.read().await;
            modules
                .get(&module_id)
                .ok_or_else(|| DaemonError::Module(format!("Module '{}' not found", module_id)))?
                .clone()
        };

        let handler_id = module_id.clone();
        let handler = tokio::spawn(async move {
            tracing::info!("Started message handler for module '{}'", handler_id);

            loop {
                // Receive message from module
                let msg = {
                    let mut process = process_arc.lock().await;
                    process.recv().await
                };

                match msg {
                    Some(msg) => {
                        let mut process = process_arc.lock().await;
                        if let Err(e) = Self::handle_module_message(
                            &handler_id,
                            msg,
                            &bus,
                            &data_layer,
                            &registry,
                            &mut *process,
                        )
                        .await
                        {
                            tracing::error!(
                                "Error handling message from module '{}': {}",
                                handler_id,
                                e
                            );
                        }
                    }
                    None => {
                        // Module process ended
                        tracing::info!("Module '{}' process ended", handler_id);
                        break;
                    }
                }
            }

            tracing::info!("Message handler for module '{}' stopped", handler_id);
        });

        // Store handler
        let mut handlers = self.handlers.write().await;
        handlers.insert(module_id, handler);

        Ok(())
    }

    /// Validate module subscription topic
    ///
    /// Modules can only subscribe to:
    /// - system.* (system events)
    /// - {module_id}.* (own events/commands)
    ///
    /// This prevents direct module-to-module communication.
    fn validate_module_subscription(module_id: &str, topic: &str) -> Result<()> {
        // Reject empty topics
        if topic.is_empty() {
            return Err(DaemonError::Module(
                "Topic cannot be empty".to_string()
            ));
        }

        // Reject overly broad wildcards
        if topic == "*" || topic == "#" {
            return Err(DaemonError::Module(format!(
                "Module '{}' cannot subscribe to global wildcard '{}'. Use 'system.*' or '{}.*' instead.",
                module_id, topic, module_id
            )));
        }

        // Allow system events
        if topic.starts_with("system.") || topic == "system" {
            return Ok(());
        }

        // Allow module's own namespace
        let module_prefix = format!("{}.", module_id);
        if topic.starts_with(&module_prefix) || topic == module_id {
            return Ok(());
        }

        // Allow wildcard subscriptions within allowed namespaces
        if topic == "system.*" || topic == "system.#" {
            return Ok(());
        }

        let module_wildcard = format!("{}.*", module_id);
        let module_hash = format!("{}.#", module_id);
        if topic == module_wildcard || topic == module_hash {
            return Ok(());
        }

        // Reject all other topics (cross-module communication)
        Err(DaemonError::Module(format!(
            "Module '{}' cannot subscribe to topic '{}'. Modules can only subscribe to 'system.*' or '{}.*' topics.",
            module_id, topic, module_id
        )))
    }

    /// Handle a message from a module
    async fn handle_module_message(
        module_id: &str,
        msg: ModuleToDaemon,
        bus: &MessageBus,
        data_layer: &DataLayer,
        registry: &ModuleRegistry,
        process: &mut ModuleProcess,
    ) -> Result<()> {
        match msg {
            ModuleToDaemon::Ack { id } => {
                tracing::debug!("Module '{}' ACK: {}", module_id, id);
            }

            ModuleToDaemon::Error { id, code, message } => {
                tracing::warn!(
                    "Module '{}' ERROR: id={}, code={}, message={:?}",
                    module_id,
                    id,
                    code,
                    message
                );
            }

            ModuleToDaemon::Publish { topic, metadata } => {
                tracing::debug!("Module '{}' publishing to topic '{}'", module_id, topic);

                // Publish to bus
                let bus_msg = BusMessage::new(
                    topic.clone(),
                    metadata,
                    MessageSource::Module {
                        id: module_id.to_string(),
                    },
                );

                bus.publish(bus_msg).await.map_err(|e| {
                    DaemonError::Module(format!("Failed to publish to bus: {}", e))
                })?;

                tracing::info!(
                    "Module '{}' published event to topic '{}'",
                    module_id,
                    topic
                );
            }

            ModuleToDaemon::SubscribeRequest { topic } => {
                tracing::debug!("Module '{}' subscribing to topic '{}'", module_id, topic);

                // Validate subscription (prevent cross-module communication)
                if let Err(e) = Self::validate_module_subscription(module_id, &topic) {
                    tracing::warn!(
                        "Module '{}' attempted to subscribe to restricted topic '{}': {}",
                        module_id,
                        topic,
                        e
                    );
                    return Err(e);
                }

                // Subscribe to bus
                let subscriber_id = format!("module:{}", module_id);
                let mut receiver = bus
                    .subscribe(subscriber_id.clone(), topic.clone())
                    .await
                    .map_err(|e| DaemonError::Module(format!("Failed to subscribe: {}", e)))?;

                // Update registry subscriptions
                let current_subs = registry.get_info(module_id).await.map(|info| info.subscriptions).unwrap_or_default();
                let mut new_subs = current_subs;
                if !new_subs.contains(&topic) {
                    new_subs.push(topic.clone());
                }
                let _ = registry.update_subscriptions(module_id, new_subs).await;

                tracing::info!(
                    "Module '{}' subscribed to topic '{}'",
                    module_id,
                    topic
                );

                // Spawn task to forward bus events to module
                let module_id_clone = module_id.to_string();
                let process_tx = process.to_module_tx.clone();
                tokio::spawn(async move {
                    while let Some(bus_msg) = receiver.recv().await {
                        // Prevent self-subscription: Skip events published by this module
                        if let MessageSource::Module { id } = &bus_msg.source {
                            if id == &module_id_clone {
                                tracing::debug!(
                                    "Module '{}' skipping self-published event on topic '{}'",
                                    module_id_clone,
                                    bus_msg.topic
                                );
                                continue;
                            }
                        }

                        let event = DaemonToModule::Event {
                            topic: bus_msg.topic,
                            data: Some(bus_msg.payload),
                            publisher: match bus_msg.source {
                                MessageSource::Module { id } => id,
                                MessageSource::Controller => "controller".to_string(),
                                MessageSource::System => "system".to_string(),
                            },
                            timestamp: bus_msg.timestamp,
                        };

                        if let Err(e) = process_tx.send(event) {
                            tracing::error!(
                                "Failed to send event to module '{}': {}",
                                module_id_clone,
                                e
                            );
                            break;
                        }
                    }
                });
            }

            ModuleToDaemon::UnsubscribeRequest { topic } => {
                tracing::debug!(
                    "Module '{}' unsubscribing from topic '{}'",
                    module_id,
                    topic
                );

                let subscriber_id = format!("module:{}", module_id);
                bus.unsubscribe(&subscriber_id, &topic).await.map_err(|e| {
                    DaemonError::Module(format!("Failed to unsubscribe: {}", e))
                })?;

                // Update registry subscriptions
                let current_subs = registry.get_info(module_id).await.map(|info| info.subscriptions).unwrap_or_default();
                let new_subs: Vec<String> = current_subs.into_iter().filter(|t| t != &topic).collect();
                let _ = registry.update_subscriptions(module_id, new_subs).await;

                tracing::info!(
                    "Module '{}' unsubscribed from topic '{}'",
                    module_id,
                    topic
                );
            }

            ModuleToDaemon::DataWrite { key, value, path } => {
                tracing::debug!("Module '{}' writing data key '{}'", module_id, key);

                if let Some(val) = value {
                    data_layer.set(key.clone(), val).map_err(|e| {
                        DaemonError::Module(format!("Failed to write data: {}", e))
                    })?;
                } else if let Some(p) = path {
                    data_layer.set_file(key.clone(), p).map_err(|e| {
                        DaemonError::Module(format!("Failed to write file reference: {}", e))
                    })?;
                } else {
                    return Err(DaemonError::Module(
                        "DataWrite must have either value or path".to_string(),
                    ));
                }

                tracing::debug!("Module '{}' wrote data key '{}'", module_id, key);
            }

            ModuleToDaemon::DataRead { key } => {
                tracing::debug!("Module '{}' reading data key '{}'", module_id, key);

                let result = data_layer.get(&key).map_err(|e| {
                    DaemonError::Module(format!("Failed to read data: {}", e))
                })?;

                let response = match result {
                    Some(DataEntry::Inline(val)) => DaemonToModule::DataResponse {
                        key: key.clone(),
                        value: Some(val),
                        path: None,
                    },
                    Some(DataEntry::File(p)) => DaemonToModule::DataResponse {
                        key: key.clone(),
                        value: None,
                        path: Some(p),
                    },
                    None => DaemonToModule::DataResponse {
                        key: key.clone(),
                        value: None,
                        path: None,
                    },
                };

                // Send response to module
                process.send(response)?;

                tracing::debug!("Module '{}' data_response sent for key '{}'", module_id, key);
            }

            ModuleToDaemon::DataDelete { key } => {
                tracing::debug!("Module '{}' deleting data key '{}'", module_id, key);

                data_layer.delete(&key).map_err(|e| {
                    DaemonError::Module(format!("Failed to delete data: {}", e))
                })?;

                tracing::debug!("Module '{}' deleted data key '{}'", module_id, key);
            }

            ModuleToDaemon::Log { message, level } => {
                let level_str = level.map(|l| format!("{:?}", l)).unwrap_or_else(|| "info".to_string());
                tracing::info!("[Module '{}' {}] {}", module_id, level_str, message);
            }
        }

        Ok(())
    }

    /// Stop a module
    pub async fn stop_module(&self, id: &str, timeout_ms: u64) -> Result<()> {
        // Abort message handler
        {
            let mut handlers = self.handlers.write().await;
            if let Some(handler) = handlers.remove(id) {
                handler.abort();
            }
        }

        // Stop module via registry
        self.registry.stop_module(id, timeout_ms).await
    }

    /// List all modules
    pub async fn list_modules(&self) -> Vec<ModuleInfo> {
        self.registry.list_modules().await
    }

    /// Get module info
    pub async fn get_info(&self, id: &str) -> Option<ModuleInfo> {
        self.registry.get_info(id).await
    }

    /// Get module count
    pub async fn count(&self) -> usize {
        self.registry.count().await
    }

    /// Send command to a module
    pub async fn send_command(&self, module_id: &str, command_id: String, payload: serde_json::Value) -> Result<()> {
        // Get module process
        let process_arc = {
            let modules = self.registry.modules.read().await;
            modules
                .get(module_id)
                .ok_or_else(|| DaemonError::Module(format!("Module '{}' not found", module_id)))?
                .clone()
        };

        // Send command message
        let command = DaemonToModule::Command {
            id: command_id,
            payload,
        };

        let process = process_arc.lock().await;
        process.send(command)?;

        tracing::info!("Sent command to module '{}'", module_id);

        Ok(())
    }

    /// Shutdown all modules
    pub async fn shutdown_all(&self, timeout_ms: u64) {
        // Abort all handlers
        {
            let mut handlers = self.handlers.write().await;
            for (_, handler) in handlers.drain() {
                handler.abort();
            }
        }

        // Shutdown via registry
        self.registry.shutdown_all(timeout_ms).await;
    }
}

impl Clone for ModuleManager {
    fn clone(&self) -> Self {
        Self {
            registry: self.registry.clone(),
            bus: self.bus.clone(),
            data_layer: self.data_layer.clone(),
            handlers: self.handlers.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_module_subscription_allowed() {
        // System events
        assert!(ModuleManager::validate_module_subscription("fibonacci", "system.shutdown").is_ok());
        assert!(ModuleManager::validate_module_subscription("fibonacci", "system").is_ok());
        assert!(ModuleManager::validate_module_subscription("fibonacci", "system.*").is_ok());
        assert!(ModuleManager::validate_module_subscription("fibonacci", "system.#").is_ok());

        // Own namespace
        assert!(ModuleManager::validate_module_subscription("fibonacci", "fibonacci.command").is_ok());
        assert!(ModuleManager::validate_module_subscription("fibonacci", "fibonacci.result").is_ok());
        assert!(ModuleManager::validate_module_subscription("fibonacci", "fibonacci").is_ok());
        assert!(ModuleManager::validate_module_subscription("fibonacci", "fibonacci.*").is_ok());
        assert!(ModuleManager::validate_module_subscription("fibonacci", "fibonacci.#").is_ok());
    }

    #[test]
    fn test_validate_module_subscription_rejected() {
        // Cross-module communication (rejected)
        assert!(ModuleManager::validate_module_subscription("fibonacci", "calculator.command").is_err());
        assert!(ModuleManager::validate_module_subscription("fibonacci", "calculator.result").is_err());
        assert!(ModuleManager::validate_module_subscription("fibonacci", "logger.log").is_err());

        // Wildcard across modules (rejected)
        assert!(ModuleManager::validate_module_subscription("fibonacci", "calculator.*").is_err());
        assert!(ModuleManager::validate_module_subscription("fibonacci", "calculator.#").is_err());

        // Generic wildcards (rejected - too broad)
        assert!(ModuleManager::validate_module_subscription("fibonacci", "*").is_err());
        assert!(ModuleManager::validate_module_subscription("fibonacci", "#").is_err());
    }

    #[test]
    fn test_validate_module_subscription_edge_cases() {
        // Module name as prefix (rejected if not exact match or with dot)
        assert!(ModuleManager::validate_module_subscription("fib", "fibonacci.command").is_err());

        // Empty topic (rejected)
        assert!(ModuleManager::validate_module_subscription("fibonacci", "").is_err());

        // Similar but different namespace (rejected)
        assert!(ModuleManager::validate_module_subscription("fibonacci", "fibonacci_v2.command").is_err());
    }
}
