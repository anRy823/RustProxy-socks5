//! Proxy Chaining Support
//! 
//! Provides functionality to chain multiple proxies together, allowing traffic
//! to be routed through a sequence of upstream proxies.

use std::net::SocketAddr;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::time::timeout;
use tracing::debug;
use base64::Engine;

use crate::protocol::Socks5Handler;
use crate::protocol::TargetAddr;
use crate::Result;
use crate::routing::{UpstreamProxy, ProxyProtocol, ProxyAuth};

/// Proxy chain configuration
#[derive(Debug, Clone)]
pub struct ProxyChain {
    /// List of proxies in the chain (first proxy is connected to directly)
    pub proxies: Vec<UpstreamProxy>,
    /// Connection timeout for each proxy in the chain
    pub connection_timeout: Duration,
}

/// Proxy chain connector
pub struct ProxyChainConnector {
    chain: ProxyChain,
}

impl ProxyChainConnector {
    /// Create a new proxy chain connector
    pub fn new(chain: ProxyChain) -> Self {
        Self { chain }
    }

    /// Connect through the proxy chain to reach the target
    pub async fn connect_through_chain(
        &self,
        target: &TargetAddr,
        port: u16,
    ) -> Result<TcpStream> {
        if self.chain.proxies.is_empty() {
            return Err(anyhow::anyhow!("Proxy chain is empty"));
        }

        debug!("Connecting through proxy chain with {} proxies", self.chain.proxies.len());

        // Connect to the first proxy directly
        let mut stream = self.connect_to_first_proxy().await?;

        // Chain through intermediate proxies
        for (i, proxy) in self.chain.proxies.iter().enumerate().skip(1) {
            debug!("Chaining through proxy {} of {}: {}", i + 1, self.chain.proxies.len(), proxy.addr);
            stream = self.chain_through_proxy(stream, proxy).await?;
        }

        // Finally connect to the target through the last proxy
        debug!("Connecting to final target: {:?}:{}", target, port);
        self.connect_to_target_through_proxy(stream, target, port).await
    }

    /// Connect to the first proxy in the chain
    async fn connect_to_first_proxy(&self) -> Result<TcpStream> {
        let first_proxy = &self.chain.proxies[0];
        debug!("Connecting to first proxy: {}", first_proxy.addr);

        let stream = timeout(
            self.chain.connection_timeout,
            TcpStream::connect(first_proxy.addr)
        ).await??;

        debug!("Connected to first proxy: {}", first_proxy.addr);
        Ok(stream)
    }

    /// Chain through an intermediate proxy
    async fn chain_through_proxy(
        &self,
        stream: TcpStream,
        proxy: &UpstreamProxy,
    ) -> Result<TcpStream> {
        match proxy.protocol {
            ProxyProtocol::Socks5 => {
                self.chain_through_socks5_proxy(stream, proxy).await
            },
            ProxyProtocol::Http => {
                self.chain_through_http_proxy(stream, proxy).await
            },
        }
    }

    /// Chain through a SOCKS5 proxy
    async fn chain_through_socks5_proxy(
        &self,
        stream: TcpStream,
        proxy: &UpstreamProxy,
    ) -> Result<TcpStream> {
        debug!("Chaining through SOCKS5 proxy: {}", proxy.addr);

        let mut handler = Socks5Handler::new(stream);

        // Perform SOCKS5 handshake
        let auth_method = if proxy.auth.is_some() {
            0x02 // Username/password authentication
        } else {
            0x00 // No authentication
        };

        handler.send_greeting(&[auth_method]).await?;
        let selected_method = handler.receive_auth_method().await?;

        if selected_method != auth_method {
            return Err(anyhow::anyhow!(
                "SOCKS5 proxy rejected authentication method: expected {}, got {}",
                auth_method, selected_method
            ));
        }

        // Authenticate if required
        if let Some(auth) = &proxy.auth {
            handler.authenticate_username_password(&auth.username, &auth.password).await?;
        }

        // Get the next proxy address (or target if this is the last proxy)
        let next_proxy_addr = if let Some(next_proxy) = self.get_next_proxy_after(proxy) {
            TargetAddr::from_socket_addr(&next_proxy.addr)
        } else {
            return Err(anyhow::anyhow!("No next proxy found in chain"));
        };

        // Send CONNECT request to next proxy
        let next_port = if let Some(next_proxy) = self.get_next_proxy_after(proxy) {
            next_proxy.addr.port()
        } else {
            return Err(anyhow::anyhow!("No next proxy found in chain"));
        };
        handler.send_connect_request(&next_proxy_addr, next_port).await?;
        let response = handler.receive_connect_response().await?;

        if response.reply_code != 0x00 {
            return Err(anyhow::anyhow!(
                "SOCKS5 proxy connection failed with reply code: {}",
                response.reply_code
            ));
        }

        debug!("Successfully chained through SOCKS5 proxy: {}", proxy.addr);
        Ok(handler.into_stream())
    }

