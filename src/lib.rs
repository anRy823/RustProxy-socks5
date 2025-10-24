//! RustProxy Library
//! 
//! Professional SOCKS5 Proxy Server Library
//! Created by [Your Name] - Professional Network Solutions
//! 
//! A high-performance, enterprise-grade SOCKS5 proxy server built with Rust
//! for maximum security, reliability, and performance.

pub mod auth;
pub mod config;
pub mod connection;
pub mod management;
pub mod metrics;
pub mod protocol;
pub mod relay;
pub mod resource;
pub mod routing;
pub mod security;
pub mod shutdown;

pub use config::Config;
pub use connection::ConnectionManager;
pub use resource::ResourceManager;
pub use shutdown::ShutdownCoordinator;

/// Common error type for the proxy server
pub type Result<T> = anyhow::Result<T>;