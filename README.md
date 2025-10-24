# 🚀 RustProxy - Professional SOCKS5 Proxy Server

**Created by Ryan M. - Professional Network Solutions**

A high-performance, enterprise-grade SOCKS5 proxy server built with Rust for maximum security, reliability, and performance.

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Rust](https://img.shields.io/badge/rust-1.70+-orange.svg)](https://www.rust-lang.org)
[![Build Status](https://img.shields.io/badge/build-passing-brightgreen.svg)](https://github.com/yourusername/rustproxy)

## ✨ Key Features

### 🔒 Security & Authentication
- **Complete SOCKS5 Protocol Support** - All commands (CONNECT, BIND, UDP ASSOCIATE)
- **Enterprise Authentication** - Secure username/password with session management
- **Advanced Access Control** - Domain/IP filtering with time-based rules
- **DDoS Protection** - Built-in protection against attacks and abuse
- **Rate Limiting** - Prevent connection flooding and resource exhaustion
- **Fail2Ban Integration** - Automatic IP blocking for failed authentication attempts

### ⚡ Performance & Reliability
- **Async/Await Architecture** - Built with Tokio for maximum concurrency
- **High-Performance Rust** - Memory-safe systems programming
- **Connection Pooling** - Efficient resource management
- **Smart Routing** - Intelligent traffic routing with health checks
- **Graceful Shutdown** - Clean connection termination

### 📊 Monitoring & Management
- **Real-time Metrics** - Comprehensive connection and performance monitoring
- **Prometheus Integration** - Industry-standard metrics collection
- **Management API** - RESTful API for configuration and monitoring
- **Structured Logging** - Detailed operational insights
- **Hot Configuration Reload** - Update settings without restart

### 🚀 Production Ready
- **Docker Support** - Containerized deployment with Docker Compose
- **Configuration Validation** - Prevent misconfigurations
- **Resource Limits** - Memory and connection management
- **Health Checks** - Built-in health monitoring
- **Scalable Architecture** - Handle thousands of concurrent connections

## Project Structure

```
src/
├── lib.rs              # Library root
├── main.rs             # CLI application entry point
├── auth/               # Authentication module
│   ├── mod.rs
│   ├── manager.rs      # Authentication manager
│   └── types.rs        # Auth-related types
├── config/             # Configuration module
│   ├── mod.rs
│   ├── manager.rs      # Configuration manager
│   └── types.rs        # Config types and structures
├── metrics/            # Metrics and monitoring
│   ├── mod.rs
│   ├── collector.rs    # Metrics collector
│   └── types.rs        # Metrics types
├── protocol/           # SOCKS5 protocol implementation
│   ├── mod.rs
│   ├── constants.rs    # Protocol constants
│   ├── handler.rs      # Protocol handler
│   └── types.rs        # Protocol types
├── relay/              # Data relay engine
│   ├── mod.rs
│   ├── engine.rs       # Relay engine
│   └── session.rs      # Relay session management
└── routing/            # Connection routing and ACL
    ├── mod.rs
    ├── router.rs       # Router implementation
    └── types.rs        # Routing types
```

## 🚀 Quick Start

### For Beginners
👉 **See the complete [User Manual](USER_MANUAL.md) for step-by-step instructions**

### For Developers

```bash
# Clone the repository
git clone https://github.com/yourusername/rustproxy.git
cd rustproxy

# Build the project
cargo build --release

# Run with default configuration
./target/release/rustproxy --config config.toml

# Validate configuration
./target/release/rustproxy --config config.toml --validate-config
```

### Quick Test

```bash
# Test the proxy with curl
curl --socks5 username:password@127.0.0.1:1080 http://httpbin.org/ip
```

## 📖 Documentation

- **[User Manual](USER_MANUAL.md)** - Complete beginner-friendly guide
- **[Advanced Configuration](docs/ADVANCED_ROUTING.md)** - Advanced routing and features
- **[Management API](docs/MANAGEMENT_API.md)** - API documentation
- **[Docker Setup](DOCKER_SETUP.md)** - Container deployment guide
- **[Metrics Guide](docs/METRICS.md)** - Monitoring and metrics

## ⚙️ Configuration

RustProxy uses TOML configuration files for easy setup:

```toml
[server]
bind_addr = "127.0.0.1:1080"
max_connections = 1000
connection_timeout = "30s"

[auth]
enabled = true
method = "userpass"

[[auth.users]]
username = "myuser"
password = "mypassword"
enabled = true

[security.rate_limiting]
enabled = true
connections_per_ip_per_minute = 60
```

See `config.toml` for a complete example configuration.

## 🐳 Docker Deployment

```bash
# Quick start with Docker Compose
docker-compose up -d

# Or build and run manually
docker build -t rustproxy .
docker run -p 1080:1080 -v $(pwd)/config.toml:/app/config.toml rustproxy
```

## 📊 Monitoring

RustProxy includes comprehensive monitoring:

- **Prometheus Metrics** - `/metrics` endpoint
- **Management API** - RESTful configuration and status API
- **Structured Logging** - JSON formatted logs
- **Health Checks** - Built-in health monitoring

## 🤝 Contributing

Contributions are welcome! Please read our contributing guidelines and submit pull requests.

## 📄 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## 👨‍💻 Author

**Ryan M.** - *Professional Network Solutions*

- GitHub: [@yourusername](https://github.com/yourusername)
- Email: peicesreeses3@gmail.com
- LinkedIn: [Your LinkedIn](https://linkedin.com/in/yourprofile)

## 🙏 Acknowledgments

- Built with [Rust](https://www.rust-lang.org/) and [Tokio](https://tokio.rs/)
- Inspired by modern proxy server architectures
- Thanks to the Rust community for excellent crates and documentation

---

**⭐ If you find RustProxy useful, please give it a star on GitHub!**