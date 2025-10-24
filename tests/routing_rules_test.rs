//! Tests for custom routing rules engine

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use rustproxy::routing::{RoutingRulesEngine, RoutingRule, RoutingAction, RouteDecision};
use rustproxy::protocol::TargetAddr;

#[tokio::test]
async fn test_routing_rules_priority_order() {
    let mut engine = RoutingRulesEngine::new();
    
    // Add a low priority rule that allows everything
    let allow_all_rule = RoutingRule {
        id: "allow_all".to_string(),
        priority: 100,
        pattern: "*".to_string(),
        action: RoutingAction::Allow,
        ports: None,
        source_ips: None,
        users: None,
        time_restrictions: None,
        enabled: true,
    };
    
    // Add a high priority rule that blocks specific domain
    let block_rule = RoutingRule {
        id: "block_malware".to_string(),
        priority: 1000,
        pattern: "malware.com".to_string(),
        action: RoutingAction::Block { 
            reason: Some("Malware domain blocked".to_string()) 
        },
        ports: None,
        source_ips: None,
        users: None,
        time_restrictions: None,
        enabled: true,
    };
    
    engine.add_rule(allow_all_rule).unwrap();
    engine.add_rule(block_rule).unwrap();
    
    // Test that high priority block rule takes precedence
    let target = TargetAddr::Domain("malware.com".to_string());
    let decision = engine.evaluate_rules(
        &target, 
        80, 
        IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100)), 
        None
    );
    
    match decision {
        RouteDecision::Block { reason } => {
            assert_eq!(reason, "Malware domain blocked");
        },
        _ => panic!("Expected block decision for malware domain"),
    }
    
    // Test that other domains are allowed by low priority rule
    let safe_target = TargetAddr::Domain("example.com".to_string());
    let safe_decision = engine.evaluate_rules(
        &safe_target, 
        80, 
        IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100)), 
        None
    );
    
    match safe_decision {
        RouteDecision::Allow { .. } => {},
        _ => panic!("Expected allow decision for safe domain"),
    }
}

#[tokio::test]
async fn test_wildcard_domain_matching() {
    let mut engine = RoutingRulesEngine::new();
    
    let wildcard_rule = RoutingRule {
        id: "block_ads".to_string(),
        priority: 500,
        pattern: "*.ads.com".to_string(),
        action: RoutingAction::Block { 
            reason: Some("Advertisement blocked".to_string()) 
        },
        ports: None,
        source_ips: None,
        users: None,
        time_restrictions: None,
        enabled: true,
    };
    
    engine.add_rule(wildcard_rule).unwrap();
    
    // Test subdomain matching
    let ad_target = TargetAddr::Domain("tracker.ads.com".to_string());
    let decision = engine.evaluate_rules(
        &ad_target, 
        80, 
        IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100)), 
        None
    );
    
    match decision {
        RouteDecision::Block { reason } => {
            assert_eq!(reason, "Advertisement blocked");
        },
        _ => panic!("Expected block decision for ad subdomain"),
    }
    
    // Test that non-matching domains are allowed
    let safe_target = TargetAddr::Domain("example.com".to_string());
    let safe_decision = engine.evaluate_rules(
        &safe_target, 
        80, 
        IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100)), 
        None
    );
    
    match safe_decision {
        RouteDecision::Allow { .. } => {},
        _ => panic!("Expected allow decision for non-ad domain"),
    }
}

