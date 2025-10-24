//! Smart Routing Implementation
//! 
//! Provides intelligent routing decisions based on latency measurements,
//! health checks, and performance metrics for upstream proxies.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::net::TcpStream;
use tokio::sync::RwLock;
use tokio::time::timeout;
use tracing::{debug, warn, info};

use crate::Result;
use super::UpstreamProxy;

/// Health status of an upstream proxy
#[derive(Debug, Clone, PartialEq)]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
    Unknown,
}

/// Performance metrics for an upstream proxy
#[derive(Debug, Clone)]
pub struct ProxyMetrics {
    /// Average latency over recent measurements
    pub avg_latency: Duration,
    /// Success rate (0.0 to 1.0)
    pub success_rate: f64,
    /// Number of recent connections
    pub connection_count: u64,
    /// Last health check timestamp
    pub last_health_check: Instant,
    /// Current health status
    pub health_status: HealthStatus,
    /// Recent latency measurements (circular buffer)
    pub recent_latencies: Vec<Duration>,
    /// Recent success/failure results
    pub recent_results: Vec<bool>,
}

impl ProxyMetrics {
    /// Create new metrics with default values
    pub fn new() -> Self {
        Self {
            avg_latency: Duration::from_millis(0),
            success_rate: 1.0,
            connection_count: 0,
            last_health_check: Instant::now(),
            health_status: HealthStatus::Unknown,
            recent_latencies: Vec::new(),
            recent_results: Vec::new(),
        }
    }

    /// Update metrics with a new latency measurement
    pub fn record_latency(&mut self, latency: Duration, success: bool) {
        // Keep only recent measurements (last 10)
        const MAX_MEASUREMENTS: usize = 10;
        
        self.recent_latencies.push(latency);
        if self.recent_latencies.len() > MAX_MEASUREMENTS {
            self.recent_latencies.remove(0);
        }
        
        self.recent_results.push(success);
        if self.recent_results.len() > MAX_MEASUREMENTS {
            self.recent_results.remove(0);
        }
        
        // Recalculate average latency
        if !self.recent_latencies.is_empty() {
            let total: Duration = self.recent_latencies.iter().sum();
            self.avg_latency = total / self.recent_latencies.len() as u32;
        }
        
        // Recalculate success rate
        if !self.recent_results.is_empty() {
            let successes = self.recent_results.iter().filter(|&&r| r).count();
            self.success_rate = successes as f64 / self.recent_results.len() as f64;
        }
        
        self.connection_count += 1;
        
        // Update health status based on metrics
        self.update_health_status();
    }

    /// Update health status based on current metrics
    fn update_health_status(&mut self) {
        if self.recent_results.is_empty() {
            self.health_status = HealthStatus::Unknown;
            return;
        }

        // Consider proxy unhealthy if success rate is below 50%
        if self.success_rate < 0.5 {
            self.health_status = HealthStatus::Unhealthy;
        }
        // Consider proxy degraded if success rate is below 80% or latency is high
        else if self.success_rate < 0.8 || self.avg_latency > Duration::from_millis(5000) {
            self.health_status = HealthStatus::Degraded;
        }
        // Otherwise, consider it healthy
        else {
            self.health_status = HealthStatus::Healthy;
        }
    }

    /// Get a score for this proxy (higher is better)
    pub fn get_score(&self) -> f64 {
        match self.health_status {
            HealthStatus::Unhealthy => 0.0,
            HealthStatus::Unknown => 0.1,
            HealthStatus::Degraded => {
                // Score based on success rate and latency
                let latency_score = 1.0 / (1.0 + self.avg_latency.as_millis() as f64 / 1000.0);
                (self.success_rate + latency_score) / 2.0 * 0.5
            },
            HealthStatus::Healthy => {
                // Score based on success rate and latency
                let latency_score = 1.0 / (1.0 + self.avg_latency.as_millis() as f64 / 1000.0);
                (self.success_rate + latency_score) / 2.0
            },
        }
    }
}

