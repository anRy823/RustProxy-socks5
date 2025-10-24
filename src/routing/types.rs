//! Routing Types

use std::net::{IpAddr, SocketAddr};
use crate::protocol::TargetAddr;

/// Routing decision for a connection request
#[derive(Debug, Clone)]
pub enum RouteDecision {
    Allow { upstream: Option<UpstreamProxy> },
    Block { reason: String },
    Redirect { target: SocketAddr },
}

/// Upstream proxy configuration
#[derive(Debug, Clone)]
pub struct UpstreamProxy {
    pub addr: SocketAddr,
    pub auth: Option<ProxyAuth>,
    pub protocol: ProxyProtocol,
}

/// Proxy authentication
#[derive(Debug, Clone)]
pub struct ProxyAuth {
    pub username: String,
    pub password: String,
}

/// Proxy protocol type
#[derive(Debug, Clone, PartialEq)]
pub enum ProxyProtocol {
    Socks5,
    Http,
}

/// Access control policy
#[derive(Debug, Clone, PartialEq)]
pub enum Policy {
    Allow,
    Block,
}

impl From<&str> for Policy {
    fn from(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "allow" => Policy::Allow,
            "block" => Policy::Block,
            _ => Policy::Allow, // Default to allow for unknown values
        }
    }
}

/// Access control action
#[derive(Debug, Clone)]
pub enum Action {
    Allow,
    Block,
    Redirect(SocketAddr),
}

impl From<&str> for Action {
    fn from(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "allow" => Action::Allow,
            "block" => Action::Block,
            _ => Action::Allow, // Default to allow for unknown values
        }
    }
}

/// Access control rule for pattern matching
#[derive(Debug, Clone)]
pub struct AccessControlRule {
    pub pattern: String,
    pub action: Action,
    pub ports: Option<Vec<u16>>,
    pub countries: Option<Vec<String>>,
}

/// Access control list for managing rules and policies
#[derive(Debug)]
pub struct AccessControlList {
    pub default_policy: Policy,
    pub rules: Vec<AccessControlRule>,
}

impl AccessControlList {
    /// Create a new ACL with default policy
    pub fn new(default_policy: Policy) -> Self {
        Self {
            default_policy,
            rules: Vec::new(),
        }
    }

    /// Add a rule to the ACL
    pub fn add_rule(&mut self, rule: AccessControlRule) {
        self.rules.push(rule);
    }

    /// Evaluate access for a target address, port, and source IP
    pub fn evaluate_access(&self, target: &TargetAddr, port: u16, source_ip: IpAddr) -> (bool, String) {
        // Check each rule in order
        for rule in &self.rules {
            if self.matches_rule(rule, target, port, source_ip) {
                match &rule.action {
                    Action::Allow => return (true, "Allowed by rule".to_string()),
                    Action::Block => return (false, format!("Blocked by rule: {}", rule.pattern)),
                    Action::Redirect(_) => return (true, "Redirected by rule".to_string()),
                }
            }
        }

        // No rule matched, use default policy
        match self.default_policy {
            Policy::Allow => (true, "Allowed by default policy".to_string()),
            Policy::Block => (false, "Blocked by default policy".to_string()),
        }
    }

    /// Check if a rule matches the given parameters
    pub fn matches_rule(&self, rule: &AccessControlRule, target: &TargetAddr, port: u16, source_ip: IpAddr) -> bool {
        // Check port restriction
        if let Some(allowed_ports) = &rule.ports {
            if !allowed_ports.contains(&port) {
                return false;
            }
        }

        // Check pattern matching
        self.matches_pattern(&rule.pattern, target, source_ip)
    }

    /// Check if a pattern matches the target address or source IP
    fn matches_pattern(&self, pattern: &str, target: &TargetAddr, source_ip: IpAddr) -> bool {
        // Handle wildcard
        if pattern == "*" {
            return true;
        }

        // Check if pattern matches source IP
        if self.matches_ip_pattern(pattern, source_ip) {
            return true;
        }

        // Check if pattern matches target
        match target {
            TargetAddr::Ipv4(ip) => self.matches_ip_pattern(pattern, IpAddr::V4(*ip)),
            TargetAddr::Ipv6(ip) => self.matches_ip_pattern(pattern, IpAddr::V6(*ip)),
            TargetAddr::Domain(domain) => self.matches_domain_pattern(pattern, domain),
        }
    }

    /// Check if an IP pattern matches an IP address
    fn matches_ip_pattern(&self, pattern: &str, ip: IpAddr) -> bool {
        // Exact match
        if let Ok(pattern_ip) = pattern.parse::<IpAddr>() {
            return pattern_ip == ip;
        }

        // CIDR notation
        if let Some((network, prefix_len)) = pattern.split_once('/') {
            if let (Ok(network_ip), Ok(prefix)) = (network.parse::<IpAddr>(), prefix_len.parse::<u8>()) {
                return self.ip_in_network(ip, network_ip, prefix);
            }
        }

        false
    }

    /// Check if an IP is in a network with given prefix length
    fn ip_in_network(&self, ip: IpAddr, network: IpAddr, prefix_len: u8) -> bool {
        match (ip, network) {
            (IpAddr::V4(ip), IpAddr::V4(net)) => {
                let ip_bits = u32::from(ip);
                let net_bits = u32::from(net);
                let mask = !((1u32 << (32 - prefix_len)) - 1);
                (ip_bits & mask) == (net_bits & mask)
            }
            (IpAddr::V6(ip), IpAddr::V6(net)) => {
                let ip_bits = u128::from(ip);
                let net_bits = u128::from(net);
                let mask = !((1u128 << (128 - prefix_len)) - 1);
                (ip_bits & mask) == (net_bits & mask)
            }
            _ => false, // Mismatched IP versions
        }
    }

    /// Check if a domain pattern matches a domain name
    fn matches_domain_pattern(&self, pattern: &str, domain: &str) -> bool {
        // Exact match
        if pattern == domain {
            return true;
        }

        // Wildcard subdomain matching (e.g., "*.example.com")
        if pattern.starts_with("*.") {
            let pattern_domain = &pattern[2..];
            return domain.ends_with(pattern_domain) && 
                   (domain.len() == pattern_domain.len() || 
                    domain.chars().nth(domain.len() - pattern_domain.len() - 1) == Some('.'));
        }

        // Suffix matching (e.g., ".example.com")
        if pattern.starts_with('.') {
            return domain.ends_with(pattern);
        }

        false
    }
}