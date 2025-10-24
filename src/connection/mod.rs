//! Connection Management Module
//! 
//! Handles TCP connection acceptance, management, and lifecycle.

pub mod manager;

pub use manager::{ConnectionManager, ConnectionInfo, ConnectionStats};