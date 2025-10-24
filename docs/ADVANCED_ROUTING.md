# Advanced Routing Features

This document describes the advanced routing features implemented in the SOCKS5 proxy server.

## Overview

The advanced routing system provides three main capabilities:

1. **Custom Routing Rules Engine** - Rule-based routing with pattern matching and priority ordering
2. **Proxy Chaining Support** - Route traffic through multiple upstream proxies in sequence
3. **Smart Routing** - Intelligent routing based on latency measurements and health checks

## 1. Custom Routing Rules Engine

### Features

- **Priority-based rule evaluation** - Rules are evaluated in priority order (highest first)
- **Pattern matching** - Support for exact matches, wildcards, regex, IP/CIDR, and domain patterns
- **Multiple action types** - Allow, Block, Redirect, Proxy, and ProxyChain actions
- **Flexible filtering** - Rules can be restricted by ports, source IPs, and users
- **Runtime management** - Rules can be added, updated, and removed at runtime

### Pattern Types

- **Exact match**: `example.com`
- **Wildcard**: `*.example.com`, `*ads*`
- **Regex**: `^.*\.test\.(com|org)$`
- **IP/CIDR**: `192.168.1.0/24`, `10.0.0.1`
- **Domain suffix**: `.example.com`
- **Subdomain wildcard**: `*.example.com`

### Action Types

- **Allow** - Allow direct connection
- **Block** - Block the connection with optional reason
- **Redirect** - Redirect to a different target address
- **Proxy** - Route through a specific upstream proxy
- **ProxyChain** - Route through multiple proxies in sequence

### Configuration Example

```toml
[[routing.rules]]
id = "block_malware"
priority = 1000
pattern = "*.malware.com"
enabled = true
ports = [80, 443]

[routing.rules.action]
type = "Block"
config = { reason = "Malware domain blocked" }
```

## 2. Proxy Chaining Support

### Features

- **Multi-protocol support** - Chain SOCKS5 and HTTP proxies
- **Authentication** - Support for proxy authentication in chains
- **Configurable timeouts** - Per-proxy connection timeouts
- **Builder pattern** - Easy chain construction with fluent API

### Usage Example

```rust
use rustproxy::routing::{ProxyChainBuilder, ProxyAuth};

let chain = ProxyChainBuilder::new()
    .add_socks5_proxy(
        "127.0.0.1:1080".parse().unwrap(),
        Some(ProxyAuth {
            username: "user".to_string(),
            password: "pass".to_string(),
        })
    )
    .add_http_proxy(
        "10.0.0.1:8080".parse().unwrap(),
        None
    )
    .with_timeout(Duration::from_secs(10))
    .build()?;
```

### Configuration Example

```toml
[[routing.rules]]
id = "secure_chain"
priority = 800
pattern = "*.secure-site.com"
enabled = true

[routing.rules.action]
type = "ProxyChain"
config = { upstream_ids = ["proxy1", "proxy2", "proxy3"] }
```

## 3. Smart Routing

### Features

- **Health monitoring** - Continuous health checks for upstream proxies
- **Latency-based selection** - Route through fastest available proxy
- **Performance metrics** - Track success rates, latency, and connection counts
- **Automatic failover** - Avoid unhealthy proxies automatically
- **Configurable thresholds** - Customize health and performance criteria

### Health Status Levels

- **Healthy** - Success rate > 80%, reasonable latency
- **Degraded** - Success rate 50-80% or high latency
- **Unhealthy** - Success rate < 50%
- **Unknown** - Insufficient data for assessment

### Configuration Example

```toml
[routing.smart_routing]
enabled = true
health_check_interval = "30s"
health_check_timeout = "5s"
min_measurements = 3
enable_latency_routing = true
enable_health_routing = true
```

### Usage Example

```rust
use rustproxy::routing::{SmartRoutingManager, SmartRoutingConfig};

let config = SmartRoutingConfig {
    health_check_interval: Duration::from_secs(30),
    health_check_timeout: Duration::from_secs(5),
    min_measurements: 5,
    enable_latency_routing: true,
    enable_health_routing: true,
};

let mut manager = SmartRoutingManager::new(config);

// Add proxies to monitor
manager.add_upstream_proxy("proxy1".to_string(), proxy1).await;
manager.add_upstream_proxy("proxy2".to_string(), proxy2).await;

// Start health checking
manager.start_health_checking().await;

// Select best proxy
if let Some((id, proxy)) = manager.select_best_proxy(&[]).await {
    println!("Selected proxy: {}", id);
}
```

## Integration

### Router Integration

The advanced routing features are integrated into the main `Router` component:

```rust
use rustproxy::routing::{Router, SmartRoutingConfig};

let mut router = Router::new(config);

// Enable smart routing
router.enable_smart_routing(SmartRoutingConfig::default()).await;

// Start health checks
router.start_smart_routing_health_checks().await;

// Record connection results for learning
router.record_connection_result("proxy1", latency, success).await;
```

### Configuration File

All features can be configured through the main configuration file:

```toml
[routing]
enabled = true

# Smart routing
[routing.smart_routing]
enabled = true
health_check_interval = "30s"
health_check_timeout = "5s"
min_measurements = 3
enable_latency_routing = true
enable_health_routing = true

# Upstream proxies
[[routing.upstream_proxies]]
name = "corporate_proxy"
addr = "10.0.0.100:8080"
protocol = "socks5"

[routing.upstream_proxies.auth]
username = "corp_user"
password = "corp_pass"

# Custom routing rules
[[routing.rules]]
id = "corporate_traffic"
priority = 800
pattern = "*.company.com"
enabled = true

[routing.rules.action]
type = "Proxy"
config = { upstream_id = "corporate_proxy" }
```

## Testing

Comprehensive tests are provided for all features:

- `tests/routing_rules_test.rs` - Custom routing rules engine tests
- `tests/proxy_chaining_test.rs` - Proxy chaining functionality tests
- `tests/smart_routing_test.rs` - Smart routing and health monitoring tests

Run tests with:

```bash
cargo test routing_rules
cargo test proxy_chaining
cargo test smart_routing
```

## Performance Considerations

- **Rule evaluation** - Rules are evaluated in priority order, place most specific rules first
- **Health checks** - Adjust health check intervals based on network conditions
- **Metrics storage** - Recent measurements are limited to prevent memory growth
- **Async operations** - All operations are non-blocking and use Tokio async runtime

## Security Considerations

- **Pattern validation** - All patterns are validated before compilation
- **Resource limits** - Connection limits and timeouts prevent resource exhaustion
- **Authentication** - Proxy chain authentication is handled securely
- **Error handling** - Detailed error information is logged but not exposed to clients

## Future Enhancements

Potential future improvements include:

- **Time-based routing** - Rules that activate based on time of day or day of week
- **Load balancing** - Distribute traffic across multiple proxies
- **Circuit breakers** - Temporary proxy disabling with automatic recovery
- **Metrics export** - Export routing metrics to monitoring systems
- **Geographic routing** - Route based on client or target geographic location