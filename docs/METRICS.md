# Metrics and Monitoring

The SOCKS5 proxy server includes comprehensive metrics collection and monitoring capabilities.

## Features

### Metrics Collection
- **Connection Tracking**: Monitor active and historical connections
- **Performance Metrics**: Track bytes transferred, connection duration, and throughput
- **Authentication Metrics**: Monitor authentication attempts and success rates
- **Access Control Metrics**: Track blocked requests and ACL rule violations

### Prometheus Integration
- **Metrics Export**: Prometheus-compatible metrics endpoint
- **Standard Metrics**: Counters, gauges, and histograms
- **Custom Labels**: Support for user-specific and destination-specific metrics

### Reporting and Analytics
- **Usage Reports**: Daily, weekly, and monthly usage reports
- **Connection Insights**: Automated analysis and recommendations
- **Real-time Statistics**: Current activity and performance metrics
- **Export Formats**: JSON and CSV export support

## Configuration

Configure metrics in your `config.toml`:

```toml
[monitoring]
enabled = true
metrics_addr = "127.0.0.1:9090"
log_level = "info"
prometheus_enabled = true
collect_connection_stats = true
max_historical_connections = 10000
```

### Configuration Options

- `enabled`: Enable/disable metrics collection
- `metrics_addr`: Address for Prometheus metrics endpoint
- `log_level`: Logging level for metrics system
- `prometheus_enabled`: Enable Prometheus metrics export
- `collect_connection_stats`: Enable detailed connection statistics
- `max_historical_connections`: Maximum number of historical connections to store

## Prometheus Metrics

The following metrics are exported at `/metrics`:

### Connection Metrics
- `socks5_connections_total`: Total number of SOCKS5 connections
- `socks5_active_connections`: Number of currently active connections
- `socks5_connection_duration_seconds`: Connection duration histogram

### Data Transfer Metrics
- `socks5_bytes_transferred_total`: Total bytes transferred through the proxy

### Authentication Metrics
- `socks5_auth_attempts_total`: Total authentication attempts
- `socks5_auth_success_total`: Total successful authentications

### Access Control Metrics
- `socks5_blocked_requests_total`: Total blocked requests

## Usage Reports

### Generating Reports

```rust
use rustproxy::metrics::{MetricsManager, ConnectionInsights};

// Create metrics manager
let mut metrics_manager = MetricsManager::new(config).await?;
metrics_manager.start().await?;

// Generate reports
let insights = metrics_manager.insights();
let daily_report = insights.generate_daily_report().await?;
let weekly_report = insights.generate_weekly_report().await?;
let monthly_report = insights.generate_monthly_report().await?;
```

### Report Contents

Reports include:
- **Summary Statistics**: Total connections, bytes transferred, unique users
- **User Activity**: Top users by connection count and data usage
- **Destination Analysis**: Most accessed destinations
- **Temporal Patterns**: Hourly usage statistics and peak hours
- **Performance Metrics**: Average connection duration and success rates

### Export Formats

```rust
use rustproxy::metrics::{export_report_json, export_report_csv};

// Export to JSON
let json_report = export_report_json(&report)?;

// Export to CSV
let csv_report = export_report_csv(&report)?;
```

## Connection Insights

The system provides automated insights and recommendations:

- **Performance Optimization**: Identifies patterns that may indicate performance issues
- **Resource Usage**: Monitors resource utilization and suggests scaling recommendations
- **Security Analysis**: Detects unusual connection patterns
- **Load Balancing**: Identifies traffic concentration and suggests load distribution

### Example Insights

- "Many connections are short-lived. Consider optimizing connection setup overhead."
- "Single user accounts for >50% of traffic. Consider load balancing or rate limiting."
- "Most traffic goes to a single destination. Consider caching or direct routing."
- "High number of active connections. Monitor system resources and consider scaling."

## API Integration

### Recording Connection Events

```rust
// Start tracking a connection
metrics_manager.record_connection_start(
    session_id,
    client_addr,
    target_addr,
    user_id,
).await?;

// Update bytes transferred
metrics_manager.update_connection_bytes(
    &session_id,
    bytes_up,
    bytes_down,
).await?;

// End connection tracking
metrics_manager.record_connection_end(&session_id).await?;
```

### Recording Authentication Events

```rust
// Record authentication attempt
metrics_manager.record_auth_attempt(success);
```

### Recording Access Control Events

```rust
// Record blocked request
metrics_manager.record_blocked_request("ACL rule violation");
```

## Monitoring Setup

### Prometheus Configuration

Add the following to your `prometheus.yml`:

```yaml
scrape_configs:
  - job_name: 'socks5-proxy'
    static_configs:
      - targets: ['localhost:9090']
    scrape_interval: 15s
    metrics_path: /metrics
```

### Grafana Dashboard

Key metrics to monitor:
- Connection rate and active connections
- Data transfer rates
- Authentication success rate
- Error rates and blocked requests
- Connection duration percentiles

## Example Usage

See `examples/metrics_demo.rs` for a complete example of using the metrics system:

```bash
cargo run --example metrics_demo
```

This example demonstrates:
- Starting the metrics system
- Simulating connections and data transfer
- Generating usage reports
- Exporting metrics in various formats

## Health Checks

The metrics server also provides a health check endpoint at `/health` that returns a simple "OK" response for monitoring system availability.

## Performance Considerations

- Historical connection data is automatically pruned to prevent memory growth
- Metrics collection can be disabled for high-performance scenarios
- Prometheus metrics are efficiently stored using the prometheus crate
- Connection statistics collection can be toggled independently

## Troubleshooting

### Common Issues

1. **Metrics endpoint not accessible**: Check `metrics_addr` configuration and firewall settings
2. **High memory usage**: Reduce `max_historical_connections` or disable detailed statistics
3. **Missing metrics**: Ensure `enabled` and `prometheus_enabled` are set to true
4. **Authentication metrics not updating**: Verify authentication events are being recorded

### Debug Logging

Enable debug logging for the metrics system:

```toml
[monitoring]
log_level = "debug"
```

This will provide detailed information about metrics collection and export operations.