//! RustProxy - Professional SOCKS5 Proxy Server
//!
//! Created by [Your Name] - Professional Network Solutions
//!
//! A high-performance, enterprise-grade SOCKS5 proxy server built with Rust
//! for maximum security, reliability, and performance.

use anyhow::{Context, Result};
use clap::Parser;
use std::path::PathBuf;

use tracing::{error, info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use rustproxy::{
    config::ConfigManager, management::ManagementServer, metrics::Metrics, ConnectionManager,
    ShutdownCoordinator,
};

/// CLI arguments for RustProxy
#[derive(Parser, Debug)]
#[command(name = "rustproxy")]
#[command(about = "RustProxy - Professional SOCKS5 Proxy Server")]
#[command(version)]
#[command(author = "[Your Name] - Professional Network Solutions")]
#[command(long_about = "
RustProxy - Professional SOCKS5 Proxy Server

Created by [Your Name] - Professional Network Solutions

A high-performance, enterprise-grade SOCKS5 proxy server built with Rust
for maximum security, reliability, and performance.

Configuration priority (highest to lowest):
1. Command-line arguments
2. Configuration file
3. Environment variables
4. Built-in defaults

Environment variables:
  RUSTPROXY_BIND_ADDR          - Bind address (e.g., 127.0.0.1:1080)
  RUSTPROXY_MAX_CONNECTIONS    - Maximum concurrent connections
  RUSTPROXY_CONNECTION_TIMEOUT - Connection timeout (e.g., 5m, 30s)
  RUSTPROXY_BUFFER_SIZE        - Buffer size in bytes
  RUSTPROXY_AUTH_ENABLED       - Enable authentication (true/false)
  RUSTPROXY_LOG_LEVEL          - Log level (trace, debug, info, warn, error)

For complete documentation, see: USER_MANUAL.md
")]
pub struct CliArgs {
    /// Configuration file path
    #[arg(
        short,
        long,
        default_value = "config.toml",
        help = "Path to configuration file"
    )]
    pub config: PathBuf,

    /// Bind address (overrides config file)
    #[arg(short, long, help = "Bind address (e.g., 127.0.0.1:1080)")]
    pub bind: Option<String>,

    /// Port to bind to (overrides config file)
    #[arg(short, long, help = "Port to bind to")]
    pub port: Option<u16>,

    /// Log level (trace, debug, info, warn, error)
    #[arg(long, default_value = "info", help = "Log level")]
    pub log_level: String,

    /// Enable verbose logging (sets log level to debug)
    #[arg(short, long, help = "Enable verbose logging")]
    pub verbose: bool,

    /// Disable authentication (overrides config file)
    #[arg(long, help = "Disable authentication")]
    pub no_auth: bool,

    /// Maximum number of concurrent connections
    #[arg(long, help = "Maximum number of concurrent connections")]
    pub max_connections: Option<usize>,

    /// Connection timeout in seconds
    #[arg(long, help = "Connection timeout in seconds")]
    pub timeout: Option<u64>,

    /// Buffer size in bytes
    #[arg(long, help = "Buffer size in bytes")]
    pub buffer_size: Option<usize>,

    /// Validate configuration and exit
    #[arg(long, help = "Validate configuration and exit")]
    pub validate_config: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = CliArgs::parse();

    // Initialize tracing
    init_tracing(&args)?;

    info!(
        "Starting RustProxy v{} - Professional SOCKS5 Proxy Server",
        env!("CARGO_PKG_VERSION")
    );
    info!("Created by [Your Name] - Professional Network Solutions");

    // Load configuration with priority: CLI args > config file > environment > defaults
    let mut config = if args.config.exists() {
        ConfigManager::load_from_file(&args.config)?
    } else {
        info!("Config file not found, checking environment variables");
        ConfigManager::load_from_env()?
    };

    // Apply CLI argument overrides (highest priority)
    config.merge_with_cli_args(
        args.bind.as_deref(),
        args.port,
        args.max_connections,
        args.no_auth,
        args.timeout,
        args.buffer_size,
    );

    // Final validation after all overrides
    config
        .validate()
        .context("Final configuration validation failed")?;

    // If validate-config flag is set, just validate and exit
    if args.validate_config {
        info!("✅ Configuration is valid");
        info!("Configuration summary:");
        info!("  Bind address: {}", config.server.bind_addr);
        info!("  Max connections: {}", config.server.max_connections);
        info!(
            "  Connection timeout: {:?}",
            config.server.connection_timeout
        );
        info!("  Buffer size: {} bytes", config.server.buffer_size);
        info!(
            "  Authentication: {}",
            if config.auth.enabled {
                "enabled"
            } else {
                "disabled"
            }
        );
        info!(
            "  Access control: {}",
            if config.access_control.enabled {
                "enabled"
            } else {
                "disabled"
            }
        );
        info!(
            "  Routing: {}",
            if config.routing.enabled {
                "enabled"
            } else {
                "disabled"
            }
        );
        info!(
            "  Monitoring: {}",
            if config.monitoring.enabled {
                "enabled"
            } else {
                "disabled"
            }
        );
        return Ok(());
    }

    info!("Configuration loaded successfully");
    info!("Bind address: {}", config.server.bind_addr);
    info!("Max connections: {}", config.server.max_connections);
    info!(
        "Authentication: {}",
        if config.auth.enabled {
            "enabled"
        } else {
            "disabled"
        }
    );

    // Create shutdown coordinator
    let shutdown_timeout = config.server.shutdown_timeout;
    let shutdown_coordinator = ShutdownCoordinator::new(shutdown_timeout);

    // Create metrics
    let metrics = std::sync::Arc::new(Metrics::new());

    // Create shared config for management API
    let config_arc = std::sync::Arc::new(tokio::sync::RwLock::new(config.clone()));

    // Start the connection manager
    let connection_manager = ConnectionManager::new(std::sync::Arc::new(config.clone()));

    // Start management API server if enabled
    let management_handle = if config.monitoring.management_api.enabled {
        info!(
            "Starting management API server on {}",
            config.monitoring.management_api.bind_addr
        );

        let management_server = ManagementServer::new(
            config.monitoring.management_api.bind_addr,
            config_arc.clone(),
            metrics.clone(),
            config.monitoring.management_api.auth.clone(),
        );

        Some(tokio::spawn(async move {
            if let Err(e) = management_server.start().await {
                error!("Management API server error: {}", e);
            }
        }))
    } else {
        info!("Management API server disabled");
        None
    };

    // Create a channel to communicate with the server task
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();

    // Start the server in a separate task
    let server_handle = tokio::spawn(async move {
        let mut manager = connection_manager;

        tokio::select! {
            result = manager.start() => {
                if let Err(e) = result {
                    error!("Server error: {}", e);
                }
            }
            _ = shutdown_rx => {
                info!("Server task received shutdown signal");
                manager.initiate_shutdown();
                if let Err(e) = manager.wait_for_connections_to_close().await {
                    error!("Error during connection cleanup: {}", e);
                }
            }
        }
    });

    info!("🚀 RustProxy started successfully!");
    info!("✅ Enterprise SOCKS5 proxy with authentication, access control, and advanced routing");
    info!("📖 For help and documentation, see USER_MANUAL.md");
    info!("🛑 Press Ctrl+C or send SIGTERM/SIGINT to shutdown gracefully");

    // Start listening for shutdown signals
    let signal_result = shutdown_coordinator.listen_for_signals().await;
    if let Err(e) = signal_result {
        error!("Error setting up signal handlers: {}", e);
    }

    // Initiate graceful shutdown
    info!("Initiating graceful shutdown...");

    // Send shutdown signal to server task
    if let Err(_) = shutdown_tx.send(()) {
        warn!("Failed to send shutdown signal to server task");
    }

    // Wait for server task to complete
    if let Err(e) = server_handle.await {
        if !e.is_cancelled() {
            error!("Server task failed: {}", e);
        }
    }

    // Shutdown management API server if it was started
    if let Some(handle) = management_handle {
        handle.abort();
        info!("Management API server shutdown");
    }

    info!("Server shutdown complete");

    Ok(())
}

/// Initialize tracing/logging
fn init_tracing(args: &CliArgs) -> Result<()> {
    let log_level = if args.verbose {
        "debug"
    } else {
        &args.log_level
    };

    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(log_level));

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .with_target(false)
                .with_thread_ids(true)
                .with_level(true)
                .with_ansi(true),
        )
        .with(env_filter)
        .init();

    Ok(())
}
