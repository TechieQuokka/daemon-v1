use daemon_v1::{
    bus::MessageBus,
    config::DaemonConfig,
    controller::IpcServer,
    module::ModuleManager,
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

    // Load configuration
    let config_path = "config.toml";
    let config = if std::path::Path::new(config_path).exists() {
        tracing::info!("Loading configuration from {}", config_path);
        match DaemonConfig::from_file(config_path) {
            Ok(cfg) => cfg,
            Err(e) => {
                tracing::error!("Failed to load config: {}", e);
                tracing::info!("Using default configuration");
                DaemonConfig::default()
            }
        }
    } else {
        tracing::info!("Configuration file not found, creating default {}", config_path);
        let default_config = DaemonConfig::default();
        if let Err(e) = default_config.to_file(config_path) {
            tracing::warn!("Failed to create config file: {}", e);
        } else {
            tracing::info!("✓ Created default configuration file: {}", config_path);
        }
        default_config
    };

    tracing::info!("Configuration: IPC={}, Bus.max_events={}, Storage.max_keys={}, Shutdown_modules={}",
        config.ipc_address, config.bus.max_events, config.storage.max_keys, config.shutdown_modules_on_exit);

    // Initialize components
    let bus = MessageBus::new(config.bus.clone());
    let data_layer = DataLayer::new(config.storage.clone());
    let module_manager = ModuleManager::new(bus.clone(), data_layer.clone());

    tracing::info!("✓ Message bus initialized");
    tracing::info!("✓ Data layer initialized (capacity: {})", config.storage.max_keys);
    tracing::info!("✓ Module manager initialized");

    // Start IPC server
    let ipc_server = IpcServer::new(
        config.ipc_address.clone(),
        bus.clone(),
        data_layer.clone(),
        module_manager.clone(),
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
    if config.shutdown_modules_on_exit {
        tracing::info!("Stopping all modules (shutdown_modules_on_exit=true)...");
        module_manager.shutdown_all(5000).await;
    } else {
        tracing::warn!("Leaving modules running (shutdown_modules_on_exit=false)");
        tracing::warn!("Module processes will continue as orphaned processes");
    }

    // Shutdown IPC server
    tracing::info!("Stopping IPC server...");
    server_handle.abort();

    tracing::info!("✓ Daemon V1 stopped");

    Ok(())
}
