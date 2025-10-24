//! Graceful Shutdown Handling
//! 
//! This module provides utilities for handling graceful shutdown of the SOCKS5 proxy server.
//! It supports SIGTERM and SIGINT signals and ensures active connections are closed cleanly.

use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, Notify};
use tokio::signal;
use tracing::{info, warn, error, debug};
use crate::connection::ConnectionManager;
use crate::Result;

/// Shutdown coordinator that manages graceful shutdown process
pub struct ShutdownCoordinator {
    /// Broadcast sender for shutdown signal
    shutdown_tx: broadcast::Sender<()>,
    /// Notification for shutdown completion
    shutdown_complete: Arc<Notify>,
    /// Shutdown timeout duration
    timeout: Duration,
}

impl ShutdownCoordinator {
    /// Create a new shutdown coordinator
    pub fn new(timeout: Duration) -> Self {
        let (shutdown_tx, _) = broadcast::channel(1);
        let shutdown_complete = Arc::new(Notify::new());
        
        Self {
            shutdown_tx,
            shutdown_complete,
            timeout,
        }
    }

    /// Get a shutdown receiver for components to listen for shutdown signals
    pub fn subscribe(&self) -> broadcast::Receiver<()> {
        self.shutdown_tx.subscribe()
    }

    /// Get a handle to wait for shutdown completion
    pub fn completion_handle(&self) -> Arc<Notify> {
        Arc::clone(&self.shutdown_complete)
    }

    /// Start listening for shutdown signals (SIGTERM, SIGINT)
    pub async fn listen_for_signals(&self) -> Result<()> {
        info!("Starting shutdown signal listener");
        
        #[cfg(unix)]
        {
            let mut sigterm = signal::unix::signal(signal::unix::SignalKind::terminate())?;
            let mut sigint = signal::unix::signal(signal::unix::SignalKind::interrupt())?;
            
            tokio::select! {
                _ = sigterm.recv() => {
                    info!("Received SIGTERM, initiating graceful shutdown");
                }
                _ = sigint.recv() => {
                    info!("Received SIGINT, initiating graceful shutdown");
                }
                _ = signal::ctrl_c() => {
                    info!("Received Ctrl+C, initiating graceful shutdown");
                }
            }
        }
        
        #[cfg(windows)]
        {
            signal::ctrl_c().await?;
            info!("Received Ctrl+C, initiating graceful shutdown");
        }
        
        // Send shutdown signal to all components
        if let Err(e) = self.shutdown_tx.send(()) {
            warn!("Failed to send shutdown signal: {}", e);
        }
        
        Ok(())
    }

    /// Perform graceful shutdown of the connection manager
    pub async fn shutdown_connection_manager(&self, connection_manager: &ConnectionManager) -> Result<()> {
        info!("Initiating graceful shutdown of connection manager");
        let start_time = Instant::now();
        
        // First, stop accepting new connections by shutting down the listener
        // This is handled by dropping the listener in the connection manager
        
        // Wait for active connections to finish
        let mut last_count = connection_manager.get_active_connections();
        info!("Waiting for {} active connections to close (timeout: {:?})", last_count, self.timeout);
        
        while last_count > 0 && start_time.elapsed() < self.timeout {
            tokio::time::sleep(Duration::from_millis(500)).await;
            
            let current_count = connection_manager.get_active_connections();
            if current_count != last_count {
                debug!("Active connections: {} -> {}", last_count, current_count);
                last_count = current_count;
            }
        }
        
        let final_count = connection_manager.get_active_connections();
        let elapsed = start_time.elapsed();
        
        if final_count == 0 {
            info!("All connections closed gracefully in {:?}", elapsed);
        } else {
            warn!("Shutdown timeout reached after {:?} with {} connections still active", 
                  elapsed, final_count);
        }
        
        // Perform final cleanup
        connection_manager.cleanup_auth_data();
        
        // Notify that shutdown is complete
        self.shutdown_complete.notify_waiters();
        
        Ok(())
    }

    /// Wait for shutdown completion with timeout
    pub async fn wait_for_completion(&self) -> Result<()> {
        tokio::time::timeout(
            self.timeout + Duration::from_secs(5), // Extra buffer for cleanup
            self.shutdown_complete.notified()
        ).await
        .map_err(|_| anyhow::anyhow!("Shutdown completion timeout"))?;
        
        Ok(())
    }
}

/// Shutdown-aware task handle that can be gracefully cancelled
pub struct ShutdownAwareTask {
    handle: tokio::task::JoinHandle<()>,
    shutdown_rx: broadcast::Receiver<()>,
}

impl ShutdownAwareTask {
    /// Create a new shutdown-aware task
    pub fn spawn<F, Fut>(
        shutdown_coordinator: &ShutdownCoordinator,
        task_name: &str,
        task_fn: F,
    ) -> Self
    where
        F: FnOnce(broadcast::Receiver<()>) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = ()> + Send,
    {
        let shutdown_rx = shutdown_coordinator.subscribe();
        let task_name = task_name.to_string();
        
        let handle = tokio::spawn(async move {
            debug!("Starting shutdown-aware task: {}", task_name);
            task_fn(shutdown_rx).await;
            debug!("Shutdown-aware task completed: {}", task_name);
        });
        
        Self {
            handle,
            shutdown_rx: shutdown_coordinator.subscribe(),
        }
    }

    /// Wait for the task to complete or shutdown signal
    pub async fn wait_for_completion_or_shutdown(mut self) -> Result<()> {
        tokio::select! {
            result = &mut self.handle => {
                match result {
                    Ok(()) => {
                        debug!("Task completed successfully");
                        Ok(())
                    }
                    Err(e) if e.is_cancelled() => {
                        debug!("Task was cancelled");
                        Ok(())
                    }
                    Err(e) => {
                        error!("Task failed: {}", e);
                        Err(anyhow::anyhow!("Task failed: {}", e))
                    }
                }
            }
            _ = self.shutdown_rx.recv() => {
                debug!("Received shutdown signal, cancelling task");
                self.handle.abort();
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::sleep;

    #[tokio::test]
    async fn test_shutdown_coordinator_creation() {
        let coordinator = ShutdownCoordinator::new(Duration::from_secs(5));
        let _receiver = coordinator.subscribe();
        let _completion = coordinator.completion_handle();
        
        // Should not panic
    }

    #[tokio::test]
    async fn test_shutdown_signal_broadcast() {
        let coordinator = ShutdownCoordinator::new(Duration::from_secs(5));
        let mut receiver = coordinator.subscribe();
        
        // Send shutdown signal
        coordinator.shutdown_tx.send(()).unwrap();
        
        // Should receive the signal
        assert!(receiver.recv().await.is_ok());
    }

    #[tokio::test]
    async fn test_shutdown_aware_task() {
        let coordinator = ShutdownCoordinator::new(Duration::from_secs(5));
        
        let task = ShutdownAwareTask::spawn(&coordinator, "test_task", |mut shutdown_rx| async move {
            tokio::select! {
                _ = sleep(Duration::from_secs(10)) => {
                    // Should not reach here due to shutdown
                }
                _ = shutdown_rx.recv() => {
                    // Should receive shutdown signal
                }
            }
        });
        
        // Send shutdown signal after a short delay
        tokio::spawn(async move {
            sleep(Duration::from_millis(100)).await;
            coordinator.shutdown_tx.send(()).unwrap();
        });
        
        // Task should complete due to shutdown signal
        assert!(task.wait_for_completion_or_shutdown().await.is_ok());
    }
}