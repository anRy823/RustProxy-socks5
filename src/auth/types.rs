//! Authentication Types

use std::collections::HashMap;
use std::net::IpAddr;
use std::time::{Duration, Instant};
use uuid::Uuid;

/// Authentication result
#[derive(Debug, Clone)]
pub struct AuthResult {
    pub success: bool,
    pub user_id: Option<String>,
    pub session_id: String,
}

/// User session information
#[derive(Debug, Clone)]
pub struct UserSession {
    pub session_id: String,
    pub user_id: String,
    pub created_at: Instant,
    pub last_activity: Instant,
    pub client_ip: IpAddr,
}

impl UserSession {
    /// Create a new user session
    pub fn new(user_id: String, client_ip: IpAddr) -> Self {
        let now = Instant::now();
        Self {
            session_id: Uuid::new_v4().to_string(),
            user_id,
            created_at: now,
            last_activity: now,
            client_ip,
        }
    }

    /// Update the last activity timestamp
    pub fn update_activity(&mut self) {
        self.last_activity = Instant::now();
    }

    /// Check if the session has expired
    pub fn is_expired(&self, timeout: Duration) -> bool {
        self.last_activity.elapsed() > timeout
    }
}

/// User information stored in the user store
#[derive(Debug, Clone)]
pub struct User {
    pub username: String,
    pub password_hash: String,
    pub enabled: bool,
    pub created_at: Instant,
}

impl User {
    /// Create a new user with hashed password
    pub fn new(username: String, password: String, enabled: bool) -> Self {
        Self {
            username,
            password_hash: Self::hash_password(&password),
            enabled,
            created_at: Instant::now(),
        }
    }

    /// Hash a password (simple implementation for now)
    fn hash_password(password: &str) -> String {
        // TODO: Use proper password hashing like bcrypt
        // For now, using a simple hash for demonstration
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        
        let mut hasher = DefaultHasher::new();
        password.hash(&mut hasher);
        format!("{:x}", hasher.finish())
    }

    /// Verify a password against the stored hash
    pub fn verify_password(&self, password: &str) -> bool {
        self.password_hash == Self::hash_password(password)
    }
}

/// Rate limiting information for authentication attempts
#[derive(Debug, Clone)]
pub struct RateLimitInfo {
    pub attempts: u32,
    pub last_attempt: Instant,
    pub blocked_until: Option<Instant>,
}

impl RateLimitInfo {
    /// Create new rate limit info
    pub fn new() -> Self {
        Self {
            attempts: 0,
            last_attempt: Instant::now(),
            blocked_until: None,
        }
    }

    /// Record a failed authentication attempt
    pub fn record_failure(&mut self) {
        self.attempts += 1;
        self.last_attempt = Instant::now();
        
        // Implement progressive delays
        let delay = match self.attempts {
            1..=3 => Duration::from_secs(1),
            4..=6 => Duration::from_secs(5),
            7..=10 => Duration::from_secs(30),
            _ => Duration::from_secs(300), // 5 minutes for excessive attempts
        };
        
        self.blocked_until = Some(self.last_attempt + delay);
    }

    /// Reset the rate limit info after successful authentication
    pub fn reset(&mut self) {
        self.attempts = 0;
        self.blocked_until = None;
    }

    /// Check if currently blocked
    pub fn is_blocked(&self) -> bool {
        if let Some(blocked_until) = self.blocked_until {
            Instant::now() < blocked_until
        } else {
            false
        }
    }

    /// Get remaining block time
    pub fn remaining_block_time(&self) -> Option<Duration> {
        if let Some(blocked_until) = self.blocked_until {
            let now = Instant::now();
            if now < blocked_until {
                Some(blocked_until - now)
            } else {
                None
            }
        } else {
            None
        }
    }
}

/// User store for managing user credentials
#[derive(Debug)]
pub struct UserStore {
    users: HashMap<String, User>,
}

