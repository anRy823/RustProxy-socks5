//! Configuration Types

use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::time::Duration;
use crate::security::SecurityConfig;

/// Main configuration structure
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    pub server: ServerConfig,
    pub auth: AuthConfig,
    pub access_control: AccessControlConfig,
    pub routing: RoutingConfig,
    pub monitoring: MonitoringConfig,
    pub security: SecurityConfig,
}

/// Server configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServerConfig {
    pub bind_addr: SocketAddr,
    pub max_connections: usize,
    #[serde(with = "humantime_serde")]
    pub connection_timeout: Duration,
    pub buffer_size: usize,
    #[serde(with = "humantime_serde")]
    pub shutdown_timeout: Duration,
    #[serde(with = "humantime_serde")]
    pub idle_timeout: Duration,
    #[serde(with = "humantime_serde")]
    pub handshake_timeout: Duration,
    pub max_memory_mb: usize,
    pub connection_pool_size: usize,
    pub enable_keepalive: bool,
    #[serde(with = "humantime_serde")]
    pub keepalive_interval: Duration,
}

/// Authentication configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AuthConfig {
    pub enabled: bool,
    pub method: String,
    pub users: Vec<UserConfig>,
}

/// User configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UserConfig {
    pub username: String,
    pub password: String,
    pub enabled: bool,
}

/// Access control configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AccessControlConfig {
    pub enabled: bool,
    pub default_policy: String,
    pub rules: Vec<AccessRule>,
}

/// Access control rule
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AccessRule {
    pub pattern: String,
    pub action: String,
    pub ports: Option<Vec<u16>>,
    pub countries: Option<Vec<String>>,
}

/// Routing configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RoutingConfig {
    pub enabled: bool,
    pub upstream_proxies: Vec<UpstreamProxyConfig>,
    pub rules: Vec<RoutingRuleConfig>,
    pub smart_routing: SmartRoutingConfigToml,
}

/// Smart routing configuration for TOML
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SmartRoutingConfigToml {
    pub enabled: bool,
    #[serde(with = "humantime_serde")]
    pub health_check_interval: Duration,
    #[serde(with = "humantime_serde")]
    pub health_check_timeout: Duration,
    pub min_measurements: usize,
    pub enable_latency_routing: bool,
    pub enable_health_routing: bool,
}

/// Routing rule configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RoutingRuleConfig {
    pub id: String,
    pub priority: u32,
    pub pattern: String,
    pub action: RoutingActionConfig,
    pub ports: Option<Vec<u16>>,
    pub source_ips: Option<Vec<String>>,
    pub users: Option<Vec<String>>,
    pub enabled: bool,
}

/// Routing action configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type", content = "config")]
pub enum RoutingActionConfig {
    Allow,
    Block { reason: Option<String> },
    Redirect { target: SocketAddr },
    Proxy { upstream_id: String },
    ProxyChain { upstream_ids: Vec<String> },
}

/// Upstream proxy configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UpstreamProxyConfig {
    pub name: String,
    pub addr: SocketAddr,
    pub protocol: String,
    pub auth: Option<ProxyAuthConfig>,
}

/// Proxy authentication configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProxyAuthConfig {
    pub username: String,
    pub password: String,
}

/// Monitoring configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MonitoringConfig {
    pub enabled: bool,
    pub metrics_addr: Option<SocketAddr>,
    pub log_level: String,
    pub prometheus_enabled: bool,
    pub collect_connection_stats: bool,
    pub max_historical_connections: usize,
    pub management_api: ManagementApiConfig,
}

/// Management API configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ManagementApiConfig {
    pub enabled: bool,
    pub bind_addr: SocketAddr,
    pub auth: crate::management::types::ApiAuthConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server: ServerConfig {
                bind_addr: "127.0.0.1:1080".parse().unwrap(),
                max_connections: 1000,
                connection_timeout: Duration::from_secs(300),
                buffer_size: 8192,
                shutdown_timeout: Duration::from_secs(30),
                idle_timeout: Duration::from_secs(60),
                handshake_timeout: Duration::from_secs(10),
                max_memory_mb: 512,
                connection_pool_size: 10,
                enable_keepalive: true,
                keepalive_interval: Duration::from_secs(30),
            },
            auth: AuthConfig {
                enabled: false,
                method: "none".to_string(),
                users: vec![],
            },
            access_control: AccessControlConfig {
                enabled: false,
                default_policy: "allow".to_string(),
                rules: vec![],
            },
            routing: RoutingConfig {
                enabled: false,
                upstream_proxies: vec![],
                rules: vec![],
                smart_routing: SmartRoutingConfigToml {
                    enabled: false,
                    health_check_interval: Duration::from_secs(30),
                    health_check_timeout: Duration::from_secs(5),
                    min_measurements: 3,
                    enable_latency_routing: true,
                    enable_health_routing: true,
                },
            },
            monitoring: MonitoringConfig {
                enabled: true,
                metrics_addr: Some("127.0.0.1:9090".parse().unwrap()),
                log_level: "info".to_string(),
                prometheus_enabled: true,
                collect_connection_stats: true,
                max_historical_connections: 10000,
                management_api: ManagementApiConfig {
                    enabled: true,
                    bind_addr: "127.0.0.1:8080".parse().unwrap(),
                    auth: crate::management::types::ApiAuthConfig::default(),
                },
            },
            security: SecurityConfig::default(),
        }
    }
}