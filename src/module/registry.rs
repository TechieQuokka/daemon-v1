use super::process::ModuleProcess;
use crate::error::{DaemonError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Module status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ModuleStatus {
    Starting,
    Running,
    Stopping,
    Stopped,
    Crashed { reason: String },
}

/// Module information
#[derive(Debug, Clone, Serialize)]
pub struct ModuleInfo {
    pub id: String,
    pub path: PathBuf,
    pub status: ModuleStatus,
    pub pid: Option<u32>,
    pub subscriptions: Vec<String>,
}

/// Module registry for managing active modules
pub struct ModuleRegistry {
    modules: Arc<RwLock<HashMap<String, ModuleProcess>>>,
    info: Arc<RwLock<HashMap<String, ModuleInfo>>>,
}

impl ModuleRegistry {
    pub fn new() -> Self {
        Self {
            modules: Arc::new(RwLock::new(HashMap::new())),
            info: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Start a new module
    pub async fn start_module(
        &self,
        id: String,
        path: PathBuf,
        config: serde_json::Value,
    ) -> Result<String> {
        // Check if module already exists
        {
            let modules = self.modules.read().await;
            if modules.contains_key(&id) {
                return Err(DaemonError::Module(format!(
                    "Module '{}' already running",
                    id
                )));
            }
        }

        // Update info to Starting
        {
            let mut info = self.info.write().await;
            info.insert(
                id.clone(),
                ModuleInfo {
                    id: id.clone(),
                    path: path.clone(),
                    status: ModuleStatus::Starting,
                    pid: None,
                    subscriptions: Vec::new(),
                },
            );
        }

        // Spawn process
        let process = ModuleProcess::spawn(id.clone(), path.clone(), config).await?;
        let pid = process.pid();

        // Update info to Running
        {
            let mut info = self.info.write().await;
            if let Some(module_info) = info.get_mut(&id) {
                module_info.status = ModuleStatus::Running;
                module_info.pid = pid;
            }
        }

        // Store process
        {
            let mut modules = self.modules.write().await;
            modules.insert(id.clone(), process);
        }

        tracing::info!("Module '{}' started (PID: {:?})", id, pid);
        Ok(id)
    }

    /// Stop a module
    pub async fn stop_module(&self, id: &str, timeout_ms: u64) -> Result<()> {
        // Update status to Stopping
        {
            let mut info = self.info.write().await;
            if let Some(module_info) = info.get_mut(id) {
                module_info.status = ModuleStatus::Stopping;
            }
        }

        // Get and remove process
        let mut process = {
            let mut modules = self.modules.write().await;
            modules
                .remove(id)
                .ok_or_else(|| DaemonError::Module(format!("Module '{}' not found", id)))?
        };

        // Shutdown
        if let Err(e) = process.shutdown(timeout_ms).await {
            tracing::warn!("Graceful shutdown failed for '{}': {}", id, e);
            process.kill().await?;
        }

        // Update status to Stopped
        {
            let mut info = self.info.write().await;
            if let Some(module_info) = info.get_mut(id) {
                module_info.status = ModuleStatus::Stopped;
                module_info.pid = None;
            }
        }

        tracing::info!("Module '{}' stopped", id);
        Ok(())
    }

    /// Send message to module
    pub async fn send_to_module(&self, id: &str, msg: crate::protocol::DaemonToModule) -> Result<()> {
        let modules = self.modules.read().await;
        let process = modules
            .get(id)
            .ok_or_else(|| DaemonError::Module(format!("Module '{}' not found", id)))?;
        process.send(msg)
    }

    /// List all modules
    pub async fn list_modules(&self) -> Vec<ModuleInfo> {
        let info = self.info.read().await;
        info.values().cloned().collect()
    }

    /// Get module info
    pub async fn get_info(&self, id: &str) -> Option<ModuleInfo> {
        let info = self.info.read().await;
        info.get(id).cloned()
    }

    /// Update module subscriptions
    pub async fn update_subscriptions(&self, id: &str, subscriptions: Vec<String>) -> Result<()> {
        let mut info = self.info.write().await;
        let module_info = info
            .get_mut(id)
            .ok_or_else(|| DaemonError::Module(format!("Module '{}' not found", id)))?;
        module_info.subscriptions = subscriptions;
        Ok(())
    }

    /// Mark module as crashed
    pub async fn mark_crashed(&self, id: &str, reason: String) -> Result<()> {
        let mut info = self.info.write().await;
        let module_info = info
            .get_mut(id)
            .ok_or_else(|| DaemonError::Module(format!("Module '{}' not found", id)))?;
        module_info.status = ModuleStatus::Crashed { reason };
        module_info.pid = None;
        Ok(())
    }

    /// Get module count
    pub async fn count(&self) -> usize {
        let modules = self.modules.read().await;
        modules.len()
    }
}

impl Default for ModuleRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for ModuleRegistry {
    fn clone(&self) -> Self {
        Self {
            modules: self.modules.clone(),
            info: self.info.clone(),
        }
    }
}
