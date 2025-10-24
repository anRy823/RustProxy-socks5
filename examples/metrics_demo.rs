//! Metrics System Demo
//! 
//! Demonstrates the metrics collection and reporting functionality

use rustproxy::metrics::{MetricsManager, export_report_json};
use rustproxy::config::MonitoringConfig;
use std::net::SocketAddr;
use std::time::Duration;
use tokio::time::sleep;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();
    
    // Create monitoring configuration
    let config = MonitoringConfig {
        enabled: true,
        metrics_addr: Some("127.0.0.1:9090".parse()?),
        log_level: "info".to_string(),
        prometheus_enabled: true,
        collect_connection_stats: true,
        max_historical_connections: 1000,
    };
    
    // Create and start metrics manager
    let mut metrics_manager = MetricsManager::new(config).await?;
    metrics_manager.start().await?;
    
    println!("Metrics system started. Prometheus metrics available at http://127.0.0.1:9090/metrics");
    
    // Simulate some connections
    simulate_connections(&metrics_manager).await?;
    
    // Generate and display reports
    generate_reports(&metrics_manager).await?;
    
    // Keep running for a bit to allow metrics scraping
    println!("Keeping server running for 30 seconds...");
    sleep(Duration::from_secs(30)).await;
    
    // Stop metrics system
    metrics_manager.stop().await;
    println!("Metrics system stopped");
    
    Ok(())
}

async fn simulate_connections(metrics_manager: &MetricsManager) -> anyhow::Result<()> {
    println!("Simulating connections...");
    
    let client_addr: SocketAddr = "192.168.1.100:12345".parse()?;
    let target_addrs = vec![
        "example.com:80".parse()?,
        "google.com:443".parse()?,
        "github.com:443".parse()?,
    ];
    
    // Simulate 10 connections
    for i in 0..10 {
        let session_id = format!("session_{}", i);
        let target_addr = target_addrs[i % target_addrs.len()];
        let user_id = if i % 3 == 0 { 
            Some(format!("user_{}", i % 3)) 
        } else { 
            None 
        };
        
        // Start connection
        metrics_manager.record_connection_start(
            session_id.clone(),
            client_addr,
            target_addr,
            user_id,
        ).await?;
        
        // Simulate some data transfer
        sleep(Duration::from_millis(100)).await;
        metrics_manager.update_connection_bytes(&session_id, 1024, 2048).await?;
        
        sleep(Duration::from_millis(200)).await;
        metrics_manager.update_connection_bytes(&session_id, 512, 1024).await?;
        
        // End connection
        metrics_manager.record_connection_end(&session_id).await?;
        
        // Simulate authentication attempts
        if i % 4 == 0 {
            metrics_manager.record_auth_attempt(true);
        } else if i % 7 == 0 {
            metrics_manager.record_auth_attempt(false);
        }
        
        // Simulate some blocked requests
        if i % 5 == 0 {
            metrics_manager.record_blocked_request("ACL rule violation");
        }
    }
    
    println!("Simulated {} connections", 10);
    Ok(())
}

async fn generate_reports(metrics_manager: &MetricsManager) -> anyhow::Result<()> {
    println!("Generating reports...");
    
    // Get current activity
    match metrics_manager.get_current_activity().await {
        Ok(activity) => {
            println!("Current Activity:");
            println!("  Active connections: {}", activity.active_connections);
            println!("  Total connections today: {}", activity.total_connections_today);
            println!("  Bytes transferred today: {}", activity.bytes_transferred_today);
            println!("  Authentication attempts: {}", activity.authentication_attempts_today);
            println!("  Blocked requests: {}", activity.blocked_requests_today);
        }
        Err(e) => {
            eprintln!("Failed to get activity summary: {}", e);
        }
    }
    
    // Generate daily report
    match metrics_manager.insights().generate_daily_report().await {
        Ok(report) => {
            println!("\nDaily Report:");
            println!("  Report ID: {}", report.report_id);
            println!("  Total connections: {}", report.summary.total_connections);
            println!("  Total bytes transferred: {}", report.summary.total_bytes_transferred);
            println!("  Average connection duration: {:.2}s", report.summary.average_connection_duration);
            println!("  Unique users: {}", report.summary.unique_users);
            
            // Export to JSON
            match export_report_json(&report) {
                Ok(json) => {
                    println!("\nReport JSON (first 200 chars):");
                    let preview = if json.len() > 200 {
                        format!("{}...", &json[..200])
                    } else {
                        json
                    };
                    println!("{}", preview);
                }
                Err(e) => {
                    eprintln!("Failed to export report to JSON: {}", e);
                }
            }
        }
        Err(e) => {
            eprintln!("Failed to generate daily report: {}", e);
        }
    }
    
    // Generate insights
    match metrics_manager.insights().generate_insights().await {
        Ok(insights) => {
            println!("\nConnection Insights:");
            for (i, insight) in insights.iter().enumerate() {
                println!("  {}: {}", i + 1, insight);
            }
        }
        Err(e) => {
            eprintln!("Failed to generate insights: {}", e);
        }
    }
    
    Ok(())
}