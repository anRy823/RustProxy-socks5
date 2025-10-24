//! Configuration File Watcher
//! 
//! Provides hot-reloading capabilities for configuration files.

use super::{Config, ConfigManager};
use crate::Result;
use anyhow::{Context, bail};
use notify::{Config as NotifyConfig, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use tokio_stream::{wrappers::BroadcastStream, StreamExt};
use tracing::{debug, error, info, warn};

/// Configuration change event
#[derive(Debug, Clone)]
pub struct ConfigChangeEvent {
    pub config: Arc<Config>,
    pub timestamp: std::time::SystemTime,
    pub file_path: PathBuf,
}

/// Configuration file watcher
pub struct ConfigWatcher {
    config_path: PathBuf,
    current_config: Arc<RwLock<Config>>,
    change_sender: broadcast::Sender<ConfigChangeEvent>,
    _watcher: RecommendedWatcher,
}

impl ConfigWatcher {
    /// Create a new configuration watcher
    pub fn new(config_path: PathBuf) -> Result<Self> {
        let (change_sender, _) = broadcast::channel(100);
        
        // Load initial configuration
        let initial_config = ConfigManager::load_from_file(&config_path)?;
        let current_config = Arc::new(RwLock::new(initial_config));
        
        // Create file watcher
        let sender_clone = change_sender.clone();
        let config_clone = current_config.clone();
        let path_clone = config_path.clone();
        
        let mut watcher = RecommendedWatcher::new(
            move |res: notify::Result<Event>| {
                match res {
                    Ok(event) => {
                        if let Err(e) = Self::handle_file_event(
                            event,
                            &path_clone,
                            &config_clone,
                            &sender_clone,
                        ) {
                            error!("Error handling file event: {}", e);
                        }
                    }
                    Err(e) => error!("File watcher error: {}", e),
                }
            },
            NotifyConfig::default(),
        )
        .context("Failed to create file watcher")?;
        
        // Watch the config file directory (watching the file directly can be unreliable)
        if let Some(parent_dir) = config_path.parent() {
            watcher
                .watch(parent_dir, RecursiveMode::NonRecursive)
                .with_context(|| format!("Failed to watch directory: {}", parent_dir.display()))?;
            
            info!("Started watching configuration directory: {}", parent_dir.display());
        } else {
            bail!("Configuration file has no parent directory: {}", config_path.display());
        }
        
        Ok(Self {
            config_path,
            current_config,
            change_sender,
            _watcher: watcher,
        })
    }
    
    /// Get the current configuration
    pub async fn get_config(&self) -> Arc<Config> {
        let config = self.current_config.read().await;
        Arc::new(config.clone())
    }
    
    /// Subscribe to configuration changes
    pub fn subscribe(&self) -> BroadcastStream<ConfigChangeEvent> {
        BroadcastStream::new(self.change_sender.subscribe())
    }
    
    /// Force reload the configuration
    pub async fn reload(&self) -> Result<()> {
        info!("Force reloading configuration from: {}", self.config_path.display());
        
        match ConfigManager::load_from_file(&self.config_path) {
            Ok(new_config) => {
                let config_arc = Arc::new(new_config);
                
                // Update current config
                {
                    let mut current = self.current_config.write().await;
                    *current = (*config_arc).clone();
                }
                
                // Notify subscribers
                let event = ConfigChangeEvent {
                    config: config_arc,
                    timestamp: std::time::SystemTime::now(),
                    file_path: self.config_path.clone(),
                };
                
                if let Err(e) = self.change_sender.send(event) {
                    warn!("No subscribers for config change event: {}", e);
                }
                
                info!("Configuration reloaded successfully");
                Ok(())
            }
            Err(e) => {
                error!("Failed to reload configuration: {}", e);
                Err(e)
            }
        }
    }
    
    /// Handle file system events
    fn handle_file_event(
        event: Event,
        config_path: &Path,
        current_config: &Arc<RwLock<Config>>,
        sender: &broadcast::Sender<ConfigChangeEvent>,
    ) -> Result<()> {
        debug!("File event: {:?}", event);
        
        // Check if the event affects our config file
        let affects_config = event.paths.iter().any(|path| {
            path.file_name() == config_path.file_name()
        });
        
        if !affects_config {
            return Ok(());
        }
        
        // Handle different event types
        match event.kind {
            EventKind::Modify(_) | EventKind::Create(_) => {
                info!("Configuration file changed, reloading...");
                
                // Add a small delay to ensure file write is complete
                std::thread::sleep(std::time::Duration::from_millis(100));
                
                match ConfigManager::load_from_file(config_path) {
                    Ok(new_config) => {
                        let config_arc = Arc::new(new_config);
                        
                        // Update current config (blocking is OK here since it's in a callback)
                        tokio::task::block_in_place(|| {
                            tokio::runtime::Handle::current().block_on(async {
                                let mut current = current_config.write().await;
                                *current = (*config_arc).clone();
                            })
                        });
                        
                        // Notify subscribers
                        let event = ConfigChangeEvent {
                            config: config_arc,
                            timestamp: std::time::SystemTime::now(),
                            file_path: config_path.to_path_buf(),
                        };
                        
                        if let Err(e) = sender.send(event) {
                            warn!("No subscribers for config change event: {}", e);
                        }
                        
                        info!("Configuration reloaded successfully");
                    }
                    Err(e) => {
                        error!("Failed to reload configuration, keeping current config: {}", e);
                        // Don't propagate the error to keep the watcher running
                    }
                }
            }
            EventKind::Remove(_) => {
                warn!("Configuration file was removed: {}", config_path.display());
                // Keep current configuration when file is removed
            }
            _ => {
                debug!("Ignoring file event type: {:?}", event.kind);
            }
        }
        
        Ok(())
    }
}

/// Configuration reload service
pub struct ConfigReloadService {
    watcher: ConfigWatcher,
}

impl ConfigReloadService {
    /// Create a new configuration reload service
    pub fn new(config_path: PathBuf) -> Result<Self> {
        let watcher = ConfigWatcher::new(config_path)?;
        Ok(Self { watcher })
    }
    
    /// Start the configuration reload service
    pub async fn start(self) -> Result<(Arc<RwLock<Config>>, broadcast::Receiver<ConfigChangeEvent>)> {
        let config = Arc::new(RwLock::new((*self.watcher.get_config().await).clone()));
        let mut change_stream = self.watcher.subscribe();
        let receiver = self.watcher.change_sender.subscribe();
        
        let config_clone = config.clone();
        
        // Spawn task to handle configuration changes
        tokio::spawn(async move {
            while let Some(change_event) = change_stream.next().await {
                match change_event {
                    Ok(event) => {
                        info!("Applying configuration change from: {}", event.file_path.display());
                        
                        // Update the shared config
                        {
                            let mut current_config = config_clone.write().await;
                            *current_config = (*event.config).clone();
                        }
                        
                        info!("Configuration updated successfully");
                    }
                    Err(e) => {
                        error!("Error receiving configuration change: {}", e);
                    }
                }
            }
        });
        
        info!("Configuration reload service started");
        Ok((config, receiver))
    }
    
    /// Get the configuration watcher
    pub fn watcher(&self) -> &ConfigWatcher {
        &self.watcher
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;
    use tokio::time::{sleep, Duration};
    
    #[tokio::test]
    async fn test_config_watcher_creation() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("test_config.toml");
        
        // Create initial config file
        let initial_config = r#"
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
        
        fs::write(&config_path, initial_config).unwrap();
        
        // Create watcher
        let watcher = ConfigWatcher::new(config_path.clone()).unwrap();
        
        // Verify initial config is loaded
        let config = watcher.get_config().await;
        assert_eq!(config.server.bind_addr.port(), 1080);
        assert_eq!(config.server.max_connections, 1000);
    }
    
    #[tokio::test]
    async fn test_config_hot_reload() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("test_config.toml");
        
        // Create initial config file
        let initial_config = r#"
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
        
        fs::write(&config_path, initial_config).unwrap();
        
        // Create watcher and subscribe to changes
        let watcher = ConfigWatcher::new(config_path.clone()).unwrap();
        let mut change_stream = watcher.subscribe();
        
        // Verify initial config
        let config = watcher.get_config().await;
        assert_eq!(config.server.max_connections, 1000);
        
        // Modify config file
        let updated_config = initial_config.replace("max_connections = 1000", "max_connections = 2000");
        fs::write(&config_path, updated_config).unwrap();
        
        // Wait for change event
        tokio::select! {
            change_event = change_stream.next() => {
                let event = change_event.unwrap().unwrap();
                assert_eq!(event.config.server.max_connections, 2000);
            }
            _ = sleep(Duration::from_secs(5)) => {
                panic!("Config change event not received within timeout");
            }
        }
        
        // Verify config was updated
        let updated_config = watcher.get_config().await;
        assert_eq!(updated_config.server.max_connections, 2000);
    }
    
    #[tokio::test]
    async fn test_invalid_config_handling() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("test_config.toml");
        
        // Create initial valid config
        let initial_config = r#"
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
        
        fs::write(&config_path, initial_config).unwrap();
        
        let watcher = ConfigWatcher::new(config_path.clone()).unwrap();
        let mut change_stream = watcher.subscribe();
        
        // Verify initial config
        let config = watcher.get_config().await;
        assert_eq!(config.server.max_connections, 1000);
        
        // Write invalid config
        let invalid_config = "invalid toml content [[[";
        fs::write(&config_path, invalid_config).unwrap();
        
        // Wait a bit to ensure file watcher processes the change
        sleep(Duration::from_millis(500)).await;
        
        // Config should remain unchanged
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
    }
}