//! Tests for connection timeout and resource management functionality

use std::time::Duration;
use tokio::time::sleep;
use rustproxy::config::Config;
use rustproxy::resource::ResourceManager;
use std::sync::Arc;

#[tokio::test]
async fn test_connection_slot_acquisition() {
    let mut config = Config::default();
    config.server.max_connections = 2; // Limit to 2 connections for testing
    
    let resource_manager = ResourceManager::new(Arc::new(config));
    
    // Should be able to acquire up to max_connections slots
    let slot1 = resource_manager.acquire_connection_slot().await;
    assert!(slot1.is_ok());
    
    let slot2 = resource_manager.acquire_connection_slot().await;
    assert!(slot2.is_ok());
    
    // Third slot should fail
    let slot3 = resource_manager.acquire_connection_slot().await;
    assert!(slot3.is_err());
    
    // After dropping slots, should be able to acquire again
    drop(slot1);
    drop(slot2);
    
    // Give a moment for cleanup
    sleep(Duration::from_millis(10)).await;
    
    let slot4 = resource_manager.acquire_connection_slot().await;
    assert!(slot4.is_ok());
}

#[tokio::test]
async fn test_memory_tracking() {
    let config = Config::default();
    let resource_manager = ResourceManager::new(Arc::new(config));
    
    // Should be able to allocate memory within limits
    assert!(resource_manager.allocate_memory(1024).is_ok());
    
    let stats = resource_manager.get_stats();
    assert_eq!(stats.memory_usage_mb, 0); // 1024 bytes is less than 1 MB
    
    // Deallocate memory
    resource_manager.deallocate_memory(1024);
    
    let stats = resource_manager.get_stats();
    assert_eq!(stats.memory_usage_mb, 0);
}

#[tokio::test]
async fn test_connection_pool() {
    let config = Config::default();
    let resource_manager = ResourceManager::new(Arc::new(config));
    
    // Should return None for non-existent upstream
    let conn = resource_manager.get_pooled_connection("test-upstream").await;
    assert!(conn.is_none());
    
    // Pool stats should show a miss
    let stats = resource_manager.get_stats();
    assert_eq!(stats.pool_misses, 1);
    assert_eq!(stats.pool_hits, 0);
}

#[tokio::test]
async fn test_resource_stats() {
    let config = Config::default();
    let resource_manager = ResourceManager::new(Arc::new(config));
    
    let stats = resource_manager.get_stats();
    assert_eq!(stats.active_connections, 0);
    assert_eq!(stats.memory_usage_mb, 0);
    assert_eq!(stats.total_connections_created, 0);
    assert_eq!(stats.total_connections_rejected, 0);
    
    // Acquire a connection slot
    let _slot = resource_manager.acquire_connection_slot().await.unwrap();
    
    let stats = resource_manager.get_stats();
    assert_eq!(stats.active_connections, 1);
    assert_eq!(stats.total_connections_created, 1);
}