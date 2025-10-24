//! Management API Server

use super::{
    api::ManagementApi,
    handlers::AppState,
    types::ApiAuthConfig,
};
use crate::{config::Config, metrics::Metrics, Result};
use anyhow::Context;
use axum::Router;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::SystemTime;
use tokio::{net::TcpListener, sync::RwLock};
use tracing::{error, info};

/// Management API server
pub struct ManagementServer {
    bind_addr: SocketAddr,
    app_state: AppState,
    auth_config: ApiAuthConfig,
}

impl ManagementServer {
    /// Create a new management server
    pub fn new(
        bind_addr: SocketAddr,
        config: Arc<RwLock<Config>>,
        metrics: Arc<Metrics>,
        auth_config: ApiAuthConfig,
    ) -> Self {
        let app_state = AppState {
            config,
            metrics,
            start_time: SystemTime::now(),
        };
        
        Self {
            bind_addr,
            app_state,
            auth_config,
        }
    }
    
    /// Start the management API server
    pub async fn start(self) -> Result<()> {
        info!("Starting management API server on {}", self.bind_addr);
        
        // Create the router
        let app = ManagementApi::create_router(self.app_state, self.auth_config);
        
        // Create TCP listener
        let listener = TcpListener::bind(self.bind_addr)
            .await
            .with_context(|| format!("Failed to bind management API server to {}", self.bind_addr))?;
        
        info!("Management API server listening on {}", self.bind_addr);
        
        // Start serving
        if let Err(e) = axum::serve(listener, app).await {
            error!("Management API server error: {}", e);
            return Err(e.into());
        }
        
        Ok(())
    }
    
    /// Create a router for testing
    pub fn create_test_router(&self) -> Router {
        ManagementApi::create_router(self.app_state.clone(), self.auth_config.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::metrics::Metrics;
    use std::sync::Arc;
    use tokio::sync::RwLock;
    
    #[tokio::test]
    async fn test_management_server_creation() {
        let config = Arc::new(RwLock::new(Config::default()));
        let metrics = Arc::new(Metrics::new());
        let auth_config = ApiAuthConfig::default();
        let bind_addr = "127.0.0.1:8080".parse().unwrap();
        
        let server = ManagementServer::new(bind_addr, config, metrics, auth_config);
        
        // Test that we can create a router
        let _router = server.create_test_router();
    }
}