//! Security Module
//! 
//! Provides security hardening features including rate limiting, DDoS protection,
//! fail2ban integration, and secure configuration management.

pub mod rate_limiter;
pub mod ddos_protection;
pub mod fail2ban;
pub mod secrets;

pub use rate_limiter::{RateLimiter, TokenBucket, RateLimitConfig};
pub use ddos_protection::{DdosProtection, DdosConfig};
pub use fail2ban::{Fail2BanManager, Fail2BanConfig};
pub use secrets::{SecretsManager, SecureConfig};

use std::net::IpAddr;
use std::time::Duration;
use serde::{Deserialize, Serialize};

/// Security configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SecurityConfig {
    pub rate_limiting: RateLimitConfig,
    pub ddos_protection: DdosConfig,
    pub fail2ban: Fail2BanConfig,
    pub secrets: SecureConfigSettings,
}

/// Secure configuration settings
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SecureConfigSettings {
    pub encrypt_config: bool,
    pub use_env_secrets: bool,
    pub secret_key_env: String,
    pub config_encryption_key_env: String,
}

/// Security event types for logging and monitoring
#[derive(Debug, Clone)]
pub enum SecurityEvent {
    RateLimitExceeded {
        ip: IpAddr,
        limit_type: String,
        current_rate: u32,
        limit: u32,
    },
    DdosAttackDetected {
        ip: IpAddr,
        connection_count: u32,
        time_window: Duration,
    },
    BruteForceDetected {
        ip: IpAddr,
        failed_attempts: u32,
        time_window: Duration,
    },
    IpBlocked {
        ip: IpAddr,
        reason: String,
        duration: Duration,
    },
    IpUnblocked {
        ip: IpAddr,
        reason: String,
    },
}

/// Security statistics
#[derive(Debug, Clone)]
pub struct SecurityStats {
    pub rate_limited_ips: usize,
    pub blocked_ips: usize,
    pub ddos_events_detected: u64,
    pub brute_force_events_detected: u64,
    pub total_blocked_connections: u64,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            rate_limiting: RateLimitConfig::default(),
            ddos_protection: DdosConfig::default(),
            fail2ban: Fail2BanConfig::default(),
            secrets: SecureConfigSettings {
                encrypt_config: false,
                use_env_secrets: true,
                secret_key_env: "SOCKS5_SECRET_KEY".to_string(),
                config_encryption_key_env: "SOCKS5_CONFIG_KEY".to_string(),
            },
        }
    }
}