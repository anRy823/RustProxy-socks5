//! Metrics Manager
//! 
//! Coordinates metrics collection, export, and reporting

use super::{Metrics, MetricsServer, ConnectionInsights};
use crate::config::MonitoringConfig;
use std::sync::Arc;
use tokio::task::JoinHandle;
use tracing::{info, error, warn};

/// Metrics management system
pub struct MetricsManager {
    metrics: Arc<Metrics>,
    insights: ConnectionInsights,
    server_handle: Option<JoinHandle<()>>,
    config: MonitoringConfig,
}

impl MetricsManager {
    /// Create a new metrics manager
    pub async fn new(config: MonitoringConfig) -> anyhow::Result<Self> {
        let metrics = Arc::new(Metrics::new());
        let insights = ConnectionInsights::new(metrics.clone());
        
        Ok(Self {
            metrics,
            insights,
            server_handle: None,
            config,
        })
    }
    
    /// Start the metrics system
    pub async fn start(&mut self) -> anyhow::Result<()> {
        if !self.config.enabled {
            info!("Metrics collection is disabled");
            return Ok(());
        }
        
        info!("Starting metrics system");
        
        // Start Prometheus metrics server if enabled
        if self.config.prometheus_enabled {
            if let Some(metrics_addr) = self.config.metrics_addr {
                let server = MetricsServer::new(
                    self.metrics.clone(),
                    metrics_addr.to_string(),
                );
                
                let handle = tokio::spawn(async move {
                    if let Err(e) = server.start().await {
                        error!(error = %e, "Metrics server failed");
                    }
                });
                
                self.server_handle = Some(handle);
                info!(addr = %metrics_addr, "Prometheus metrics server started");
            } else {
                warn!("Prometheus enabled but no metrics_addr configured");
            }
        }
        
        Ok(())
    }
    
    /// Stop the metrics system
    pub async fn stop(&mut self) {
        info!("Stopping metrics system");
        
        if let Some(handle) = self.server_handle.take() {
            handle.abort();
            info!("Metrics server stopped");
        }
    }
    
    /// Get metrics collector reference
    pub fn metrics(&self) -> Arc<Metrics> {
        self.metrics.clone()
    }
    
    /// Get connection insights generator
    pub fn insights(&self) -> &ConnectionInsights {
        &self.insights
    }
    
    /// Generate and log daily report
    pub async fn generate_daily_report(&self) -> anyhow::Result<()> {
        if !self.config.enabled {
            return Ok(());
        }
        
        info!("Generating daily usage report");
        
        match self.insights.generate_daily_report().await {
            Ok(report) => {
                info!(
                    report_id = %report.report_id,
                    total_connections = report.summary.total_connections,
                    total_bytes = report.summary.total_bytes_transferred,
                    unique_users = report.summary.unique_users,
                    "Daily report generated"
                );
                
                // Log insights
                match self.insights.generate_insights().await {
                    Ok(insights) => {
                        for insight in insights {
                            info!(insight = %insight, "Connection insight");
                        }
                    }
                    Err(e) => {
                        error!(error = %e, "Failed to generate insights");
                    }
                }
            }
            Err(e) => {
                error!(error = %e, "Failed to generate daily report");
            }
        }
        
        Ok(())
    }
    
    /// Get current activity summary
    pub async fn get_current_activity(&self) -> anyhow::Result<super::ActivitySummary> {
        self.insights.get_realtime_stats().await
    }
    
    /// Record connection start
    pub async fn record_connection_start(
        &self,
        session_id: String,
        client_addr: std::net::SocketAddr,
        target_addr: std::net::SocketAddr,
        user_id: Option<String>,
    ) -> anyhow::Result<()> {
        if self.config.collect_connection_stats {
            self.metrics.start_connection(session_id, client_addr, target_addr, user_id)?;
        }
        Ok(())
    }
    
    /// Record connection end
    pub async fn record_connection_end(&self, session_id: &str) -> anyhow::Result<()> {
        if self.config.collect_connection_stats {
            self.metrics.end_connection(session_id)?;
        }
        Ok(())
    }
    
    /// Update connection bytes
    pub async fn update_connection_bytes(
        &self,
        session_id: &str,
        bytes_up: u64,
        bytes_down: u64,
    ) -> anyhow::Result<()> {
        if self.config.collect_connection_stats {
            self.metrics.update_connection_bytes(session_id, bytes_up, bytes_down)?;
        }
        Ok(())
    }
    
    /// Record authentication attempt
    pub fn record_auth_attempt(&self, success: bool) {
        if self.config.enabled {
            self.metrics.increment_auth_attempts(success);
        }
    }
    
    /// Record blocked request
    pub fn record_blocked_request(&self, reason: &str) {
        if self.config.enabled {
            self.metrics.record_blocked_request(reason);
        }
    }
}

impl Drop for MetricsManager {
    fn drop(&mut self) {
        if let Some(handle) = self.server_handle.take() {
            handle.abort();
        }
    }
}