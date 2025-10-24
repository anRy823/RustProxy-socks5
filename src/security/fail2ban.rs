//! Fail2Ban Integration and Brute Force Protection
//! 
//! Implements brute force attack detection, progressive authentication delays,
//! and IP blacklist management similar to fail2ban functionality.

use std::collections::{HashMap, VecDeque};
use std::net::IpAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use serde::{Deserialize, Serialize};
use tracing::{debug, warn, info};

/// Fail2Ban configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Fail2BanConfig {
    pub enabled: bool,
    pub max_auth_failures: u32,
    pub failure_window_minutes: u64,
    pub ban_duration_minutes: u64,
    pub progressive_ban_multiplier: f64,
    pub max_ban_duration_hours: u64,
    pub enable_progressive_delays: bool,
    pub base_delay_ms: u64,
    pub max_delay_ms: u64,
    pub whitelist_ips: Vec<String>,
    pub cleanup_interval_seconds: u64,
}

impl Default for Fail2BanConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_auth_failures: 5,
            failure_window_minutes: 10,
            ban_duration_minutes: 30,
            progressive_ban_multiplier: 2.0,
            max_ban_duration_hours: 24,
            enable_progressive_delays: true,
            base_delay_ms: 1000, // 1 second
            max_delay_ms: 30000, // 30 seconds
            whitelist_ips: vec![
                "127.0.0.1".to_string(),
                "::1".to_string(),
            ],
            cleanup_interval_seconds: 300, // 5 minutes
        }
    }
}

/// Brute force detector for tracking authentication failures
#[derive(Debug)]
struct BruteForceDetector {
    failure_times: VecDeque<Instant>,
    total_failures: u64,
    total_successes: u64,
    ban_count: u32,
    banned_until: Option<Instant>,
    last_activity: Instant,
    last_failure_time: Option<Instant>,
}

impl BruteForceDetector {
    fn new() -> Self {
        Self {
            failure_times: VecDeque::new(),
            total_failures: 0,
            total_successes: 0,
            ban_count: 0,
            banned_until: None,
            last_activity: Instant::now(),
            last_failure_time: None,
        }
    }

    /// Record an authentication failure
    fn record_failure(&mut self, config: &Fail2BanConfig) -> bool {
        let now = Instant::now();
        self.last_activity = now;
        self.last_failure_time = Some(now);
        self.total_failures += 1;

        // Check if currently banned
        if let Some(banned_until) = self.banned_until {
            if now < banned_until {
                debug!("Authentication blocked due to active ban");
                return false;
            } else {
                // Ban expired, reset some counters but keep history
                self.banned_until = None;
                debug!("Fail2ban block expired, allowing authentication attempts");
            }
        }

        // Add current failure time
        self.failure_times.push_back(now);

        // Remove old failure times outside the window
        let window_start = now - Duration::from_secs(config.failure_window_minutes * 60);
        while let Some(&front_time) = self.failure_times.front() {
            if front_time < window_start {
                self.failure_times.pop_front();
            } else {
                break;
            }
        }

        // Check if failure threshold exceeded
        if self.failure_times.len() as u32 >= config.max_auth_failures {
            self.ban_count += 1;
            
            // Calculate progressive ban duration
            let base_duration = Duration::from_secs(config.ban_duration_minutes * 60);
            let multiplier = config.progressive_ban_multiplier.powi(self.ban_count as i32 - 1);
            let ban_duration = Duration::from_secs((base_duration.as_secs() as f64 * multiplier) as u64);
            
            // Cap at maximum ban duration
            let max_duration = Duration::from_secs(config.max_ban_duration_hours * 3600);
            let final_duration = ban_duration.min(max_duration);
            
            self.banned_until = Some(now + final_duration);
            
            warn!("Brute force attack detected: {} failures in {}m (ban #{}, duration: {:?})",
                  self.failure_times.len(), config.failure_window_minutes, 
                  self.ban_count, final_duration);
            
            return false;
        }

        true
    }

