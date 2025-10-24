//! Tests for proxy chaining functionality

use std::net::{SocketAddr, Ipv4Addr};
use std::time::Duration;
use rustproxy::routing::{ProxyChainBuilder, ProxyAuth, ProxyProtocol};

#[tokio::test]
async fn test_proxy_chain_builder() {
    let chain = ProxyChainBuilder::new()
        .add_socks5_proxy(
            SocketAddr::new(std::net::IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 1080),
            None
        )
        .add_http_proxy(
            SocketAddr::new(std::net::IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080),
            Some(ProxyAuth {
                username: "user".to_string(),
                password: "pass".to_string(),
            })
        )
        .with_timeout(Duration::from_secs(10))
        .build()
        .unwrap();

    assert_eq!(chain.proxies.len(), 2);
    assert_eq!(chain.connection_timeout, Duration::from_secs(10));
    
    // Check first proxy (SOCKS5)
    assert_eq!(chain.proxies[0].protocol, ProxyProtocol::Socks5);
    assert!(chain.proxies[0].auth.is_none());
    
    // Check second proxy (HTTP with auth)
    assert_eq!(chain.proxies[1].protocol, ProxyProtocol::Http);
    assert!(chain.proxies[1].auth.is_some());
    
    if let Some(auth) = &chain.proxies[1].auth {
        assert_eq!(auth.username, "user");
        assert_eq!(auth.password, "pass");
    }
}

#[tokio::test]
async fn test_empty_chain_fails() {
    let result = ProxyChainBuilder::new().build();
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("cannot be empty"));
}

#[tokio::test]
async fn test_single_proxy_chain() {
    let chain = ProxyChainBuilder::new()
        .add_socks5_proxy(
            SocketAddr::new(std::net::IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 1080),
            Some(ProxyAuth {
                username: "test".to_string(),
                password: "test123".to_string(),
            })
        )
        .build()
        .unwrap();

    assert_eq!(chain.proxies.len(), 1);
    assert_eq!(chain.proxies[0].protocol, ProxyProtocol::Socks5);
    assert!(chain.proxies[0].auth.is_some());
}

#[tokio::test]
async fn test_mixed_protocol_chain() {
    let chain = ProxyChainBuilder::new()
        .add_http_proxy(
            SocketAddr::new(std::net::IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)), 3128),
            None
        )
        .add_socks5_proxy(
            SocketAddr::new(std::net::IpAddr::V4(Ipv4Addr::new(10, 0, 0, 2)), 1080),
            Some(ProxyAuth {
                username: "socks_user".to_string(),
                password: "socks_pass".to_string(),
            })
        )
        .add_http_proxy(
            SocketAddr::new(std::net::IpAddr::V4(Ipv4Addr::new(10, 0, 0, 3)), 8080),
            Some(ProxyAuth {
                username: "http_user".to_string(),
                password: "http_pass".to_string(),
            })
        )
        .with_timeout(Duration::from_secs(30))
        .build()
        .unwrap();

    assert_eq!(chain.proxies.len(), 3);
    assert_eq!(chain.connection_timeout, Duration::from_secs(30));
    
    // Check proxy sequence
    assert_eq!(chain.proxies[0].protocol, ProxyProtocol::Http);
    assert!(chain.proxies[0].auth.is_none());
    
    assert_eq!(chain.proxies[1].protocol, ProxyProtocol::Socks5);
    assert!(chain.proxies[1].auth.is_some());
    
    assert_eq!(chain.proxies[2].protocol, ProxyProtocol::Http);
    assert!(chain.proxies[2].auth.is_some());
}

#[tokio::test]
async fn test_default_timeout() {
    let chain = ProxyChainBuilder::new()
        .add_socks5_proxy(
            SocketAddr::new(std::net::IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 1080),
            None
        )
        .build()
        .unwrap();

    // Default timeout should be 30 seconds
    assert_eq!(chain.connection_timeout, Duration::from_secs(30));
}