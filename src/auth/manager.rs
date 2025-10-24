//! Authentication Manager

use crate::Result;
use super::{AuthResult, UserStore, SessionTracker, RateLimitInfo};
use crate::protocol::AuthMethod;
use crate::config::Config;
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tracing::{debug, warn, info};

/// Manages user authentication and sessions
pub struct AuthManager {
    user_store: Arc<Mutex<UserStore>>,
    session_tracker: Arc<Mutex<SessionTracker>>,
    ip_rate_limits: Arc<Mutex<HashMap<IpAddr, RateLimitInfo>>>,
    user_rate_limits: Arc<Mutex<HashMap<String, RateLimitInfo>>>,
    config: Arc<Config>,
}

impl AuthManager {
    /// Create a new authentication manager
    pub fn new(config: Arc<Config>) -> Self {
        let mut user_store = UserStore::new();
        user_store.load_from_config(&config.auth.users);
        
        Self {
            user_store: Arc::new(Mutex::new(user_store)),
            session_tracker: Arc::new(Mutex::new(SessionTracker::new())),
            ip_rate_limits: Arc::new(Mutex::new(HashMap::new())),
            user_rate_limits: Arc::new(Mutex::new(HashMap::new())),
            config,
        }
    }

    /// Authenticate a user with the given method and credentials
    pub async fn authenticate(&self, method: AuthMethod, credentials: &[u8], client_ip: IpAddr) -> Result<AuthResult> {
        debug!("Authentication attempt from {}: method={:?}", client_ip, method);

        // Check rate limiting first
        if self.is_rate_limited(client_ip) {
            warn!("Rate limited authentication attempt from {}", client_ip);
            return Ok(AuthResult {
                success: false,
                user_id: None,
                session_id: String::new(),
            });
        }

        match method {
            AuthMethod::NoAuth => {
                if !self.config.auth.enabled {
                    debug!("No authentication required, allowing connection from {}", client_ip);
                    let session_id = self.create_session("anonymous".to_string(), client_ip);
                    Ok(AuthResult {
                        success: true,
                        user_id: Some("anonymous".to_string()),
                        session_id,
                    })
                } else {
                    warn!("No authentication attempted but authentication is required from {}", client_ip);
                    self.record_auth_failure(client_ip);
                    Ok(AuthResult {
                        success: false,
                        user_id: None,
                        session_id: String::new(),
                    })
                }
            }
            AuthMethod::UserPass => {
                if let Some((username, password)) = self.parse_userpass_credentials(credentials) {
                    // Check user-specific rate limiting
                    if self.is_user_rate_limited(&username) {
                        warn!("User '{}' is rate limited from {}", username, client_ip);
                        return Ok(AuthResult {
                            success: false,
                            user_id: None,
                            session_id: String::new(),
                        });
                    }

                    if self.validate_user(&username, &password) {
                        info!("Successful authentication for user '{}' from {}", username, client_ip);
                        self.reset_rate_limit(client_ip);
                        self.reset_user_rate_limit(&username);
                        let session_id = self.create_session(username.clone(), client_ip);
                        Ok(AuthResult {
                            success: true,
                            user_id: Some(username),
                            session_id,
                        })
                    } else {
                        warn!("Failed authentication for user '{}' from {}", username, client_ip);
                        self.record_auth_failure(client_ip);
                        self.record_user_auth_failure(&username);
                        Ok(AuthResult {
                            success: false,
                            user_id: None,
                            session_id: String::new(),
                        })
                    }
                } else {
                    warn!("Invalid username/password credentials format from {}", client_ip);
                    self.record_auth_failure(client_ip);
                    Ok(AuthResult {
                        success: false,
                        user_id: None,
                        session_id: String::new(),
                    })
                }
            }
            AuthMethod::Unsupported => {
                warn!("Unsupported authentication method from {}", client_ip);
                Ok(AuthResult {
                    success: false,
                    user_id: None,
                    session_id: String::new(),
                })
            }
        }
    }

    /// Validate user credentials
    pub fn validate_user(&self, username: &str, password: &str) -> bool {
        let user_store = self.user_store.lock().unwrap();
        user_store.validate_credentials(username, password)
    }

    /// Create a new session for a user
    pub fn create_session(&self, user_id: String, client_ip: IpAddr) -> String {
        let mut session_tracker = self.session_tracker.lock().unwrap();
        session_tracker.create_session(user_id, client_ip)
    }

    /// Parse username/password credentials from SOCKS5 auth packet
    fn parse_userpass_credentials(&self, credentials: &[u8]) -> Option<(String, String)> {
        if credentials.len() < 3 {
            return None;
        }

        // RFC 1929: Username/Password Authentication for SOCKS V5
        // +----+------+----------+------+----------+
        // |VER | ULEN |  UNAME   | PLEN |  PASSWD  |
        // +----+------+----------+------+----------+
        // | 1  |  1   | 1 to 255 |  1   | 1 to 255 |
        // +----+------+----------+------+----------+

        let version = credentials[0];
        if version != 0x01 {
            return None;
        }

        let username_len = credentials[1] as usize;
        if credentials.len() < 2 + username_len + 1 {
            return None;
        }

        let username = String::from_utf8_lossy(&credentials[2..2 + username_len]).to_string();
        
        let password_len = credentials[2 + username_len] as usize;
        if credentials.len() < 2 + username_len + 1 + password_len {
            return None;
        }

        let password = String::from_utf8_lossy(&credentials[3 + username_len..3 + username_len + password_len]).to_string();

        Some((username, password))
    }