    /// Record a successful authentication
    fn record_success(&mut self) {
        self.last_activity = Instant::now();
        self.total_successes += 1;
        
        // Clear recent failures on successful auth (but keep total counts)
        self.failure_times.clear();
        self.last_failure_time = None;
    }

    /// Check if IP is currently banned
    fn is_banned(&self) -> bool {
        if let Some(banned_until) = self.banned_until {
            Instant::now() < banned_until
        } else {
            false
        }
    }

    /// Get progressive delay based on recent failures
    fn get_progressive_delay(&self, config: &Fail2BanConfig) -> Duration {
        if !config.enable_progressive_delays {
            return Duration::from_millis(0);
        }

        let failure_count = self.failure_times.len() as u32;
        if failure_count == 0 {
            return Duration::from_millis(0);
        }

        // Exponential backoff based on failures in window
        let base_delay = config.base_delay_ms;
        let max_delay = config.max_delay_ms;
        
        let delay_ms = base_delay * 2_u64.pow(failure_count.saturating_sub(1).min(10));
        let capped_delay = delay_ms.min(max_delay);
        
        Duration::from_millis(capped_delay)
    }

    /// Get time until ban expires
    fn time_until_unban(&self) -> Option<Duration> {
        if let Some(banned_until) = self.banned_until {
            let now = Instant::now();
            if now < banned_until {
                Some(banned_until - now)
            } else {
                None
            }
        } else {
            None
        }
    }
}

/// Main Fail2Ban manager implementation
pub struct Fail2BanManager {
    config: Fail2BanConfig,
    ip_detectors: Arc<Mutex<HashMap<IpAddr, BruteForceDetector>>>,
    whitelist: Arc<Vec<IpAddr>>,
    stats: Arc<Mutex<InternalFail2BanStats>>,
}

#[derive(Debug, Default)]
struct InternalFail2BanStats {
    total_auth_attempts: u64,
    total_auth_failures: u64,
    total_bans_issued: u64,
    currently_banned_ips: usize,
    total_brute_force_events: u64,
}

impl Fail2BanManager {
    /// Create a new Fail2Ban manager
    pub fn new(config: Fail2BanConfig) -> Self {
        // Parse whitelist IPs
        let whitelist: Vec<IpAddr> = config.whitelist_ips.iter()
            .filter_map(|ip_str| ip_str.parse().ok())
            .collect();
        
        info!("Fail2Ban initialized with {} whitelisted IPs", whitelist.len());
        
        Self {
            config,
            ip_detectors: Arc::new(Mutex::new(HashMap::new())),
            whitelist: Arc::new(whitelist),
            stats: Arc::new(Mutex::new(InternalFail2BanStats::default())),
        }
    }

    /// Check if an authentication attempt should be allowed
    pub fn check_auth_attempt(&self, ip: IpAddr) -> Fail2BanDecision {
        if !self.config.enabled {
            return Fail2BanDecision::Allow;
        }

        // Check whitelist
        if self.whitelist.contains(&ip) {
            debug!("IP {} is whitelisted, allowing authentication", ip);
            return Fail2BanDecision::Allow;
        }

        // Update stats
        {
            let mut stats = self.stats.lock().unwrap();
            stats.total_auth_attempts += 1;
        }

        let ip_detectors = self.ip_detectors.lock().unwrap();
        if let Some(detector) = ip_detectors.get(&ip) {
            if detector.is_banned() {
                debug!("Authentication attempt from banned IP {}", ip);
                return Fail2BanDecision::Block {
                    reason: "IP is currently banned due to brute force protection".to_string(),
                    delay: Duration::from_millis(self.config.max_delay_ms),
                    time_until_unban: detector.time_until_unban(),
                };
            }

            // Apply progressive delay based on recent failures
            let delay = detector.get_progressive_delay(&self.config);
            if delay > Duration::from_millis(0) {
                debug!("Applying progressive delay of {:?} for IP {}", delay, ip);
                return Fail2BanDecision::Delay {
                    delay,
                    reason: format!("Progressive delay due to {} recent failures", 
                                  detector.failure_times.len()),
                };
            }
        }

        Fail2BanDecision::Allow
    }

