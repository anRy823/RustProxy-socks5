//! Management API Handlers

use super::types::*;
use crate::config::{Config, UserConfig};
use crate::metrics::Metrics;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::SystemTime;
use tokio::sync::RwLock;
use tracing::{error, info};

/// Shared application state for handlers
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<RwLock<Config>>,
    pub metrics: Arc<Metrics>,
    pub start_time: SystemTime,
}

/// Query parameters for pagination
#[derive(Debug, Deserialize)]
pub struct PaginationQuery {
    pub page: Option<usize>,
    pub limit: Option<usize>,
}

/// Health check handler
pub async fn health_check() -> Json<ApiResponse<HealthStatus>> {
    let mut checks = HashMap::new();
    
    // Basic health check
    checks.insert(
        "server".to_string(),
        CheckResult {
            status: "healthy".to_string(),
            message: Some("Server is running".to_string()),
            duration_ms: 0,
        },
    );
    
    // Memory check (simplified)
    let memory_status = if get_memory_usage() < 90.0 {
        "healthy"
    } else {
        "warning"
    };
    
    checks.insert(
        "memory".to_string(),
        CheckResult {
            status: memory_status.to_string(),
            message: Some(format!("Memory usage: {:.1}%", get_memory_usage())),
            duration_ms: 1,
        },
    );
    
    let overall_status = if checks.values().all(|c| c.status == "healthy") {
        "healthy"
    } else {
        "degraded"
    };
    
    let health = HealthStatus {
        status: overall_status.to_string(),
        checks,
        timestamp: SystemTime::now(),
    };
    
    Json(ApiResponse::success(health))
}

