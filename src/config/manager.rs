//! Configuration Manager

use super::Config;
use crate::Result;
use anyhow::{Context, bail};
use std::path::Path;
use std::net::SocketAddr;

/// Manages configuration loading and validation
pub struct ConfigManager;

impl ConfigManager {
    /// Load configuration from file
    pub fn load_from_file(path: &Path) -> Result<Config> {
        if path.exists() {
            tracing::info!("Loading configuration from: {}", path.display());
            let content = std::fs::read_to_string(path)
                .with_context(|| format!("Failed to read config file: {}", path.display()))?;
            
            let config: Config = toml::from_str(&content)
                .with_context(|| format!("Failed to parse config file: {}", path.display()))?;
            
            config.validate()
                .with_context(|| "Configuration validation failed")?;
            
            tracing::info!("Configuration loaded and validated successfully");
            Ok(config)
        } else {
            tracing::warn!("Configuration file not found at {}, using defaults", path.display());
            let config = Config::default();
            config.validate()?;
            Ok(config)
        }
    }

    /// Load configuration from environment variables
    pub fn load_from_env() -> Result<Config> {
        let mut config = Config::default();
        
        // Override with environment variables if present
        if let Ok(bind_addr) = std::env::var("SOCKS5_BIND_ADDR") {
            config.server.bind_addr = bind_addr.parse::<SocketAddr>()
                .with_context(|| format!("Invalid SOCKS5_BIND_ADDR: {}", bind_addr))?;
        }
        
        if let Ok(max_conn) = std::env::var("SOCKS5_MAX_CONNECTIONS") {
            config.server.max_connections = max_conn.parse::<usize>()
                .with_context(|| format!("Invalid SOCKS5_MAX_CONNECTIONS: {}", max_conn))?;
        }
        
        if let Ok(timeout) = std::env::var("SOCKS5_CONNECTION_TIMEOUT") {
            config.server.connection_timeout = humantime::parse_duration(&timeout)
                .with_context(|| format!("Invalid SOCKS5_CONNECTION_TIMEOUT: {}", timeout))?;
        }
        
        if let Ok(buffer_size) = std::env::var("SOCKS5_BUFFER_SIZE") {
            config.server.buffer_size = buffer_size.parse::<usize>()
                .with_context(|| format!("Invalid SOCKS5_BUFFER_SIZE: {}", buffer_size))?;
        }
        
        if let Ok(auth_enabled) = std::env::var("SOCKS5_AUTH_ENABLED") {
            config.auth.enabled = auth_enabled.parse::<bool>()
                .with_context(|| format!("Invalid SOCKS5_AUTH_ENABLED: {}", auth_enabled))?;
        }
        
        if let Ok(log_level) = std::env::var("SOCKS5_LOG_LEVEL") {
            config.monitoring.log_level = log_level;
        }
        
        config.validate()?;
        Ok(config)
    }
}

impl Config {
    /// Validate the configuration
    pub fn validate(&self) -> Result<()> {
        // Validate server configuration
        self.validate_server_config()
            .with_context(|| "Server configuration validation failed")?;
        
        // Validate authentication configuration
        self.validate_auth_config()
            .with_context(|| "Authentication configuration validation failed")?;
        
        // Validate access control configuration
        self.validate_access_control_config()
            .with_context(|| "Access control configuration validation failed")?;
        
        // Validate routing configuration
        self.validate_routing_config()
            .with_context(|| "Routing configuration validation failed")?;
        
        // Validate monitoring configuration
        self.validate_monitoring_config()
            .with_context(|| "Monitoring configuration validation failed")?;
        
        Ok(())
    }
    
    /// Validate server configuration
    fn validate_server_config(&self) -> Result<()> {
        if self.server.max_connections == 0 {
            bail!("max_connections must be greater than 0");
        }
        
        if self.server.max_connections > 100000 {
            bail!("max_connections cannot exceed 100,000 for safety");
        }
        
        if self.server.connection_timeout.as_secs() == 0 {
            bail!("connection_timeout must be greater than 0");
        }
        
        if self.server.connection_timeout.as_secs() > 3600 {
            bail!("connection_timeout cannot exceed 1 hour");
        }
        
        if self.server.buffer_size < 1024 {
            bail!("buffer_size must be at least 1024 bytes");
        }
        
        if self.server.buffer_size > 1048576 {
            bail!("buffer_size cannot exceed 1MB");
        }
        
        Ok(())
    }
    
