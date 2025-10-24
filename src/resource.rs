//! Resource Management
//!
//! This module provides utilities for managing system resources including
//! memory usage, connection limits, and connection pooling.

use crate::config::Config;
use crate::Result;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{RwLock, Semaphore};
use tracing::{debug, info, warn};

/// Resource manager that tracks and enforces resource limits
pub struct ResourceManager {
    config: Arc<Config>,
    /// Current memory usage in bytes
    memory_usage: AtomicU64,
    /// Connection semaphore for limiting concurrent connections
    connection_semaphore: Arc<Semaphore>,
    /// Connection pool for upstream proxies
    connection_pool: Arc<RwLock<ConnectionPool>>,
    /// Statistics
    stats: ResourceStats,
}

/// Resource usage statistics
#[derive(Debug, Default)]
pub struct ResourceStats {
    pub peak_memory_usage: AtomicU64,
    pub peak_connections: AtomicUsize,
    pub total_connections_created: AtomicUsize,
    pub total_connections_rejected: AtomicUsize,
    pub pool_hits: AtomicUsize,
    pub pool_misses: AtomicUsize,
}

/// Connection pool for reusing upstream proxy connections
#[derive(Debug)]
struct ConnectionPool {
    pools: HashMap<String, Vec<PooledConnection>>,
    max_pool_size: usize,
}

/// A pooled connection with metadata
#[derive(Debug)]
struct PooledConnection {
    stream: tokio::net::TcpStream,
    created_at: Instant,
    last_used: Instant,
    _upstream_id: String,
}

impl ResourceManager {
    /// Create a new resource manager
    pub fn new(config: Arc<Config>) -> Self {
        let max_connections = config.server.max_connections;
        let connection_pool = Arc::new(RwLock::new(ConnectionPool::new(
            config.server.connection_pool_size,
        )));

        Self {
            config,
            memory_usage: AtomicU64::new(0),
            connection_semaphore: Arc::new(Semaphore::new(max_connections)),
            connection_pool,
            stats: ResourceStats::default(),
        }
    }

    /// Try to acquire a connection slot
    pub async fn acquire_connection_slot(&self) -> Result<ConnectionSlot> {
        // Try to acquire a permit from the semaphore
        match Arc::clone(&self.connection_semaphore).try_acquire_owned() {
            Ok(permit) => {
                self.stats
                    .total_connections_created
                    .fetch_add(1, Ordering::Relaxed);

                let current_permits = self.config.server.max_connections
                    - self.connection_semaphore.available_permits();

                // Update peak connections
                let mut peak = self.stats.peak_connections.load(Ordering::Relaxed);
                while current_permits > peak {
                    match self.stats.peak_connections.compare_exchange_weak(
                        peak,
                        current_permits,
                        Ordering::Relaxed,
                        Ordering::Relaxed,
                    ) {
                        Ok(_) => break,
                        Err(x) => peak = x,
                    }
                }

                debug!(
                    "Acquired connection slot, active connections: {}",
                    current_permits
                );
                Ok(ConnectionSlot::new(permit))
            }
            Err(_) => {
                self.stats
                    .total_connections_rejected
                    .fetch_add(1, Ordering::Relaxed);
                warn!(
                    "Connection limit reached ({}), rejecting connection",
                    self.config.server.max_connections
                );
                Err(anyhow::anyhow!("Connection limit reached"))
            }
        }
    }

    /// Track memory allocation
    pub fn allocate_memory(&self, bytes: u64) -> Result<()> {
        let current = self.memory_usage.fetch_add(bytes, Ordering::Relaxed) + bytes;
        let max_memory_bytes = (self.config.server.max_memory_mb as u64) * 1024 * 1024;

        if current > max_memory_bytes {
            self.memory_usage.fetch_sub(bytes, Ordering::Relaxed);
            warn!(
                "Memory limit exceeded: {} MB > {} MB",
                current / 1024 / 1024,
                max_memory_bytes / 1024 / 1024
            );
            return Err(anyhow::anyhow!("Memory limit exceeded"));
        }

        // Update peak memory usage
        let mut peak = self.stats.peak_memory_usage.load(Ordering::Relaxed);
        while current > peak {
            match self.stats.peak_memory_usage.compare_exchange_weak(
                peak,
                current,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(x) => peak = x,
            }
        }

        debug!(
            "Allocated {} bytes, total memory usage: {} MB",
            bytes,
            current / 1024 / 1024
        );
        Ok(())
    }