    /// Chain through an HTTP proxy
    async fn chain_through_http_proxy(
        &self,
        mut stream: TcpStream,
        proxy: &UpstreamProxy,
    ) -> Result<TcpStream> {
        debug!("Chaining through HTTP proxy: {}", proxy.addr);

        // Get the next proxy address
        let next_proxy_addr = if let Some(next_proxy) = self.get_next_proxy_after(proxy) {
            next_proxy.addr
        } else {
            return Err(anyhow::anyhow!("No next proxy found in chain"));
        };

        // Build HTTP CONNECT request
        let mut request = format!(
            "CONNECT {}:{} HTTP/1.1\r\nHost: {}:{}\r\n",
            next_proxy_addr.ip(),
            next_proxy_addr.port(),
            next_proxy_addr.ip(),
            next_proxy_addr.port()
        );

        // Add authentication if required
        if let Some(auth) = &proxy.auth {
            let credentials = base64::engine::general_purpose::STANDARD.encode(format!("{}:{}", auth.username, auth.password));
            request.push_str(&format!("Proxy-Authorization: Basic {}\r\n", credentials));
        }

        request.push_str("\r\n");

        // Send CONNECT request
        use tokio::io::{AsyncWriteExt, AsyncReadExt};
        stream.write_all(request.as_bytes()).await?;

        // Read response
        let mut response = vec![0u8; 1024];
        let n = stream.read(&mut response).await?;
        let response_str = String::from_utf8_lossy(&response[..n]);

        // Check if connection was successful
        if !response_str.starts_with("HTTP/1.1 200") && !response_str.starts_with("HTTP/1.0 200") {
            return Err(anyhow::anyhow!(
                "HTTP proxy connection failed: {}",
                response_str.lines().next().unwrap_or("Unknown error")
            ));
        }

        debug!("Successfully chained through HTTP proxy: {}", proxy.addr);
        Ok(stream)
    }

    /// Connect to the final target through the last proxy in the chain
    async fn connect_to_target_through_proxy(
        &self,
        stream: TcpStream,
        target: &TargetAddr,
        port: u16,
    ) -> Result<TcpStream> {
        let last_proxy = self.chain.proxies.last().unwrap();
        debug!("Connecting to target {:?}:{} through last proxy: {}", target, port, last_proxy.addr);

        match last_proxy.protocol {
            ProxyProtocol::Socks5 => {
                self.connect_to_target_through_socks5(stream, last_proxy, target, port).await
            },
            ProxyProtocol::Http => {
                self.connect_to_target_through_http(stream, last_proxy, target, port).await
            },
        }
    }

    /// Connect to target through SOCKS5 proxy
    async fn connect_to_target_through_socks5(
        &self,
        stream: TcpStream,
        proxy: &UpstreamProxy,
        target: &TargetAddr,
        port: u16,
    ) -> Result<TcpStream> {
        let mut handler = Socks5Handler::new(stream);

        // Perform SOCKS5 handshake
        let auth_method = if proxy.auth.is_some() { 0x02 } else { 0x00 };
        handler.send_greeting(&[auth_method]).await?;
        let selected_method = handler.receive_auth_method().await?;

        if selected_method != auth_method {
            return Err(anyhow::anyhow!("SOCKS5 authentication method rejected"));
        }

        // Authenticate if required
        if let Some(auth) = &proxy.auth {
            handler.authenticate_username_password(&auth.username, &auth.password).await?;
        }

        // Send CONNECT request to target
        handler.send_connect_request(target, port).await?;
        let response = handler.receive_connect_response().await?;

        if response.reply_code != 0x00 {
            return Err(anyhow::anyhow!(
                "SOCKS5 connection to target failed with reply code: {}",
                response.reply_code
            ));
        }

        debug!("Successfully connected to target through SOCKS5 proxy chain");
        Ok(handler.into_stream())
    }