/// Get server status
pub async fn get_server_status(State(state): State<AppState>) -> Json<ApiResponse<ServerStatus>> {
    let uptime = SystemTime::now()
        .duration_since(state.start_time)
        .unwrap_or_default()
        .as_secs();
    
    let _config = state.config.read().await;
    
    let status = ServerStatus {
        uptime_seconds: uptime,
        active_connections: state.metrics.get_active_connections(),
        total_connections: state.metrics.get_total_connections(),
        bytes_transferred: state.metrics.get_bytes_transferred(),
        memory_usage_mb: get_memory_usage(),
        cpu_usage_percent: get_cpu_usage(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        config_last_modified: SystemTime::now(), // TODO: Track actual config modification time
    };
    
    Json(ApiResponse::success(status))
}

/// Get current configuration
pub async fn get_config(State(state): State<AppState>) -> Json<ApiResponse<Config>> {
    let config = state.config.read().await;
    Json(ApiResponse::success((*config).clone()))
}

/// Update configuration
pub async fn update_config(
    State(state): State<AppState>,
    Json(request): Json<ConfigUpdateRequest>,
) -> Result<Json<ApiResponse<ValidationResult>>, StatusCode> {
    // Validate the new configuration
    match request.config.validate() {
        Ok(()) => {
            let validation = ValidationResult {
                valid: true,
                errors: vec![],
                warnings: vec![],
            };
            
            if !request.validate_only {
                // Apply the configuration
                let mut config = state.config.write().await;
                *config = request.config;
                info!("Configuration updated via management API");
            }
            
            Ok(Json(ApiResponse::success(validation)))
        }
        Err(e) => {
            let validation = ValidationResult {
                valid: false,
                errors: vec![e.to_string()],
                warnings: vec![],
            };
            Ok(Json(ApiResponse::success(validation)))
        }
    }
}

/// Get active connections
pub async fn get_connections(
    State(state): State<AppState>,
    Query(pagination): Query<PaginationQuery>,
) -> Json<ApiResponse<Vec<ConnectionInfo>>> {
    let connections = state.metrics.get_active_connection_info();
    
    let page = pagination.page.unwrap_or(1);
    let limit = pagination.limit.unwrap_or(50).min(1000); // Cap at 1000
    let start = (page - 1) * limit;
    let _end = start + limit;
    
    let paginated: Vec<ConnectionInfo> = connections
        .into_iter()
        .skip(start)
        .take(limit)
        .collect();
    
    Json(ApiResponse::success(paginated))
}

/// Get statistics summary
pub async fn get_stats(State(state): State<AppState>) -> Json<ApiResponse<StatsSummary>> {
    let uptime = SystemTime::now()
        .duration_since(state.start_time)
        .unwrap_or_default()
        .as_secs();
    
    let stats = StatsSummary {
        total_connections: state.metrics.get_total_connections(),
        active_connections: state.metrics.get_active_connections(),
        bytes_transferred: state.metrics.get_bytes_transferred(),
        auth_attempts: state.metrics.get_auth_attempts(),
        auth_failures: state.metrics.get_auth_failures(),
        blocked_requests: state.metrics.get_blocked_requests(),
        uptime_seconds: uptime,
        top_destinations: state.metrics.get_top_destinations(10),
        top_users: state.metrics.get_top_users(10),
    };
    
    Json(ApiResponse::success(stats))
}

/// Create a new user
pub async fn create_user(
    State(state): State<AppState>,
    Json(request): Json<CreateUserRequest>,
) -> Result<Json<ApiResponse<UserInfo>>, StatusCode> {
    // Validate username
    if request.username.is_empty() || request.username.len() > 255 {
        return Ok(Json(ApiResponse::error(
            "Username must be between 1 and 255 characters".to_string(),
        )));
    }
    
    // Validate password
    if request.password.is_empty() || request.password.len() > 255 {
        return Ok(Json(ApiResponse::error(
            "Password must be between 1 and 255 characters".to_string(),
        )));
    }
    
    let mut config = state.config.write().await;
    
    // Check if user already exists
    if config.auth.users.iter().any(|u| u.username == request.username) {
        return Ok(Json(ApiResponse::error(
            "User already exists".to_string(),
        )));
    }
    
    // Add new user
    let new_user = UserConfig {
        username: request.username.clone(),
        password: request.password,
        enabled: request.enabled,
    };
    
    config.auth.users.push(new_user);
    
    let user_info = UserInfo {
        username: request.username,
        enabled: request.enabled,
        created_at: SystemTime::now(),
        last_login: None,
        connection_count: 0,
    };
    
    info!("User created via management API: {}", user_info.username);
    Ok(Json(ApiResponse::success(user_info)))
}

/// Get user information
pub async fn get_user(
    State(state): State<AppState>,
    Path(username): Path<String>,
) -> Json<ApiResponse<UserInfo>> {
    let config = state.config.read().await;
    
    if let Some(user) = config.auth.users.iter().find(|u| u.username == username) {
        let user_info = UserInfo {
            username: user.username.clone(),
            enabled: user.enabled,
            created_at: SystemTime::now(), // TODO: Track actual creation time
            last_login: None,               // TODO: Track last login
            connection_count: 0,            // TODO: Get from metrics
        };
        Json(ApiResponse::success(user_info))
    } else {
        Json(ApiResponse::error("User not found".to_string()))
    }
}

/// Delete a user
pub async fn delete_user(
    State(state): State<AppState>,
    Path(username): Path<String>,
) -> Json<ApiResponse<()>> {
    let mut config = state.config.write().await;
    
    let initial_len = config.auth.users.len();
    config.auth.users.retain(|u| u.username != username);
    
    if config.auth.users.len() < initial_len {
        info!("User deleted via management API: {}", username);
        Json(ApiResponse::success(()))
    } else {
        Json(ApiResponse::error("User not found".to_string()))
    }
}

/// Export metrics in various formats
pub async fn export_metrics(
    State(state): State<AppState>,
    Json(request): Json<MetricsExportRequest>,
) -> Result<String, StatusCode> {
    match request.format.as_str() {
        "prometheus" => {
            let metrics = state.metrics.export_prometheus();
            Ok(metrics)
        }
        "json" => {
            let stats = StatsSummary {
                total_connections: state.metrics.get_total_connections(),
                active_connections: state.metrics.get_active_connections(),
                bytes_transferred: state.metrics.get_bytes_transferred(),
                auth_attempts: state.metrics.get_auth_attempts(),
                auth_failures: state.metrics.get_auth_failures(),
                blocked_requests: state.metrics.get_blocked_requests(),
                uptime_seconds: SystemTime::now()
                    .duration_since(state.start_time)
                    .unwrap_or_default()
                    .as_secs(),
                top_destinations: state.metrics.get_top_destinations(10),
                top_users: state.metrics.get_top_users(10),
            };
            
            match serde_json::to_string_pretty(&stats) {
                Ok(json) => Ok(json),
                Err(e) => {
                    error!("Failed to serialize metrics to JSON: {}", e);
                    Err(StatusCode::INTERNAL_SERVER_ERROR)
                }
            }
        }
        _ => Err(StatusCode::BAD_REQUEST),
    }
}

/// Reload configuration from file
pub async fn reload_config(State(_state): State<AppState>) -> Json<ApiResponse<()>> {
    // This would typically trigger a config reload from the watcher
    // For now, we'll just return success
    info!("Configuration reload requested via management API");
    Json(ApiResponse::success(()))
}

// Helper functions for system metrics (simplified implementations)
fn get_memory_usage() -> f64 {
    // Simplified memory usage calculation
    // In a real implementation, you'd use system APIs or crates like `sysinfo`
    50.0 // Placeholder value
}

fn get_cpu_usage() -> f64 {
    // Simplified CPU usage calculation
    // In a real implementation, you'd use system APIs or crates like `sysinfo`
    25.0 // Placeholder value
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::metrics::Metrics;
    use std::sync::Arc;
    use tokio::sync::RwLock;
    
    fn create_test_state() -> AppState {
        AppState {
            config: Arc::new(RwLock::new(Config::default())),
            metrics: Arc::new(Metrics::new()),
            start_time: SystemTime::now(),
        }
    }
    
    #[tokio::test]
    async fn test_health_check() {
        let response = health_check().await;
        assert!(response.0.success);
        assert!(response.0.data.is_some());
    }
    
    #[tokio::test]
    async fn test_get_server_status() {
        let state = create_test_state();
        let response = get_server_status(State(state)).await;
        assert!(response.0.success);
        assert!(response.0.data.is_some());
    }
    
    #[tokio::test]
    async fn test_create_user() {
        let state = create_test_state();
        let request = CreateUserRequest {
            username: "testuser".to_string(),
            password: "testpass".to_string(),
            enabled: true,
        };
        
        let response = create_user(State(state.clone()), Json(request)).await.unwrap();
        assert!(response.0.success);
        
        // Verify user was added to config
        let config = state.config.read().await;
        assert!(config.auth.users.iter().any(|u| u.username == "testuser"));
    }
    
    #[tokio::test]
    async fn test_create_duplicate_user() {
        let state = create_test_state();
        
        // Add initial user
        {
            let mut config = state.config.write().await;
            config.auth.users.push(UserConfig {
                username: "existing".to_string(),
                password: "pass".to_string(),
                enabled: true,
            });
        }
        
        // Try to create duplicate
        let request = CreateUserRequest {
            username: "existing".to_string(),
            password: "newpass".to_string(),
            enabled: true,
        };
        
        let response = create_user(State(state), Json(request)).await.unwrap();
        assert!(!response.0.success);
        assert!(response.0.error.is_some());
    }
}