    /// Track memory deallocation
    pub fn deallocate_memory(&self, bytes: u64) {
        let current = self.memory_usage.fetch_sub(bytes, Ordering::Relaxed);
        debug!(
            "Deallocated {} bytes, total memory usage: {} MB",
            bytes,
            current.saturating_sub(bytes) / 1024 / 1024
        );
    }

    /// Get a pooled connection for an upstream proxy
    pub async fn get_pooled_connection(&self, upstream_id: &str) -> Option<tokio::net::TcpStream> {
        let mut pool = self.connection_pool.write().await;

        if let Some(connections) = pool.pools.get_mut(upstream_id) {
            // Find a healthy connection
            while let Some(mut conn) = connections.pop() {
                // Check if connection is still valid and not too old
                let age = conn.created_at.elapsed();
                let idle_time = conn.last_used.elapsed();

                if age < Duration::from_secs(300) && idle_time < self.config.server.idle_timeout {
                    // Update last used time
                    conn.last_used = Instant::now();
                    self.stats.pool_hits.fetch_add(1, Ordering::Relaxed);
                    debug!("Reusing pooled connection for upstream: {}", upstream_id);
                    return Some(conn.stream);
                } else {
                    debug!(
                        "Discarding stale pooled connection for upstream: {}",
                        upstream_id
                    );
                }
            }
        }

        self.stats.pool_misses.fetch_add(1, Ordering::Relaxed);
        None
    }

    /// Return a connection to the pool
    pub async fn return_connection_to_pool(
        &self,
        upstream_id: String,
        stream: tokio::net::TcpStream,
    ) {
        let mut pool = self.connection_pool.write().await;

        let max_pool_size = pool.max_pool_size;
        let connections = pool
            .pools
            .entry(upstream_id.clone())
            .or_insert_with(Vec::new);

        // Don't exceed pool size
        if connections.len() >= max_pool_size {
            debug!(
                "Connection pool full for upstream: {}, dropping connection",
                upstream_id
            );
            return;
        }

        let pooled_conn = PooledConnection {
            stream,
            created_at: Instant::now(),
            last_used: Instant::now(),
            _upstream_id: upstream_id.clone(),
        };

        connections.push(pooled_conn);
        debug!(
            "Returned connection to pool for upstream: {} (pool size: {})",
            upstream_id,
            connections.len()
        );
    }

    /// Clean up expired connections from the pool
    pub async fn cleanup_connection_pool(&self) {
        let mut pool = self.connection_pool.write().await;
        let mut total_removed = 0;

        let idle_timeout = self.config.server.idle_timeout;

        // Collect upstream IDs to avoid borrowing issues
        let upstream_ids: Vec<String> = pool.pools.keys().cloned().collect();

        // Clean up expired connections for each upstream
        for upstream_id in &upstream_ids {
            if let Some(connections) = pool.pools.get_mut(upstream_id) {
                let initial_count = connections.len();
                connections.retain(|conn| {
                    let age = conn.created_at.elapsed();
                    let idle_time = conn.last_used.elapsed();
                    age < Duration::from_secs(300) && idle_time < idle_timeout
                });
                let removed = initial_count - connections.len();
                total_removed += removed;

                if removed > 0 {
                    debug!(
                        "Cleaned up {} expired connections for upstream: {}",
                        removed, upstream_id
                    );
                }
            }
        }

        // Remove empty pools
        pool.pools.retain(|_, connections| !connections.is_empty());

        if total_removed > 0 {
            info!(
                "Connection pool cleanup: removed {} expired connections",
                total_removed
            );
        }
    }

