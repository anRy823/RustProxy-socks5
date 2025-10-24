//! Authentication Module
//! 
//! Handles user authentication and session management.

pub mod manager;
pub mod types;

pub use manager::{AuthManager, AuthStats};
pub use types::{AuthResult, UserSession, User, UserStore, SessionTracker, RateLimitInfo};