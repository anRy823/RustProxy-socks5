//! Connection Router

use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use tokio::net::lookup_host;
use tracing::{debug, warn, error};

use crate::config::{Config, UpstreamProxyConfig, RoutingRuleConfig, RoutingActionConfig};
use crate::Result;
use crate::protocol::TargetAddr;
use super::{RouteDecision, UpstreamProxy, ProxyAuth, ProxyProtocol, AclManager, GeoIpReader, GeoIpFilter, RoutingRulesEngine, RoutingRule, RoutingAction, SmartRoutingManager, SmartRoutingConfig};



/// Handles routing decisions and access control
pub struct Router {
    config: Arc<Config>,
    acl_manager: Option<AclManager>,
    rules_engine: RoutingRulesEngine,
    smart_routing: Option<SmartRoutingManager>,
}

impl Router {
    /// Create a new router with configuration
    pub fn new(config: Arc<Config>) -> Self {
        let acl_manager = if config.access_control.enabled {
            Some(AclManager::new(&config.access_control))
        } else {
            None
        };

        let mut rules_engine = RoutingRulesEngine::new();
        
        // Load routing rules from configuration
        for rule_config in &config.routing.rules {
            if let Ok(rule) = Self::config_to_routing_rule(rule_config) {
                if let Err(e) = rules_engine.add_rule(rule) {
                    warn!("Failed to add routing rule '{}': {}", rule_config.id, e);
                }
            }
        }
        
        // Load upstream proxies
        for upstream_config in &config.routing.upstream_proxies {
            let upstream = Self::config_to_upstream_proxy(upstream_config);
            rules_engine.add_upstream_proxy(upstream_config.name.clone(), upstream);
        }

        Self {
            config,
            acl_manager,
            rules_engine,
            smart_routing: None,
        }
    }

    /// Create a new router with GeoIP support
    pub fn with_geoip<P: AsRef<std::path::Path>>(
        config: Arc<Config>, 
        geoip_db_path: P
    ) -> std::result::Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let acl_manager = if config.access_control.enabled {
            // Try to load GeoIP database
            match GeoIpReader::new(geoip_db_path) {
                Ok(reader) => {
                    let geoip_filter = GeoIpFilter::new(reader);
                    Some(AclManager::with_geoip(&config.access_control, geoip_filter))
                }
                Err(e) => {
                    warn!("Failed to load GeoIP database, using ACL without GeoIP: {}", e);
                    Some(AclManager::new(&config.access_control))
                }
            }
        } else {
            None
        };

        let mut rules_engine = RoutingRulesEngine::new();
        
        // Load routing rules from configuration
        for rule_config in &config.routing.rules {
            if let Ok(rule) = Self::config_to_routing_rule(rule_config) {
                if let Err(e) = rules_engine.add_rule(rule) {
                    warn!("Failed to add routing rule '{}': {}", rule_config.id, e);
                }
            }
        }
        
        // Load upstream proxies
        for upstream_config in &config.routing.upstream_proxies {
            let upstream = Self::config_to_upstream_proxy(upstream_config);
            rules_engine.add_upstream_proxy(upstream_config.name.clone(), upstream);
        }