    /// Record an authentication failure
    pub fn record_auth_failure(&self, ip: IpAddr) {
        if !self.config.enabled {
            return;
        }

        // Check whitelist
        if self.whitelist.contains(&ip) {
            debug!("Not recording failure for whitelisted IP {}", ip);
            return;
        }

        // Update stats
        {
            let mut stats = self.stats.lock().unwrap();
            stats.total_auth_failures += 1;
        }

        let mut ip_detectors = self.ip_detectors.lock().unwrap();
        let detector = ip_detectors.entry(ip).or_insert_with(BruteForceDetector::new);
        
        let was_banned_before = detector.is_banned();
        let allowed = detector.record_failure(&self.config);
        
        if !allowed && !was_banned_before {
            // New ban issued
            info!("Issued fail2ban for IP {} after {} failures", ip, detector.total_failures);
            
            {
                let mut stats = self.stats.lock().unwrap();
                stats.total_bans_issued += 1;
                stats.total_brute_force_events += 1;
            }
        }
    }

    /// Record a successful authentication
    pub fn record_auth_success(&self, ip: IpAddr) {
        if !self.config.enabled {
            return;
        }

        let mut ip_detectors = self.ip_detectors.lock().unwrap();
        if let Some(detector) = ip_detectors.get_mut(&ip) {
            detector.record_success();
            debug!("Recorded successful authentication for IP {}", ip);
        }
    }

    /// Manually ban an IP address
    pub fn ban_ip(&self, ip: IpAddr, duration: Duration, reason: &str) {
        // Don't ban whitelisted IPs
        if self.whitelist.contains(&ip) {
            warn!("Attempted to ban whitelisted IP {}: {}", ip, reason);
            return;
        }

        let mut ip_detectors = self.ip_detectors.lock().unwrap();
        let detector = ip_detectors.entry(ip).or_insert_with(BruteForceDetector::new);
        
        detector.banned_until = Some(Instant::now() + duration);
        detector.ban_count += 1;
        
        info!("Manually banned IP {} for {:?}: {}", ip, duration, reason);
        
        {
            let mut stats = self.stats.lock().unwrap();
            stats.total_bans_issued += 1;
        }
    }

    /// Unban an IP address
    pub fn unban_ip(&self, ip: IpAddr) -> bool {
        let mut ip_detectors = self.ip_detectors.lock().unwrap();
        if let Some(detector) = ip_detectors.get_mut(&ip) {
            if detector.is_banned() {
                detector.banned_until = None;
                // Reset failure count but keep history
                detector.failure_times.clear();
                info!("Unbanned IP {}", ip);
                return true;
            }
        }
        false
    }

    /// Check if an IP is currently banned
    pub fn is_ip_banned(&self, ip: IpAddr) -> bool {
        let ip_detectors = self.ip_detectors.lock().unwrap();
        if let Some(detector) = ip_detectors.get(&ip) {
            detector.is_banned()
        } else {
            false
        }
    }

    /// Get list of currently banned IPs
    pub fn get_banned_ips(&self) -> Vec<IpAddr> {
        let ip_detectors = self.ip_detectors.lock().unwrap();
        ip_detectors.iter()
            .filter(|(_, detector)| detector.is_banned())
            .map(|(ip, _)| *ip)
            .collect()
    }

