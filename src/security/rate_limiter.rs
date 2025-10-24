//! Rate Limiting Implementation
//! 
//! Implements token bucket rate limiting per IP address to prevent abuse
//! and connection flooding attacks.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use serde::{Deserialize, Serialize};
use tracing::{debug, warn, info};

/// Rate limiting configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RateLimitConfig {
    pub enabled: bool,
    pub connections_per_ip_per_minute: u32,
    pub connections_per_ip_burst: u32,
    pub auth_attempts_per_ip_per_minute: u32,
    pub auth_attempts_per_ip_burst: u32,
    pub global_connections_per_second: u32,
    pub cleanup_interval_seconds: u64,
    pub block_duration_minutes: u64,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            connections_per_ip_per_minute: 60,
            connections_per_ip_burst: 10,
            auth_attempts_per_ip_per_minute: 10,
            auth_attempts_per_ip_burst: 3,
            global_connections_per_second: 1000,
            cleanup_interval_seconds: 300, // 5 minutes
            block_duration_minutes: 15,
        }
    }
}

/// Token bucket for rate limiting
#[derive(Debug, Clone)]
pub struct TokenBucket {
    capacity: u32,
    tokens: f64,
    refill_rate: f64, // tokens per second
    last_refill: Instant,
}

impl TokenBucket {
    /// Create a new token bucket
    pub fn new(capacity: u32, refill_rate_per_minute: u32) -> Self {
        let refill_rate = refill_rate_per_minute as f64 / 60.0; // Convert to per second
        Self {
            capacity,
            tokens: capacity as f64,
            refill_rate,
            last_refill: Instant::now(),
        }
    }

    /// Try to consume tokens from the bucket
    pub fn try_consume(&mut self, tokens: u32) -> bool {
        self.refill();
        
        if self.tokens >= tokens as f64 {
            self.tokens -= tokens as f64;
            true
        } else {
            false
        }
    }

    /// Refill tokens based on elapsed time
    fn refill(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        
        if elapsed > 0.0 {
            let tokens_to_add = elapsed * self.refill_rate;
            self.tokens = (self.tokens + tokens_to_add).min(self.capacity as f64);
            self.last_refill = now;
        }
    }

    /// Get current token count
    pub fn current_tokens(&mut self) -> f64 {
        self.refill();
        self.tokens
    }

    /// Check if bucket is empty
    pub fn is_empty(&mut self) -> bool {
        self.refill();
        self.tokens < 1.0
    }
}

/// Rate limiter for tracking per-IP limits
#[derive(Debug)]
struct IpRateLimit {
    connection_bucket: TokenBucket,
    auth_bucket: TokenBucket,
    last_activity: Instant,
    total_connections: u64,
    total_auth_attempts: u64,
    blocked_until: Option<Instant>,
}

impl IpRateLimit {
    fn new(config: &RateLimitConfig) -> Self {
        Self {
            connection_bucket: TokenBucket::new(
                config.connections_per_ip_burst,
                config.connections_per_ip_per_minute,
            ),
            auth_bucket: TokenBucket::new(
                config.auth_attempts_per_ip_burst,
                config.auth_attempts_per_ip_per_minute,
            ),
            last_activity: Instant::now(),
            total_connections: 0,
            total_auth_attempts: 0,
            blocked_until: None,
        }
    }

    fn is_blocked(&self) -> bool {
        if let Some(blocked_until) = self.blocked_until {
            Instant::now() < blocked_until
        } else {
            false
        }
    }

    fn block_for_duration(&mut self, duration: Duration) {
        self.blocked_until = Some(Instant::now() + duration);
    }

    fn unblock(&mut self) {
        self.blocked_until = None;
    }
}

/// Main rate limiter implementation
pub struct RateLimiter {
    config: RateLimitConfig,
    ip_limits: Arc<Mutex<HashMap<IpAddr, IpRateLimit>>>,
    global_bucket: Arc<Mutex<TokenBucket>>,
    stats: Arc<Mutex<InternalRateLimiterStats>>,
}

