//! Metrics Collector

use super::{ConnectionStats, ActiveConnection, MetricsRegistry, HistoricalStats, ActivitySummary};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime};
use prometheus::{Counter, Gauge, Histogram, Registry, TextEncoder};
use tracing::{info, warn, error, debug};

/// Collects and exports metrics
pub struct Metrics {
    registry: Arc<MetricsRegistry>,
    prometheus_registry: Registry,
    
    // Prometheus metrics
    connections_total: Counter,
    active_connections: Gauge,
    bytes_transferred_total: Counter,
    connection_duration: Histogram,
    auth_attempts_total: Counter,
    auth_success_total: Counter,
    blocked_requests_total: Counter,
    
    // Internal counters
    total_connections: AtomicU64,
    total_bytes: AtomicU64,
    auth_attempts: AtomicU64,
    auth_successes: AtomicU64,
    blocked_requests: AtomicU64,
}

impl Metrics {
    /// Create a new metrics collector
    pub fn new() -> Self {
        let prometheus_registry = Registry::new();
        
        // Create Prometheus metrics
        let connections_total = Counter::new(
            "socks5_connections_total",
            "Total number of SOCKS5 connections"
        ).expect("Failed to create connections_total counter");
        
        let active_connections = Gauge::new(
            "socks5_active_connections",
            "Number of currently active SOCKS5 connections"
        ).expect("Failed to create active_connections gauge");
        
        let bytes_transferred_total = Counter::new(
            "socks5_bytes_transferred_total",
            "Total bytes transferred through the proxy"
        ).expect("Failed to create bytes_transferred_total counter");
        
        let connection_duration = Histogram::with_opts(
            prometheus::HistogramOpts::new(
                "socks5_connection_duration_seconds",
                "Duration of SOCKS5 connections in seconds"
            ).buckets(vec![0.1, 0.5, 1.0, 5.0, 10.0, 30.0, 60.0, 300.0, 600.0])
        ).expect("Failed to create connection_duration histogram");
        
        let auth_attempts_total = Counter::new(
            "socks5_auth_attempts_total",
            "Total authentication attempts"
        ).expect("Failed to create auth_attempts_total counter");
        
        let auth_success_total = Counter::new(
            "socks5_auth_success_total",
            "Total successful authentications"
        ).expect("Failed to create auth_success_total counter");
        
        let blocked_requests_total = Counter::new(
            "socks5_blocked_requests_total",
            "Total blocked requests"
        ).expect("Failed to create blocked_requests_total counter");
        
        // Register metrics
        prometheus_registry.register(Box::new(connections_total.clone()))
            .expect("Failed to register connections_total");
        prometheus_registry.register(Box::new(active_connections.clone()))
            .expect("Failed to register active_connections");
        prometheus_registry.register(Box::new(bytes_transferred_total.clone()))
            .expect("Failed to register bytes_transferred_total");
        prometheus_registry.register(Box::new(connection_duration.clone()))
            .expect("Failed to register connection_duration");
        prometheus_registry.register(Box::new(auth_attempts_total.clone()))
            .expect("Failed to register auth_attempts_total");
        prometheus_registry.register(Box::new(auth_success_total.clone()))
            .expect("Failed to register auth_success_total");
        prometheus_registry.register(Box::new(blocked_requests_total.clone()))
            .expect("Failed to register blocked_requests_total");
        
        let registry = Arc::new(MetricsRegistry {
            active_connections: RwLock::new(HashMap::new()),
            historical_connections: RwLock::new(Vec::new()),
            daily_stats: RwLock::new(HashMap::new()),
        });
        
        Self {
            registry,
            prometheus_registry,
            connections_total,
            active_connections,
            bytes_transferred_total,
            connection_duration,
            auth_attempts_total,
            auth_success_total,
            blocked_requests_total,
            total_connections: AtomicU64::new(0),
            total_bytes: AtomicU64::new(0),
            auth_attempts: AtomicU64::new(0),
            auth_successes: AtomicU64::new(0),
            blocked_requests: AtomicU64::new(0),
        }
    }
    
