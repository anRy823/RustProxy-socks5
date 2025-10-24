//! Routing Module
//! 
//! Handles connection routing and access control.

pub mod acl;
pub mod chain;
pub mod geoip;
pub mod router;
pub mod rules;
pub mod smart;
pub mod types;

pub use acl::AclManager;
pub use chain::{ProxyChain, ProxyChainConnector, ProxyChainBuilder};
pub use geoip::{GeoIpReader, GeoIpFilter};
pub use router::{Router, RoutingStats};
pub use rules::{RoutingRulesEngine, RoutingRule, RoutingAction, Priority};
pub use smart::{SmartRoutingManager, SmartRoutingConfig, HealthStatus, HealthSummary, ProxyMetrics};
pub use types::*;