        Ok(Self {
            config,
            acl_manager,
            rules_engine,
            smart_routing: None,
        })
    }

    /// Make a routing decision for the given request
    pub async fn route_request(
        &self,
        target: &TargetAddr,
        port: u16,
        source_ip: IpAddr,
        user: Option<&str>,
    ) -> RouteDecision {
        debug!("Making routing decision for target: {:?}, port: {}, source: {}", target, port, source_ip);

        // Step 1: Check access control
        if let Some(acl) = &self.acl_manager {
            let (allowed, reason) = acl.check_access(target, port, source_ip);
            if !allowed {
                warn!("Access denied for {}:{} from {}: {}", 
                      self.target_to_string(target), port, source_ip, reason);
                return RouteDecision::Block { reason };
            }
            debug!("Access allowed for {}:{} from {}: {}", 
                   self.target_to_string(target), port, source_ip, reason);
        }

        // Step 2: Apply custom routing rules (if routing is enabled)
        if self.config.routing.enabled {
            let rules_decision = self.rules_engine.evaluate_rules(target, port, source_ip, user);
            
            // If rules engine made a decision other than default allow, use it
            match &rules_decision {
                RouteDecision::Allow { upstream: None } => {
                    // No specific rule matched, fall back to legacy upstream selection
                    let upstream = self.select_upstream_proxy(target, port).await;
                    RouteDecision::Allow { upstream }
                },
                _ => {
                    // Rules engine made a specific decision (block, redirect, or proxy)
                    debug!("Custom routing rule applied: {:?}", rules_decision);
                    rules_decision
                }
            }
        } else {
            // Routing disabled, allow direct connection
            debug!("Routing disabled, allowing direct connection");
            RouteDecision::Allow { upstream: None }
        }
    }

    /// Check if access is allowed for the given target
    pub fn check_access(&self, target: &TargetAddr, port: u16, source_ip: IpAddr) -> bool {
        if let Some(acl) = &self.acl_manager {
            let (allowed, reason) = acl.check_access(target, port, source_ip);
            debug!("Access check for {}:{} from {}: {} ({})", 
                   self.target_to_string(target), port, source_ip, allowed, reason);
            allowed
        } else {
            // No ACL configured, allow all
            true
        }
    }

    /// Resolve target address to socket addresses
    pub async fn resolve_target(&self, addr: &TargetAddr) -> Result<Vec<SocketAddr>> {
        match addr {
            TargetAddr::Ipv4(ip) => {
                // IP address, no resolution needed
                Ok(vec![SocketAddr::new(IpAddr::V4(*ip), 0)])
            }
            TargetAddr::Ipv6(ip) => {
                // IP address, no resolution needed
                Ok(vec![SocketAddr::new(IpAddr::V6(*ip), 0)])
            }
            TargetAddr::Domain(domain) => {
                // Domain name, perform DNS resolution
                self.resolve_domain(domain).await
            }
        }
    }

    /// Resolve a domain name to IP addresses
    async fn resolve_domain(&self, domain: &str) -> Result<Vec<SocketAddr>> {
        debug!("Resolving domain: {}", domain);
        
        // Use a dummy port for resolution, caller should replace with actual port
        let host_port = format!("{}:80", domain);
        
        match lookup_host(host_port).await {
            Ok(addrs) => {
                let resolved: Vec<SocketAddr> = addrs.collect();
                debug!("Resolved {} to {} addresses", domain, resolved.len());
                Ok(resolved)
            }
            Err(e) => {
                error!("Failed to resolve domain {}: {}", domain, e);
                Err(anyhow::anyhow!("DNS resolution failed for {}: {}", domain, e))
            }
        }
    }

    /// Select an upstream proxy for the given target (if any)
    async fn select_upstream_proxy(&self, _target: &TargetAddr, _port: u16) -> Option<UpstreamProxy> {
        // Use smart routing if available
        if let Some(smart_routing) = &self.smart_routing {
            if let Some((proxy_id, proxy)) = smart_routing.select_best_proxy(&[]).await {
                debug!("Smart routing selected upstream proxy: {}", proxy_id);
                return Some(proxy);
            }
        }
        
        // Fallback to simple selection
        if let Some(upstream_config) = self.config.routing.upstream_proxies.first() {
            debug!("Selected upstream proxy (fallback): {}", upstream_config.name);
            Some(Self::config_to_upstream_proxy(upstream_config))
        } else {
            debug!("No upstream proxies configured");
            None
        }
    }

    /// Convert routing rule configuration to RoutingRule
    fn config_to_routing_rule(config: &RoutingRuleConfig) -> std::result::Result<RoutingRule, String> {
        let action = Self::config_to_routing_action(&config.action)?;
        
        Ok(RoutingRule {
            id: config.id.clone(),
            priority: config.priority,
            pattern: config.pattern.clone(),
            action,
            ports: config.ports.clone(),
            source_ips: config.source_ips.clone(),
            users: config.users.clone(),
            time_restrictions: None, // Not implemented yet
            enabled: config.enabled,
        })
    }

    /// Convert routing action configuration to RoutingAction
    fn config_to_routing_action(config: &RoutingActionConfig) -> std::result::Result<RoutingAction, String> {
        match config {
            RoutingActionConfig::Allow => Ok(RoutingAction::Allow),
            RoutingActionConfig::Block { reason } => Ok(RoutingAction::Block { 
                reason: reason.clone() 
            }),
            RoutingActionConfig::Redirect { target } => Ok(RoutingAction::Redirect { 
                target: *target 
            }),
            RoutingActionConfig::Proxy { upstream_id } => Ok(RoutingAction::Proxy { 
                upstream_id: upstream_id.clone() 
            }),
            RoutingActionConfig::ProxyChain { upstream_ids } => Ok(RoutingAction::ProxyChain { 
                upstream_ids: upstream_ids.clone() 
            }),
        }
    }

    /// Convert upstream proxy configuration to UpstreamProxy
    fn config_to_upstream_proxy(config: &UpstreamProxyConfig) -> UpstreamProxy {
        let auth = config.auth.as_ref().map(|auth_config| ProxyAuth {
            username: auth_config.username.clone(),
            password: auth_config.password.clone(),
        });

        let protocol = match config.protocol.to_lowercase().as_str() {
            "socks5" => ProxyProtocol::Socks5,
            "http" => ProxyProtocol::Http,
            _ => {
                warn!("Unknown proxy protocol '{}', defaulting to SOCKS5", config.protocol);
                ProxyProtocol::Socks5
            }
        };

        UpstreamProxy {
            addr: config.addr,
            auth,
            protocol,
        }
    }



    /// Convert TargetAddr to string for logging
    fn target_to_string(&self, target: &TargetAddr) -> String {
        match target {
            TargetAddr::Ipv4(ip) => ip.to_string(),
            TargetAddr::Ipv6(ip) => ip.to_string(),
            TargetAddr::Domain(domain) => domain.clone(),
        }
    }

    /// Get ACL statistics
    pub fn get_acl_stats(&self) -> Option<AclStats> {
        self.acl_manager.as_ref().map(|acl| AclStats {
            enabled: true,
            default_policy: format!("{:?}", acl.get_default_policy()),
            rule_count: acl.get_rule_count(),
            geoip_enabled: acl.has_geoip(),
        })
    }

    /// Check if routing is enabled
    pub fn is_routing_enabled(&self) -> bool {
        self.config.routing.enabled
    }

    /// Get the number of configured upstream proxies
    pub fn get_upstream_proxy_count(&self) -> usize {
        self.config.routing.upstream_proxies.len()
    }

    /// Add a routing rule at runtime
    pub fn add_routing_rule(&mut self, rule: RoutingRule) -> std::result::Result<(), String> {
        self.rules_engine.add_rule(rule)
    }

    /// Remove a routing rule by ID
    pub fn remove_routing_rule(&mut self, rule_id: &str) -> bool {
        self.rules_engine.remove_rule(rule_id)
    }

    /// Update a routing rule
    pub fn update_routing_rule(&mut self, rule: RoutingRule) -> std::result::Result<(), String> {
        self.rules_engine.update_rule(rule)
    }

    /// Get all routing rules
    pub fn get_routing_rules(&self) -> &[RoutingRule] {
        self.rules_engine.get_rules()
    }

    /// Get routing rules statistics
    pub async fn get_routing_stats(&self) -> RoutingStats {
        let health_summary = if let Some(smart_routing) = &self.smart_routing {
            Some(smart_routing.get_health_summary().await)
        } else {
            None
        };

        RoutingStats {
            enabled: self.config.routing.enabled,
            total_rules: self.rules_engine.rule_count(),
            enabled_rules: self.rules_engine.enabled_rule_count(),
            upstream_proxies: self.get_upstream_proxy_count(),
            smart_routing_enabled: self.smart_routing.is_some(),
            health_summary,
        }
    }

    /// Add an upstream proxy at runtime
    pub async fn add_upstream_proxy(&mut self, id: String, proxy: UpstreamProxy) {
        self.rules_engine.add_upstream_proxy(id.clone(), proxy.clone());
        
        // Also add to smart routing if enabled
        if let Some(smart_routing) = &mut self.smart_routing {
            smart_routing.add_upstream_proxy(id, proxy).await;
        }
    }

    /// Enable smart routing with the given configuration
    pub async fn enable_smart_routing(&mut self, config: SmartRoutingConfig) {
        let mut smart_routing = SmartRoutingManager::new(config);
        
        // Add existing upstream proxies to smart routing
        for upstream_config in &self.config.routing.upstream_proxies {
            let upstream = Self::config_to_upstream_proxy(upstream_config);
            smart_routing.add_upstream_proxy(upstream_config.name.clone(), upstream).await;
        }
        
        self.smart_routing = Some(smart_routing);
    }

    /// Start smart routing health checks (if enabled)
    pub async fn start_smart_routing_health_checks(&self) {
        if let Some(smart_routing) = &self.smart_routing {
            smart_routing.start_health_checking().await;
        }
    }

    /// Record a connection result for smart routing
    pub async fn record_connection_result(&self, proxy_id: &str, latency: std::time::Duration, success: bool) {
        if let Some(smart_routing) = &self.smart_routing {
            smart_routing.record_connection_result(proxy_id, latency, success).await;
        }
    }

    /// Get smart routing health summary
    pub async fn get_smart_routing_health(&self) -> Option<super::HealthSummary> {
        if let Some(smart_routing) = &self.smart_routing {
            Some(smart_routing.get_health_summary().await)
        } else {
            None
        }
    }

    /// Force health check for all proxies
    pub async fn force_health_check(&self) {
        if let Some(smart_routing) = &self.smart_routing {
            smart_routing.force_health_check().await;
        }
    }

    /// Check if smart routing is enabled
    pub fn is_smart_routing_enabled(&self) -> bool {
        self.smart_routing.is_some()
    }
}

/// Routing statistics for monitoring
#[derive(Debug, Clone)]
pub struct RoutingStats {
    pub enabled: bool,
    pub total_rules: usize,
    pub enabled_rules: usize,
    pub upstream_proxies: usize,
    pub smart_routing_enabled: bool,
    pub health_summary: Option<super::HealthSummary>,
}

/// ACL statistics for monitoring
#[derive(Debug, Clone)]
pub struct AclStats {
    pub enabled: bool,
    pub default_policy: String,
    pub rule_count: usize,
    pub geoip_enabled: bool,
}