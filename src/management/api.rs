//! Management API Routes

use super::{
    auth::{auth_middleware, ApiAuth},
    handlers::*,
    types::ApiAuthConfig,
};
use axum::{
    middleware,
    routing::{delete, get, post, put},
    Router,
};
use std::sync::Arc;
use tower_http::cors::CorsLayer;

/// Management API router
pub struct ManagementApi;

impl ManagementApi {
    /// Create the management API router
    pub fn create_router(state: AppState, auth_config: ApiAuthConfig) -> Router {
        let auth = Arc::new(ApiAuth::new(auth_config));
        
        // Public routes (no authentication required)
        let public_routes = Router::new()
            .route("/health", get(health_check));
        
        // Protected routes (authentication required)
        let protected_routes = Router::new()
            // Server management
            .route("/status", get(get_server_status))
            .route("/config", get(get_config))
            .route("/config", put(update_config))
            .route("/config/reload", post(reload_config))
            
            // Connection management
            .route("/connections", get(get_connections))
            
            // Statistics and metrics
            .route("/stats", get(get_stats))
            .route("/metrics/export", post(export_metrics))
            
            // User management
            .route("/users", post(create_user))
            .route("/users/:username", get(get_user))
            .route("/users/:username", delete(delete_user))
            
            // Add authentication middleware to protected routes
            .layer(middleware::from_fn_with_state(auth.clone(), auth_middleware))
            .with_state(state);
        
        // Combine public and protected routes
        Router::new()
            .nest("/api/v1", public_routes.merge(protected_routes))
            .layer(CorsLayer::permissive()) // Configure CORS as needed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::metrics::Metrics;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use std::sync::Arc;
    use std::time::SystemTime;
    use tokio::sync::RwLock;
    use tower::ServiceExt;
    
    fn create_test_state() -> AppState {
        AppState {
            config: Arc::new(RwLock::new(Config::default())),
            metrics: Arc::new(Metrics::new()),
            start_time: SystemTime::now(),
        }
    }
    
    #[tokio::test]
    async fn test_public_health_endpoint() {
        let state = create_test_state();
        let auth_config = ApiAuthConfig {
            enabled: false,
            ..Default::default()
        };
        
        let app = ManagementApi::create_router(state, auth_config);
        
        let request = Request::builder()
            .uri("/api/v1/health")
            .body(Body::empty())
            .unwrap();
        
        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }
    
    #[tokio::test]
    async fn test_protected_endpoint_without_auth() {
        let state = create_test_state();
        let auth_config = ApiAuthConfig {
            enabled: true,
            api_key: Some("test-key".to_string()),
            ..Default::default()
        };
        
        let app = ManagementApi::create_router(state, auth_config);
        
        let request = Request::builder()
            .uri("/api/v1/status")
            .body(Body::empty())
            .unwrap();
        
        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
    
    #[tokio::test]
    async fn test_protected_endpoint_with_auth() {
        let state = create_test_state();
        let auth_config = ApiAuthConfig {
            enabled: true,
            api_key: Some("test-key".to_string()),
            ..Default::default()
        };
        
        let app = ManagementApi::create_router(state, auth_config);
        
        let request = Request::builder()
            .uri("/api/v1/status")
            .header("x-api-key", "test-key")
            .body(Body::empty())
            .unwrap();
        
        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }
}