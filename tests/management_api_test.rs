//! Management API Integration Tests

use rustproxy::{
    config::Config,
    management::{ManagementServer, types::ApiAuthConfig},
    metrics::Metrics,
};
use std::sync::Arc;
use tokio::sync::RwLock;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use tower::ServiceExt;

#[tokio::test]
async fn test_management_api_health_endpoint() {
    // Create test configuration
    let config = Arc::new(RwLock::new(Config::default()));
    let metrics = Arc::new(Metrics::new());
    
    // Configure API authentication (disabled for testing)
    let auth_config = ApiAuthConfig {
        enabled: false,
        api_key: None,
        basic_auth: None,
        jwt: None,
    };
    
    // Create management server
    let management_server = ManagementServer::new(
        "127.0.0.1:8080".parse().unwrap(),
        config,
        metrics,
        auth_config,
    );
    
    // Create test router
    let app = management_server.create_test_router();
    
    // Test health endpoint
    let request = Request::builder()
        .uri("/api/v1/health")
        .body(Body::empty())
        .unwrap();
    
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_management_api_status_endpoint() {
    // Create test configuration
    let config = Arc::new(RwLock::new(Config::default()));
    let metrics = Arc::new(Metrics::new());
    
    // Configure API authentication (disabled for testing)
    let auth_config = ApiAuthConfig {
        enabled: false,
        api_key: None,
        basic_auth: None,
        jwt: None,
    };
    
    // Create management server
    let management_server = ManagementServer::new(
        "127.0.0.1:8080".parse().unwrap(),
        config,
        metrics,
        auth_config,
    );
    
    // Create test router
    let app = management_server.create_test_router();
    
    // Test status endpoint
    let request = Request::builder()
        .uri("/api/v1/status")
        .body(Body::empty())
        .unwrap();
    
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_management_api_config_endpoint() {
    // Create test configuration
    let config = Arc::new(RwLock::new(Config::default()));
    let metrics = Arc::new(Metrics::new());
    
    // Configure API authentication (disabled for testing)
    let auth_config = ApiAuthConfig {
        enabled: false,
        api_key: None,
        basic_auth: None,
        jwt: None,
    };
    
    // Create management server
    let management_server = ManagementServer::new(
        "127.0.0.1:8080".parse().unwrap(),
        config,
        metrics,
        auth_config,
    );
    
    // Create test router
    let app = management_server.create_test_router();
    
    // Test config endpoint
    let request = Request::builder()
        .uri("/api/v1/config")
        .body(Body::empty())
        .unwrap();
    
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_management_api_stats_endpoint() {
    // Create test configuration
    let config = Arc::new(RwLock::new(Config::default()));
    let metrics = Arc::new(Metrics::new());
    
    // Configure API authentication (disabled for testing)
    let auth_config = ApiAuthConfig {
        enabled: false,
        api_key: None,
        basic_auth: None,
        jwt: None,
    };
    
    // Create management server
    let management_server = ManagementServer::new(
        "127.0.0.1:8080".parse().unwrap(),
        config,
        metrics,
        auth_config,
    );
    
    // Create test router
    let app = management_server.create_test_router();
    
    // Test stats endpoint
    let request = Request::builder()
        .uri("/api/v1/stats")
        .body(Body::empty())
        .unwrap();
    
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_management_api_connections_endpoint() {
    // Create test configuration
    let config = Arc::new(RwLock::new(Config::default()));
    let metrics = Arc::new(Metrics::new());
    
    // Configure API authentication (disabled for testing)
    let auth_config = ApiAuthConfig {
        enabled: false,
        api_key: None,
        basic_auth: None,
        jwt: None,
    };
    
    // Create management server
    let management_server = ManagementServer::new(
        "127.0.0.1:8080".parse().unwrap(),
        config,
        metrics,
        auth_config,
    );
    
    // Create test router
    let app = management_server.create_test_router();
    
    // Test connections endpoint
    let request = Request::builder()
        .uri("/api/v1/connections")
        .body(Body::empty())
        .unwrap();
    
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_management_api_authentication() {
    // Create test configuration
    let config = Arc::new(RwLock::new(Config::default()));
    let metrics = Arc::new(Metrics::new());
    
    // Configure API authentication (enabled for testing)
    let auth_config = ApiAuthConfig {
        enabled: true,
        api_key: Some("test-api-key".to_string()),
        basic_auth: None,
        jwt: None,
    };
    
    // Create management server
    let management_server = ManagementServer::new(
        "127.0.0.1:8080".parse().unwrap(),
        config,
        metrics,
        auth_config,
    );
    
    // Create test router
    let app = management_server.create_test_router();
    
    // Test protected endpoint without authentication - should fail
    let request = Request::builder()
        .uri("/api/v1/status")
        .body(Body::empty())
        .unwrap();
    
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    
    // Test protected endpoint with correct API key - should succeed
    let request = Request::builder()
        .uri("/api/v1/status")
        .header("x-api-key", "test-api-key")
        .body(Body::empty())
        .unwrap();
    
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    
    // Test protected endpoint with incorrect API key - should fail
    let request = Request::builder()
        .uri("/api/v1/status")
        .header("x-api-key", "wrong-key")
        .body(Body::empty())
        .unwrap();
    
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_management_api_user_management() {
    use serde_json::json;
    
    // Create test configuration
    let config = Arc::new(RwLock::new(Config::default()));
    let metrics = Arc::new(Metrics::new());
    
    // Configure API authentication (disabled for testing)
    let auth_config = ApiAuthConfig {
        enabled: false,
        api_key: None,
        basic_auth: None,
        jwt: None,
    };
    
    // Create management server
    let management_server = ManagementServer::new(
        "127.0.0.1:8080".parse().unwrap(),
        config,
        metrics,
        auth_config,
    );
    
    // Create test router
    let app = management_server.create_test_router();
    
    // Test creating a new user
    let user_data = json!({
        "username": "testuser",
        "password": "testpass",
        "enabled": true
    });
    
    let request = Request::builder()
        .uri("/api/v1/users")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(user_data.to_string()))
        .unwrap();
    
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    
    // Test getting the created user
    let request = Request::builder()
        .uri("/api/v1/users/testuser")
        .body(Body::empty())
        .unwrap();
    
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    
    // Test deleting the user
    let request = Request::builder()
        .uri("/api/v1/users/testuser")
        .method("DELETE")
        .body(Body::empty())
        .unwrap();
    
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}