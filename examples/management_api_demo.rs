//! Management API Demo
//! 
//! Demonstrates configuration hot-reloading and management API usage.

use anyhow::Result;
use rustproxy::{
    config::ConfigReloadService,
    management::{ManagementServer, types::ApiAuthConfig},
    metrics::Metrics,
};
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{info, error};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();
    
    info!("Starting Management API Demo");
    
    // Create a sample configuration file
    let config_path = PathBuf::from("demo_config.toml");
    create_sample_config(&config_path).await?;
    
    // Start configuration watcher
    let config_service = ConfigReloadService::new(config_path.clone())?;
    let (shared_config, mut config_changes) = config_service.start().await?;
    
    // Create metrics
    let metrics = Arc::new(Metrics::new());
    
    // Configure management API authentication
    let auth_config = ApiAuthConfig {
        enabled: true,
        api_key: Some("demo-api-key".to_string()),
        basic_auth: None,
        jwt: None,
    };
    
    // Start management API server
    let management_server = ManagementServer::new(
        "127.0.0.1:8080".parse()?,
        shared_config.clone(),
        metrics.clone(),
        auth_config,
    );
    
    // Start management server in background
    let server_handle = tokio::spawn(async move {
        if let Err(e) = management_server.start().await {
            error!("Management server error: {}", e);
        }
    });
    
    // Start config change listener
    let config_listener_handle = tokio::spawn(async move {
        while let Ok(change_event) = config_changes.recv().await {
            info!("Configuration changed: {}", change_event.file_path.display());
            info!("New bind address: {}", change_event.config.server.bind_addr);
        }
    });
    
    info!("Management API Demo started!");
    info!("Management API available at: http://127.0.0.1:8080");
    info!("API Key: demo-api-key");
    info!("");
    info!("Try these endpoints:");
    info!("  GET  /api/v1/health                    - Health check (no auth)");
    info!("  GET  /api/v1/status                    - Server status");
    info!("  GET  /api/v1/config                    - Current configuration");
    info!("  GET  /api/v1/stats                     - Statistics summary");
    info!("  POST /api/v1/users                     - Create user");
    info!("  POST /api/v1/config/reload             - Reload configuration");
    info!("");
    info!("Example curl commands:");
    info!("  curl http://127.0.0.1:8080/api/v1/health");
    info!("  curl -H 'x-api-key: demo-api-key' http://127.0.0.1:8080/api/v1/status");
    info!("  curl -H 'x-api-key: demo-api-key' http://127.0.0.1:8080/api/v1/config");
    info!("");
    info!("To test hot-reloading, modify demo_config.toml and watch the logs!");
    info!("");
    info!("Press Ctrl+C to exit");
    
    // Wait for Ctrl+C
    tokio::signal::ctrl_c().await?;
    
    info!("Shutting down...");
    
    // Cancel tasks
    server_handle.abort();
    config_listener_handle.abort();
    
    // Clean up demo config file
    if config_path.exists() {
        std::fs::remove_file(&config_path)?;
        info!("Cleaned up demo configuration file");
    }
    
    Ok(())
}

async fn create_sample_config(path: &PathBuf) -> Result<()> {
    let config_content = r#"
[server]
bind_addr = "127.0.0.1:1080"
max_connections = 1000
connection_timeout = "5m"
buffer_size = 8192
shutdown_timeout = "30s"
idle_timeout = "1m"
handshake_timeout = "10s"
max_memory_mb = 512
connection_pool_size = 10
enable_keepalive = true
keepalive_interval = "30s"

[auth]
enabled = true
method = "userpass"

[[auth.users]]
username = "demo_user"
password = "demo_pass"
enabled = true

[[auth.users]]
username = "test_user"
password = "test_pass"
enabled = true

[access_control]
enabled = false
default_policy = "allow"
rules = []

[routing]
enabled = false
upstream_proxies = []
rules = []

[routing.smart_routing]
enabled = false
health_check_interval = "30s"
health_check_timeout = "5s"
min_measurements = 3
enable_latency_routing = true
enable_health_routing = true

[monitoring]
enabled = true
metrics_addr = "127.0.0.1:9090"
log_level = "info"
prometheus_enabled = true
collect_connection_stats = true
max_historical_connections = 10000

[monitoring.management_api]
enabled = true
bind_addr = "127.0.0.1:8080"

[monitoring.management_api.auth]
enabled = true
api_key = "demo-api-key"

[security]
rate_limiting_enabled = true
max_requests_per_minute = 60
ddos_protection_enabled = true
connection_flood_threshold = 100
fail2ban_enabled = true
max_failed_attempts = 5
ban_duration = "1h"
secrets_encryption_enabled = false
"#;
    
    tokio::fs::write(path, config_content).await?;
    info!("Created sample configuration file: {}", path.display());
    Ok(())
}