    /// Check if an IP is currently rate limited
    fn is_rate_limited(&self, client_ip: IpAddr) -> bool {
        let ip_rate_limits = self.ip_rate_limits.lock().unwrap();
        if let Some(rate_limit) = ip_rate_limits.get(&client_ip) {
            rate_limit.is_blocked()
        } else {
            false
        }
    }

    /// Check if a user is currently rate limited
    fn is_user_rate_limited(&self, username: &str) -> bool {
        let user_rate_limits = self.user_rate_limits.lock().unwrap();
        if let Some(rate_limit) = user_rate_limits.get(username) {
            rate_limit.is_blocked()
        } else {
            false
        }
    }

    /// Record an authentication failure for rate limiting
    fn record_auth_failure(&self, client_ip: IpAddr) {
        let mut ip_rate_limits = self.ip_rate_limits.lock().unwrap();
        let rate_limit = ip_rate_limits.entry(client_ip).or_insert_with(RateLimitInfo::new);
        rate_limit.record_failure();
    }

    /// Record an authentication failure for a specific user
    fn record_user_auth_failure(&self, username: &str) {
        let mut user_rate_limits = self.user_rate_limits.lock().unwrap();
        let rate_limit = user_rate_limits.entry(username.to_string()).or_insert_with(RateLimitInfo::new);
        rate_limit.record_failure();
    }

    /// Reset rate limiting for an IP after successful authentication
    fn reset_rate_limit(&self, client_ip: IpAddr) {
        let mut ip_rate_limits = self.ip_rate_limits.lock().unwrap();
        if let Some(rate_limit) = ip_rate_limits.get_mut(&client_ip) {
            rate_limit.reset();
        }
    }

    /// Reset rate limiting for a user after successful authentication
    fn reset_user_rate_limit(&self, username: &str) {
        let mut user_rate_limits = self.user_rate_limits.lock().unwrap();
        if let Some(rate_limit) = user_rate_limits.get_mut(username) {
            rate_limit.reset();
        }
    }

    /// Get session information
    pub fn get_session(&self, session_id: &str) -> Option<super::UserSession> {
        let session_tracker = self.session_tracker.lock().unwrap();
        session_tracker.get_session(session_id).cloned()
    }

    /// Update session activity
    pub fn update_session_activity(&self, session_id: &str) -> bool {
        let mut session_tracker = self.session_tracker.lock().unwrap();
        session_tracker.update_session_activity(session_id)
    }

    /// Remove a session
    pub fn remove_session(&self, session_id: &str) -> bool {
        let mut session_tracker = self.session_tracker.lock().unwrap();
        session_tracker.remove_session(session_id)
    }

    /// Clean up expired sessions and rate limits
    pub fn cleanup_expired(&self) {
        // Clean up expired sessions
        let session_timeout = Duration::from_secs(3600); // 1 hour default
        let mut session_tracker = self.session_tracker.lock().unwrap();
        let expired_count = session_tracker.cleanup_expired_sessions(session_timeout);
        if expired_count > 0 {
            debug!("Cleaned up {} expired sessions", expired_count);
        }

        // Clean up old IP rate limit entries
        let mut ip_rate_limits = self.ip_rate_limits.lock().unwrap();
        let cutoff = std::time::Instant::now() - Duration::from_secs(3600); // 1 hour
        ip_rate_limits.retain(|_, rate_limit| {
            rate_limit.last_attempt > cutoff || rate_limit.is_blocked()
        });

        // Clean up old user rate limit entries
        let mut user_rate_limits = self.user_rate_limits.lock().unwrap();
        user_rate_limits.retain(|_, rate_limit| {
            rate_limit.last_attempt > cutoff || rate_limit.is_blocked()
        });
    }

    /// Get authentication statistics
    pub fn get_stats(&self) -> AuthStats {
        let session_tracker = self.session_tracker.lock().unwrap();
        let ip_rate_limits = self.ip_rate_limits.lock().unwrap();
        let user_rate_limits = self.user_rate_limits.lock().unwrap();
        
        AuthStats {
            active_sessions: session_tracker.active_session_count(),
            rate_limited_ips: ip_rate_limits.len(),
            rate_limited_users: user_rate_limits.len(),
        }
    }

    /// Reload user configuration
    pub fn reload_users(&self, config: &Config) {
        let mut user_store = self.user_store.lock().unwrap();
        user_store.load_from_config(&config.auth.users);
        info!("Reloaded {} users from configuration", config.auth.users.len());
    }
}

/// Authentication statistics
#[derive(Debug, Clone)]
pub struct AuthStats {
    pub active_sessions: usize,
    pub rate_limited_ips: usize,
    pub rate_limited_users: usize,
}