    /// Start tracking a new connection
    pub fn start_connection(
        &self,
        session_id: String,
        client_addr: std::net::SocketAddr,
        target_addr: std::net::SocketAddr,
        user_id: Option<String>,
    ) -> anyhow::Result<()> {
        let connection = ActiveConnection::new(session_id.clone(), client_addr, target_addr, user_id.clone());
        
        {
            let mut active = self.registry.active_connections.write()
                .map_err(|_| anyhow::anyhow!("Failed to acquire write lock on active connections"))?;
            active.insert(session_id.clone(), connection);
        }
        
        // Update metrics
        self.connections_total.inc();
        self.active_connections.inc();
        self.total_connections.fetch_add(1, Ordering::Relaxed);
        
        info!(
            session_id = %session_id,
            client_addr = %client_addr,
            target_addr = %target_addr,
            user_id = ?user_id,
            "Started tracking connection"
        );
        
        Ok(())
    }
    
    /// Stop tracking a connection and record final statistics
    pub fn end_connection(&self, session_id: &str) -> anyhow::Result<()> {
        let connection = {
            let mut active = self.registry.active_connections.write()
                .map_err(|_| anyhow::anyhow!("Failed to acquire write lock on active connections"))?;
            active.remove(session_id)
        };
        
        if let Some(connection) = connection {
            let stats = connection.to_stats();
            
            // Update metrics
            self.active_connections.dec();
            self.connection_duration.observe(stats.duration.as_secs_f64());
            self.bytes_transferred_total.inc_by((stats.bytes_up + stats.bytes_down) as f64);
            self.total_bytes.fetch_add(stats.bytes_up + stats.bytes_down, Ordering::Relaxed);
            
            // Store historical data
            {
                let mut historical = self.registry.historical_connections.write()
                    .map_err(|_| anyhow::anyhow!("Failed to acquire write lock on historical connections"))?;
                historical.push(stats.clone());
                
                // Keep only last 10000 connections to prevent memory growth
                if historical.len() > 10000 {
                    historical.drain(0..1000);
                }
            }
            
            info!(
                session_id = %session_id,
                duration_secs = stats.duration.as_secs(),
                bytes_up = stats.bytes_up,
                bytes_down = stats.bytes_down,
                "Ended connection tracking"
            );
        } else {
            warn!(session_id = %session_id, "Attempted to end tracking for unknown connection");
        }
        
        Ok(())
    }
    
    /// Update bytes transferred for an active connection
    pub fn update_connection_bytes(&self, session_id: &str, bytes_up: u64, bytes_down: u64) -> anyhow::Result<()> {
        let active = self.registry.active_connections.read()
            .map_err(|_| anyhow::anyhow!("Failed to acquire read lock on active connections"))?;
        
        if let Some(connection) = active.get(session_id) {
            connection.add_bytes_up(bytes_up);
            connection.add_bytes_down(bytes_down);
            
            debug!(
                session_id = %session_id,
                bytes_up = bytes_up,
                bytes_down = bytes_down,
                "Updated connection bytes"
            );
        }
        
        Ok(())
    }

    /// Record connection statistics
    pub fn record_connection(&self, stats: &ConnectionStats) {
        // This method is for recording already completed connections
        self.connection_duration.observe(stats.duration.as_secs_f64());
        self.bytes_transferred_total.inc_by((stats.bytes_up + stats.bytes_down) as f64);
        self.total_bytes.fetch_add(stats.bytes_up + stats.bytes_down, Ordering::Relaxed);
        
        // Store in historical data
        if let Ok(mut historical) = self.registry.historical_connections.write() {
            historical.push(stats.clone());
            
            // Keep only last 10000 connections
            if historical.len() > 10000 {
                historical.drain(0..1000);
            }
        }
        
        info!(
            session_id = %stats.session_id,
            duration_secs = stats.duration.as_secs(),
            bytes_total = stats.bytes_up + stats.bytes_down,
            "Recorded connection statistics"
        );
    }

    /// Increment authentication attempts counter
    pub fn increment_auth_attempts(&self, success: bool) {
        self.auth_attempts_total.inc();
        self.auth_attempts.fetch_add(1, Ordering::Relaxed);
        
        if success {
            self.auth_success_total.inc();
            self.auth_successes.fetch_add(1, Ordering::Relaxed);
        }
        
        debug!(success = success, "Recorded authentication attempt");
    }