impl Default for ProxyMetrics {
    fn default() -> Self {
        Self::new()
    }
}

/// Smart routing configuration
#[derive(Debug, Clone)]
pub struct SmartRoutingConfig {
    /// Health check interval
    pub health_check_interval: Duration,
    /// Connection timeout for health checks
    pub health_check_timeout: Duration,
    /// Minimum number of measurements before making routing decisions
    pub min_measurements: usize,
    /// Enable latency-based routing
    pub enable_latency_routing: bool,
    /// Enable health-based routing
    pub enable_health_routing: bool,
}

impl Default for SmartRoutingConfig {
    fn default() -> Self {
        Self {
            health_check_interval: Duration::from_secs(30),
            health_check_timeout: Duration::from_secs(5),
            min_measurements: 3,
            enable_latency_routing: true,
            enable_health_routing: true,
        }
    }
}

/// Smart routing manager
pub struct SmartRoutingManager {
    config: SmartRoutingConfig,
    metrics: Arc<RwLock<HashMap<String, ProxyMetrics>>>,
    upstream_proxies: HashMap<String, UpstreamProxy>,
}

impl SmartRoutingManager {
    /// Create a new smart routing manager
    pub fn new(config: SmartRoutingConfig) -> Self {
        Self {
            config,
            metrics: Arc::new(RwLock::new(HashMap::new())),
            upstream_proxies: HashMap::new(),
        }
    }

    /// Add an upstream proxy to be managed
    pub async fn add_upstream_proxy(&mut self, id: String, proxy: UpstreamProxy) {
        self.upstream_proxies.insert(id.clone(), proxy);
        
        // Initialize metrics for this proxy
        let mut metrics_guard = self.metrics.write().await;
        metrics_guard.insert(id, ProxyMetrics::new());
    }

    /// Remove an upstream proxy
    pub async fn remove_upstream_proxy(&mut self, id: &str) {
        self.upstream_proxies.remove(id);
        
        // Remove metrics for this proxy
        let mut metrics_guard = self.metrics.write().await;
        metrics_guard.remove(id);
    }

    /// Select the best upstream proxy based on current metrics
    pub async fn select_best_proxy(&self, exclude_ids: &[String]) -> Option<(String, UpstreamProxy)> {
        let metrics_guard = self.metrics.read().await;
        
        let mut best_proxy: Option<(String, UpstreamProxy, f64)> = None;
        
        for (id, proxy) in &self.upstream_proxies {
            // Skip excluded proxies
            if exclude_ids.contains(id) {
                continue;
            }
            
            let score = if let Some(metrics) = metrics_guard.get(id) {
                // Skip unhealthy proxies if health routing is enabled
                if self.config.enable_health_routing && metrics.health_status == HealthStatus::Unhealthy {
                    continue;
                }
                
                // Require minimum measurements for latency-based routing
                if self.config.enable_latency_routing && 
                   metrics.recent_latencies.len() < self.config.min_measurements {
                    0.5 // Default score for proxies without enough measurements
                } else {
                    metrics.get_score()
                }
            } else {
                0.5 // Default score for proxies without metrics
            };
            
            match &best_proxy {
                None => {
                    best_proxy = Some((id.clone(), proxy.clone(), score));
                },
                Some((_, _, best_score)) => {
                    if score > *best_score {
                        best_proxy = Some((id.clone(), proxy.clone(), score));
                    }
                },
            }
        }
        
        if let Some((id, proxy, score)) = best_proxy {
            debug!("Selected proxy '{}' with score {:.3}", id, score);
            Some((id, proxy))
        } else {
            debug!("No suitable proxy found");
            None
        }
    }

    /// Record a connection attempt result
    pub async fn record_connection_result(&self, proxy_id: &str, latency: Duration, success: bool) {
        let mut metrics_guard = self.metrics.write().await;
        
        if let Some(metrics) = metrics_guard.get_mut(proxy_id) {
            metrics.record_latency(latency, success);
            debug!("Recorded connection result for '{}': latency={:?}, success={}, score={:.3}", 
                   proxy_id, latency, success, metrics.get_score());
        }
    }