#[derive(Debug, Default)]
struct InternalRateLimiterStats {
    total_connections_checked: u64,
    total_connections_blocked: u64,
    total_auth_attempts_checked: u64,
    total_auth_attempts_blocked: u64,
    currently_blocked_ips: usize,
}

impl RateLimiter {
    /// Create a new rate limiter
    pub fn new(config: RateLimitConfig) -> Self {
        let global_bucket = TokenBucket::new(
            config.global_connections_per_second * 10, // 10 second burst capacity
            config.global_connections_per_second * 60, // Convert to per minute
        );

        Self {
            config,
            ip_limits: Arc::new(Mutex::new(HashMap::new())),
            global_bucket: Arc::new(Mutex::new(global_bucket)),
            stats: Arc::new(Mutex::new(InternalRateLimiterStats::default())),
        }
    }

    /// Check if a connection from the given IP should be allowed
    pub fn check_connection_rate(&self, ip: IpAddr) -> bool {
        if !self.config.enabled {
            return true;
        }

        // Update stats
        {
            let mut stats = self.stats.lock().unwrap();
            stats.total_connections_checked += 1;
        }

        // Check global rate limit first
        {
            let mut global_bucket = self.global_bucket.lock().unwrap();
            if !global_bucket.try_consume(1) {
                warn!("Global connection rate limit exceeded");
                self.increment_blocked_connections();
                return false;
            }
        }

        // Check per-IP rate limit
        let mut ip_limits = self.ip_limits.lock().unwrap();
        let ip_limit = ip_limits.entry(ip).or_insert_with(|| IpRateLimit::new(&self.config));

        // Check if IP is currently blocked
        if ip_limit.is_blocked() {
            debug!("Connection from {} blocked due to temporary ban", ip);
            self.increment_blocked_connections();
            return false;
        }

        // Try to consume connection token
        if ip_limit.connection_bucket.try_consume(1) {
            ip_limit.last_activity = Instant::now();
            ip_limit.total_connections += 1;
            debug!("Connection from {} allowed (tokens remaining: {:.1})", 
                   ip, ip_limit.connection_bucket.current_tokens());
            true
        } else {
            warn!("Connection rate limit exceeded for IP {}", ip);
            
            // Block IP for configured duration
            let block_duration = Duration::from_secs(self.config.block_duration_minutes * 60);
            ip_limit.block_for_duration(block_duration);
            
            info!("Temporarily blocked IP {} for {:?} due to connection rate limit", ip, block_duration);
            
            self.increment_blocked_connections();
            false
        }
    }

    /// Check if an authentication attempt from the given IP should be allowed
    pub fn check_auth_rate(&self, ip: IpAddr) -> bool {
        if !self.config.enabled {
            return true;
        }

        // Update stats
        {
            let mut stats = self.stats.lock().unwrap();
            stats.total_auth_attempts_checked += 1;
        }

        let mut ip_limits = self.ip_limits.lock().unwrap();
        let ip_limit = ip_limits.entry(ip).or_insert_with(|| IpRateLimit::new(&self.config));

        // Check if IP is currently blocked
        if ip_limit.is_blocked() {
            debug!("Auth attempt from {} blocked due to temporary ban", ip);
            self.increment_blocked_auth_attempts();
            return false;
        }

        // Try to consume auth token
        if ip_limit.auth_bucket.try_consume(1) {
            ip_limit.last_activity = Instant::now();
            ip_limit.total_auth_attempts += 1;
            debug!("Auth attempt from {} allowed (tokens remaining: {:.1})", 
                   ip, ip_limit.auth_bucket.current_tokens());
            true
        } else {
            warn!("Authentication rate limit exceeded for IP {}", ip);
            
            // Block IP for configured duration
            let block_duration = Duration::from_secs(self.config.block_duration_minutes * 60);
            ip_limit.block_for_duration(block_duration);
            
            info!("Temporarily blocked IP {} for {:?} due to auth rate limit", ip, block_duration);
            
            self.increment_blocked_auth_attempts();
            false
        }
    }