impl UserStore {
    /// Create a new empty user store
    pub fn new() -> Self {
        Self {
            users: HashMap::new(),
        }
    }

    /// Add a user to the store
    pub fn add_user(&mut self, username: String, password: String, enabled: bool) {
        let user = User::new(username.clone(), password, enabled);
        self.users.insert(username, user);
    }

    /// Get a user by username
    pub fn get_user(&self, username: &str) -> Option<&User> {
        self.users.get(username)
    }

    /// Validate user credentials
    pub fn validate_credentials(&self, username: &str, password: &str) -> bool {
        if let Some(user) = self.get_user(username) {
            user.enabled && user.verify_password(password)
        } else {
            false
        }
    }

    /// Load users from configuration
    pub fn load_from_config(&mut self, users: &[crate::config::UserConfig]) {
        self.users.clear();
        for user_config in users {
            self.add_user(
                user_config.username.clone(),
                user_config.password.clone(),
                user_config.enabled,
            );
        }
    }

    /// Get all usernames
    pub fn get_usernames(&self) -> Vec<String> {
        self.users.keys().cloned().collect()
    }

    /// Check if user exists
    pub fn user_exists(&self, username: &str) -> bool {
        self.users.contains_key(username)
    }
}

/// Session tracker for managing active user sessions
#[derive(Debug)]
pub struct SessionTracker {
    sessions: HashMap<String, UserSession>,
    user_sessions: HashMap<String, Vec<String>>, // user_id -> session_ids
}

impl SessionTracker {
    /// Create a new session tracker
    pub fn new() -> Self {
        Self {
            sessions: HashMap::new(),
            user_sessions: HashMap::new(),
        }
    }

    /// Create a new session for a user
    pub fn create_session(&mut self, user_id: String, client_ip: IpAddr) -> String {
        let session = UserSession::new(user_id.clone(), client_ip);
        let session_id = session.session_id.clone();
        
        // Add to sessions map
        self.sessions.insert(session_id.clone(), session);
        
        // Add to user sessions map
        self.user_sessions
            .entry(user_id)
            .or_insert_with(Vec::new)
            .push(session_id.clone());
        
        session_id
    }

    /// Get a session by ID
    pub fn get_session(&self, session_id: &str) -> Option<&UserSession> {
        self.sessions.get(session_id)
    }

    /// Update session activity
    pub fn update_session_activity(&mut self, session_id: &str) -> bool {
        if let Some(session) = self.sessions.get_mut(session_id) {
            session.update_activity();
            true
        } else {
            false
        }
    }

    /// Remove a session
    pub fn remove_session(&mut self, session_id: &str) -> bool {
        if let Some(session) = self.sessions.remove(session_id) {
            // Remove from user sessions map
            if let Some(user_sessions) = self.user_sessions.get_mut(&session.user_id) {
                user_sessions.retain(|id| id != session_id);
                if user_sessions.is_empty() {
                    self.user_sessions.remove(&session.user_id);
                }
            }
            true
        } else {
            false
        }
    }

    /// Clean up expired sessions
    pub fn cleanup_expired_sessions(&mut self, timeout: Duration) -> usize {
        let mut expired_sessions = Vec::new();
        
        for (session_id, session) in &self.sessions {
            if session.is_expired(timeout) {
                expired_sessions.push(session_id.clone());
            }
        }
        
        let count = expired_sessions.len();
        for session_id in expired_sessions {
            self.remove_session(&session_id);
        }
        
        count
    }

    /// Get active session count
    pub fn active_session_count(&self) -> usize {
        self.sessions.len()
    }

    /// Get sessions for a user
    pub fn get_user_sessions(&self, user_id: &str) -> Vec<&UserSession> {
        if let Some(session_ids) = self.user_sessions.get(user_id) {
            session_ids
                .iter()
                .filter_map(|id| self.sessions.get(id))
                .collect()
        } else {
            Vec::new()
        }
    }
}