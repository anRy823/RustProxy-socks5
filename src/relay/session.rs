//! Relay Session

use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;
use serde::{Deserialize, Serialize};
use tracing::{info, debug};

/// Represents an active relay session
#[derive(Debug)]
pub struct RelaySession {
    pub session_id: String,
    pub client_addr: SocketAddr,
    pub target_addr: SocketAddr,
    pub start_time: Instant,
    pub bytes_up: AtomicU64,
    pub bytes_down: AtomicU64,
}

/// Connection statistics for completed sessions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionStats {
    pub session_id: String,
    pub client_addr: SocketAddr,
    pub target_addr: SocketAddr,
    pub start_time: std::time::SystemTime,
    pub duration_ms: u64,
    pub bytes_up: u64,
    pub bytes_down: u64,
    pub total_bytes: u64,
    pub user_id: Option<String>,
}

impl RelaySession {
    /// Create a new relay session
    pub fn new(session_id: String, client_addr: SocketAddr, target_addr: SocketAddr) -> Self {
        debug!("Creating new relay session: {} ({} -> {})", 
               session_id, client_addr, target_addr);
        
        Self {
            session_id,
            client_addr,
            target_addr,
            start_time: Instant::now(),
            bytes_up: AtomicU64::new(0),
            bytes_down: AtomicU64::new(0),
        }
    }

    /// Get bytes transferred upstream (client to target)
    pub fn bytes_up(&self) -> u64 {
        self.bytes_up.load(Ordering::Relaxed)
    }

    /// Get bytes transferred downstream (target to client)
    pub fn bytes_down(&self) -> u64 {
        self.bytes_down.load(Ordering::Relaxed)
    }

    /// Get total bytes transferred
    pub fn total_bytes(&self) -> u64 {
        self.bytes_up() + self.bytes_down()
    }

    /// Get session duration
    pub fn duration(&self) -> std::time::Duration {
        self.start_time.elapsed()
    }

    /// Update bytes transferred upstream
    pub fn update_bytes_up(&self, bytes: u64) {
        self.bytes_up.store(bytes, Ordering::Relaxed);
    }

    /// Update bytes transferred downstream
    pub fn update_bytes_down(&self, bytes: u64) {
        self.bytes_down.store(bytes, Ordering::Relaxed);
    }

    /// Add bytes to upstream counter
    pub fn add_bytes_up(&self, bytes: u64) {
        self.bytes_up.fetch_add(bytes, Ordering::Relaxed);
    }

    /// Add bytes to downstream counter
    pub fn add_bytes_down(&self, bytes: u64) {
        self.bytes_down.fetch_add(bytes, Ordering::Relaxed);
    }

    /// Generate connection statistics
    pub fn to_stats(&self, user_id: Option<String>) -> ConnectionStats {
        let duration = self.duration();
        let start_time = std::time::SystemTime::now() - duration;
        
        ConnectionStats {
            session_id: self.session_id.clone(),
            client_addr: self.client_addr,
            target_addr: self.target_addr,
            start_time,
            duration_ms: duration.as_millis() as u64,
            bytes_up: self.bytes_up(),
            bytes_down: self.bytes_down(),
            total_bytes: self.total_bytes(),
            user_id,
        }
    }

    /// Log session statistics
    pub fn log_stats(&self, user_id: Option<&str>) {
        let duration = self.duration();
        let bytes_up = self.bytes_up();
        let bytes_down = self.bytes_down();
        let total_bytes = self.total_bytes();
        
        info!(
            session_id = %self.session_id,
            client_addr = %self.client_addr,
            target_addr = %self.target_addr,
            duration_ms = duration.as_millis(),
            bytes_up = bytes_up,
            bytes_down = bytes_down,
            total_bytes = total_bytes,
            user_id = user_id,
            "Relay session completed"
        );
        
        // Also log in a more human-readable format
        info!("Session {} completed: {} -> {} | Duration: {:?} | Up: {} bytes | Down: {} bytes | Total: {} bytes{}",
              self.session_id,
              self.client_addr,
              self.target_addr,
              duration,
              bytes_up,
              bytes_down,
              total_bytes,
              user_id.map(|u| format!(" | User: {}", u)).unwrap_or_default()
        );
    }
}