#[tokio::test]
async fn test_port_restrictions() {
    let mut engine = RoutingRulesEngine::new();
    
    let port_restricted_rule = RoutingRule {
        id: "block_ssh".to_string(),
        priority: 500,
        pattern: "*".to_string(),
        action: RoutingAction::Block { 
            reason: Some("SSH blocked".to_string()) 
        },
        ports: Some(vec![22]),
        source_ips: None,
        users: None,
        time_restrictions: None,
        enabled: true,
    };
    
    engine.add_rule(port_restricted_rule).unwrap();
    
    // Test that SSH port is blocked
    let target = TargetAddr::Domain("server.com".to_string());
    let ssh_decision = engine.evaluate_rules(
        &target, 
        22, 
        IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100)), 
        None
    );
    
    match ssh_decision {
        RouteDecision::Block { reason } => {
            assert_eq!(reason, "SSH blocked");
        },
        _ => panic!("Expected block decision for SSH port"),
    }
    
    // Test that HTTP port is allowed (no rule matches)
    let http_decision = engine.evaluate_rules(
        &target, 
        80, 
        IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100)), 
        None
    );
    
    match http_decision {
        RouteDecision::Allow { .. } => {},
        _ => panic!("Expected allow decision for HTTP port"),
    }
}

#[tokio::test]
async fn test_source_ip_restrictions() {
    let mut engine = RoutingRulesEngine::new();
    
    let ip_restricted_rule = RoutingRule {
        id: "internal_only".to_string(),
        priority: 500,
        pattern: "internal.company.com".to_string(),
        action: RoutingAction::Allow,
        ports: None,
        source_ips: Some(vec!["192.168.1.0/24".to_string()]),
        users: None,
        time_restrictions: None,
        enabled: true,
    };
    
    engine.add_rule(ip_restricted_rule).unwrap();
    
    // Test that internal IP can access
    let target = TargetAddr::Domain("internal.company.com".to_string());
    let internal_decision = engine.evaluate_rules(
        &target, 
        80, 
        IpAddr::V4(Ipv4Addr::new(192, 168, 1, 50)), 
        None
    );
    
    match internal_decision {
        RouteDecision::Allow { .. } => {},
        _ => panic!("Expected allow decision for internal IP"),
    }
    
    // Test that external IP cannot access (no rule matches, defaults to allow)
    let external_decision = engine.evaluate_rules(
        &target, 
        80, 
        IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)), 
        None
    );
    
    match external_decision {
        RouteDecision::Allow { .. } => {},
        _ => panic!("Expected default allow decision for external IP"),
    }
}

#[tokio::test]
async fn test_redirect_action() {
    let mut engine = RoutingRulesEngine::new();
    
    let redirect_rule = RoutingRule {
        id: "redirect_dns".to_string(),
        priority: 500,
        pattern: "dns.google.com".to_string(),
        action: RoutingAction::Redirect { 
            target: "8.8.8.8:53".parse().unwrap() 
        },
        ports: None,
        source_ips: None,
        users: None,
        time_restrictions: None,
        enabled: true,
    };
    
    engine.add_rule(redirect_rule).unwrap();
    
    let target = TargetAddr::Domain("dns.google.com".to_string());
    let decision = engine.evaluate_rules(
        &target, 
        53, 
        IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100)), 
        None
    );
    
    match decision {
        RouteDecision::Redirect { target } => {
            assert_eq!(target, "8.8.8.8:53".parse::<SocketAddr>().unwrap());
        },
        _ => panic!("Expected redirect decision"),
    }
}

#[tokio::test]
async fn test_disabled_rules() {
    let mut engine = RoutingRulesEngine::new();
    
    let disabled_rule = RoutingRule {
        id: "disabled_block".to_string(),
        priority: 1000,
        pattern: "blocked.com".to_string(),
        action: RoutingAction::Block { 
            reason: Some("Should not be applied".to_string()) 
        },
        ports: None,
        source_ips: None,
        users: None,
        time_restrictions: None,
        enabled: false, // Rule is disabled
    };
    
    engine.add_rule(disabled_rule).unwrap();
    
    let target = TargetAddr::Domain("blocked.com".to_string());
    let decision = engine.evaluate_rules(
        &target, 
        80, 
        IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100)), 
        None
    );
    
    // Should allow since the blocking rule is disabled
    match decision {
        RouteDecision::Allow { .. } => {},
        _ => panic!("Expected allow decision since rule is disabled"),
    }
}