    /// Manually block an IP address
    pub fn block_ip(&self, ip: IpAddr, duration: Duration, reason: &str) {
        let mut ip_limits = self.ip_limits.lock().unwrap();
        let ip_limit = ip_limits.entry(ip).or_insert_with(|| IpRateLimit::new(&self.config));
        
        ip_limit.block_for_duration(duration);
        info!("Manually blocked IP {} for {:?}: {}", ip, duration, reason);
    }

    /// Unblock an IP address
    pub fn unblock_ip(&self, ip: IpAddr) -> bool {
        let mut ip_limits = self.ip_limits.lock().unwrap();
        if let Some(ip_limit) = ip_limits.get_mut(&ip) {
            if ip_limit.is_blocked() {
                ip_limit.unblock();
                info!("Unblocked IP {}", ip);
                return true;
            }
        }
        false
    }

    /// Check if an IP is currently blocked
    pub fn is_ip_blocked(&self, ip: IpAddr) -> bool {
        let ip_limits = self.ip_limits.lock().unwrap();
        if let Some(ip_limit) = ip_limits.get(&ip) {
            ip_limit.is_blocked()
        } else {
            false
        }
    }

    /// Get list of currently blocked IPs
    pub fn get_blocked_ips(&self) -> Vec<IpAddr> {
        let ip_limits = self.ip_limits.lock().unwrap();
        ip_limits.iter()
            .filter(|(_, limit)| limit.is_blocked())
            .map(|(ip, _)| *ip)
            .collect()
    }

    /// Clean up old rate limit entries
    pub fn cleanup_old_entries(&self) {
        let cleanup_threshold = Duration::from_secs(self.config.cleanup_interval_seconds * 2);
        let cutoff_time = Instant::now() - cleanup_threshold;
        
        let mut ip_limits = self.ip_limits.lock().unwrap();
        let initial_count = ip_limits.len();
        
        ip_limits.retain(|_, limit| {
            // Keep if recently active or currently blocked
            limit.last_activity > cutoff_time || limit.is_blocked()
        });
        
        let removed_count = initial_count - ip_limits.len();
        if removed_count > 0 {
            debug!("Cleaned up {} old rate limit entries", removed_count);
        }

        // Update blocked IP count in stats
        let blocked_count = ip_limits.iter().filter(|(_, limit)| limit.is_blocked()).count();
        {
            let mut stats = self.stats.lock().unwrap();
            stats.currently_blocked_ips = blocked_count;
        }
    }

    /// Get rate limiter statistics
    pub fn get_stats(&self) -> RateLimiterStats {
        let stats = self.stats.lock().unwrap();
        RateLimiterStats {
            total_connections_checked: stats.total_connections_checked,
            total_connections_blocked: stats.total_connections_blocked,
            total_auth_attempts_checked: stats.total_auth_attempts_checked,
            total_auth_attempts_blocked: stats.total_auth_attempts_blocked,
            currently_blocked_ips: stats.currently_blocked_ips,
        }
    }

    /// Get detailed IP statistics
    pub fn get_ip_stats(&self, ip: IpAddr) -> Option<IpStats> {
        let ip_limits = self.ip_limits.lock().unwrap();
        ip_limits.get(&ip).map(|limit| IpStats {
            ip,
            total_connections: limit.total_connections,
            total_auth_attempts: limit.total_auth_attempts,
            connection_tokens_remaining: limit.connection_bucket.clone().current_tokens(),
            auth_tokens_remaining: limit.auth_bucket.clone().current_tokens(),
            is_blocked: limit.is_blocked(),
            blocked_until: limit.blocked_until,
            last_activity: limit.last_activity,
        })
    }

