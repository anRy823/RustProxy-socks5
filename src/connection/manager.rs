//! Connection Manager Implementation

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, AtomicBool, Ordering};
use std::time::Instant;

use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{RwLock, broadcast};
use tokio::time::{timeout, Duration};
use tracing::{info, warn, error, debug, instrument};
use crate::config::Config;
use crate::auth::AuthManager;
use crate::protocol::{Socks5Handler, AuthMethod};
use crate::resource::ResourceManager;
use crate::security::{RateLimiter, DdosProtection, Fail2BanManager};
use crate::security::ddos_protection::DdosDecision;
use crate::security::fail2ban::Fail2BanDecision;
use crate::routing::{Router, RouteDecision};
use crate::relay::RelayEngine;
use crate::Result;

/// Connection information for tracking
#[derive(Debug, Clone)]
pub struct ConnectionInfo {
    pub id: String,
    pub addr: SocketAddr,
    pub start_time: Instant,
}

/// Manages TCP connections and their lifecycle
pub struct ConnectionManager {
    listener: Option<TcpListener>,
    config: Arc<Config>,
    auth_manager: Arc<AuthManager>,
    resource_manager: Arc<ResourceManager>,
    rate_limiter: Arc<RateLimiter>,
    ddos_protection: Arc<DdosProtection>,
    fail2ban_manager: Arc<Fail2BanManager>,
    active_connections: Arc<AtomicUsize>,
    connection_tracker: Arc<RwLock<HashMap<String, ConnectionInfo>>>,
    next_connection_id: Arc<AtomicUsize>,
    shutdown_flag: Arc<AtomicBool>,
    shutdown_tx: broadcast::Sender<()>,
}

impl ConnectionManager {
    /// Create a new ConnectionManager
    pub fn new(config: Arc<Config>) -> Self {
        let auth_manager = Arc::new(AuthManager::new(Arc::clone(&config)));
        let resource_manager = Arc::new(ResourceManager::new(Arc::clone(&config)));
        let rate_limiter = Arc::new(RateLimiter::new(config.security.rate_limiting.clone()));
        let ddos_protection = Arc::new(DdosProtection::new(config.security.ddos_protection.clone()));
        let fail2ban_manager = Arc::new(Fail2BanManager::new(config.security.fail2ban.clone()));
        let (shutdown_tx, _) = broadcast::channel(1);
        
        Self {
            listener: None,
            config,
            auth_manager,
            resource_manager,
            rate_limiter,
            ddos_protection,
            fail2ban_manager,
            active_connections: Arc::new(AtomicUsize::new(0)),
            connection_tracker: Arc::new(RwLock::new(HashMap::new())),
            next_connection_id: Arc::new(AtomicUsize::new(1)),
            shutdown_flag: Arc::new(AtomicBool::new(false)),
            shutdown_tx,
        }
    }

    /// Get the authentication manager
    pub fn auth_manager(&self) -> &Arc<AuthManager> {
        &self.auth_manager
    }

    /// Start background cleanup task for sessions and rate limits
    fn start_cleanup_task(&self) {
        let auth_manager = Arc::clone(&self.auth_manager);
        let resource_manager = Arc::clone(&self.resource_manager);
        let rate_limiter = Arc::clone(&self.rate_limiter);
        let ddos_protection = Arc::clone(&self.ddos_protection);
        let fail2ban_manager = Arc::clone(&self.fail2ban_manager);
        let connection_tracker = Arc::clone(&self.connection_tracker);
        let idle_timeout = self.config.server.idle_timeout;
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(60)); // Check every minute
            