    /// Get list of IPs with recent failures (potential threats)
    pub fn get_suspicious_ips(&self) -> Vec<IpAddr> {
        let ip_detectors = self.ip_detectors.lock().unwrap();
        let threshold = Duration::from_secs(self.config.failure_window_minutes * 60);
        let cutoff = Instant::now() - threshold;
        
        ip_detectors.iter()
            .filter(|(_, detector)| {
                !detector.failure_times.is_empty() && 
                detector.last_failure_time.map_or(false, |t| t > cutoff)
            })
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
            // Keep if recently active, currently banned, or has recent failures
            detector.last_activity > cutoff_time || 
            detector.is_banned() || 
            !detector.failure_times.is_empty()
        });
        
        let removed_count = initial_count - ip_detectors.len();
        if removed_count > 0 {
            debug!("Cleaned up {} old fail2ban detector entries", removed_count);
        }

        // Update banned IP count in stats
        let banned_count = ip_detectors.iter().filter(|(_, detector)| detector.is_banned()).count();
        {
            let mut stats = self.stats.lock().unwrap();
            stats.currently_banned_ips = banned_count;
        }
    }

    /// Get fail2ban statistics
    pub fn get_stats(&self) -> Fail2BanStats {
        let stats = self.stats.lock().unwrap();
        Fail2BanStats {
            total_auth_attempts: stats.total_auth_attempts,
            total_auth_failures: stats.total_auth_failures,
            total_bans_issued: stats.total_bans_issued,
            currently_banned_ips: stats.currently_banned_ips,
            total_brute_force_events: stats.total_brute_force_events,
        }
    }

    /// Get detailed IP statistics
    pub fn get_ip_stats(&self, ip: IpAddr) -> Option<IpFail2BanStats> {
        let ip_detectors = self.ip_detectors.lock().unwrap();
        ip_detectors.get(&ip).map(|detector| IpFail2BanStats {
            ip,
            total_failures: detector.total_failures,
            total_successes: detector.total_successes,
            failures_in_window: detector.failure_times.len() as u32,
            ban_count: detector.ban_count,
            is_banned: detector.is_banned(),
            banned_until: detector.banned_until,
            time_until_unban: detector.time_until_unban(),
            last_failure_time: detector.last_failure_time,
            last_activity: detector.last_activity,
        })
    }

    /// Get all IP statistics
    pub fn get_all_ip_stats(&self) -> Vec<IpFail2BanStats> {
        let ip_detectors = self.ip_detectors.lock().unwrap();
        ip_detectors.iter().map(|(ip, detector)| IpFail2BanStats {
            ip: *ip,
            total_failures: detector.total_failures,
            total_successes: detector.total_successes,
            failures_in_window: detector.failure_times.len() as u32,
            ban_count: detector.ban_count,
            is_banned: detector.is_banned(),
            banned_until: detector.banned_until,
            time_until_unban: detector.time_until_unban(),
            last_failure_time: detector.last_failure_time,
            last_activity: detector.last_activity,
        }).collect()
    }

    /// Add IP to whitelist
    pub fn add_to_whitelist(&mut self, ip: IpAddr) {
        let whitelist = Arc::make_mut(&mut self.whitelist);
        if !whitelist.contains(&ip) {
            whitelist.push(ip);
            info!("Added IP {} to fail2ban whitelist", ip);
        }
    }

    /// Remove IP from whitelist
    pub fn remove_from_whitelist(&mut self, ip: IpAddr) -> bool {
        let whitelist = Arc::make_mut(&mut self.whitelist);
        if let Some(pos) = whitelist.iter().position(|&x| x == ip) {
            whitelist.remove(pos);
            info!("Removed IP {} from fail2ban whitelist", ip);
            true
        } else {
            false
        }
    }

    /// Get current whitelist
    pub fn get_whitelist(&self) -> Vec<IpAddr> {
        self.whitelist.as_ref().clone()
    }
}

/// Decision result from fail2ban check
#[derive(Debug, Clone)]
pub enum Fail2BanDecision {
    Allow,
    Block {
        reason: String,
        delay: Duration,
        time_until_unban: Option<Duration>,
    },
    Delay {
        delay: Duration,
        reason: String,
    },
}

/// Fail2Ban statistics
#[derive(Debug, Clone)]
pub struct Fail2BanStats {
    pub total_auth_attempts: u64,
    pub total_auth_failures: u64,
    pub total_bans_issued: u64,
    pub currently_banned_ips: usize,
    pub total_brute_force_events: u64,
}

/// Statistics for a specific IP address
#[derive(Debug, Clone)]
pub struct IpFail2BanStats {
    pub ip: IpAddr,
    pub total_failures: u64,
    pub total_successes: u64,
    pub failures_in_window: u32,
    pub ban_count: u32,
    pub is_banned: bool,
    pub banned_until: Option<Instant>,
    pub time_until_unban: Option<Duration>,
    pub last_failure_time: Option<Instant>,
    pub last_activity: Instant,
}



