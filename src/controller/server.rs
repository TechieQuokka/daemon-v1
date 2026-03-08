use crate::bus::MessageBus;
use crate::controller::handler::CommandHandler;
use crate::error::{DaemonError, Result};
use crate::module::ModuleRegistry;
use crate::protocol::{ControllerRequest, ControllerResponse};
use crate::storage::DataLayer;
use futures::{SinkExt, StreamExt};
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::RwLock;
use tokio_util::codec::{FramedRead, FramedWrite, LinesCodec};

/// IPC server for Controller communication
pub struct IpcServer {
    address: String,
    handler: Arc<CommandHandler>,
    shutdown_tx: Arc<RwLock<Option<tokio::sync::broadcast::Sender<()>>>>,
}

impl IpcServer {
    pub fn new(
        address: String,
        bus: MessageBus,
        data_layer: DataLayer,
        module_registry: ModuleRegistry,
    ) -> Self {
        let handler = Arc::new(CommandHandler::new(bus, data_layer, module_registry));
        Self {
            address,
            handler,
            shutdown_tx: Arc::new(RwLock::new(None)),
        }
    }

    /// Start the IPC server
    pub async fn start(&self) -> Result<()> {
        let listener = TcpListener::bind(&self.address)
            .await
            .map_err(|e| DaemonError::Ipc(format!("Failed to bind to {}: {}", self.address, e)))?;

        tracing::info!("IPC server listening on {}", self.address);

        let (shutdown_tx, _) = tokio::sync::broadcast::channel(1);
        {
            let mut tx = self.shutdown_tx.write().await;
            *tx = Some(shutdown_tx.clone());
        }

        loop {
            tokio::select! {
                result = listener.accept() => {
                    match result {
                        Ok((stream, addr)) => {
                            tracing::info!("New connection from {}", addr);
                            let handler = self.handler.clone();
                            let mut shutdown_rx = shutdown_tx.subscribe();
                            tokio::spawn(async move {
                                tokio::select! {
                                    _ = Self::handle_connection(stream, handler) => {},
                                    _ = shutdown_rx.recv() => {
                                        tracing::info!("Connection handler shutting down");
                                    }
                                }
                            });
                        }
                        Err(e) => {
                            tracing::error!("Failed to accept connection: {}", e);
                        }
                    }
                }
                _ = async {
                    let rx = self.shutdown_tx.read().await;
                    if let Some(tx) = rx.as_ref() {
                        let mut sub = tx.subscribe();
                        sub.recv().await.ok();
                    }
                } => {
                    tracing::info!("IPC server shutting down");
                    break;
                }
            }
        }

        Ok(())
    }

    /// Handle a single connection
    async fn handle_connection(stream: TcpStream, handler: Arc<CommandHandler>) {
        let (reader, writer) = stream.into_split();
        let mut reader = FramedRead::new(reader, LinesCodec::new());
        let mut writer = FramedWrite::new(writer, LinesCodec::new());

        while let Some(line) = reader.next().await {
            match line {
                Ok(json) => {
                    match serde_json::from_str::<ControllerRequest>(&json) {
                        Ok(request) => {
                            let response = handler.handle(request).await;
                            if let Ok(json) = serde_json::to_string(&response) {
                                if let Err(e) = writer.send(json).await {
                                    tracing::error!("Failed to send response: {}", e);
                                    break;
                                }
                            }
                        }
                        Err(e) => {
                            tracing::warn!("Failed to parse request: {}", e);
                            let error_response = ControllerResponse::error(
                                "unknown".to_string(),
                                format!("Invalid request: {}", e),
                            );
                            if let Ok(json) = serde_json::to_string(&error_response) {
                                let _ = writer.send(json).await;
                            }
                        }
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to read from connection: {}", e);
                    break;
                }
            }
        }

        tracing::info!("Connection closed");
    }

    /// Shutdown the server
    pub async fn shutdown(&self) -> Result<()> {
        let tx = self.shutdown_tx.read().await;
        if let Some(tx) = tx.as_ref() {
            let _ = tx.send(());
        }
        Ok(())
    }
}