    /// Start background health checking
    pub async fn start_health_checking(&self) {
        let metrics = Arc::clone(&self.metrics);
        let proxies = self.upstream_proxies.clone();
        let config = self.config.clone();
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(config.health_check_interval);
            
            loop {
                interval.tick().await;
                
                for (id, proxy) in &proxies {
                    let proxy_id = id.clone();
                    let proxy_addr = proxy.addr;
                    let timeout_duration = config.health_check_timeout;
                    let metrics_clone = Arc::clone(&metrics);
                    
                    tokio::spawn(async move {
                        let start_time = Instant::now();
                        let result = Self::health_check_proxy(proxy_addr, timeout_duration).await;
                        let latency = start_time.elapsed();
                        
                        let mut metrics_guard = metrics_clone.write().await;
                        if let Some(proxy_metrics) = metrics_guard.get_mut(&proxy_id) {
                            proxy_metrics.last_health_check = Instant::now();
                            
                            match result {
                                Ok(()) => {
                                    debug!("Health check passed for '{}': {:?}", proxy_id, latency);
                                    proxy_metrics.record_latency(latency, true);
                                },
                                Err(e) => {
                                    warn!("Health check failed for '{}': {}", proxy_id, e);
                                    proxy_metrics.record_latency(latency, false);
                                },
                            }
                        }
                    });
                }
            }
        });
    }

    /// Perform a health check on a proxy
    async fn health_check_proxy(addr: SocketAddr, timeout_duration: Duration) -> Result<()> {
        // Simple TCP connection test
        match timeout(timeout_duration, TcpStream::connect(addr)).await {
            Ok(Ok(_stream)) => {
                debug!("Health check connection successful to {}", addr);
                Ok(())
            },
            Ok(Err(e)) => {
                Err(anyhow::anyhow!("Connection failed: {}", e))
            },
            Err(_) => {
                Err(anyhow::anyhow!("Connection timeout"))
            },
        }
    }

    /// Get current metrics for all proxies
    pub async fn get_all_metrics(&self) -> HashMap<String, ProxyMetrics> {
        self.metrics.read().await.clone()
    }

    /// Get metrics for a specific proxy
    pub async fn get_proxy_metrics(&self, proxy_id: &str) -> Option<ProxyMetrics> {
        self.metrics.read().await.get(proxy_id).cloned()
    }

    /// Get health status summary
    pub async fn get_health_summary(&self) -> HealthSummary {
        let metrics_guard = self.metrics.read().await;
        
        let mut healthy = 0;
        let mut degraded = 0;
        let mut unhealthy = 0;
        let mut unknown = 0;
        
        for metrics in metrics_guard.values() {
            match metrics.health_status {
                HealthStatus::Healthy => healthy += 1,
                HealthStatus::Degraded => degraded += 1,
                HealthStatus::Unhealthy => unhealthy += 1,
                HealthStatus::Unknown => unknown += 1,
            }
        }
        
        HealthSummary {
            total_proxies: metrics_guard.len(),
            healthy,
            degraded,
            unhealthy,
            unknown,
        }
    }

    /// Force a health check for all proxies
    pub async fn force_health_check(&self) {
        info!("Forcing health check for all proxies");
        
        for (id, proxy) in &self.upstream_proxies {
            let proxy_id = id.clone();
            let proxy_addr = proxy.addr;
            let timeout_duration = self.config.health_check_timeout;
            let metrics = Arc::clone(&self.metrics);
            
            tokio::spawn(async move {
                let start_time = Instant::now();
                let result = Self::health_check_proxy(proxy_addr, timeout_duration).await;
                let latency = start_time.elapsed();
                
                let mut metrics_guard = metrics.write().await;
                if let Some(proxy_metrics) = metrics_guard.get_mut(&proxy_id) {
                    proxy_metrics.last_health_check = Instant::now();
                    
                    match result {
                        Ok(()) => {
                            info!("Forced health check passed for '{}': {:?}", proxy_id, latency);
                            proxy_metrics.record_latency(latency, true);
                        },
                        Err(e) => {
                            warn!("Forced health check failed for '{}': {}", proxy_id, e);
                            proxy_metrics.record_latency(latency, false);
                        },
                    }
                }
            });
        }
    }
}

