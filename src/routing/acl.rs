//! Access Control List Implementation

use std::net::IpAddr;
use crate::config::{AccessControlConfig, AccessRule};
use crate::protocol::TargetAddr;
use super::types::{AccessControlList, AccessControlRule, Action, Policy};
use super::geoip::GeoIpFilter;

/// ACL manager for handling access control configuration
pub struct AclManager {
    acl: AccessControlList,
    geoip_filter: Option<GeoIpFilter>,
}

impl AclManager {
    /// Create a new ACL manager from configuration
    pub fn new(config: &AccessControlConfig) -> Self {
        let default_policy = Policy::from(config.default_policy.as_str());
        let mut acl = AccessControlList::new(default_policy);

        // Convert configuration rules to ACL rules
        for rule in &config.rules {
            let acl_rule = AccessControlRule {
                pattern: rule.pattern.clone(),
                action: Action::from(rule.action.as_str()),
                ports: rule.ports.clone(),
                countries: rule.countries.clone(),
            };
            acl.add_rule(acl_rule);
        }

        Self { 
            acl,
            geoip_filter: None,
        }
    }

    /// Create a new ACL manager with GeoIP support
    pub fn with_geoip(config: &AccessControlConfig, geoip_filter: GeoIpFilter) -> Self {
        let mut manager = Self::new(config);
        manager.geoip_filter = Some(geoip_filter);
        manager
    }

    /// Check if access is allowed for the given parameters
    pub fn check_access(&self, target: &TargetAddr, port: u16, source_ip: IpAddr) -> (bool, String) {
        // First check standard ACL rules
        let (allowed, reason) = self.acl.evaluate_access(target, port, source_ip);
        
        if !allowed {
            return (false, reason);
        }

        // If standard ACL allows, check GeoIP restrictions if available
        if let Some(geoip) = &self.geoip_filter {
            // Check if any rules have country restrictions that apply
            for rule in &self.acl.rules {
                if let Some(countries) = &rule.countries {
                    if self.acl.matches_rule(rule, target, port, source_ip) {
                        // This rule applies and has country restrictions
                        match &rule.action {
                            Action::Allow => {
                                // Allow rule with country allowlist
                                if !geoip.is_country_allowed(source_ip, countries) {
                                    return (false, format!("Country not in allowlist for rule: {}", rule.pattern));
                                }
                            }
                            Action::Block => {
                                // Block rule with country blocklist
                                if geoip.is_country_blocked(source_ip, countries) {
                                    return (false, format!("Country blocked by rule: {}", rule.pattern));
                                }
                            }
                            Action::Redirect(_) => {
                                // Redirect rules can also have country restrictions
                                if !geoip.is_country_allowed(source_ip, countries) {
                                    return (false, format!("Country not allowed for redirect rule: {}", rule.pattern));
                                }
                            }
                        }
                    }
                }
            }
        }

        (allowed, reason)
    }

    /// Get the default policy
    pub fn get_default_policy(&self) -> &Policy {
        &self.acl.default_policy
    }

    /// Get the number of rules
    pub fn get_rule_count(&self) -> usize {
        self.acl.rules.len()
    }

    /// Check if GeoIP filtering is available
    pub fn has_geoip(&self) -> bool {
        self.geoip_filter.is_some()
    }

    /// Get country for an IP address (if GeoIP is available)
    pub fn get_country(&self, ip: IpAddr) -> Option<String> {
        self.geoip_filter.as_ref()?.get_country(ip)
    }
}

impl From<&AccessRule> for AccessControlRule {
    fn from(rule: &AccessRule) -> Self {
        Self {
            pattern: rule.pattern.clone(),
            action: Action::from(rule.action.as_str()),
            ports: rule.ports.clone(),
            countries: rule.countries.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    #[test]
    fn test_acl_manager_creation() {
        let config = AccessControlConfig {
            enabled: true,
            default_policy: "allow".to_string(),
            rules: vec![
                AccessRule {
                    pattern: "192.168.1.0/24".to_string(),
                    action: "block".to_string(),
                    ports: Some(vec![80, 443]),
                    countries: None,
                },
            ],
        };

        let acl_manager = AclManager::new(&config);
        assert_eq!(*acl_manager.get_default_policy(), Policy::Allow);
        assert_eq!(acl_manager.get_rule_count(), 1);
    }

    #[test]
    fn test_ip_access_control() {
        let config = AccessControlConfig {
            enabled: true,
            default_policy: "allow".to_string(),
            rules: vec![
                AccessRule {
                    pattern: "192.168.1.0/24".to_string(),
                    action: "block".to_string(),
                    ports: None,
                    countries: None,
                },
            ],
        };

        let acl_manager = AclManager::new(&config);
        
        // Test blocked IP
        let blocked_ip = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100));
        let target = TargetAddr::Domain("example.com".to_string());
        let (allowed, _reason) = acl_manager.check_access(&target, 80, blocked_ip);
        assert!(!allowed);

        // Test allowed IP
        let allowed_ip = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1));
        let (allowed, _reason) = acl_manager.check_access(&target, 80, allowed_ip);
        assert!(allowed);
    }

    #[test]
    fn test_port_restrictions() {
        let config = AccessControlConfig {
            enabled: true,
            default_policy: "allow".to_string(),
            rules: vec![
                AccessRule {
                    pattern: "*".to_string(),
                    action: "block".to_string(),
                    ports: Some(vec![22, 23]),
                    countries: None,
                },
            ],
        };

        let acl_manager = AclManager::new(&config);
        let source_ip = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1));
        let target = TargetAddr::Domain("example.com".to_string());

        // Test blocked port
        let (allowed, _reason) = acl_manager.check_access(&target, 22, source_ip);
        assert!(!allowed);

        // Test allowed port
        let (allowed, _reason) = acl_manager.check_access(&target, 80, source_ip);
        assert!(allowed);
    }

    #[test]
    fn test_domain_patterns() {
        let config = AccessControlConfig {
            enabled: true,
            default_policy: "allow".to_string(),
            rules: vec![
                AccessRule {
                    pattern: "*.malicious.com".to_string(),
                    action: "block".to_string(),
                    ports: None,
                    countries: None,
                },
            ],
        };

        let acl_manager = AclManager::new(&config);
        let source_ip = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1));

        // Test blocked domain
        let blocked_target = TargetAddr::Domain("sub.malicious.com".to_string());
        let (allowed, _reason) = acl_manager.check_access(&blocked_target, 80, source_ip);
        assert!(!allowed);

        // Test allowed domain
        let allowed_target = TargetAddr::Domain("example.com".to_string());
        let (allowed, _reason) = acl_manager.check_access(&allowed_target, 80, source_ip);
        assert!(allowed);
    }
}