    /// Get all IP statistics
    pub fn get_all_ip_stats(&self) -> Vec<IpStats> {
        let ip_limits = self.ip_limits.lock().unwrap();
        ip_limits.iter().map(|(ip, limit)| IpStats {
            ip: *ip,
            total_connections: limit.total_connections,
            total_auth_attempts: limit.total_auth_attempts,
            connection_tokens_remaining: limit.connection_bucket.clone().current_tokens(),
            auth_tokens_remaining: limit.auth_bucket.clone().current_tokens(),
            is_blocked: limit.is_blocked(),
            blocked_until: limit.blocked_until,
            last_activity: limit.last_activity,
        }).collect()
    }

    fn increment_blocked_connections(&self) {
        let mut stats = self.stats.lock().unwrap();
        stats.total_connections_blocked += 1;
    }

    fn increment_blocked_auth_attempts(&self) {
        let mut stats = self.stats.lock().unwrap();
        stats.total_auth_attempts_blocked += 1;
    }
}

/// Statistics for a specific IP address
#[derive(Debug, Clone)]
pub struct IpStats {
    pub ip: IpAddr,
    pub total_connections: u64,
    pub total_auth_attempts: u64,
    pub connection_tokens_remaining: f64,
    pub auth_tokens_remaining: f64,
    pub is_blocked: bool,
    pub blocked_until: Option<Instant>,
    pub last_activity: Instant,
}

/// Rate limiter statistics
#[derive(Debug, Clone)]
pub struct RateLimiterStats {
    pub total_connections_checked: u64,
    pub total_connections_blocked: u64,
    pub total_auth_attempts_checked: u64,
    pub total_auth_attempts_blocked: u64,
    pub currently_blocked_ips: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_token_bucket_basic() {
        let mut bucket = TokenBucket::new(10, 60); // 10 capacity, 1 per second refill
        
        // Should be able to consume initial tokens
        assert!(bucket.try_consume(5));
        assert!(bucket.try_consume(5));
        
        // Should not be able to consume more
        assert!(!bucket.try_consume(1));
    }

    #[test]
    fn test_token_bucket_refill() {
        let mut bucket = TokenBucket::new(2, 120); // 2 capacity, 2 per second refill
        
        // Consume all tokens
        assert!(bucket.try_consume(2));
        assert!(!bucket.try_consume(1));
        
        // Wait for refill
        thread::sleep(Duration::from_millis(600)); // Wait for ~1 token
        assert!(bucket.try_consume(1));
    }

    #[test]
    fn test_rate_limiter_connection_limit() {
        let config = RateLimitConfig {
            enabled: true,
            connections_per_ip_burst: 2,
            connections_per_ip_per_minute: 60,
            ..Default::default()
        };
        
        let limiter = RateLimiter::new(config);
        let ip = "127.0.0.1".parse().unwrap();
        
        // Should allow burst connections
        assert!(limiter.check_connection_rate(ip));
        assert!(limiter.check_connection_rate(ip));
        
        // Should block after burst limit
        assert!(!limiter.check_connection_rate(ip));
    }

    #[test]
    fn test_rate_limiter_disabled() {
        let config = RateLimitConfig {
            enabled: false,
            ..Default::default()
        };
        
        let limiter = RateLimiter::new(config);
        let ip = "127.0.0.1".parse().unwrap();
        
        // Should always allow when disabled
        for _ in 0..100 {
            assert!(limiter.check_connection_rate(ip));
        }
    }

    #[test]
    fn test_manual_ip_blocking() {
        let config = RateLimitConfig::default();
        let limiter = RateLimiter::new(config);
        let ip = "127.0.0.1".parse().unwrap();
        
        // Should initially allow
        assert!(limiter.check_connection_rate(ip));
        
        // Block IP manually
        limiter.block_ip(ip, Duration::from_secs(1), "test");
        assert!(limiter.is_ip_blocked(ip));
        assert!(!limiter.check_connection_rate(ip));
        
        // Unblock IP
        assert!(limiter.unblock_ip(ip));
        assert!(!limiter.is_ip_blocked(ip));
        assert!(limiter.check_connection_rate(ip));
    }
}