//! Configuration Hot-Reload Integration Tests

use anyhow::Result;
use rustproxy::config::{ConfigReloadService, ConfigWatcher};
use std::fs;
use std::time::Duration;
use tempfile::TempDir;
use tokio::time::sleep;
use tokio_stream::StreamExt;

#[tokio::test]
async fn test_config_hot_reload_integration() -> Result<()> {
    // Create temporary directory and config file
    let temp_dir = TempDir::new()?;
    let config_path = temp_dir.path().join("test_config.toml");

    // Create initial configuration
    let initial_config = create_test_config(1080, 1000);
    fs::write(&config_path, &initial_config)?;

    // Start configuration reload service
    let config_service = ConfigReloadService::new(config_path.clone())?;
    let (shared_config, mut config_changes) = config_service.start().await?;

    // Verify initial configuration
    {
        let config = shared_config.read().await;
        assert_eq!(config.server.bind_addr.port(), 1080);
        assert_eq!(config.server.max_connections, 1000);
    }

    // Modify configuration file
    let updated_config = create_test_config(1081, 2000);
    fs::write(&config_path, &updated_config)?;

    // Wait for configuration change event
    tokio::select! {
        change_event = config_changes.recv() => {
            let event = change_event?;
            assert_eq!(event.config.server.bind_addr.port(), 1081);
            assert_eq!(event.config.server.max_connections, 2000);
        }
        _ = sleep(Duration::from_secs(5)) => {
            panic!("Configuration change event not received within timeout");
        }
    }

    // Verify shared configuration was updated
    {
        let config = shared_config.read().await;
        assert_eq!(config.server.bind_addr.port(), 1081);
        assert_eq!(config.server.max_connections, 2000);
    }

    Ok(())
}

#[tokio::test]
async fn test_config_watcher_invalid_config() -> Result<()> {
    // Create temporary directory and config file
    let temp_dir = TempDir::new()?;
    let config_path = temp_dir.path().join("test_config.toml");

    // Create initial valid configuration
    let initial_config = create_test_config(1080, 1000);
    fs::write(&config_path, &initial_config)?;

    // Create watcher
    let watcher = ConfigWatcher::new(config_path.clone())?;
    let mut change_stream = watcher.subscribe();

    // Verify initial configuration
    let config = watcher.get_config().await;
    assert_eq!(config.server.max_connections, 1000);

    // Write invalid configuration
    let invalid_config = "invalid toml content [[[";
    fs::write(&config_path, invalid_config)?;

    // Wait a bit to ensure file watcher processes the change
    sleep(Duration::from_millis(500)).await;

    // Configuration should remain unchanged
    let config_after_invalid = watcher.get_config().await;
    assert_eq!(config_after_invalid.server.max_connections, 1000);

    // No change event should be emitted for invalid config
    tokio::select! {
        _ = change_stream.next() => {
            panic!("Change event should not be emitted for invalid config");
        }
        _ = sleep(Duration::from_millis(200)) => {
            // Expected - no change event
        }
    }

    Ok(())
}

#[tokio::test]
async fn test_config_watcher_force_reload() -> Result<()> {
    // Create temporary directory and config file
    let temp_dir = TempDir::new()?;
    let config_path = temp_dir.path().join("test_config.toml");

    // Create initial configuration
    let initial_config = create_test_config(1080, 1000);
    fs::write(&config_path, &initial_config)?;

    // Create watcher
    let watcher = ConfigWatcher::new(config_path.clone())?;
    let mut change_stream = watcher.subscribe();

    // Modify config file
    let updated_config = create_test_config(1082, 3000);
    fs::write(&config_path, &updated_config)?;

    // Force reload
    watcher.reload().await?;

    // Should receive change event
    tokio::select! {
        change_event = change_stream.next() => {
            let event = change_event.unwrap()?;
            assert_eq!(event.config.server.bind_addr.port(), 1082);
            assert_eq!(event.config.server.max_connections, 3000);
        }
        _ = sleep(Duration::from_secs(2)) => {
            panic!("Change event not received within timeout");
        }
    }

    Ok(())
}

fn create_test_config(port: u16, max_connections: usize) -> String {
    format!(
        r#"
[server]
bind_addr = "127.0.0.1:{}"
max_connections = {}
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
enabled = false
method = "none"
users = []

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
api_key = "test-key"

[security]
rate_limiting_enabled = true
max_requests_per_minute = 60
ddos_protection_enabled = true
connection_flood_threshold = 100
fail2ban_enabled = true
max_failed_attempts = 5
ban_duration = "1h"
secrets_encryption_enabled = false
"#,
        port, max_connections
    )
}
