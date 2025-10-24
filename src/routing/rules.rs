//! Custom Routing Rules Engine
//! 
//! Provides advanced rule-based routing with pattern matching, priority ordering,
//! and support for domain-based blocking, allowing, and redirection.

use std::net::{IpAddr, SocketAddr};
use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

use crate::protocol::TargetAddr;
use super::{RouteDecision, UpstreamProxy};

/// Priority level for routing rules (higher number = higher priority)
pub type Priority = u32;

/// Custom routing rule with pattern matching and actions
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RoutingRule {
    /// Unique identifier for the rule
    pub id: String,
    /// Rule priority (higher number = higher priority)
    pub priority: Priority,
    /// Pattern to match against (domain, IP, or wildcard)
    pub pattern: String,
    /// Action to take when rule matches
    pub action: RoutingAction,
    /// Optional port restrictions
    pub ports: Option<Vec<u16>>,
    /// Optional source IP restrictions
    pub source_ips: Option<Vec<String>>,
    /// Optional user restrictions
    pub users: Option<Vec<String>>,
    /// Optional time-based restrictions (future enhancement)
    pub time_restrictions: Option<TimeRestriction>,
    /// Whether the rule is enabled
    pub enabled: bool,
}

/// Actions that can be taken when a routing rule matches
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type", content = "config")]
pub enum RoutingAction {
    /// Allow direct connection
    Allow,
    /// Block the connection
    Block { reason: Option<String> },
    /// Redirect to a different target
    Redirect { target: SocketAddr },
    /// Route through a specific upstream proxy
    Proxy { upstream_id: String },
    /// Route through multiple proxies in sequence (proxy chaining)
    ProxyChain { upstream_ids: Vec<String> },
}

/// Time-based restrictions for rules (future enhancement)
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TimeRestriction {
    /// Days of week (0=Sunday, 6=Saturday)
    pub days: Option<Vec<u8>>,
    /// Start time (HH:MM format)
    pub start_time: Option<String>,
    /// End time (HH:MM format)
    pub end_time: Option<String>,
}

/// Pattern matching types
#[derive(Debug, Clone)]
pub enum PatternType {
    /// Exact match
    Exact(String),
    /// Wildcard match (supports * and ?)
    Wildcard(String),
    /// Regular expression match
    Regex(regex::Regex),
    /// IP/CIDR match
    IpCidr(ipnet::IpNet),
    /// Domain suffix match (.example.com)
    DomainSuffix(String),
    /// Subdomain wildcard (*.example.com)
    SubdomainWildcard(String),
}

/// Custom routing rules engine
pub struct RoutingRulesEngine {
    /// Ordered list of rules (sorted by priority)
    rules: Vec<RoutingRule>,
    /// Compiled patterns for efficient matching
    compiled_patterns: HashMap<String, PatternType>,
    /// Upstream proxy configurations
    upstream_proxies: HashMap<String, UpstreamProxy>,
}

impl RoutingRulesEngine {
    /// Create a new routing rules engine
    pub fn new() -> Self {
        Self {
            rules: Vec::new(),
            compiled_patterns: HashMap::new(),
            upstream_proxies: HashMap::new(),
        }
    }

    /// Add a routing rule
    pub fn add_rule(&mut self, rule: RoutingRule) -> Result<(), String> {
        // Validate the rule
        self.validate_rule(&rule)?;
        
        // Compile the pattern
        let pattern = self.compile_pattern(&rule.pattern)?;
        self.compiled_patterns.insert(rule.id.clone(), pattern);
        
        // Add the rule and maintain priority order
        self.rules.push(rule);
        self.sort_rules_by_priority();
        
        debug!("Added routing rule: {}", self.rules.last().unwrap().id);
        Ok(())
    }

    /// Remove a routing rule by ID
    pub fn remove_rule(&mut self, rule_id: &str) -> bool {
        if let Some(pos) = self.rules.iter().position(|r| r.id == rule_id) {
            self.rules.remove(pos);
            self.compiled_patterns.remove(rule_id);
            debug!("Removed routing rule: {}", rule_id);
            true
        } else {
            false
        }
    }

    /// Update a routing rule
    pub fn update_rule(&mut self, rule: RoutingRule) -> Result<(), String> {
        // Remove existing rule if it exists
        self.remove_rule(&rule.id);
        
        // Add the updated rule
        self.add_rule(rule)
    }

