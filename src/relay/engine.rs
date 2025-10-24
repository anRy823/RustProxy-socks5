//! Relay Engine

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::net::{TcpStream, lookup_host};
use tokio::time::timeout;
use tracing::{debug, error, info, warn};
use anyhow::{anyhow, Context};

use crate::Result;
use crate::protocol::types::TargetAddr;
use crate::protocol::constants::*;
use super::{RelaySession, session::ConnectionStats};

/// Handles data relay between client and target connections
pub struct RelayEngine {
    connection_timeout: Duration,
    active_sessions: Arc<Mutex<HashMap<String, Arc<RelaySession>>>>,
}

impl RelayEngine {
    /// Create a new relay engine
    pub fn new() -> Self {
        Self {
            connection_timeout: Duration::from_secs(300), // Default 5 minute timeout for data relay
            active_sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Create a new relay engine with custom timeout
    pub fn with_timeout(connection_timeout: Duration) -> Self {
        Self {
            connection_timeout,
            active_sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Create a new relay engine from configuration
    pub fn from_config(config: &crate::config::Config) -> Self {
        Self {
            connection_timeout: config.server.connection_timeout,
            active_sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Establish connection to target server
    pub async fn connect_to_target(&self, target_addr: &TargetAddr, port: u16) -> Result<(TcpStream, SocketAddr)> {
        debug!("Attempting to connect to target: {:?}:{}", target_addr, port);

        // Resolve target address to socket addresses
        let socket_addrs = self.resolve_target_address(target_addr, port).await
            .context("Failed to resolve target address")?;

        // Try connecting to each resolved address
        let mut last_error = None;
        for addr in socket_addrs {
            match self.try_connect_to_address(addr).await {
                Ok(stream) => {
                    info!("Successfully connected to target: {}", addr);
                    return Ok((stream, addr));
                }
                Err(e) => {
                    warn!("Failed to connect to {}: {}", addr, e);
                    last_error = Some(e);
                }
            }
        }

        // If we get here, all connection attempts failed
        let error_msg = format!("Failed to connect to target {}:{}", target_addr.to_string(), port);
        if let Some(e) = last_error {
            Err(anyhow!("{}: {}", error_msg, e))
        } else {
            Err(anyhow!("{}: No addresses resolved", error_msg))
        }
    }

    /// Resolve target address to socket addresses
    async fn resolve_target_address(&self, target_addr: &TargetAddr, port: u16) -> Result<Vec<SocketAddr>> {
        match target_addr {
            TargetAddr::Ipv4(ip) => {
                let addr = SocketAddr::new(IpAddr::V4(*ip), port);
                Ok(vec![addr])
            }
            TargetAddr::Ipv6(ip) => {
                let addr = SocketAddr::new(IpAddr::V6(*ip), port);
                Ok(vec![addr])
            }
            TargetAddr::Domain(domain) => {
                debug!("Resolving domain: {}:{}", domain, port);
                
                // Use tokio's lookup_host for DNS resolution
                let host_port = format!("{}:{}", domain, port);
                let lookup_future = lookup_host(host_port);
                match timeout(self.connection_timeout, lookup_future).await {
                    Ok(Ok(addrs)) => {
                        let resolved_addrs: Vec<SocketAddr> = addrs.collect();
                        if resolved_addrs.is_empty() {
                            return Err(anyhow!("DNS resolution returned no addresses for {}", domain));
                        }
                        debug!("Resolved {} to {} addresses", domain, resolved_addrs.len());
                        Ok(resolved_addrs)
                    }
                    Ok(Err(e)) => {
                        error!("DNS resolution failed for {}: {}", domain, e);
                        Err(anyhow!("DNS resolution failed for {}: {}", domain, e))
                    }
                    Err(_) => {
                        error!("DNS resolution timed out for {}", domain);
                        Err(anyhow!("DNS resolution timed out for {}", domain))
                    }
                }
            }
        }
    }

    /// Try to connect to a specific socket address
    async fn try_connect_to_address(&self, addr: SocketAddr) -> Result<TcpStream> {
        match timeout(self.connection_timeout, TcpStream::connect(addr)).await {
            Ok(Ok(stream)) => Ok(stream),
            Ok(Err(e)) => Err(anyhow!("Connection failed: {}", e)),
            Err(_) => Err(anyhow!("Connection timed out")),
        }
    }

    /// Convert connection error to appropriate SOCKS5 error code
    pub fn connection_error_to_socks5_code(&self, error: &anyhow::Error) -> u8 {
        let error_str = error.to_string().to_lowercase();
        
        if error_str.contains("timed out") || error_str.contains("timeout") {
            SOCKS5_REPLY_TTL_EXPIRED
        } else if error_str.contains("connection refused") || error_str.contains("refused") {
            SOCKS5_REPLY_CONNECTION_REFUSED
        } else if error_str.contains("network unreachable") || error_str.contains("unreachable") {
            SOCKS5_REPLY_NETWORK_UNREACHABLE
        } else if error_str.contains("host unreachable") || error_str.contains("no route") {
            SOCKS5_REPLY_HOST_UNREACHABLE
        } else if error_str.contains("dns") || error_str.contains("resolution") {
            SOCKS5_REPLY_HOST_UNREACHABLE
        } else {
            SOCKS5_REPLY_GENERAL_FAILURE
        }
    }

    /// Start a relay session between client and target
    pub async fn start_relay(&self, client: TcpStream, target: TcpStream) -> Result<Arc<RelaySession>> {
        let client_addr = client.peer_addr()
            .context("Failed to get client address")?;
        let target_addr = target.peer_addr()
            .context("Failed to get target address")?;
        
        // Generate a unique session ID using timestamp and addresses
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis();
        let session_id = format!("relay_{}_{}", timestamp, client_addr.port());

        let session = Arc::new(RelaySession::new(session_id.clone(), client_addr, target_addr));
        
        // Add to active sessions
        {
            let mut sessions = self.active_sessions.lock().unwrap();
            sessions.insert(session_id.clone(), session.clone());
        }
        
        info!("Started relay session {} from {} to {}", 
              session.session_id, client_addr, target_addr);
        
        Ok(session)
    }

    /// Start a complete relay session with immediate data transfer
    pub async fn start_complete_relay_with_user(
        &self,
        client: TcpStream,
        target: TcpStream,
        user_id: Option<String>,
    ) -> Result<crate::relay::session::ConnectionStats> {
        let client_addr = client.peer_addr()
            .context("Failed to get client address")?;
        let target_addr = target.peer_addr()
            .context("Failed to get target address")?;
        
        // Generate a unique session ID using timestamp and addresses
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis();
        let session_id = format!("relay_{}_{}", timestamp, client_addr.port());

        let session = Arc::new(RelaySession::new(session_id.clone(), client_addr, target_addr));
        
        // Add to active sessions
        {
            let mut sessions = self.active_sessions.lock().unwrap();
            sessions.insert(session_id.clone(), session.clone());
        }
        
        info!("Started complete relay session {} from {} to {}", 
              session.session_id, client_addr, target_addr);
        
        // Start the actual data relay immediately
        self.relay_data_with_user(&session, client, target, user_id).await
    }

    /// Remove a session from active tracking
    pub fn remove_session(&self, session_id: &str) {
        let mut sessions = self.active_sessions.lock().unwrap();
        if sessions.remove(session_id).is_some() {
            debug!("Removed session {} from active tracking", session_id);
        }
    }

    /// Relay data bidirectionally between client and target
    pub async fn relay_data(
        &self,
        session: &Arc<RelaySession>,
        mut client: TcpStream,
        mut target: TcpStream,
    ) -> Result<ConnectionStats> {
        info!("Starting bidirectional data relay for session {}", session.session_id);
        
        // Use tokio's copy_bidirectional for efficient data transfer with timeout
        let result = timeout(
            self.connection_timeout,
            tokio::io::copy_bidirectional(&mut client, &mut target)
        ).await;
        
        // Remove from active sessions when done
        self.remove_session(&session.session_id);
        
        match result {
            Ok(Ok((bytes_to_target, bytes_to_client))) => {
                // Update session statistics
                session.update_bytes_up(bytes_to_target);
                session.update_bytes_down(bytes_to_client);
                
                // Log detailed statistics
                session.log_stats(None);
                
                // Generate and return connection statistics
                let stats = session.to_stats(None);
                
                info!("Relay session {} completed successfully. Transferred {} bytes up, {} bytes down in {:?}", 
                      session.session_id, bytes_to_target, bytes_to_client, session.duration());
                
                Ok(stats)
            }
            Ok(Err(e)) => {
                error!("Relay session {} failed after {:?}: {}", 
                       session.session_id, session.duration(), e);
                
                // Log partial statistics even on failure
                session.log_stats(None);
                
                Err(anyhow!("Data relay failed: {}", e))
            }
            Err(_) => {
                error!("Relay session {} timed out after {:?}", 
                       session.session_id, session.duration());
                
                // Log partial statistics even on timeout
                session.log_stats(None);
                
                Err(anyhow!("Data relay timed out after {:?}", self.connection_timeout))
            }
        }
    }

    /// Relay data with user context for authentication tracking
    pub async fn relay_data_with_user(
        &self,
        session: &Arc<RelaySession>,
        mut client: TcpStream,
        mut target: TcpStream,
        user_id: Option<String>,
    ) -> Result<ConnectionStats> {
        info!("Starting bidirectional data relay for session {} (user: {:?})", 
              session.session_id, user_id);
        
        // Use tokio's copy_bidirectional for efficient data transfer with timeout
        let result = timeout(
            self.connection_timeout,
            tokio::io::copy_bidirectional(&mut client, &mut target)
        ).await;
        
        // Remove from active sessions when done
        self.remove_session(&session.session_id);
        
        match result {
            Ok(Ok((bytes_to_target, bytes_to_client))) => {
                // Update session statistics
                session.update_bytes_up(bytes_to_target);
                session.update_bytes_down(bytes_to_client);
                
                // Log detailed statistics with user context
                session.log_stats(user_id.as_deref());
                
                // Generate and return connection statistics
                let stats = session.to_stats(user_id);
                
                info!("Relay session {} completed successfully. Transferred {} bytes up, {} bytes down in {:?}", 
                      session.session_id, bytes_to_target, bytes_to_client, session.duration());
                
                Ok(stats)
            }
            Ok(Err(e)) => {
                error!("Relay session {} failed after {:?}: {}", 
                       session.session_id, session.duration(), e);
                
                // Log partial statistics even on failure
                session.log_stats(user_id.as_deref());
                
                Err(anyhow!("Data relay failed: {}", e))
            }
            Err(_) => {
                error!("Relay session {} timed out after {:?} (user: {:?})", 
                       session.session_id, session.duration(), user_id);
                
                // Log partial statistics even on timeout
                session.log_stats(user_id.as_deref());
                
                Err(anyhow!("Data relay timed out after {:?}", self.connection_timeout))
            }
        }
    }

    /// Start a complete relay session (connect + relay)
    pub async fn start_complete_relay(
        &self,
        client: TcpStream,
        target_addr: &TargetAddr,
        port: u16,
    ) -> Result<()> {
        // Establish connection to target
        let (target_stream, resolved_addr) = self.connect_to_target(target_addr, port).await
            .context("Failed to connect to target")?;
        
        // Create relay session
        let session = self.start_relay(client, target_stream).await
            .context("Failed to start relay session")?;
        
        // Get streams back for relay (we need to reconnect since we consumed them)
        let client_addr = session.client_addr;
        let target_addr_resolved = resolved_addr;
        
        // For now, we'll need to restructure this to avoid consuming the streams twice
        // This is a design issue that will be addressed in the integration
        info!("Relay session {} established between {} and {}", 
              session.session_id, client_addr, target_addr_resolved);
        
        Ok(())
    }

    /// Get all active sessions
    pub fn get_active_sessions(&self) -> Vec<Arc<RelaySession>> {
        let sessions = self.active_sessions.lock().unwrap();
        sessions.values().cloned().collect()
    }

    /// Get active session count
    pub fn active_session_count(&self) -> usize {
        let sessions = self.active_sessions.lock().unwrap();
        sessions.len()
    }

    /// Get session by ID
    pub fn get_session(&self, session_id: &str) -> Option<Arc<RelaySession>> {
        let sessions = self.active_sessions.lock().unwrap();
        sessions.get(session_id).cloned()
    }

    /// Get connection statistics for all active sessions
    pub fn get_active_session_stats(&self) -> Vec<ConnectionStats> {
        let sessions = self.active_sessions.lock().unwrap();
        sessions.values()
            .map(|session| session.to_stats(None))
            .collect()
    }
}