use thiserror::Error;

/// Daemon error types
#[derive(Debug, Error)]
pub enum DaemonError {
    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Module error: {0}")]
    Module(String),

    #[error("Message bus error: {0}")]
    Bus(String),

    #[error("Storage error: {0}")]
    Storage(String),

    #[error("Protocol error: {0}")]
    Protocol(String),

    #[error("IPC error: {0}")]
    Ipc(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Internal error: {0}")]
    Internal(String),
}

pub type Result<T> = std::result::Result<T, DaemonError>;

/// Module-specific error codes (0xxx: Daemon, 1xxx: calculator, etc.)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    // Daemon common errors (0000-0999)
    UnknownCommand = 1,
    InvalidFormat = 2,
    ModuleNotFound = 3,

    // Calculator module errors (1000-1999)
    CalculatorInvalidInput = 1001,
    CalculatorOverflow = 1002,
    CalculatorTimeout = 1003,

    // Logger module errors (2000-2999)
    LoggerFileNotFound = 2001,
    LoggerPermissionDenied = 2002,

    // Monitor module errors (3000-3999)
    // ... (can be extended)
}

impl ErrorCode {
    pub fn as_u32(self) -> u32 {
        self as u32
    }
}