    /// Add an upstream proxy configuration
    pub fn add_upstream_proxy(&mut self, id: String, proxy: UpstreamProxy) {
        self.upstream_proxies.insert(id.clone(), proxy);
        debug!("Added upstream proxy: {}", id);
    }

    /// Evaluate routing rules for a connection request
    pub fn evaluate_rules(
        &self,
        target: &TargetAddr,
        port: u16,
        source_ip: IpAddr,
        user: Option<&str>,
    ) -> RouteDecision {
        debug!("Evaluating routing rules for target: {:?}, port: {}, source: {}", 
               target, port, source_ip);

        // Check each rule in priority order
        for rule in &self.rules {
            if !rule.enabled {
                continue;
            }

            if self.matches_rule(rule, target, port, source_ip, user) {
                debug!("Rule '{}' matched, applying action: {:?}", rule.id, rule.action);
                return self.apply_action(&rule.action, target, port);
            }
        }

        // No rules matched, allow direct connection
        debug!("No routing rules matched, allowing direct connection");
        RouteDecision::Allow { upstream: None }
    }

    /// Check if a rule matches the given parameters
    fn matches_rule(
        &self,
        rule: &RoutingRule,
        target: &TargetAddr,
        port: u16,
        source_ip: IpAddr,
        user: Option<&str>,
    ) -> bool {
        // Check port restrictions
        if let Some(allowed_ports) = &rule.ports {
            if !allowed_ports.contains(&port) {
                return false;
            }
        }

        // Check source IP restrictions
        if let Some(source_patterns) = &rule.source_ips {
            if !self.matches_source_ip(source_patterns, source_ip) {
                return false;
            }
        }

        // Check user restrictions
        if let Some(allowed_users) = &rule.users {
            match user {
                Some(u) if allowed_users.contains(&u.to_string()) => {},
                Some(_) => return false, // User not in allowed list
                None if !allowed_users.is_empty() => return false, // No user but rule requires one
                None => {}, // No user and rule doesn't require one
            }
        }

        // Check time restrictions (if implemented)
        if let Some(_time_restriction) = &rule.time_restrictions {
            // TODO: Implement time-based matching
            // For now, assume time restrictions pass
        }

        // Check pattern match
        if let Some(pattern) = self.compiled_patterns.get(&rule.id) {
            self.matches_pattern(pattern, target)
        } else {
            warn!("No compiled pattern found for rule: {}", rule.id);
            false
        }
    }

    /// Check if source IP matches any of the patterns
    fn matches_source_ip(&self, patterns: &[String], source_ip: IpAddr) -> bool {
        for pattern in patterns {
            if self.matches_ip_pattern(pattern, source_ip) {
                return true;
            }
        }
        false
    }

    /// Check if an IP pattern matches an IP address
    fn matches_ip_pattern(&self, pattern: &str, ip: IpAddr) -> bool {
        // Try exact IP match
        if let Ok(pattern_ip) = pattern.parse::<IpAddr>() {
            return pattern_ip == ip;
        }

        // Try CIDR match
        if let Ok(cidr) = pattern.parse::<ipnet::IpNet>() {
            return cidr.contains(&ip);
        }

        false
    }

    /// Check if a compiled pattern matches the target
    fn matches_pattern(&self, pattern: &PatternType, target: &TargetAddr) -> bool {
        let target_str = match target {
            TargetAddr::Ipv4(ip) => ip.to_string(),
            TargetAddr::Ipv6(ip) => ip.to_string(),
            TargetAddr::Domain(domain) => domain.clone(),
        };

        match pattern {
            PatternType::Exact(exact) => target_str == *exact,
            PatternType::Wildcard(wildcard) => self.matches_wildcard(wildcard, &target_str),
            PatternType::Regex(regex) => regex.is_match(&target_str),
            PatternType::IpCidr(cidr) => {
                match target {
                    TargetAddr::Ipv4(ip) => cidr.contains(&IpAddr::V4(*ip)),
                    TargetAddr::Ipv6(ip) => cidr.contains(&IpAddr::V6(*ip)),
                    TargetAddr::Domain(_) => false,
                }
            },
            PatternType::DomainSuffix(suffix) => {
                match target {
                    TargetAddr::Domain(domain) => domain.ends_with(suffix),
                    _ => false,
                }
            },
            PatternType::SubdomainWildcard(base_domain) => {
                match target {
                    TargetAddr::Domain(domain) => {
                        domain == base_domain || 
                        (domain.ends_with(base_domain) && 
                         domain.chars().nth(domain.len() - base_domain.len() - 1) == Some('.'))
                    },
                    _ => false,
                }
            },
        }
    }

