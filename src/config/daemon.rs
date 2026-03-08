use crate::bus::BusConfig;
use crate::storage::StorageConfig;
use serde::{Deserialize, Serialize};

/// Daemon configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonConfig {
    /// IPC server address (e.g., "127.0.0.1:9000")
    #[serde(default = "default_ipc_address")]
    pub ipc_address: String,

    /// Message bus configuration
    #[serde(default)]
    pub bus: BusConfig,

    /// Data layer configuration
    #[serde(default)]
    pub storage: StorageConfig,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            ipc_address: default_ipc_address(),
            bus: BusConfig::default(),
            storage: StorageConfig::default(),
        }
    }
}

fn default_ipc_address() -> String {
    "127.0.0.1:9000".to_string()
}

impl DaemonConfig {
    /// Load configuration from TOML file
    pub fn from_file(path: &str) -> crate::Result<Self> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| crate::error::DaemonError::Config(format!("Failed to read config: {}", e)))?;

        let config: Self = toml::from_str(&content)
            .map_err(|e| crate::error::DaemonError::Config(format!("Failed to parse config: {}", e)))?;

        Ok(config)
    }

    /// Save configuration to TOML file
    pub fn to_file(&self, path: &str) -> crate::Result<()> {
        let content = toml::to_string_pretty(self)
            .map_err(|e| crate::error::DaemonError::Config(format!("Failed to serialize config: {}", e)))?;

        std::fs::write(path, content)
            .map_err(|e| crate::error::DaemonError::Config(format!("Failed to write config: {}", e)))?;

        Ok(())
    }
}
