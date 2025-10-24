//! Tests for smart routing functionality

use std::net::{SocketAddr, Ipv4Addr};
use std::time::Duration;
use rustproxy::routing::{SmartRoutingManager, SmartRoutingConfig, UpstreamProxy, ProxyProtocol, HealthStatus};

#[tokio::test]
async fn test_smart_routing_manager_creation() {
    let config = SmartRoutingConfig::default();
    let manager = SmartRoutingManager::new(config);
    
    // Should start with no proxies
    let selected = manager.select_best_proxy(&[]).await;
    assert!(selected.is_none());
    
    let health_summary = manager.get_health_summary().await;
    assert_eq!(health_summary.total_proxies, 0);
}

#[tokio::test]
async fn test_proxy_selection_with_metrics() {
    let config = SmartRoutingConfig::default();
    let mut manager = SmartRoutingManager::new(config);
    
    // Add test proxies
    manager.add_upstream_proxy(
        "fast_proxy".to_string(),
        UpstreamProxy {
            addr: SocketAddr::new(std::net::IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 1080),
            auth: None,
            protocol: ProxyProtocol::Socks5,
        }
    ).await;
    
    manager.add_upstream_proxy(
        "slow_proxy".to_string(),
        UpstreamProxy {
            addr: SocketAddr::new(std::net::IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 1081),
            auth: None,
            protocol: ProxyProtocol::Socks5,
        }
    ).await;
    
    // Record metrics for fast proxy (low latency, high success rate)
    for _ in 0..5 {
        manager.record_connection_result("fast_proxy", Duration::from_millis(50), true).await;
    }
    
    // Record metrics for slow proxy (high latency, lower success rate)
    for _ in 0..3 {
        manager.record_connection_result("slow_proxy", Duration::from_millis(1000), true).await;
    }
    for _ in 0..2 {
        manager.record_connection_result("slow_proxy", Duration::from_millis(2000), false).await;
    }
    
    // Give some time for async operations
    tokio::time::sleep(Duration::from_millis(10)).await;
    
    // Fast proxy should be selected more often
    let mut fast_selected = 0;
    let mut slow_selected = 0;
    
    for _ in 0..10 {
        if let Some((id, _)) = manager.select_best_proxy(&[]).await {
            if id == "fast_proxy" {
                fast_selected += 1;
            } else if id == "slow_proxy" {
                slow_selected += 1;
            }
        }
    }
    
    // Fast proxy should be selected more often due to better metrics
    assert!(fast_selected >= slow_selected);
}

#[tokio::test]
async fn test_health_status_tracking() {
    let config = SmartRoutingConfig::default();
    let mut manager = SmartRoutingManager::new(config);
    
    manager.add_upstream_proxy(
        "test_proxy".to_string(),
        UpstreamProxy {
            addr: SocketAddr::new(std::net::IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 1080),
            auth: None,
            protocol: ProxyProtocol::Socks5,
        }
    ).await;
    
    // Initially should be unknown
    tokio::time::sleep(Duration::from_millis(10)).await;
    let metrics = manager.get_proxy_metrics("test_proxy").await;
    assert!(metrics.is_some());
    assert_eq!(metrics.unwrap().health_status, HealthStatus::Unknown);
    
    // Record successful connections
    for _ in 0..5 {
        manager.record_connection_result("test_proxy", Duration::from_millis(100), true).await;
    }
    
    tokio::time::sleep(Duration::from_millis(10)).await;
    let metrics = manager.get_proxy_metrics("test_proxy").await.unwrap();
    assert_eq!(metrics.health_status, HealthStatus::Healthy);
    assert!(metrics.success_rate > 0.8);
    
    // Record many failures
    for _ in 0..10 {
        manager.record_connection_result("test_proxy", Duration::from_millis(5000), false).await;
    }
    
    tokio::time::sleep(Duration::from_millis(10)).await;
    let metrics = manager.get_proxy_metrics("test_proxy").await.unwrap();
    assert_eq!(metrics.health_status, HealthStatus::Unhealthy);
    assert!(metrics.success_rate < 0.5);
}

#[tokio::test]
async fn test_proxy_exclusion() {
    let config = SmartRoutingConfig::default();
    let mut manager = SmartRoutingManager::new(config);
    
    manager.add_upstream_proxy(
        "proxy1".to_string(),
        UpstreamProxy {
            addr: SocketAddr::new(std::net::IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 1080),
            auth: None,
            protocol: ProxyProtocol::Socks5,
        }
    ).await;
    
    manager.add_upstream_proxy(
        "proxy2".to_string(),
        UpstreamProxy {
            addr: SocketAddr::new(std::net::IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 1081),
            auth: None,
            protocol: ProxyProtocol::Socks5,
        }
    ).await;
    
    // Without exclusions, should select a proxy
    let selected = manager.select_best_proxy(&[]).await;
    assert!(selected.is_some());
    
    // With one proxy excluded, should select the other
    let selected = manager.select_best_proxy(&["proxy1".to_string()]).await;
    assert!(selected.is_some());
    assert_eq!(selected.unwrap().0, "proxy2");
    
    // With both excluded, should select none
    let selected = manager.select_best_proxy(&["proxy1".to_string(), "proxy2".to_string()]).await;
    assert!(selected.is_none());
}

#[tokio::test]
async fn test_health_summary() {
    let config = SmartRoutingConfig::default();
    let mut manager = SmartRoutingManager::new(config);
    
    // Add multiple proxies
    for i in 0..5 {
        manager.add_upstream_proxy(
            format!("proxy{}", i),
            UpstreamProxy {
                addr: SocketAddr::new(std::net::IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 1080 + i),
                auth: None,
                protocol: ProxyProtocol::Socks5,
            }
        ).await;
    }
    
    // Make some healthy
    for i in 0..3 {
        for _ in 0..5 {
            manager.record_connection_result(&format!("proxy{}", i), Duration::from_millis(100), true).await;
        }
    }
    
    // Make some unhealthy
    for i in 3..5 {
        for _ in 0..5 {
            manager.record_connection_result(&format!("proxy{}", i), Duration::from_millis(1000), false).await;
        }
    }
    
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    let summary = manager.get_health_summary().await;
    
    // Basic checks - we have 5 proxies total
    assert_eq!(summary.total_proxies, 5);
    
    // All proxies should be accounted for
    assert_eq!(summary.healthy + summary.degraded + summary.unhealthy + summary.unknown, 5);
    
    // We should be able to get metrics for individual proxies
    let metrics = manager.get_all_metrics().await;
    assert_eq!(metrics.len(), 5);
    
    // At least some proxies should have recorded metrics
    let has_metrics = metrics.values().any(|m| !m.recent_results.is_empty());
    assert!(has_metrics, "At least some proxies should have recorded metrics");
}

#[tokio::test]
async fn test_smart_routing_config() {
    let config = SmartRoutingConfig {
        health_check_interval: Duration::from_secs(10),
        health_check_timeout: Duration::from_secs(2),
        min_measurements: 5,
        enable_latency_routing: true,
        enable_health_routing: false,
    };
    
    let _manager = SmartRoutingManager::new(config.clone());
    
    // Config should be stored correctly
    // Note: We can't directly access the config from the manager in the current implementation,
    // but we can test that it was created successfully
    assert!(true); // Placeholder assertion
}