    /// Match wildcard patterns (* and ?)
    fn matches_wildcard(&self, pattern: &str, text: &str) -> bool {
        // Convert wildcard pattern to regex
        let regex_pattern = pattern
            .replace(".", r"\.")
            .replace("*", ".*")
            .replace("?", ".");
        
        if let Ok(regex) = regex::Regex::new(&format!("^{}$", regex_pattern)) {
            regex.is_match(text)
        } else {
            false
        }
    }

    /// Apply the action specified by a matching rule
    fn apply_action(&self, action: &RoutingAction, _target: &TargetAddr, _port: u16) -> RouteDecision {
        match action {
            RoutingAction::Allow => RouteDecision::Allow { upstream: None },
            RoutingAction::Block { reason } => {
                let block_reason = reason.clone().unwrap_or_else(|| "Blocked by routing rule".to_string());
                RouteDecision::Block { reason: block_reason }
            },
            RoutingAction::Redirect { target } => RouteDecision::Redirect { target: *target },
            RoutingAction::Proxy { upstream_id } => {
                if let Some(upstream) = self.upstream_proxies.get(upstream_id) {
                    RouteDecision::Allow { upstream: Some(upstream.clone()) }
                } else {
                    warn!("Upstream proxy '{}' not found, allowing direct connection", upstream_id);
                    RouteDecision::Allow { upstream: None }
                }
            },
            RoutingAction::ProxyChain { upstream_ids } => {
                // Create a proxy chain from the upstream IDs
                if upstream_ids.is_empty() {
                    RouteDecision::Allow { upstream: None }
                } else {
                    // For now, we'll use the first proxy in the chain as the upstream
                    // Full proxy chaining will be handled by the relay engine
                    if let Some(first_id) = upstream_ids.first() {
                        if let Some(upstream) = self.upstream_proxies.get(first_id) {
                            // TODO: Store the full chain information for the relay engine
                            RouteDecision::Allow { upstream: Some(upstream.clone()) }
                        } else {
                            warn!("First upstream proxy '{}' in chain not found", first_id);
                            RouteDecision::Allow { upstream: None }
                        }
                    } else {
                        RouteDecision::Allow { upstream: None }
                    }
                }
            },
        }
    }

    /// Compile a pattern string into a PatternType for efficient matching
    fn compile_pattern(&self, pattern: &str) -> Result<PatternType, String> {
        // Check for different pattern types
        
        // IP/CIDR pattern
        if let Ok(cidr) = pattern.parse::<ipnet::IpNet>() {
            return Ok(PatternType::IpCidr(cidr));
        }

        // IP address pattern
        if pattern.parse::<IpAddr>().is_ok() {
            return Ok(PatternType::Exact(pattern.to_string()));
        }

        // Domain suffix pattern (.example.com)
        if pattern.starts_with('.') {
            return Ok(PatternType::DomainSuffix(pattern.to_string()));
        }

        // Subdomain wildcard pattern (*.example.com)
        if pattern.starts_with("*.") {
            let base_domain = &pattern[2..];
            return Ok(PatternType::SubdomainWildcard(base_domain.to_string()));
        }

        // Regex pattern (starts with ^)
        if pattern.starts_with('^') {
            match regex::Regex::new(pattern) {
                Ok(regex) => return Ok(PatternType::Regex(regex)),
                Err(e) => return Err(format!("Invalid regex pattern '{}': {}", pattern, e)),
            }
        }

        // Wildcard pattern (contains * or ?)
        if pattern.contains('*') || pattern.contains('?') {
            return Ok(PatternType::Wildcard(pattern.to_string()));
        }

        // Default to exact match
        Ok(PatternType::Exact(pattern.to_string()))
    }

