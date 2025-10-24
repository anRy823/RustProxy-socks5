//! DDoS Protection Implementation
//! 
//! Provides connection flood detection and mitigation to protect against
//! distributed denial of service attacks.

use std::collections::{HashMap, VecDeque};
use std::net::IpAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use serde::{Deserialize, Serialize};
use tracing::{debug, warn, info};

/// DDoS protection configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DdosConfig {
    pub enabled: bool,
    pub connection_threshold: u32,
    pub time_window_seconds: u64,
    pub block_duration_minutes: u64,
    pub max_connections_per_ip: u32,
    pub global_connection_threshold: u32,
    pub enable_progressive_delays: bool,
    pub base_delay_ms: u64,
    pub max_delay_ms: u64,
    pub cleanup_interval_seconds: u64,
}

impl Default for DdosConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            connection_threshold: 50, // connections per time window
            time_window_seconds: 60,  // 1 minute window
            block_duration_minutes: 30,
            max_connections_per_ip: 10, // concurrent connections
            global_connection_threshold: 5000, // total concurrent connections
            enable_progressive_delays: true,
            base_delay_ms: 100,
            max_delay_ms: 5000,
            cleanup_interval_seconds: 300, // 5 minutes
        }
    }
}

/// Connection flood detector for tracking connection patterns
#[derive(Debug)]
struct ConnectionFloodDetector {
    connection_times: VecDeque<Instant>,
    total_connections: u64,
    blocked_until: Option<Instant>,
    last_activity: Instant,
    current_connections: u32,
    violation_count: u32,
}

impl ConnectionFloodDetector {
    fn new() -> Self {
        Self {
            connection_times: VecDeque::new(),
            total_connections: 0,
            blocked_until: None,
            last_activity: Instant::now(),
            current_connections: 0,
            violation_count: 0,
        }
    }

    /// Record a new connection attempt
    fn record_connection(&mut self, config: &DdosConfig) -> bool {
        let now = Instant::now();
        self.last_activity = now;
        self.total_connections += 1;

        // Check if currently blocked
        if let Some(blocked_until) = self.blocked_until {
            if now < blocked_until {
                debug!("Connection blocked due to active DDoS protection");
                return false;
            } else {
                // Unblock expired blocks
                self.blocked_until = None;
                self.violation_count = 0;
                debug!("DDoS protection block expired, allowing connections");
            }
        }

        // Add current connection time
        self.connection_times.push_back(now);

        // Remove old connection times outside the window
        let window_start = now - Duration::from_secs(config.time_window_seconds);
        while let Some(&front_time) = self.connection_times.front() {
            if front_time < window_start {
                self.connection_times.pop_front();
            } else {
                break;
            }
        }

        // Check if threshold exceeded
        if self.connection_times.len() as u32 > config.connection_threshold {
            self.violation_count += 1;
            
            // Calculate progressive block duration
            let base_duration = Duration::from_secs(config.block_duration_minutes * 60);
            let multiplier = 2_u32.pow(self.violation_count.min(5)); // Cap at 2^5 = 32x
            let block_duration = base_duration * multiplier;
            
            self.blocked_until = Some(now + block_duration);
            
            warn!("DDoS threshold exceeded: {} connections in {}s (violation #{}, blocking for {:?})",
                  self.connection_times.len(), config.time_window_seconds, 
                  self.violation_count, block_duration);
            
            return false;
        }

        true
    }

    /// Record connection start (increment concurrent count)
    fn connection_started(&mut self) {
        self.current_connections += 1;
    }

    /// Record connection end (decrement concurrent count)
    fn connection_ended(&mut self) {
        if self.current_connections > 0 {
            self.current_connections -= 1;
        }
    }

    /// Check if IP is currently blocked
    fn is_blocked(&self) -> bool {
        if let Some(blocked_until) = self.blocked_until {
            Instant::now() < blocked_until
        } else {
            false
        }
    }

    /// Check if concurrent connection limit exceeded
    fn exceeds_concurrent_limit(&self, config: &DdosConfig) -> bool {
        self.current_connections >= config.max_connections_per_ip
    }

    /// Get progressive delay based on violation count
    fn get_progressive_delay(&self, config: &DdosConfig) -> Duration {
        if !config.enable_progressive_delays || self.violation_count == 0 {
            return Duration::from_millis(0);
        }

        let base_delay = config.base_delay_ms;
        let max_delay = config.max_delay_ms;
        
        // Exponential backoff: base_delay * 2^(violation_count - 1)
        let delay_ms = base_delay * 2_u64.pow((self.violation_count - 1).min(10));
        let capped_delay = delay_ms.min(max_delay);
        
        Duration::from_millis(capped_delay)
    }
}