#[cfg(test)]
mod tests {
    use super::*;


    #[test]
    fn test_brute_force_detector() {
        let config = Fail2BanConfig {
            max_auth_failures: 3,
            failure_window_minutes: 1,
            ..Default::default()
        };
        
        let mut detector = BruteForceDetector::new();
        
        // Should allow initial failures
        assert!(detector.record_failure(&config));
        assert!(detector.record_failure(&config));
        
        // Should ban after threshold
        assert!(!detector.record_failure(&config));
        assert!(detector.is_banned());
    }

    #[test]
    fn test_fail2ban_manager_basic() {
        let config = Fail2BanConfig {
            enabled: true,
            max_auth_failures: 2,
            failure_window_minutes: 1,
            ..Default::default()
        };
        
        let manager = Fail2BanManager::new(config);
        let ip = "192.168.1.100".parse().unwrap();
        
        // Should allow initial attempts
        assert!(matches!(manager.check_auth_attempt(ip), Fail2BanDecision::Allow));
        
        // Record failures
        manager.record_auth_failure(ip);
        manager.record_auth_failure(ip);
        
        // Should block after failures
        assert!(matches!(manager.check_auth_attempt(ip), Fail2BanDecision::Block { .. }));
        assert!(manager.is_ip_banned(ip));
    }

    #[test]
    fn test_whitelist_protection() {
        let config = Fail2BanConfig {
            enabled: true,
            max_auth_failures: 1,
            whitelist_ips: vec!["127.0.0.1".to_string()],
            ..Default::default()
        };
        
        let manager = Fail2BanManager::new(config);
        let ip = "127.0.0.1".parse().unwrap();
        
        // Should always allow whitelisted IPs
        for _ in 0..10 {
            assert!(matches!(manager.check_auth_attempt(ip), Fail2BanDecision::Allow));
            manager.record_auth_failure(ip);
        }
        
        assert!(!manager.is_ip_banned(ip));
    }

    #[test]
    fn test_progressive_delays() {
        let config = Fail2BanConfig {
            enabled: true,
            max_auth_failures: 10, // High threshold to test delays
            enable_progressive_delays: true,
            base_delay_ms: 100,
            ..Default::default()
        };
        
        let manager = Fail2BanManager::new(config);
        let ip = "192.168.1.100".parse().unwrap();
        
        // First attempt should be allowed
        assert!(matches!(manager.check_auth_attempt(ip), Fail2BanDecision::Allow));
        
        // Record failure and check for delay
        manager.record_auth_failure(ip);
        assert!(matches!(manager.check_auth_attempt(ip), Fail2BanDecision::Delay { .. }));
    }

    #[test]
    fn test_manual_ban_unban() {
        let config = Fail2BanConfig::default();
        let manager = Fail2BanManager::new(config);
        let ip = "192.168.1.100".parse().unwrap();
        
        // Should initially allow
        assert!(matches!(manager.check_auth_attempt(ip), Fail2BanDecision::Allow));
        
        // Ban IP manually
        manager.ban_ip(ip, Duration::from_secs(1), "test");
        assert!(manager.is_ip_banned(ip));
        assert!(matches!(manager.check_auth_attempt(ip), Fail2BanDecision::Block { .. }));
        
        // Unban IP
        assert!(manager.unban_ip(ip));
        assert!(!manager.is_ip_banned(ip));
        assert!(matches!(manager.check_auth_attempt(ip), Fail2BanDecision::Allow));
    }

    #[test]
    fn test_success_clears_failures() {
        let config = Fail2BanConfig {
            enabled: true,
            max_auth_failures: 3,
            ..Default::default()
        };
        
        let manager = Fail2BanManager::new(config);
        let ip = "192.168.1.100".parse().unwrap();
        
        // Record some failures
        manager.record_auth_failure(ip);
        manager.record_auth_failure(ip);
        
        // Record success - should clear failures
        manager.record_auth_success(ip);
        
        // Should not be banned even after more failures
        manager.record_auth_failure(ip);
        manager.record_auth_failure(ip);
        assert!(!manager.is_ip_banned(ip));
    }
}