    /// Record blocked request
    pub fn record_blocked_request(&self, reason: &str) {
        self.blocked_requests_total.inc();
        self.blocked_requests.fetch_add(1, Ordering::Relaxed);
        
        info!(reason = %reason, "Recorded blocked request");
    }
    
    /// Get current activity summary
    pub fn get_activity_summary(&self) -> anyhow::Result<ActivitySummary> {
        let active = self.registry.active_connections.read()
            .map_err(|_| anyhow::anyhow!("Failed to acquire read lock on active connections"))?;
        
        let historical = self.registry.historical_connections.read()
            .map_err(|_| anyhow::anyhow!("Failed to acquire read lock on historical connections"))?;
        
        // Calculate today's stats (simplified - using all historical data for now)
        let mut user_counts: HashMap<String, u64> = HashMap::new();
        let mut destination_counts: HashMap<String, u64> = HashMap::new();
        let mut total_bytes_today = 0u64;
        
        for stats in historical.iter() {
            if let Some(user_id) = &stats.user_id {
                *user_counts.entry(user_id.clone()).or_insert(0) += 1;
            }
            
            let dest = stats.target_addr.to_string();
            *destination_counts.entry(dest).or_insert(0) += 1;
            
            total_bytes_today += stats.bytes_up + stats.bytes_down;
        }
        
        // Sort and get top entries
        let mut top_users: Vec<_> = user_counts.into_iter().collect();
        top_users.sort_by(|a, b| b.1.cmp(&a.1));
        top_users.truncate(10);
        
        let mut top_destinations: Vec<_> = destination_counts.into_iter().collect();
        top_destinations.sort_by(|a, b| b.1.cmp(&a.1));
        top_destinations.truncate(10);
        
        Ok(ActivitySummary {
            active_connections: active.len(),
            total_connections_today: historical.len() as u64,
            bytes_transferred_today: total_bytes_today,
            authentication_attempts_today: self.auth_attempts.load(Ordering::Relaxed),
            blocked_requests_today: self.blocked_requests.load(Ordering::Relaxed),
            top_users,
            top_destinations,
        })
    }
    
    /// Get historical statistics
    pub fn get_historical_stats(&self) -> anyhow::Result<HistoricalStats> {
        let historical = self.registry.historical_connections.read()
            .map_err(|_| anyhow::anyhow!("Failed to acquire read lock on historical connections"))?;
        
        if historical.is_empty() {
            return Ok(HistoricalStats {
                total_connections: 0,
                total_bytes_transferred: 0,
                average_connection_duration: Duration::from_secs(0),
                top_destinations: Vec::new(),
                user_activity: HashMap::new(),
            });
        }
        
        let total_connections = historical.len() as u64;
        let total_bytes_transferred: u64 = historical.iter()
            .map(|s| s.bytes_up + s.bytes_down)
            .sum();
        
        let total_duration: Duration = historical.iter()
            .map(|s| s.duration)
            .sum();
        let average_connection_duration = total_duration / historical.len() as u32;
        
        // Calculate top destinations
        let mut destination_counts: HashMap<String, u64> = HashMap::new();
        let mut user_activity: HashMap<String, u64> = HashMap::new();
        
        for stats in historical.iter() {
            let dest = stats.target_addr.to_string();
            *destination_counts.entry(dest).or_insert(0) += 1;
            
            if let Some(user_id) = &stats.user_id {
                *user_activity.entry(user_id.clone()).or_insert(0) += 1;
            }
        }
        
        let mut top_destinations: Vec<_> = destination_counts.into_iter().collect();
        top_destinations.sort_by(|a, b| b.1.cmp(&a.1));
        top_destinations.truncate(10);
        
        Ok(HistoricalStats {
            total_connections,
            total_bytes_transferred,
            average_connection_duration,
            top_destinations,
            user_activity,
        })
    }

    /// Export metrics in Prometheus format
    pub fn export_prometheus(&self) -> String {
        let encoder = TextEncoder::new();
        let metric_families = self.prometheus_registry.gather();
        
        match encoder.encode_to_string(&metric_families) {
            Ok(output) => output,
            Err(e) => {
                error!(error = %e, "Failed to encode Prometheus metrics");
                String::new()
            }
        }
    }
    
    /// Get number of active connections
    pub fn get_active_connections(&self) -> usize {
        self.registry.active_connections.read()
            .map(|active| active.len())
            .unwrap_or(0)
    }
    
