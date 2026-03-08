use daemon_v1::{
    bus::MessageBus,
    config::DaemonConfig,
    controller::IpcServer,
    module::ModuleRegistry,
    storage::DataLayer,
};
use tracing::Level;
use tracing_subscriber::FmtSubscriber;

#[tokio::main]
async fn main() -> daemon_v1::Result<()> {
    // Initialize logging
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber)
        .expect("setting default subscriber failed");

    tracing::info!("Starting Daemon V1");

    // Load configuration (default for now)
    let config = DaemonConfig::default();
    tracing::info!("Configuration: IPC={}, Bus.max_events={}, Storage.max_keys={}",
        config.ipc_address, config.bus.max_events, config.storage.max_keys);

    // Initialize components
    let bus = MessageBus::new(config.bus.clone());
    let data_layer = DataLayer::new(config.storage.clone());
    let module_registry = ModuleRegistry::new();

    tracing::info!("✓ Message bus initialized");
    tracing::info!("✓ Data layer initialized (capacity: {})", config.storage.max_keys);
    tracing::info!("✓ Module registry initialized");

    // Start IPC server
    let ipc_server = IpcServer::new(
        config.ipc_address.clone(),
        bus.clone(),
        data_layer.clone(),
        module_registry.clone(),
    );

    tracing::info!("Starting IPC server on {}", config.ipc_address);

    // Run server in background
    let server_handle = {
        let server = ipc_server;
        tokio::spawn(async move {
            if let Err(e) = server.start().await {
                tracing::error!("IPC server error: {}", e);
            }
        })
    };

    tracing::info!("✓ Daemon V1 is running");
    tracing::info!("");
    tracing::info!("Press Ctrl+C to shutdown");

    // Wait for shutdown signal
    tokio::signal::ctrl_c()
        .await
        .expect("Failed to listen for ctrl-c");

    tracing::info!("");
    tracing::info!("Shutdown signal received, stopping daemon...");

    // Graceful shutdown
    // TODO: Stop all modules
    // TODO: Shutdown IPC server
    server_handle.abort();

    tracing::info!("✓ Daemon V1 stopped");

    Ok(())
}
