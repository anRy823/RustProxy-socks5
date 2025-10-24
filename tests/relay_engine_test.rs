//! Tests for the relay engine

use std::net::Ipv4Addr;
use tokio::net::{TcpListener, TcpStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use rustproxy::relay::RelayEngine;
use rustproxy::protocol::types::TargetAddr;

#[tokio::test]
async fn test_target_connection_establishment() {
    let relay_engine = RelayEngine::new();
    
    // Test IPv4 connection
    let target_addr = TargetAddr::Ipv4(Ipv4Addr::new(127, 0, 0, 1));
    
    // Start a test server
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let server_addr = listener.local_addr().unwrap();
    
    // Spawn server task
    tokio::spawn(async move {
        if let Ok((mut stream, _)) = listener.accept().await {
            let mut buf = [0u8; 1024];
            if let Ok(n) = stream.read(&mut buf).await {
                let _ = stream.write_all(&buf[..n]).await;
            }
        }
    });
    
    // Test connection
    let result = relay_engine.connect_to_target(&target_addr, server_addr.port()).await;
    assert!(result.is_ok(), "Should be able to connect to localhost");
}

#[tokio::test]
async fn test_dns_resolution() {
    let relay_engine = RelayEngine::new();
    
    // Test domain resolution
    let target_addr = TargetAddr::Domain("localhost".to_string());
    let result = relay_engine.connect_to_target(&target_addr, 80).await;
    
    // This might fail if no service is running on port 80, but DNS resolution should work
    // The error should be connection refused, not DNS resolution failure
    if let Err(e) = result {
        let error_msg = e.to_string().to_lowercase();
        assert!(
            error_msg.contains("connection refused") || 
            error_msg.contains("connection failed") ||
            error_msg.contains("timed out"),
            "Error should be connection-related, not DNS: {}", e
        );
    }
}

#[tokio::test]
async fn test_session_tracking() {
    let relay_engine = RelayEngine::new();
    
    // Initially no active sessions
    assert_eq!(relay_engine.active_session_count(), 0);
    
    // Create mock streams
    let listener1 = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let listener2 = TcpListener::bind("127.0.0.1:0").await.unwrap();
    
    let addr1 = listener1.local_addr().unwrap();
    let addr2 = listener2.local_addr().unwrap();
    
    let client = TcpStream::connect(addr1).await.unwrap();
    let target = TcpStream::connect(addr2).await.unwrap();
    
    // Accept connections
    let (_, _) = listener1.accept().await.unwrap();
    let (_, _) = listener2.accept().await.unwrap();
    
    // Start relay session
    let session = relay_engine.start_relay(client, target).await.unwrap();
    
    // Should have one active session
    assert_eq!(relay_engine.active_session_count(), 1);
    
    // Should be able to retrieve the session
    let retrieved_session = relay_engine.get_session(&session.session_id);
    assert!(retrieved_session.is_some());
    
    // Remove session
    relay_engine.remove_session(&session.session_id);
    assert_eq!(relay_engine.active_session_count(), 0);
}

#[tokio::test]
async fn test_connection_stats() {
    use rustproxy::relay::RelaySession;
    use std::net::SocketAddr;
    
    let client_addr: SocketAddr = "127.0.0.1:12345".parse().unwrap();
    let target_addr: SocketAddr = "127.0.0.1:54321".parse().unwrap();
    
    let session = RelaySession::new(
        "test_session".to_string(),
        client_addr,
        target_addr,
    );
    
    // Test initial state
    assert_eq!(session.bytes_up(), 0);
    assert_eq!(session.bytes_down(), 0);
    assert_eq!(session.total_bytes(), 0);
    
    // Update statistics
    session.update_bytes_up(1024);
    session.update_bytes_down(2048);
    
    assert_eq!(session.bytes_up(), 1024);
    assert_eq!(session.bytes_down(), 2048);
    assert_eq!(session.total_bytes(), 3072);
    
    // Test stats generation
    let stats = session.to_stats(Some("test_user".to_string()));
    assert_eq!(stats.session_id, "test_session");
    assert_eq!(stats.client_addr, client_addr);
    assert_eq!(stats.target_addr, target_addr);
    assert_eq!(stats.bytes_up, 1024);
    assert_eq!(stats.bytes_down, 2048);
    assert_eq!(stats.total_bytes, 3072);
    assert_eq!(stats.user_id, Some("test_user".to_string()));
}