    /// Get total number of connections
    pub fn get_total_connections(&self) -> u64 {
        self.total_connections.load(Ordering::Relaxed)
    }
    
    /// Get total bytes transferred
    pub fn get_bytes_transferred(&self) -> u64 {
        self.total_bytes.load(Ordering::Relaxed)
    }
    
    /// Get total authentication attempts
    pub fn get_auth_attempts(&self) -> u64 {
        self.auth_attempts.load(Ordering::Relaxed)
    }
    
    /// Get authentication failures
    pub fn get_auth_failures(&self) -> u64 {
        let attempts = self.auth_attempts.load(Ordering::Relaxed);
        let successes = self.auth_successes.load(Ordering::Relaxed);
        attempts.saturating_sub(successes)
    }
    
    /// Get blocked requests count
    pub fn get_blocked_requests(&self) -> u64 {
        self.blocked_requests.load(Ordering::Relaxed)
    }
    
    /// Get active connection information for management API
    pub fn get_active_connection_info(&self) -> Vec<crate::management::types::ConnectionInfo> {
        use crate::management::types::ConnectionInfo;
        
        self.registry.active_connections.read()
            .map(|active| {
                active.iter().map(|(id, conn)| {
                    ConnectionInfo {
                        id: id.clone(),
                        client_addr: conn.client_addr,
                        target_addr: Some(conn.target_addr),
                        user_id: conn.user_id.clone(),
                        start_time: SystemTime::now() - conn.start_time.elapsed(),
                        bytes_up: conn.bytes_up.load(Ordering::Relaxed),
                        bytes_down: conn.bytes_down.load(Ordering::Relaxed),
                        status: "active".to_string(),
                    }
                }).collect()
            })
            .unwrap_or_default()
    }
    
    /// Get top destinations for management API
    pub fn get_top_destinations(&self, limit: usize) -> Vec<crate::management::types::DestinationStats> {
        use crate::management::types::DestinationStats;
        
        let historical = match self.registry.historical_connections.read() {
            Ok(guard) => guard,
            Err(_) => {
                warn!("Failed to acquire read lock on historical connections");
                return Vec::new();
            }
        };
        
        let mut destination_stats: std::collections::HashMap<String, (u64, u64)> = std::collections::HashMap::new();
        
        for stats in historical.iter() {
            let dest = stats.target_addr.to_string();
            let entry = destination_stats.entry(dest).or_insert((0, 0));
            entry.0 += 1; // connection count
            entry.1 += stats.bytes_up + stats.bytes_down; // bytes transferred
        }
        
        let mut sorted: Vec<_> = destination_stats.into_iter()
            .map(|(dest, (count, bytes))| DestinationStats {
                destination: dest,
                connection_count: count,
                bytes_transferred: bytes,
            })
            .collect();
        
        sorted.sort_by(|a, b| b.connection_count.cmp(&a.connection_count));
        sorted.truncate(limit);
        sorted
    }
    
    /// Get top users for management API
    pub fn get_top_users(&self, limit: usize) -> Vec<crate::management::types::UserStats> {
        use crate::management::types::UserStats;
        
        let historical = match self.registry.historical_connections.read() {
            Ok(guard) => guard,
            Err(_) => {
                warn!("Failed to acquire read lock on historical connections");
                return Vec::new();
            }
        };
        
        let mut user_stats: std::collections::HashMap<String, (u64, u64, std::time::SystemTime)> = std::collections::HashMap::new();
        
        for stats in historical.iter() {
            if let Some(user_id) = &stats.user_id {
                let entry = user_stats.entry(user_id.clone()).or_insert((0, 0, stats.start_time));
                entry.0 += 1; // connection count
                entry.1 += stats.bytes_up + stats.bytes_down; // bytes transferred
                if stats.start_time > entry.2 {
                    entry.2 = stats.start_time; // last activity
                }
            }
        }
        
        let mut sorted: Vec<_> = user_stats.into_iter()
            .map(|(user, (count, bytes, last_activity))| UserStats {
                username: user,
                connection_count: count,
                bytes_transferred: bytes,
                last_activity,
            })
            .collect();
        
        sorted.sort_by(|a, b| b.connection_count.cmp(&a.connection_count));
        sorted.truncate(limit);
        sorted
    }
}