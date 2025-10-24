//! Management API Types

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::SystemTime;
use crate::config::Config;

/// API response wrapper
#[derive(Debug, Serialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<String>,
    pub timestamp: SystemTime,
}

impl<T> ApiResponse<T> {
    pub fn success(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
            timestamp: SystemTime::now(),
        }
    }
    
    pub fn error(message: String) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(message),
            timestamp: SystemTime::now(),
        }
    }
}

/// Server status information
#[derive(Debug, Serialize)]
pub struct ServerStatus {
    pub uptime_seconds: u64,
    pub active_connections: usize,
    pub total_connections: u64,
    pub bytes_transferred: u64,
    pub memory_usage_mb: f64,
    pub cpu_usage_percent: f64,
    pub version: String,
    pub config_last_modified: SystemTime,
}

/// Connection information
#[derive(Debug, Serialize)]
pub struct ConnectionInfo {
    pub id: String,
    pub client_addr: SocketAddr,
    pub target_addr: Option<SocketAddr>,
    pub user_id: Option<String>,
    pub start_time: SystemTime,
    pub bytes_up: u64,
    pub bytes_down: u64,
    pub status: String,
}

/// User management request
#[derive(Debug, Deserialize)]
pub struct CreateUserRequest {
    pub username: String,
    pub password: String,
    pub enabled: bool,
}

/// User management response
#[derive(Debug, Serialize)]
pub struct UserInfo {
    pub username: String,
    pub enabled: bool,
    pub created_at: SystemTime,
    pub last_login: Option<SystemTime>,
    pub connection_count: u64,
}

/// Configuration update request
#[derive(Debug, Deserialize)]
pub struct ConfigUpdateRequest {
    pub config: Config,
    pub validate_only: bool,
}

/// Statistics summary
#[derive(Debug, Serialize)]
pub struct StatsSummary {
    pub total_connections: u64,
    pub active_connections: usize,
    pub bytes_transferred: u64,
    pub auth_attempts: u64,
    pub auth_failures: u64,
    pub blocked_requests: u64,
    pub uptime_seconds: u64,
    pub top_destinations: Vec<DestinationStats>,
    pub top_users: Vec<UserStats>,
}

/// Destination statistics
#[derive(Debug, Serialize)]
pub struct DestinationStats {
    pub destination: String,
    pub connection_count: u64,
    pub bytes_transferred: u64,
}

/// User statistics
#[derive(Debug, Serialize)]
pub struct UserStats {
    pub username: String,
    pub connection_count: u64,
    pub bytes_transferred: u64,
    pub last_activity: SystemTime,
}

/// Access control rule management
#[derive(Debug, Deserialize)]
pub struct CreateRuleRequest {
    pub pattern: String,
    pub action: String,
    pub ports: Option<Vec<u16>>,
    pub countries: Option<Vec<String>>,
    pub enabled: bool,
}

/// Rule information
#[derive(Debug, Serialize)]
pub struct RuleInfo {
    pub id: String,
    pub pattern: String,
    pub action: String,
    pub ports: Option<Vec<u16>>,
    pub countries: Option<Vec<String>>,
    pub enabled: bool,
    pub created_at: SystemTime,
    pub hit_count: u64,
}

/// Health check response
#[derive(Debug, Serialize)]
pub struct HealthStatus {
    pub status: String,
    pub checks: HashMap<String, CheckResult>,
    pub timestamp: SystemTime,
}

/// Individual health check result
#[derive(Debug, Serialize)]
pub struct CheckResult {
    pub status: String,
    pub message: Option<String>,
    pub duration_ms: u64,
}

/// Log entry for audit trail
#[derive(Debug, Serialize)]
pub struct AuditLogEntry {
    pub timestamp: SystemTime,
    pub user: String,
    pub action: String,
    pub resource: String,
    pub details: Option<String>,
    pub success: bool,
}

/// Metrics export format
#[derive(Debug, Deserialize)]
pub struct MetricsExportRequest {
    pub format: String, // "prometheus", "json"
    pub include_histograms: bool,
}

/// Configuration validation result
#[derive(Debug, Serialize)]
pub struct ValidationResult {
    pub valid: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

/// API authentication configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ApiAuthConfig {
    pub enabled: bool,
    pub api_key: Option<String>,
    pub basic_auth: Option<BasicAuthConfig>,
    pub jwt: Option<JwtConfig>,
}

/// Basic authentication configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BasicAuthConfig {
    pub username: String,
    pub password: String,
}

/// JWT authentication configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct JwtConfig {
    pub secret: String,
    pub expiry_hours: u64,
    pub issuer: String,
}

impl Default for ApiAuthConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            api_key: Some("default-api-key-change-me".to_string()),
            basic_auth: None,
            jwt: None,
        }
    }
}