    /// Connect to target through HTTP proxy
    async fn connect_to_target_through_http(
        &self,
        mut stream: TcpStream,
        proxy: &UpstreamProxy,
        target: &TargetAddr,
        port: u16,
    ) -> Result<TcpStream> {
        // Build HTTP CONNECT request for target
        let target_host = match target {
            TargetAddr::Ipv4(ip) => ip.to_string(),
            TargetAddr::Ipv6(ip) => format!("[{}]", ip),
            TargetAddr::Domain(domain) => domain.clone(),
        };

        let mut request = format!(
            "CONNECT {}:{} HTTP/1.1\r\nHost: {}:{}\r\n",
            target_host, port, target_host, port
        );

        // Add authentication if required
        if let Some(auth) = &proxy.auth {
            let credentials = base64::engine::general_purpose::STANDARD.encode(format!("{}:{}", auth.username, auth.password));
            request.push_str(&format!("Proxy-Authorization: Basic {}\r\n", credentials));
        }

        request.push_str("\r\n");

        // Send CONNECT request
        use tokio::io::{AsyncWriteExt, AsyncReadExt};
        stream.write_all(request.as_bytes()).await?;

        // Read response
        let mut response = vec![0u8; 1024];
        let n = stream.read(&mut response).await?;
        let response_str = String::from_utf8_lossy(&response[..n]);

        // Check if connection was successful
        if !response_str.starts_with("HTTP/1.1 200") && !response_str.starts_with("HTTP/1.0 200") {
            return Err(anyhow::anyhow!(
                "HTTP proxy connection to target failed: {}",
                response_str.lines().next().unwrap_or("Unknown error")
            ));
        }

        debug!("Successfully connected to target through HTTP proxy chain");
        Ok(stream)
    }

    /// Get the next proxy in the chain after the given proxy
    fn get_next_proxy_after(&self, current_proxy: &UpstreamProxy) -> Option<&UpstreamProxy> {
        let current_index = self.chain.proxies.iter().position(|p| {
            p.addr == current_proxy.addr && p.protocol == current_proxy.protocol
        })?;

        self.chain.proxies.get(current_index + 1)
    }
}

/// Helper trait to convert TargetAddr to/from SocketAddr


/// Proxy chain builder for easier configuration
pub struct ProxyChainBuilder {
    proxies: Vec<UpstreamProxy>,
    connection_timeout: Duration,
}

impl ProxyChainBuilder {
    /// Create a new proxy chain builder
    pub fn new() -> Self {
        Self {
            proxies: Vec::new(),
            connection_timeout: Duration::from_secs(30),
        }
    }

    /// Add a SOCKS5 proxy to the chain
    pub fn add_socks5_proxy(mut self, addr: SocketAddr, auth: Option<ProxyAuth>) -> Self {
        self.proxies.push(UpstreamProxy {
            addr,
            auth,
            protocol: ProxyProtocol::Socks5,
        });
        self
    }

    /// Add an HTTP proxy to the chain
    pub fn add_http_proxy(mut self, addr: SocketAddr, auth: Option<ProxyAuth>) -> Self {
        self.proxies.push(UpstreamProxy {
            addr,
            auth,
            protocol: ProxyProtocol::Http,
        });
        self
    }

    /// Set connection timeout for each proxy in the chain
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.connection_timeout = timeout;
        self
    }

    /// Build the proxy chain
    pub fn build(self) -> Result<ProxyChain> {
        if self.proxies.is_empty() {
            return Err(anyhow::anyhow!("Proxy chain cannot be empty"));
        }

        Ok(ProxyChain {
            proxies: self.proxies,
            connection_timeout: self.connection_timeout,
        })
    }
}

impl Default for ProxyChainBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    #[test]
    fn test_proxy_chain_builder() {
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
    }

    #[test]
    fn test_empty_chain_fails() {
        let result = ProxyChainBuilder::new().build();
        assert!(result.is_err());
    }

    #[test]
    fn test_target_addr_from_socket_addr() {
        let ipv4_addr = SocketAddr::new(std::net::IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)), 80);
        let target = TargetAddr::from_socket_addr(&ipv4_addr);
        
        match target {
            TargetAddr::Ipv4(ip) => assert_eq!(ip, Ipv4Addr::new(192, 168, 1, 1)),
            _ => panic!("Expected IPv4 target address"),
        }
    }
}