    /// Validate authentication configuration
    fn validate_auth_config(&self) -> Result<()> {
        if !["none", "userpass"].contains(&self.auth.method.as_str()) {
            bail!("auth.method must be 'none' or 'userpass'");
        }
        
        if self.auth.enabled && self.auth.method == "userpass" && self.auth.users.is_empty() {
            bail!("When userpass authentication is enabled, at least one user must be configured");
        }
        
        // Validate user configurations
        for (i, user) in self.auth.users.iter().enumerate() {
            if user.username.is_empty() {
                bail!("User {} has empty username", i);
            }
            
            if user.username.len() > 255 {
                bail!("User {} username exceeds 255 characters", i);
            }
            
            if user.password.is_empty() {
                bail!("User {} has empty password", i);
            }
            
            if user.password.len() > 255 {
                bail!("User {} password exceeds 255 characters", i);
            }
        }
        
        Ok(())
    }
    
    /// Validate access control configuration
    fn validate_access_control_config(&self) -> Result<()> {
        if !["allow", "block"].contains(&self.access_control.default_policy.as_str()) {
            bail!("access_control.default_policy must be 'allow' or 'block'");
        }
        
        // Validate access rules
        for (i, rule) in self.access_control.rules.iter().enumerate() {
            if rule.pattern.is_empty() {
                bail!("Access rule {} has empty pattern", i);
            }
            
            if !["allow", "block", "redirect"].contains(&rule.action.as_str()) {
                bail!("Access rule {} action must be 'allow', 'block', or 'redirect'", i);
            }
            
            if let Some(ports) = &rule.ports {
                for &port in ports {
                    if port == 0 {
                        bail!("Access rule {} contains invalid port 0", i);
                    }
                }
            }
        }
        
        Ok(())
    }
    
    /// Validate routing configuration
    fn validate_routing_config(&self) -> Result<()> {
        // Validate upstream proxy configurations
        for (i, proxy) in self.routing.upstream_proxies.iter().enumerate() {
            if proxy.name.is_empty() {
                bail!("Upstream proxy {} has empty name", i);
            }
            
            if !["socks5", "http", "https"].contains(&proxy.protocol.as_str()) {
                bail!("Upstream proxy {} protocol must be 'socks5', 'http', or 'https'", i);
            }
            
            if let Some(auth) = &proxy.auth {
                if auth.username.is_empty() {
                    bail!("Upstream proxy {} has empty auth username", i);
                }
                
                if auth.password.is_empty() {
                    bail!("Upstream proxy {} has empty auth password", i);
                }
            }
        }
        
        Ok(())
    }
    
    /// Validate monitoring configuration
    fn validate_monitoring_config(&self) -> Result<()> {
        let valid_log_levels = ["trace", "debug", "info", "warn", "error"];
        if !valid_log_levels.contains(&self.monitoring.log_level.as_str()) {
            bail!("monitoring.log_level must be one of: {}", valid_log_levels.join(", "));
        }
        
        Ok(())
    }

    /// Merge with CLI arguments
    pub fn merge_with_cli_args(
        &mut self,
        bind: Option<&str>,
        port: Option<u16>,
        max_connections: Option<usize>,
        no_auth: bool,
        timeout: Option<u64>,
        buffer_size: Option<usize>,
    ) {
        // Override bind address if provided
        if let Some(bind_str) = bind {
            if let Ok(addr) = bind_str.parse::<std::net::SocketAddr>() {
                self.server.bind_addr = addr;
                tracing::info!("CLI override: bind address set to {}", addr);
            } else {
                tracing::warn!("Invalid bind address provided: {}", bind_str);
            }
        }

        // Override port if provided
        if let Some(port) = port {
            self.server.bind_addr.set_port(port);
            tracing::info!("CLI override: port set to {}", port);
        }

        // Override max connections if provided
        if let Some(max_conn) = max_connections {
            self.server.max_connections = max_conn;
            tracing::info!("CLI override: max connections set to {}", max_conn);
        }

        // Override authentication if no_auth is specified
        if no_auth {
            self.auth.enabled = false;
            tracing::info!("CLI override: authentication disabled");
        }

        // Override timeout if provided
        if let Some(timeout_secs) = timeout {
            self.server.connection_timeout = std::time::Duration::from_secs(timeout_secs);
            tracing::info!("CLI override: connection timeout set to {}s", timeout_secs);
        }

        // Override buffer size if provided
        if let Some(buffer_size) = buffer_size {
            self.server.buffer_size = buffer_size;
            tracing::info!("CLI override: buffer size set to {} bytes", buffer_size);
        }
    }
}