    /// Get current resource usage statistics
    pub fn get_stats(&self) -> ResourceUsageStats {
        let active_connections =
            self.config.server.max_connections - self.connection_semaphore.available_permits();

        ResourceUsageStats {
            memory_usage_mb: self.memory_usage.load(Ordering::Relaxed) / 1024 / 1024,
            peak_memory_usage_mb: self.stats.peak_memory_usage.load(Ordering::Relaxed)
                / 1024
                / 1024,
            active_connections,
            peak_connections: self.stats.peak_connections.load(Ordering::Relaxed),
            total_connections_created: self.stats.total_connections_created.load(Ordering::Relaxed),
            total_connections_rejected: self
                .stats
                .total_connections_rejected
                .load(Ordering::Relaxed),
            pool_hits: self.stats.pool_hits.load(Ordering::Relaxed),
            pool_misses: self.stats.pool_misses.load(Ordering::Relaxed),
            max_connections: self.config.server.max_connections,
            max_memory_mb: self.config.server.max_memory_mb,
        }
    }

    /// Start background cleanup task
    pub fn start_cleanup_task(self: Arc<Self>) {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(60));

            loop {
                interval.tick().await;

                debug!("Running resource manager cleanup");
                self.cleanup_connection_pool().await;

                let stats = self.get_stats();
                debug!(
                    "Resource stats: {} MB memory, {} active connections, pool hit rate: {:.1}%",
                    stats.memory_usage_mb,
                    stats.active_connections,
                    if stats.pool_hits + stats.pool_misses > 0 {
                        (stats.pool_hits as f64 / (stats.pool_hits + stats.pool_misses) as f64)
                            * 100.0
                    } else {
                        0.0
                    }
                );
            }
        });

        info!("Started resource manager cleanup task");
    }
}

/// Connection slot that automatically releases when dropped
pub struct ConnectionSlot {
    _permit: tokio::sync::OwnedSemaphorePermit,
}

impl ConnectionSlot {
    fn new(permit: tokio::sync::OwnedSemaphorePermit) -> Self {
        Self { _permit: permit }
    }
}

impl Drop for ConnectionSlot {
    fn drop(&mut self) {
        debug!("Released connection slot (permit dropped automatically)");
    }
}

impl ConnectionPool {
    fn new(max_pool_size: usize) -> Self {
        Self {
            pools: HashMap::new(),
            max_pool_size,
        }
    }
}

/// Resource usage statistics for monitoring
#[derive(Debug, Clone)]
pub struct ResourceUsageStats {
    pub memory_usage_mb: u64,
    pub peak_memory_usage_mb: u64,
    pub active_connections: usize,
    pub peak_connections: usize,
    pub total_connections_created: usize,
    pub total_connections_rejected: usize,
    pub pool_hits: usize,
    pub pool_misses: usize,
    pub max_connections: usize,
    pub max_memory_mb: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    #[tokio::test]
    async fn test_connection_slot_acquisition() {
        let config = Arc::new(Config::default());
        let resource_manager = ResourceManager::new(config);

        // Should be able to acquire connection slots up to the limit
        let mut slots = Vec::new();
        for _ in 0..10 {
            let slot = resource_manager.acquire_connection_slot().await;
            assert!(slot.is_ok());
            slots.push(slot.unwrap());
        }

        assert_eq!(resource_manager.get_stats().active_connections, 10);

        // Drop all slots
        drop(slots);

        // Give some time for cleanup
        tokio::time::sleep(Duration::from_millis(10)).await;
        assert_eq!(resource_manager.get_stats().active_connections, 0);
    }

    #[tokio::test]
    async fn test_memory_tracking() {
        let config = Arc::new(Config::default());
        let resource_manager = ResourceManager::new(config);

        // Should be able to allocate memory within limits
        assert!(resource_manager.allocate_memory(1024).is_ok());
        assert_eq!(resource_manager.memory_usage.load(Ordering::Relaxed), 1024);

        // Deallocate memory
        resource_manager.deallocate_memory(512);
        assert_eq!(resource_manager.memory_usage.load(Ordering::Relaxed), 512);

        // Clean up remaining
        resource_manager.deallocate_memory(512);
        assert_eq!(resource_manager.memory_usage.load(Ordering::Relaxed), 0);
    }

    #[tokio::test]
    async fn test_connection_pool() {
        let config = Arc::new(Config::default());
        let resource_manager = ResourceManager::new(config);

        // Should return None for non-existent upstream
        let conn = resource_manager
            .get_pooled_connection("test-upstream")
            .await;
        assert!(conn.is_none());

        // Pool stats should show a miss
        let stats = resource_manager.get_stats();
        assert_eq!(stats.pool_misses, 1);
    }
}