/// Main DDoS protection implementation
pub struct DdosProtection {
    config: DdosConfig,
    ip_detectors: Arc<Mutex<HashMap<IpAddr, ConnectionFloodDetector>>>,
    global_stats: Arc<Mutex<GlobalDdosStats>>,
}

#[derive(Debug, Default)]
struct GlobalDdosStats {
    total_connections_checked: u64,
    total_connections_blocked: u64,
    total_ddos_events: u64,
    current_global_connections: u32,
    currently_blocked_ips: usize,
    peak_global_connections: u32,
}

impl DdosProtection {
    /// Create a new DDoS protection instance
    pub fn new(config: DdosConfig) -> Self {
        Self {
            config,
            ip_detectors: Arc::new(Mutex::new(HashMap::new())),
            global_stats: Arc::new(Mutex::new(GlobalDdosStats::default())),
        }
    }

    /// Check if a connection should be allowed and record the attempt
    pub fn check_connection(&self, ip: IpAddr) -> DdosDecision {
        if !self.config.enabled {
            return DdosDecision::Allow;
        }

        // Update global stats
        {
            let mut stats = self.global_stats.lock().unwrap();
            stats.total_connections_checked += 1;
        }

        // Check global connection limit
        if self.exceeds_global_limit() {
            warn!("Global connection limit exceeded, rejecting connection from {}", ip);
            self.increment_blocked_connections();
            return DdosDecision::Block {
                reason: "Global connection limit exceeded".to_string(),
                delay: Duration::from_millis(self.config.base_delay_ms),
            };
        }

        let mut ip_detectors = self.ip_detectors.lock().unwrap();
        let detector = ip_detectors.entry(ip).or_insert_with(ConnectionFloodDetector::new);

        // Check if IP is currently blocked
        if detector.is_blocked() {
            debug!("Connection from {} blocked due to active DDoS protection", ip);
            self.increment_blocked_connections();
            return DdosDecision::Block {
                reason: "IP temporarily blocked due to DDoS protection".to_string(),
                delay: detector.get_progressive_delay(&self.config),
            };
        }

        // Check concurrent connection limit
        if detector.exceeds_concurrent_limit(&self.config) {
            warn!("Concurrent connection limit exceeded for IP {}: {} connections", 
                  ip, detector.current_connections);
            self.increment_blocked_connections();
            return DdosDecision::Block {
                reason: format!("Too many concurrent connections ({})", detector.current_connections),
                delay: Duration::from_millis(self.config.base_delay_ms),
            };
        }

        // Record connection attempt and check flood detection
        if detector.record_connection(&self.config) {
            debug!("Connection from {} allowed by DDoS protection", ip);
            DdosDecision::Allow
        } else {
            info!("DDoS attack detected from {}, blocking connection", ip);
            
            // Update global DDoS event counter
            {
                let mut stats = self.global_stats.lock().unwrap();
                stats.total_ddos_events += 1;
            }
            
            self.increment_blocked_connections();
            DdosDecision::Block {
                reason: "DDoS attack pattern detected".to_string(),
                delay: detector.get_progressive_delay(&self.config),
            }
        }
    }

    /// Record that a connection has started (for concurrent tracking)
    pub fn connection_started(&self, ip: IpAddr) {
        if !self.config.enabled {
            return;
        }

        let mut ip_detectors = self.ip_detectors.lock().unwrap();
        if let Some(detector) = ip_detectors.get_mut(&ip) {
            detector.connection_started();
        }

        // Update global connection count
        {
            let mut stats = self.global_stats.lock().unwrap();
            stats.current_global_connections += 1;
            if stats.current_global_connections > stats.peak_global_connections {
                stats.peak_global_connections = stats.current_global_connections;
            }
        }
    }

    /// Record that a connection has ended (for concurrent tracking)
    pub fn connection_ended(&self, ip: IpAddr) {
        if !self.config.enabled {
            return;
        }

        let mut ip_detectors = self.ip_detectors.lock().unwrap();
        if let Some(detector) = ip_detectors.get_mut(&ip) {
            detector.connection_ended();
        }

        // Update global connection count
        {
            let mut stats = self.global_stats.lock().unwrap();
            if stats.current_global_connections > 0 {
                stats.current_global_connections -= 1;
            }
        }
    }

