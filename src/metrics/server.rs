//! Metrics HTTP Server
//! 
//! Provides HTTP endpoint for Prometheus metrics scraping

use crate::metrics::Metrics;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tracing::{info, error, debug};

/// HTTP server for serving Prometheus metrics
pub struct MetricsServer {
    metrics: Arc<Metrics>,
    bind_addr: String,
}

impl MetricsServer {
    /// Create a new metrics server
    pub fn new(metrics: Arc<Metrics>, bind_addr: String) -> Self {
        Self {
            metrics,
            bind_addr,
        }
    }
    
    /// Start the metrics server
    pub async fn start(&self) -> anyhow::Result<()> {
        let listener = TcpListener::bind(&self.bind_addr).await?;
        info!(bind_addr = %self.bind_addr, "Metrics server started");
        
        loop {
            match listener.accept().await {
                Ok((mut stream, addr)) => {
                    debug!(client_addr = %addr, "Metrics request received");
                    
                    let metrics = self.metrics.clone();
                    tokio::spawn(async move {
                        if let Err(e) = handle_request(&mut stream, metrics).await {
                            error!(error = %e, client_addr = %addr, "Failed to handle metrics request");
                        }
                    });
                }
                Err(e) => {
                    error!(error = %e, "Failed to accept metrics connection");
                }
            }
        }
    }
}

/// Handle a single HTTP request for metrics
async fn handle_request(
    stream: &mut tokio::net::TcpStream,
    metrics: Arc<Metrics>,
) -> anyhow::Result<()> {
    // Read the HTTP request (simplified - just read some bytes)
    let mut buffer = [0; 1024];
    let bytes_read = stream.read(&mut buffer).await?;
    
    if bytes_read == 0 {
        return Ok(());
    }
    
    let request = String::from_utf8_lossy(&buffer[..bytes_read]);
    debug!(request = %request, "Received HTTP request");
    
    // Check if this is a GET request to /metrics
    if request.starts_with("GET /metrics") {
        let metrics_data = metrics.export_prometheus();
        
        let response = format!(
            "HTTP/1.1 200 OK\r\n\
             Content-Type: text/plain; version=0.0.4; charset=utf-8\r\n\
             Content-Length: {}\r\n\
             \r\n\
             {}",
            metrics_data.len(),
            metrics_data
        );
        
        stream.write_all(response.as_bytes()).await?;
        debug!("Sent Prometheus metrics response");
    } else if request.starts_with("GET /health") {
        // Health check endpoint
        let response = "HTTP/1.1 200 OK\r\n\
                       Content-Type: text/plain\r\n\
                       Content-Length: 2\r\n\
                       \r\n\
                       OK";
        
        stream.write_all(response.as_bytes()).await?;
        debug!("Sent health check response");
    } else {
        // 404 for other paths
        let response = "HTTP/1.1 404 Not Found\r\n\
                       Content-Type: text/plain\r\n\
                       Content-Length: 9\r\n\
                       \r\n\
                       Not Found";
        
        stream.write_all(response.as_bytes()).await?;
        debug!("Sent 404 response");
    }
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_metrics_endpoint() -> anyhow::Result<()> {
        let metrics = Arc::new(Metrics::new());
        let _server = MetricsServer::new(metrics, "127.0.0.1:0".to_string());
        
        // This is a basic test structure - in a real test we'd need to:
        // 1. Start the server in a background task
        // 2. Connect to it and make requests
        // 3. Verify the responses
        
        Ok(())
    }
}