/// Health status summary
#[derive(Debug, Clone)]
pub struct HealthSummary {
    pub total_proxies: usize,
    pub healthy: usize,
    pub degraded: usize,
    pub unhealthy: usize,
    pub unknown: usize,
}

impl HealthSummary {
    /// Get the overall health percentage
    pub fn health_percentage(&self) -> f64 {
        if self.total_proxies == 0 {
            return 0.0;
        }
        
        (self.healthy as f64 / self.total_proxies as f64) * 100.0
    }

    /// Check if the overall system is healthy
    pub fn is_healthy(&self) -> bool {
        self.unhealthy == 0 && self.degraded <= self.total_proxies / 4
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;
    use tokio::time::sleep;
    use crate::routing::ProxyProtocol;

    #[test]
    fn test_proxy_metrics_scoring() {
        let mut metrics = ProxyMetrics::new();
        
        // Initially unknown
        assert_eq!(metrics.health_status, HealthStatus::Unknown);
        assert_eq!(metrics.get_score(), 0.1);
        
        // Record some successful connections with low latency
        for _ in 0..5 {
            metrics.record_latency(Duration::from_millis(100), true);
        }
        
        assert_eq!(metrics.health_status, HealthStatus::Healthy);
        assert!(metrics.get_score() > 0.5);
        
        // Record some failures
        for _ in 0..3 {
            metrics.record_latency(Duration::from_millis(1000), false);
        }
        
        assert_eq!(metrics.health_status, HealthStatus::Degraded);
        assert!(metrics.get_score() < 0.5);
        assert!(metrics.get_score() > 0.0);
    }

    #[test]
    fn test_health_summary() {
        let summary = HealthSummary {
            total_proxies: 10,
            healthy: 8,
            degraded: 1,
            unhealthy: 1,
            unknown: 0,
        };
        
        assert_eq!(summary.health_percentage(), 80.0);
        assert!(!summary.is_healthy()); // Has unhealthy proxies
        
        let healthy_summary = HealthSummary {
            total_proxies: 10,
            healthy: 9,
            degraded: 1,
            unhealthy: 0,
            unknown: 0,
        };
        
        assert!(healthy_summary.is_healthy()); // No unhealthy, few degraded
    }

    #[tokio::test]
    async fn test_smart_routing_manager() {
        let config = SmartRoutingConfig::default();
        let mut manager = SmartRoutingManager::new(config);
        
        // Add some test proxies
        manager.add_upstream_proxy(
            "proxy1".to_string(),
            UpstreamProxy {
                addr: SocketAddr::new(std::net::IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 1080),
                auth: None,
                protocol: ProxyProtocol::Socks5,
            }
        ).await;
        
        manager.add_upstream_proxy(
            "proxy2".to_string(),
            UpstreamProxy {
                addr: SocketAddr::new(std::net::IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 1081),
                auth: None,
                protocol: ProxyProtocol::Socks5,
            }
        ).await;
        
        // Should be able to select a proxy
        let selected = manager.select_best_proxy(&[]).await;
        assert!(selected.is_some());
        
        // Record some metrics
        manager.record_connection_result("proxy1", Duration::from_millis(100), true).await;
        manager.record_connection_result("proxy2", Duration::from_millis(500), true).await;
        
        // Give some time for async operations
        sleep(Duration::from_millis(10)).await;
        
        // Get metrics
        let metrics = manager.get_all_metrics().await;
        assert_eq!(metrics.len(), 2);
    }
}