    /// Manually block an IP address
    pub fn block_ip(&self, ip: IpAddr, duration: Duration, reason: &str) {
        let mut ip_detectors = self.ip_detectors.lock().unwrap();
        let detector = ip_detectors.entry(ip).or_insert_with(ConnectionFloodDetector::new);
        
        detector.blocked_until = Some(Instant::now() + duration);
        detector.violation_count += 1;
        
        info!("Manually blocked IP {} for {:?}: {}", ip, duration, reason);
    }

    /// Unblock an IP address
    pub fn unblock_ip(&self, ip: IpAddr) -> bool {
        let mut ip_detectors = self.ip_detectors.lock().unwrap();
        if let Some(detector) = ip_detectors.get_mut(&ip) {
            if detector.is_blocked() {
                detector.blocked_until = None;
                detector.violation_count = 0;
                info!("Unblocked IP {} from DDoS protection", ip);
                return true;
            }
        }
        false
    }

    /// Check if an IP is currently blocked
    pub fn is_ip_blocked(&self, ip: IpAddr) -> bool {
        let ip_detectors = self.ip_detectors.lock().unwrap();
        if let Some(detector) = ip_detectors.get(&ip) {
            detector.is_blocked()
        } else {
            false
        }
    }

    /// Get list of currently blocked IPs
    pub fn get_blocked_ips(&self) -> Vec<IpAddr> {
        let ip_detectors = self.ip_detectors.lock().unwrap();
        ip_detectors.iter()
            .filter(|(_, detector)| detector.is_blocked())
            .map(|(ip, _)| *ip)
            .collect()
    }

    /// Clean up old detector entries
    pub fn cleanup_old_entries(&self) {
        let cleanup_threshold = Duration::from_secs(self.config.cleanup_interval_seconds * 2);
        let cutoff_time = Instant::now() - cleanup_threshold;
        
        let mut ip_detectors = self.ip_detectors.lock().unwrap();
        let initial_count = ip_detectors.len();
        
        ip_detectors.retain(|_, detector| {
            // Keep if recently active, currently blocked, or has active connections
            detector.last_activity > cutoff_time || 
            detector.is_blocked() || 
            detector.current_connections > 0
        });
        
        let removed_count = initial_count - ip_detectors.len();
        if removed_count > 0 {
            debug!("Cleaned up {} old DDoS detector entries", removed_count);
        }

        // Update blocked IP count in stats
        let blocked_count = ip_detectors.iter().filter(|(_, detector)| detector.is_blocked()).count();
        {
            let mut stats = self.global_stats.lock().unwrap();
            stats.currently_blocked_ips = blocked_count;
        }
    }

    /// Get DDoS protection statistics
    pub fn get_stats(&self) -> DdosStats {
        let stats = self.global_stats.lock().unwrap();
        DdosStats {
            total_connections_checked: stats.total_connections_checked,
            total_connections_blocked: stats.total_connections_blocked,
            total_ddos_events: stats.total_ddos_events,
            current_global_connections: stats.current_global_connections,
            currently_blocked_ips: stats.currently_blocked_ips,
            peak_global_connections: stats.peak_global_connections,
        }
    }

    /// Get detailed IP statistics
    pub fn get_ip_stats(&self, ip: IpAddr) -> Option<IpDdosStats> {
        let ip_detectors = self.ip_detectors.lock().unwrap();
        ip_detectors.get(&ip).map(|detector| IpDdosStats {
            ip,
            total_connections: detector.total_connections,
            current_connections: detector.current_connections,
            connections_in_window: detector.connection_times.len() as u32,
            is_blocked: detector.is_blocked(),
            blocked_until: detector.blocked_until,
            violation_count: detector.violation_count,
            last_activity: detector.last_activity,
        })
    }

    /// Get all IP statistics
    pub fn get_all_ip_stats(&self) -> Vec<IpDdosStats> {
        let ip_detectors = self.ip_detectors.lock().unwrap();
        ip_detectors.iter().map(|(ip, detector)| IpDdosStats {
            ip: *ip,
            total_connections: detector.total_connections,
            current_connections: detector.current_connections,
            connections_in_window: detector.connection_times.len() as u32,
            is_blocked: detector.is_blocked(),
            blocked_until: detector.blocked_until,
            violation_count: detector.violation_count,
            last_activity: detector.last_activity,
        }).collect()
    }

