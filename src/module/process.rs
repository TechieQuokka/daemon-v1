use crate::error::{DaemonError, Result};
use crate::protocol::{DaemonToModule, ModuleToDaemon};
use futures::{SinkExt, StreamExt};
use std::path::PathBuf;
use std::process::Stdio;
use tokio::process::{Child, Command};
use tokio::sync::mpsc;
use tokio_util::codec::{FramedRead, FramedWrite, LinesCodec};

/// Module process wrapper
pub struct ModuleProcess {
    pub id: String,
    pub path: PathBuf,
    child: Child,
    pub(crate) to_module_tx: mpsc::UnboundedSender<DaemonToModule>,
    from_module_rx: mpsc::UnboundedReceiver<ModuleToDaemon>,
}

impl ModuleProcess {
    /// Spawn a new module process
    pub async fn spawn(id: String, path: PathBuf, config: serde_json::Value) -> Result<Self> {
        let mut child = Command::new(&path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            // Use inherit instead of piped to prevent stderr pipe buffer from filling up.
            // When piped, the OS pipe buffer (~64KB) fills up if Daemon doesn't read stderr,
            // causing the module process to block on stderr writes.
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| DaemonError::Module(format!("Failed to spawn module: {}", e)))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| DaemonError::Module("Failed to capture stdin".to_string()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| DaemonError::Module("Failed to capture stdout".to_string()))?;

        // Setup stdin writer (Daemon -> Module)
        let (to_module_tx, mut to_module_rx) = mpsc::unbounded_channel::<DaemonToModule>();
        tokio::spawn(async move {
            let mut writer = FramedWrite::new(stdin, LinesCodec::new());
            while let Some(msg) = to_module_rx.recv().await {
                if let Ok(json) = serde_json::to_string(&msg) {
                    if let Err(e) = writer.send(json).await {
                        tracing::error!("Failed to send to module: {}", e);
                        break;
                    }
                }
            }
        });

        // Setup stdout reader (Module -> Daemon)
        let (from_module_tx, from_module_rx) = mpsc::unbounded_channel::<ModuleToDaemon>();
        tokio::spawn(async move {
            let mut reader = FramedRead::new(stdout, LinesCodec::new());
            while let Some(line) = reader.next().await {
                match line {
                    Ok(json) => {
                        match serde_json::from_str::<ModuleToDaemon>(&json) {
                            Ok(msg) => {
                                if from_module_tx.send(msg).is_err() {
                                    break;
                                }
                            }
                            Err(e) => {
                                tracing::warn!("Failed to parse module message: {}", e);
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!("Failed to read from module: {}", e);
                        break;
                    }
                }
            }
        });

        // Send init message
        let init_msg = DaemonToModule::Init {
            module_name: id.clone(),
            config,
        };
        to_module_tx
            .send(init_msg)
            .map_err(|e| DaemonError::Module(format!("Failed to send init: {}", e)))?;

        Ok(Self {
            id,
            path,
            child,
            to_module_tx,
            from_module_rx,
        })
    }

    /// Send message to module
    pub fn send(&self, msg: DaemonToModule) -> Result<()> {
        self.to_module_tx
            .send(msg)
            .map_err(|e| DaemonError::Module(format!("Failed to send message: {}", e)))
    }

    /// Receive message from module
    pub async fn recv(&mut self) -> Option<ModuleToDaemon> {
        self.from_module_rx.recv().await
    }

    /// Shutdown module gracefully
    pub async fn shutdown(&mut self, timeout_ms: u64) -> Result<()> {
        let shutdown_msg = DaemonToModule::Shutdown {
            force: false,
            timeout: Some(timeout_ms),
        };
        self.send(shutdown_msg)?;

        // Wait for process to exit
        tokio::time::timeout(
            std::time::Duration::from_millis(timeout_ms),
            self.child.wait(),
        )
        .await
        .map_err(|_| DaemonError::Module("Shutdown timeout".to_string()))?
        .map_err(|e| DaemonError::Module(format!("Wait failed: {}", e)))?;

        Ok(())
    }

    /// Force kill module
    pub async fn kill(&mut self) -> Result<()> {
        self.child
            .kill()
            .await
            .map_err(|e| DaemonError::Module(format!("Kill failed: {}", e)))
    }

    /// Get process ID
    pub fn pid(&self) -> Option<u32> {
        self.child.id()
    }
}