    /// Validate a routing rule
    fn validate_rule(&self, rule: &RoutingRule) -> Result<(), String> {
        // Check for duplicate rule ID
        if self.rules.iter().any(|r| r.id == rule.id) {
            return Err(format!("Rule with ID '{}' already exists", rule.id));
        }

        // Validate pattern
        self.compile_pattern(&rule.pattern)?;

        // Validate action-specific requirements
        match &rule.action {
            RoutingAction::Proxy { upstream_id } => {
                if upstream_id.is_empty() {
                    return Err("Proxy action requires non-empty upstream_id".to_string());
                }
            },
            RoutingAction::ProxyChain { upstream_ids } => {
                if upstream_ids.is_empty() {
                    return Err("ProxyChain action requires at least one upstream_id".to_string());
                }
            },
            _ => {}, // Other actions don't need validation
        }

        Ok(())
    }

    /// Sort rules by priority (highest first)
    fn sort_rules_by_priority(&mut self) {
        self.rules.sort_by(|a, b| b.priority.cmp(&a.priority));
    }

    /// Get all rules (for management/debugging)
    pub fn get_rules(&self) -> &[RoutingRule] {
        &self.rules
    }

    /// Get rule count
    pub fn rule_count(&self) -> usize {
        self.rules.len()
    }

    /// Get enabled rule count
    pub fn enabled_rule_count(&self) -> usize {
        self.rules.iter().filter(|r| r.enabled).count()
    }

    /// Clear all rules
    pub fn clear_rules(&mut self) {
        self.rules.clear();
        self.compiled_patterns.clear();
        debug!("Cleared all routing rules");
    }
}

impl Default for RoutingRulesEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    #[test]
    fn test_exact_pattern_matching() {
        let mut engine = RoutingRulesEngine::new();
        
        let rule = RoutingRule {
            id: "test1".to_string(),
            priority: 100,
            pattern: "example.com".to_string(),
            action: RoutingAction::Block { reason: None },
            ports: None,
            source_ips: None,
            users: None,
            time_restrictions: None,
            enabled: true,
        };
        
        engine.add_rule(rule).unwrap();
        
        let target = TargetAddr::Domain("example.com".to_string());
        let decision = engine.evaluate_rules(&target, 80, IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), None);
        
        match decision {
            RouteDecision::Block { .. } => {},
            _ => panic!("Expected block decision"),
        }
    }

    #[test]
    fn test_wildcard_pattern_matching() {
        let mut engine = RoutingRulesEngine::new();
        
        let rule = RoutingRule {
            id: "test2".to_string(),
            priority: 100,
            pattern: "*.example.com".to_string(),
            action: RoutingAction::Allow,
            ports: None,
            source_ips: None,
            users: None,
            time_restrictions: None,
            enabled: true,
        };
        
        engine.add_rule(rule).unwrap();
        
        let target = TargetAddr::Domain("sub.example.com".to_string());
        let decision = engine.evaluate_rules(&target, 80, IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), None);
        
        match decision {
            RouteDecision::Allow { .. } => {},
            _ => panic!("Expected allow decision"),
        }
    }

    #[test]
    fn test_priority_ordering() {
        let mut engine = RoutingRulesEngine::new();
        
        // Add lower priority rule first
        let rule1 = RoutingRule {
            id: "low_priority".to_string(),
            priority: 50,
            pattern: "*.com".to_string(),
            action: RoutingAction::Allow,
            ports: None,
            source_ips: None,
            users: None,
            time_restrictions: None,
            enabled: true,
        };
        
        // Add higher priority rule
        let rule2 = RoutingRule {
            id: "high_priority".to_string(),
            priority: 100,
            pattern: "blocked.com".to_string(),
            action: RoutingAction::Block { reason: Some("High priority block".to_string()) },
            ports: None,
            source_ips: None,
            users: None,
            time_restrictions: None,
            enabled: true,
        };
        
        engine.add_rule(rule1).unwrap();
        engine.add_rule(rule2).unwrap();
        
        let target = TargetAddr::Domain("blocked.com".to_string());
        let decision = engine.evaluate_rules(&target, 80, IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), None);
        
        match decision {
            RouteDecision::Block { reason } => {
                assert_eq!(reason, "High priority block");
            },
            _ => panic!("Expected block decision from high priority rule"),
        }
    }
}