            loop {
                interval.tick().await;
                
                debug!("Running periodic cleanup of expired sessions, rate limits, and idle connections");
                
                // Cleanup authentication data
                auth_manager.cleanup_expired();
                
                // Cleanup resource manager connection pool
                resource_manager.cleanup_connection_pool().await;
                
                // Cleanup security components
                rate_limiter.cleanup_old_entries();
                ddos_protection.cleanup_old_entries();
                fail2ban_manager.cleanup_old_entries();
                
                // Check for idle connections that should be closed
                let mut idle_connections = Vec::new();
                {
                    let tracker = connection_tracker.read().await;
                    for (conn_id, conn_info) in tracker.iter() {
                        if conn_info.start_time.elapsed() > idle_timeout {
                            idle_connections.push(conn_id.clone());
                        }
                    }
                }
                
                if !idle_connections.is_empty() {
                    warn!("Found {} idle connections exceeding timeout of {:?}", 
                          idle_connections.len(), idle_timeout);
                    // Note: In a real implementation, we would need a way to actually close these connections
                    // For now, we just log them as they will be cleaned up when they naturally close
                }
                
                let auth_stats = auth_manager.get_stats();
                let resource_stats = resource_manager.get_stats();
                debug!("Cleanup stats - Auth: {} active sessions, {} rate limited IPs, {} rate limited users; Resources: {} MB memory, {} active connections", 
                       auth_stats.active_sessions, auth_stats.rate_limited_ips, auth_stats.rate_limited_users,
                       resource_stats.memory_usage_mb, resource_stats.active_connections);
            }
        });
        
        info!("Started background cleanup task for authentication, resources, and idle connections");
    }

    /// Start the connection manager and begin accepting connections
    pub async fn start(&mut self) -> Result<()> {
        let bind_addr = self.config.server.bind_addr;
        
        info!("Binding TCP listener to {}", bind_addr);
        let listener = TcpListener::bind(bind_addr).await?;
        
        info!("Successfully bound to {}", bind_addr);
        self.listener = Some(listener);
        
        // Start background cleanup task
        self.start_cleanup_task();
        
        // Start resource manager cleanup task
        Arc::clone(&self.resource_manager).start_cleanup_task();
        
        self.accept_connections().await
    }

    /// Main connection acceptance loop
    async fn accept_connections(&self) -> Result<()> {
        let listener = self.listener.as_ref()
            .ok_or_else(|| anyhow::anyhow!("Listener not initialized"))?;

        info!("Starting connection acceptance loop");
        let mut shutdown_rx = self.shutdown_tx.subscribe();
        
        loop {
            // Check shutdown flag
            if self.shutdown_flag.load(Ordering::Relaxed) {
                info!("Shutdown flag set, stopping connection acceptance");
                break;
            }

            tokio::select! {
                // Listen for new connections
                accept_result = listener.accept() => {
                    match accept_result {
                        Ok((stream, addr)) => {
                            debug!("Accepted connection from {}", addr);
                            
                            // Check if we're shutting down
                            if self.shutdown_flag.load(Ordering::Relaxed) {
                                debug!("Rejecting connection from {} due to shutdown", addr);
                                continue;
                            }

                            // Security checks: Rate limiting
                            if !self.rate_limiter.check_connection_rate(addr.ip()) {
                                warn!("Connection from {} blocked by rate limiter", addr);
                                continue;
                            }

                            // Security checks: DDoS protection
                            match self.ddos_protection.check_connection(addr.ip()) {
                                DdosDecision::Allow => {
                                    debug!("Connection from {} allowed by DDoS protection", addr);
                                }
                                DdosDecision::Block { reason, delay } => {
                                    warn!("Connection from {} blocked by DDoS protection: {} (delay: {:?})", 
                                          addr, reason, delay);
                                    
                                    // Apply delay if configured
                                    if delay > Duration::from_millis(0) {
                                        tokio::time::sleep(delay).await;
                                    }
                                    continue;
                                }
                            }

                            // Security checks: Fail2Ban
                            match self.fail2ban_manager.check_auth_attempt(addr.ip()) {
                                Fail2BanDecision::Allow => {
                                    debug!("Connection from {} allowed by fail2ban", addr);
                                }
                                Fail2BanDecision::Block { reason, delay, .. } => {
                                    warn!("Connection from {} blocked by fail2ban: {}", addr, reason);
                                    
                                    // Apply delay if configured
                                    if delay > Duration::from_millis(0) {
                                        tokio::time::sleep(delay).await;
                                    }
                                    continue;
                                }
                                Fail2BanDecision::Delay { delay, reason } => {
                                    debug!("Applying delay for connection from {}: {} ({:?})", 
                                           addr, reason, delay);
                                    tokio::time::sleep(delay).await;
                                }
                            }
                            
                            // Try to acquire a connection slot from resource manager
                            let connection_slot = match self.resource_manager.acquire_connection_slot().await {
                                Ok(slot) => slot,
                                Err(_) => {
                                    warn!("Connection limit reached, rejecting connection from {}", addr);
                                    // Connection will be dropped automatically
                                    continue;
                                }
                            };

                            // Generate unique connection ID
                            let connection_id = format!("conn_{}", 
                                self.next_connection_id.fetch_add(1, Ordering::Relaxed));
                            
                            // Create connection info
                            let conn_info = ConnectionInfo {
                                id: connection_id.clone(),
                                addr,
                                start_time: Instant::now(),
                            };

                            // Spawn task to handle the connection
                            let config = Arc::clone(&self.config);
                            let auth_manager = Arc::clone(&self.auth_manager);
                            let ddos_protection = Arc::clone(&self.ddos_protection);
                            let fail2ban_manager = Arc::clone(&self.fail2ban_manager);
                            let active_connections = Arc::clone(&self.active_connections);
                            let connection_tracker = Arc::clone(&self.connection_tracker);
                            let shutdown_flag = Arc::clone(&self.shutdown_flag);
                            let shutdown_rx = self.shutdown_tx.subscribe();
                            
                            tokio::spawn(async move {
                                // Keep the connection slot alive for the duration of the connection
                                let _connection_slot = connection_slot;
                                
                                // Record connection start for DDoS tracking
                                ddos_protection.connection_started(addr.ip());
                                
                                // Increment active connection count and track connection
                                active_connections.fetch_add(1, Ordering::Relaxed);
                                {
                                    let mut tracker = connection_tracker.write().await;
                                    tracker.insert(connection_id.clone(), conn_info.clone());
                                }
                                
                                info!("Started handling connection {} from {}", connection_id, addr);
                                
                                // Handle the connection with timeout and shutdown awareness
                                let handshake_timeout = config.server.handshake_timeout;
                                let result = timeout(
                                    handshake_timeout,
                                    Self::handle_connection_with_shutdown(
                                        stream, addr, config, auth_manager, fail2ban_manager.clone(),
                                        connection_id.clone(), shutdown_flag, shutdown_rx
                                    )
                                ).await;
                                
                                match result {
                                    Ok(Ok(())) => {
                                        debug!("Connection {} completed successfully", connection_id);
                                    }
                                    Ok(Err(e)) => {
                                        error!("Error handling connection {}: {}", connection_id, e);
                                    }
                                    Err(_) => {
                                        warn!("Connection {} handshake timed out after {:?}", connection_id, handshake_timeout);
                                    }
                                }
                                
                                // Clean up: remove from tracker and decrement count
                                {
                                    let mut tracker = connection_tracker.write().await;
                                    if let Some(removed_conn) = tracker.remove(&connection_id) {
                                        let duration = removed_conn.start_time.elapsed();
                                        info!("Connection {} from {} closed after {:?}", 
                                              connection_id, addr, duration);
                                    }
                                }
                                
                                // Record connection end for DDoS tracking
                                ddos_protection.connection_ended(addr.ip());
                                
                                active_connections.fetch_sub(1, Ordering::Relaxed);
                            });
                        }
                        Err(e) => {
                            error!("Error accepting connection: {}", e);
                            // Continue accepting connections even if one fails
                        }
                    }
                }
                // Listen for shutdown signal
                _ = shutdown_rx.recv() => {
                    info!("Received shutdown signal, stopping connection acceptance");
                    self.shutdown_flag.store(true, Ordering::Relaxed);
                    break;
                }
            }
        }
        
        info!("Connection acceptance loop stopped");
        Ok(())
    }

    /// Handle a single connection with shutdown awareness
    #[instrument(skip(stream, _config, auth_manager, fail2ban_manager, _shutdown_flag, shutdown_rx), fields(connection_id = %connection_id, addr = %addr))]
    async fn handle_connection_with_shutdown(
        stream: TcpStream, 
        addr: SocketAddr, 
        _config: Arc<Config>,
        auth_manager: Arc<AuthManager>,
        fail2ban_manager: Arc<Fail2BanManager>,
        connection_id: String,
        _shutdown_flag: Arc<AtomicBool>,
        mut shutdown_rx: broadcast::Receiver<()>,
    ) -> Result<()> {
        tokio::select! {
            result = Self::handle_connection_static(stream, addr, _config, auth_manager, fail2ban_manager, connection_id.clone()) => {
                result
            }
            _ = shutdown_rx.recv() => {
                info!("Connection {} received shutdown signal, closing gracefully", connection_id);
                Ok(())
            }
        }
    }

    /// Handle a single connection (static method for use in spawned tasks)
    #[instrument(skip(stream, config, auth_manager, fail2ban_manager), fields(connection_id = %connection_id, addr = %addr))]
    async fn handle_connection_static(
        stream: TcpStream, 
        addr: SocketAddr, 
        config: Arc<Config>,
        auth_manager: Arc<AuthManager>,
        fail2ban_manager: Arc<Fail2BanManager>,
        connection_id: String,
    ) -> Result<()> {
        debug!("Processing SOCKS5 connection {} from {}", connection_id, addr);
        
        // Note: TCP keepalive configuration would require additional dependencies
        // For now, we rely on OS defaults and connection timeouts
        
        let mut handler = Socks5Handler::new(stream);
        
        // Step 1: Handle SOCKS5 handshake
        let auth_method = match handler.handle_handshake().await {
            Ok(method) => {
                debug!("SOCKS5 handshake completed for {}, selected auth method: {:?}", addr, method);
                method
            }
            Err(e) => {
                error!("SOCKS5 handshake failed for {}: {}", addr, e);
                return Err(e);
            }
        };

        // Step 2: Handle authentication if required
        let auth_result = match auth_method {
            AuthMethod::NoAuth => {
                // No authentication required
                auth_manager.authenticate(AuthMethod::NoAuth, &[], addr.ip()).await?
            }
            AuthMethod::UserPass => {
                // Username/password authentication required
                debug!("Performing username/password authentication for {}", addr);
                
                let credentials = match handler.handle_userpass_auth().await {
                    Ok(creds) => creds,
                    Err(e) => {
                        error!("Failed to read username/password credentials from {}: {}", addr, e);
                        handler.send_userpass_auth_response(false).await?;
                        return Err(e);
                    }
                };

                let auth_result = auth_manager.authenticate(AuthMethod::UserPass, &credentials, addr.ip()).await?;
                
                // Send authentication response
                handler.send_userpass_auth_response(auth_result.success).await?;
                
                if !auth_result.success {
                    warn!("Authentication failed for connection from {}", addr);
                    
                    // Record authentication failure for fail2ban
                    fail2ban_manager.record_auth_failure(addr.ip());
                    
                    return Ok(()); // Close connection
                } else {
                    // Record successful authentication
                    fail2ban_manager.record_auth_success(addr.ip());
                }
                
                info!("Authentication successful for user '{}' from {}", 
                      auth_result.user_id.as_deref().unwrap_or("unknown"), addr);
                
                auth_result
            }
            AuthMethod::Unsupported => {
                warn!("Unsupported authentication method requested by {}", addr);
                return Ok(()); // Close connection
            }
        };

        // Step 3: Handle SOCKS5 request
        let command = match handler.handle_request().await {
            Ok(cmd) => {
                debug!("SOCKS5 request received from {}: {:?}", addr, cmd);
                cmd
            }
            Err(e) => {
                error!("Failed to handle SOCKS5 request from {}: {}", addr, e);
                return Err(e);
            }
        };

        // Step 4: Process the command (only CONNECT is supported for now)
        match command {
            crate::protocol::Socks5Command::Connect { addr: target_addr, port } => {
                // Create router for access control and routing decisions
                let router = Router::new(Arc::clone(&config));
                
                // Make routing decision
                let route_decision = router.route_request(
                    &target_addr, 
                    port, 
                    addr.ip(), 
                    auth_result.user_id.as_deref()
                ).await;
                
                match route_decision {
                    RouteDecision::Allow { upstream } => {
                        // Connection is allowed, proceed with establishing target connection
                        debug!("Connection to {}:{} allowed for {}", 
                               Self::target_to_string(&target_addr), port, addr);
                        
                        // Create relay engine
                        let relay_engine = RelayEngine::from_config(&config);
                        
                        // Establish connection to target (either direct or through upstream proxy)
                        let target_stream = match upstream {
                            Some(upstream_proxy) => {
                                // Connect through upstream proxy
                                debug!("Connecting to {}:{} through upstream proxy {:?}", 
                                       Self::target_to_string(&target_addr), port, upstream_proxy.addr);
                                
                                // For now, implement direct connection
                                // TODO: Implement upstream proxy chaining in future enhancement
                                match relay_engine.connect_to_target(&target_addr, port).await {
                                    Ok((stream, resolved_addr)) => {
                                        info!("Connected to target {} (resolved to {})", 
                                              Self::target_to_string(&target_addr), resolved_addr);
                                        stream
                                    }
                                    Err(e) => {
                                        error!("Failed to connect to target {}:{}: {}", 
                                               Self::target_to_string(&target_addr), port, e);
                                        
                                        // Send appropriate SOCKS5 error response
                                        let error_code = relay_engine.connection_error_to_socks5_code(&e);
                                        let response = crate::protocol::Socks5Response::error(error_code);
                                        let _ = handler.send_response(response).await;
                                        return Err(e);
                                    }
                                }
                            }
                            None => {
                                // Direct connection
                                debug!("Connecting directly to {}:{}", 
                                       Self::target_to_string(&target_addr), port);
                                
                                match relay_engine.connect_to_target(&target_addr, port).await {
                                    Ok((stream, resolved_addr)) => {
                                        info!("Connected to target {} (resolved to {})", 
                                              Self::target_to_string(&target_addr), resolved_addr);
                                        stream
                                    }
                                    Err(e) => {
                                        error!("Failed to connect to target {}:{}: {}", 
                                               Self::target_to_string(&target_addr), port, e);
                                        
                                        // Send appropriate SOCKS5 error response
                                        let error_code = relay_engine.connection_error_to_socks5_code(&e);
                                        let response = crate::protocol::Socks5Response::error(error_code);
                                        let _ = handler.send_response(response).await;
                                        return Err(e);
                                    }
                                }
                            }
                        };
                        
                        // Send success response to client
                        let response = crate::protocol::Socks5Response::success(
                            crate::protocol::TargetAddr::Ipv4(std::net::Ipv4Addr::new(0, 0, 0, 0)),
                            0
                        );
                        
                        if let Err(e) = handler.send_response(response).await {
                            error!("Failed to send SOCKS5 success response to {}: {}", addr, e);
                            return Err(e);
                        }
                        
                        // Get the client stream back from the handler
                        let client_stream = handler.into_stream();
                        
                        // Start complete data relay with bidirectional transfer
                        info!("Starting complete data relay for connection {} from {} to {}:{}", 
                              connection_id, addr, Self::target_to_string(&target_addr), port);
                        
                        // Start the complete relay session with immediate data transfer
                        match relay_engine.start_complete_relay_with_user(
                            client_stream,
                            target_stream,
                            auth_result.user_id.clone()
                        ).await {
                            Ok(stats) => {
                                info!("SOCKS5 connection {} relay completed successfully: {} bytes up, {} bytes down in {:?}", 
                                      connection_id, stats.bytes_up, stats.bytes_down, 
                                      std::time::Duration::from_millis(stats.duration_ms));
                            }
                            Err(e) => {
                                error!("SOCKS5 connection {} relay failed: {}", connection_id, e);
                                return Err(e);
                            }
                        }
                    }
                    RouteDecision::Block { reason } => {
                        warn!("Connection to {}:{} blocked for {}: {}", 
                              Self::target_to_string(&target_addr), port, addr, reason);
                        
                        // Send connection not allowed response
                        let response = crate::protocol::Socks5Response::error(
                            crate::protocol::constants::SOCKS5_REPLY_CONNECTION_NOT_ALLOWED
                        );
                        let _ = handler.send_response(response).await;
                        return Ok(());
                    }
                    RouteDecision::Redirect { target: redirect_addr } => {
                        info!("Connection to {}:{} redirected to {} for {}", 
                              Self::target_to_string(&target_addr), port, redirect_addr, addr);
                        
                        // For redirect, we would need to establish connection to redirect target
                        // For now, treat as block
                        let response = crate::protocol::Socks5Response::error(
                            crate::protocol::constants::SOCKS5_REPLY_GENERAL_FAILURE
                        );
                        let _ = handler.send_response(response).await;
                        return Ok(());
                    }
                }
            }
            crate::protocol::Socks5Command::Bind { addr: bind_addr, port: bind_port } => {
                info!("BIND command requested by {} for {}:{}", addr, 
                      Self::target_to_string(&bind_addr), bind_port);
                
                // Create router for access control
                let router = Router::new(Arc::clone(&config));
                
                // Check if BIND is allowed
                let route_decision = router.route_request(
                    &bind_addr, 
                    bind_port, 
                    addr.ip(), 
                    auth_result.user_id.as_deref()
                ).await;
                
                match route_decision {
                    RouteDecision::Allow { .. } => {
                        // Implement BIND command
                        match Self::handle_bind_command(&bind_addr, bind_port, &mut handler).await {
                            Ok(()) => {
                                info!("BIND command completed successfully for {}", addr);
                            }
                            Err(e) => {
                                error!("BIND command failed for {}: {}", addr, e);
                                let response = crate::protocol::Socks5Response::error(
                                    crate::protocol::constants::SOCKS5_REPLY_GENERAL_FAILURE
                                );
                                let _ = handler.send_response(response).await;
                                return Err(e);
                            }
                        }
                    }
                    RouteDecision::Block { reason } => {
                        warn!("BIND to {}:{} blocked for {}: {}", 
                              Self::target_to_string(&bind_addr), bind_port, addr, reason);
                        let response = crate::protocol::Socks5Response::error(
                            crate::protocol::constants::SOCKS5_REPLY_CONNECTION_NOT_ALLOWED
                        );
                        let _ = handler.send_response(response).await;
                        return Ok(());
                    }
                    RouteDecision::Redirect { .. } => {
                        warn!("BIND redirect not supported for {}", addr);
                        let response = crate::protocol::Socks5Response::error(
                            crate::protocol::constants::SOCKS5_REPLY_GENERAL_FAILURE
                        );
                        let _ = handler.send_response(response).await;
                        return Ok(());
                    }
                }
            }
            crate::protocol::Socks5Command::UdpAssociate { addr: udp_addr, port: udp_port } => {
                info!("UDP ASSOCIATE command requested by {} for {}:{}", addr, 
                      Self::target_to_string(&udp_addr), udp_port);
                
                // Create router for access control
                let router = Router::new(Arc::clone(&config));
                
                // Check if UDP ASSOCIATE is allowed
                let route_decision = router.route_request(
                    &udp_addr, 
                    udp_port, 
                    addr.ip(), 
                    auth_result.user_id.as_deref()
                ).await;
                
                match route_decision {
                    RouteDecision::Allow { .. } => {
                        // Implement UDP ASSOCIATE command
                        match Self::handle_udp_associate_command(&udp_addr, udp_port, &mut handler).await {
                            Ok(()) => {
                                info!("UDP ASSOCIATE command completed successfully for {}", addr);
                            }
                            Err(e) => {
                                error!("UDP ASSOCIATE command failed for {}: {}", addr, e);
                                let response = crate::protocol::Socks5Response::error(
                                    crate::protocol::constants::SOCKS5_REPLY_GENERAL_FAILURE
                                );
                                let _ = handler.send_response(response).await;
                                return Err(e);
                            }
                        }
                    }
                    RouteDecision::Block { reason } => {
                        warn!("UDP ASSOCIATE to {}:{} blocked for {}: {}", 
                              Self::target_to_string(&udp_addr), udp_port, addr, reason);
                        let response = crate::protocol::Socks5Response::error(
                            crate::protocol::constants::SOCKS5_REPLY_CONNECTION_NOT_ALLOWED
                        );
                        let _ = handler.send_response(response).await;
                        return Ok(());
                    }
                    RouteDecision::Redirect { .. } => {
                        warn!("UDP ASSOCIATE redirect not supported for {}", addr);
                        let response = crate::protocol::Socks5Response::error(
                            crate::protocol::constants::SOCKS5_REPLY_GENERAL_FAILURE
                        );
                        let _ = handler.send_response(response).await;
                        return Ok(());
                    }
                }
            }
        }

        // Update session activity before closing
        if !auth_result.session_id.is_empty() {
            auth_manager.update_session_activity(&auth_result.session_id);
        }

        info!("SOCKS5 connection {} from {} completed successfully (user: {}, session: {})", 
              connection_id, addr, 
              auth_result.user_id.as_deref().unwrap_or("anonymous"),
              auth_result.session_id);
        
        Ok(())
    }

    /// Convert TargetAddr to string for logging
    fn target_to_string(target: &crate::protocol::TargetAddr) -> String {
        match target {
            crate::protocol::TargetAddr::Ipv4(ip) => ip.to_string(),
            crate::protocol::TargetAddr::Ipv6(ip) => ip.to_string(),
            crate::protocol::TargetAddr::Domain(domain) => domain.clone(),
        }
    }

    /// Handle SOCKS5 BIND command
    async fn handle_bind_command(
        bind_addr: &crate::protocol::TargetAddr,
        _bind_port: u16,
        handler: &mut crate::protocol::Socks5Handler,
    ) -> Result<()> {
        use tokio::net::TcpListener;
        use std::net::IpAddr;
        use anyhow::Context;
        
        // For BIND, we need to create a listening socket and wait for incoming connections
        // The bind address from the client is usually 0.0.0.0:0 to let the server choose
        
        // Create a listener on an available port
        let listener = match bind_addr {
            crate::protocol::TargetAddr::Ipv4(_) => {
                // Bind to IPv4
                TcpListener::bind("0.0.0.0:0").await
                    .context("Failed to bind IPv4 listener")?
            }
            crate::protocol::TargetAddr::Ipv6(_) => {
                // Bind to IPv6
                TcpListener::bind("[::]:0").await
                    .context("Failed to bind IPv6 listener")?
            }
            crate::protocol::TargetAddr::Domain(_) => {
                // For domain names, default to IPv4
                TcpListener::bind("0.0.0.0:0").await
                    .context("Failed to bind listener for domain")?
            }
        };
        
        let local_addr = listener.local_addr()
            .context("Failed to get listener local address")?;
        
        info!("BIND listener created on {}", local_addr);
        
        // Send success response with the bound address
        let bind_response_addr = match local_addr.ip() {
            IpAddr::V4(ipv4) => crate::protocol::TargetAddr::Ipv4(ipv4),
            IpAddr::V6(ipv6) => crate::protocol::TargetAddr::Ipv6(ipv6),
        };
        
        let response = crate::protocol::Socks5Response::success(
            bind_response_addr,
            local_addr.port()
        );
        
        handler.send_response(response).await
            .context("Failed to send BIND response")?;
        
        // Wait for incoming connection (with timeout)
        let timeout_duration = std::time::Duration::from_secs(300); // 5 minutes
        
        match tokio::time::timeout(timeout_duration, listener.accept()).await {
            Ok(Ok((incoming_stream, incoming_addr))) => {
                info!("BIND received incoming connection from {}", incoming_addr);
                
                // Send second response indicating successful connection
                let incoming_response_addr = match incoming_addr.ip() {
                    IpAddr::V4(ipv4) => crate::protocol::TargetAddr::Ipv4(ipv4),
                    IpAddr::V6(ipv6) => crate::protocol::TargetAddr::Ipv6(ipv6),
                };
                
                let connection_response = crate::protocol::Socks5Response::success(
                    incoming_response_addr,
                    incoming_addr.port()
                );
                
                handler.send_response(connection_response).await
                    .context("Failed to send BIND connection response")?;
                
                // Now we would typically relay data between the client and the incoming connection
                // For now, we'll just close the incoming connection
                drop(incoming_stream);
                
                info!("BIND command completed successfully");
                Ok(())
            }
            Ok(Err(e)) => {
                error!("BIND listener accept failed: {}", e);
                Err(anyhow::anyhow!("BIND accept failed: {}", e))
            }
            Err(_) => {
                warn!("BIND listener timed out waiting for incoming connection");
                Err(anyhow::anyhow!("BIND timeout"))
            }
        }
    }

    /// Handle SOCKS5 UDP ASSOCIATE command
    async fn handle_udp_associate_command(
        udp_addr: &crate::protocol::TargetAddr,
        _udp_port: u16,
        handler: &mut crate::protocol::Socks5Handler,
    ) -> Result<()> {
        use tokio::net::UdpSocket;
        use std::net::IpAddr;
        use anyhow::Context;
        
        // For UDP ASSOCIATE, we need to create a UDP socket for relaying UDP packets
        // The client will send UDP packets to this socket, and we'll relay them to the target
        
        // Create a UDP socket on an available port
        let socket = match udp_addr {
            crate::protocol::TargetAddr::Ipv4(_) => {
                // Bind to IPv4
                UdpSocket::bind("0.0.0.0:0").await
                    .context("Failed to bind IPv4 UDP socket")?
            }
            crate::protocol::TargetAddr::Ipv6(_) => {
                // Bind to IPv6
                UdpSocket::bind("[::]:0").await
                    .context("Failed to bind IPv6 UDP socket")?
            }
            crate::protocol::TargetAddr::Domain(_) => {
                // For domain names, default to IPv4
                UdpSocket::bind("0.0.0.0:0").await
                    .context("Failed to bind UDP socket for domain")?
            }
        };
        
        let local_addr = socket.local_addr()
            .context("Failed to get UDP socket local address")?;
        
        info!("UDP ASSOCIATE socket created on {}", local_addr);
        
        // Send success response with the UDP relay address
        let udp_response_addr = match local_addr.ip() {
            IpAddr::V4(ipv4) => crate::protocol::TargetAddr::Ipv4(ipv4),
            IpAddr::V6(ipv6) => crate::protocol::TargetAddr::Ipv6(ipv6),
        };
        
        let response = crate::protocol::Socks5Response::success(
            udp_response_addr,
            local_addr.port()
        );
        
        handler.send_response(response).await
            .context("Failed to send UDP ASSOCIATE response")?;
        
        // Keep the TCP connection alive and relay UDP packets
        // For a full implementation, we would:
        // 1. Keep the TCP connection open (client closes it to end UDP association)
        // 2. Listen for UDP packets on the socket
        // 3. Parse SOCKS5 UDP request format
        // 4. Relay packets to/from the target
        
        // For now, we'll just keep the connection alive for a short time
        info!("UDP ASSOCIATE established, keeping connection alive");
        
        // Wait for the client to close the TCP connection or timeout
        let timeout_duration = std::time::Duration::from_secs(300); // 5 minutes
        
        // In a real implementation, we would monitor the TCP connection and relay UDP packets
        // For now, we'll just wait and then close
        tokio::time::sleep(timeout_duration).await;
        
        info!("UDP ASSOCIATE command completed");
        Ok(())
    }

    /// Get the number of active connections
    pub fn get_active_connections(&self) -> usize {
        self.active_connections.load(Ordering::Relaxed)
    }

    /// Get the bind address if listener is initialized
    pub fn get_bind_addr(&self) -> Option<SocketAddr> {
        self.listener.as_ref()
            .and_then(|listener| listener.local_addr().ok())
    }

    /// Get information about all active connections
    pub async fn get_active_connection_info(&self) -> Vec<ConnectionInfo> {
        let tracker = self.connection_tracker.read().await;
        tracker.values().cloned().collect()
    }

    /// Get connection statistics
    pub async fn get_connection_stats(&self) -> ConnectionStats {
        let tracker = self.connection_tracker.read().await;
        let active_count = tracker.len();
        let total_connections = self.next_connection_id.load(Ordering::Relaxed).saturating_sub(1);
        
        ConnectionStats {
            active_connections: active_count,
            total_connections_served: total_connections,
            max_connections_allowed: self.config.server.max_connections,
        }
    }

    /// Get authentication statistics
    pub fn get_auth_stats(&self) -> crate::auth::AuthStats {
        self.auth_manager.get_stats()
    }

    /// Get rate limiter statistics
    pub fn get_rate_limiter_stats(&self) -> crate::security::rate_limiter::RateLimiterStats {
        self.rate_limiter.get_stats()
    }

    /// Get DDoS protection statistics
    pub fn get_ddos_stats(&self) -> crate::security::ddos_protection::DdosStats {
        self.ddos_protection.get_stats()
    }

    /// Get fail2ban statistics
    pub fn get_fail2ban_stats(&self) -> crate::security::fail2ban::Fail2BanStats {
        self.fail2ban_manager.get_stats()
    }

    /// Get security managers for external access
    pub fn rate_limiter(&self) -> &Arc<RateLimiter> {
        &self.rate_limiter
    }

    pub fn ddos_protection(&self) -> &Arc<DdosProtection> {
        &self.ddos_protection
    }

    pub fn fail2ban_manager(&self) -> &Arc<Fail2BanManager> {
        &self.fail2ban_manager
    }

    /// Force cleanup of expired sessions and rate limits
    pub fn cleanup_auth_data(&self) {
        self.auth_manager.cleanup_expired();
    }

    /// Initiate graceful shutdown
    pub fn initiate_shutdown(&self) {
        info!("Initiating graceful shutdown of connection manager");
        self.shutdown_flag.store(true, Ordering::Relaxed);
        
        // Send shutdown signal to all connection handlers
        if let Err(e) = self.shutdown_tx.send(()) {
            warn!("Failed to send shutdown signal to connection handlers: {}", e);
        }
    }

    /// Get a shutdown receiver for external components
    pub fn subscribe_shutdown(&self) -> broadcast::Receiver<()> {
        self.shutdown_tx.subscribe()
    }

    /// Check if shutdown has been initiated
    pub fn is_shutting_down(&self) -> bool {
        self.shutdown_flag.load(Ordering::Relaxed)
    }

    /// Wait for all connections to close gracefully
    pub async fn wait_for_connections_to_close(&self) -> Result<()> {
        let shutdown_timeout = self.config.server.shutdown_timeout;
        let start_time = Instant::now();
        
        info!("Waiting for {} active connections to close (timeout: {:?})", 
              self.get_active_connections(), shutdown_timeout);
        
        while self.get_active_connections() > 0 && start_time.elapsed() < shutdown_timeout {
            debug!("Waiting for {} active connections to close", self.get_active_connections());
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        
        let remaining = self.get_active_connections();
        let elapsed = start_time.elapsed();
        
        if remaining == 0 {
            info!("All connections closed gracefully in {:?}", elapsed);
        } else {
            warn!("Shutdown timeout reached after {:?} with {} connections still active", 
                  elapsed, remaining);
        }
        
        Ok(())
    }



    /// Gracefully shutdown the connection manager
    pub async fn shutdown(&self) -> Result<()> {
        self.initiate_shutdown();
        self.wait_for_connections_to_close().await
    }
}

/// Connection statistics
#[derive(Debug, Clone)]
pub struct ConnectionStats {
    pub active_connections: usize,
    pub total_connections_served: usize,
    pub max_connections_allowed: usize,
}