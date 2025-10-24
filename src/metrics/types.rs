//! Metrics Types

use std::net::SocketAddr;
use std::time::{Duration, Instant, SystemTime};
use std::sync::atomic::{AtomicU64, Ordering};
use std::collections::HashMap;
use std::sync::RwLock;

/// Connection statistics
#[derive(Debug, Clone)]
pub struct ConnectionStats {
    pub session_id: String,
    pub client_addr: SocketAddr,
    pub target_addr: SocketAddr,
    pub start_time: SystemTime,
    pub duration: Duration,
    pub bytes_up: u64,
    pub bytes_down: u64,
    pub user_id: Option<String>,
}

/// Active connection tracking
#[derive(Debug)]
pub struct ActiveConnection {
    pub session_id: String,
    pub client_addr: SocketAddr,
    pub target_addr: SocketAddr,
    pub start_time: Instant,
    pub bytes_up: AtomicU64,
    pub bytes_down: AtomicU64,
    pub user_id: Option<String>,
}

impl ActiveConnection {
    pub fn new(
        session_id: String,
        client_addr: SocketAddr,
        target_addr: SocketAddr,
        user_id: Option<String>,
    ) -> Self {
        Self {
            session_id,
            client_addr,
            target_addr,
            start_time: Instant::now(),
            bytes_up: AtomicU64::new(0),
            bytes_down: AtomicU64::new(0),
            user_id,
        }
    }

    pub fn add_bytes_up(&self, bytes: u64) {
        self.bytes_up.fetch_add(bytes, Ordering::Relaxed);
    }

    pub fn add_bytes_down(&self, bytes: u64) {
        self.bytes_down.fetch_add(bytes, Ordering::Relaxed);
    }

    pub fn get_bytes_up(&self) -> u64 {
        self.bytes_up.load(Ordering::Relaxed)
    }

    pub fn get_bytes_down(&self) -> u64 {
        self.bytes_down.load(Ordering::Relaxed)
    }

    pub fn duration(&self) -> Duration {
        self.start_time.elapsed()
    }

    pub fn to_stats(&self) -> ConnectionStats {
        ConnectionStats {
            session_id: self.session_id.clone(),
            client_addr: self.client_addr,
            target_addr: self.target_addr,
            start_time: SystemTime::now() - self.duration(), // Approximate start time
            duration: self.duration(),
            bytes_up: self.get_bytes_up(),
            bytes_down: self.get_bytes_down(),
            user_id: self.user_id.clone(),
        }
    }
}

/// Historical connection data for reporting
#[derive(Debug, Clone)]
pub struct HistoricalStats {
    pub total_connections: u64,
    pub total_bytes_transferred: u64,
    pub average_connection_duration: Duration,
    pub top_destinations: Vec<(String, u64)>,
    pub user_activity: HashMap<String, u64>,
}

/// Connection activity summary
#[derive(Debug, Clone)]
pub struct ActivitySummary {
    pub active_connections: usize,
    pub total_connections_today: u64,
    pub bytes_transferred_today: u64,
    pub authentication_attempts_today: u64,
    pub blocked_requests_today: u64,
    pub top_users: Vec<(String, u64)>,
    pub top_destinations: Vec<(String, u64)>,
}

/// Metrics registry for storing active connections and historical data
#[derive(Debug)]
pub struct MetricsRegistry {
    pub active_connections: RwLock<HashMap<String, ActiveConnection>>,
    pub historical_connections: RwLock<Vec<ConnectionStats>>,
    pub daily_stats: RwLock<HashMap<String, u64>>, // date -> count mappings
}