    fn exceeds_global_limit(&self) -> bool {
        let stats = self.global_stats.lock().unwrap();
        stats.current_global_connections >= self.config.global_connection_threshold
    }

    fn increment_blocked_connections(&self) {
        let mut stats = self.global_stats.lock().unwrap();
        stats.total_connections_blocked += 1;
    }
}

/// Decision result from DDoS protection check
#[derive(Debug, Clone)]
pub enum DdosDecision {
    Allow,
    Block {
        reason: String,
        delay: Duration,
    },
}

/// DDoS protection statistics
#[derive(Debug, Clone)]
pub struct DdosStats {
    pub total_connections_checked: u64,
    pub total_connections_blocked: u64,
    pub total_ddos_events: u64,
    pub current_global_connections: u32,
    pub currently_blocked_ips: usize,
    pub peak_global_connections: u32,
}

/// Statistics for a specific IP address
#[derive(Debug, Clone)]
pub struct IpDdosStats {
    pub ip: IpAddr,
    pub total_connections: u64,
    pub current_connections: u32,
    pub connections_in_window: u32,
    pub is_blocked: bool,
    pub blocked_until: Option<Instant>,
    pub violation_count: u32,
    pub last_activity: Instant,
}



#[cfg(test)]
mod tests {
    use super::*;


    #[test]
    fn test_connection_flood_detector() {
        let config = DdosConfig {
            connection_threshold: 3,
            time_window_seconds: 1,
            ..Default::default()
        };
        
        let mut detector = ConnectionFloodDetector::new();
        
        // Should allow initial connections
        assert!(detector.record_connection(&config));
        assert!(detector.record_connection(&config));
        assert!(detector.record_connection(&config));
        
        // Should block after threshold
        assert!(!detector.record_connection(&config));
        assert!(detector.is_blocked());
    }

    #[test]
    fn test_ddos_protection_basic() {
        let config = DdosConfig {
            enabled: true,
            connection_threshold: 2,
            time_window_seconds: 1,
            max_connections_per_ip: 5,
            ..Default::default()
        };
        
        let protection = DdosProtection::new(config);
        let ip = "127.0.0.1".parse().unwrap();
        
        // Should allow initial connections
        assert!(matches!(protection.check_connection(ip), DdosDecision::Allow));
        assert!(matches!(protection.check_connection(ip), DdosDecision::Allow));
        
        // Should block after threshold
        assert!(matches!(protection.check_connection(ip), DdosDecision::Block { .. }));
    }

    #[test]
    fn test_concurrent_connection_limit() {
        let config = DdosConfig {
            enabled: true,
            max_connections_per_ip: 2,
            connection_threshold: 100, // High threshold to avoid flood detection
            ..Default::default()
        };
        
        let protection = DdosProtection::new(config);
        let ip = "127.0.0.1".parse().unwrap();
        
        // Start connections up to limit
        protection.connection_started(ip);
        protection.connection_started(ip);
        
        // Should block new connections
        assert!(matches!(protection.check_connection(ip), DdosDecision::Block { .. }));
        
        // End a connection and try again
        protection.connection_ended(ip);
        assert!(matches!(protection.check_connection(ip), DdosDecision::Allow));
    }

    #[test]
    fn test_ddos_protection_disabled() {
        let config = DdosConfig {
            enabled: false,
            ..Default::default()
        };
        
        let protection = DdosProtection::new(config);
        let ip = "127.0.0.1".parse().unwrap();
        
        // Should always allow when disabled
        for _ in 0..100 {
            assert!(matches!(protection.check_connection(ip), DdosDecision::Allow));
        }
    }

    #[test]
    fn test_manual_ip_blocking() {
        let config = DdosConfig::default();
        let protection = DdosProtection::new(config);
        let ip = "127.0.0.1".parse().unwrap();
        
        // Should initially allow
        assert!(matches!(protection.check_connection(ip), DdosDecision::Allow));
        
        // Block IP manually
        protection.block_ip(ip, Duration::from_secs(1), "test");
        assert!(protection.is_ip_blocked(ip));
        assert!(matches!(protection.check_connection(ip), DdosDecision::Block { .. }));
        
        // Unblock IP
        assert!(protection.unblock_ip(ip));
        assert!(!protection.is_ip_blocked(ip));
        assert!(matches!(protection.check_connection(ip), DdosDecision::Allow));
    }
}