# 🚀 RustProxy - User Manual

**A High-Performance SOCKS5 Proxy Server**

*Created by [Your Name] - Professional Network Solutions*

---

## 📋 Table of Contents

1. [What is RustProxy?](#what-is-rustproxy)
2. [Quick Start Guide](#quick-start-guide)
3. [Installation](#installation)
4. [Basic Configuration](#basic-configuration)
5. [Running the Proxy](#running-the-proxy)
6. [Using the Proxy](#using-the-proxy)
7. [Security Features](#security-features)
8. [Troubleshooting](#troubleshooting)
9. [Advanced Features](#advanced-features)
10. [Support](#support)

---

## 🎯 What is RustProxy?

RustProxy is a professional-grade SOCKS5 proxy server built with Rust for maximum performance and security. It allows you to:

- **Route internet traffic** through a secure proxy server
- **Protect your privacy** by hiding your real IP address
- **Bypass network restrictions** and access blocked content
- **Secure your connection** with enterprise-grade authentication
- **Monitor and control** network access with advanced features

### Key Features ✨

- ✅ **Complete SOCKS5 Support** - All commands (CONNECT, BIND, UDP ASSOCIATE)
- ✅ **User Authentication** - Secure username/password protection
- ✅ **Access Control** - Block or allow specific websites/IPs
- ✅ **Rate Limiting** - Prevent abuse and overuse
- ✅ **DDoS Protection** - Built-in security against attacks
- ✅ **Real-time Monitoring** - Track connections and usage
- ✅ **Easy Configuration** - Simple TOML configuration files

---

## 🚀 Quick Start Guide

### For Complete Beginners

**Step 1:** Download RustProxy
**Step 2:** Create a simple configuration file
**Step 3:** Run the proxy server
**Step 4:** Configure your browser or application to use the proxy

Let's walk through each step!

---

## 💾 Installation

### Option 1: Download Pre-built Binary (Easiest)

1. **Download** the latest release from the releases page
2. **Extract** the files to a folder (e.g., `C:\RustProxy\`)
3. **Done!** You now have `rustproxy.exe` ready to use

### Option 2: Build from Source (Advanced)

If you have Rust installed:

```bash
# Clone the repository
git clone https://github.com/yourusername/rustproxy.git
cd rustproxy

# Build the project
cargo build --release

# The binary will be in target/release/rustproxy.exe
```

---

## ⚙️ Basic Configuration

### Creating Your First Config File

Create a file called `config.toml` in the same folder as your `rustproxy.exe`:

```toml
# Basic RustProxy Configuration
# Save this as 'config.toml'

[server]
bind_addr = "127.0.0.1:1080"        # Where the proxy listens
max_connections = 100                # Maximum simultaneous connections
connection_timeout = "30s"          # How long to wait for connections
buffer_size = 8192                  # Data buffer size
max_memory_mb = 256                 # Maximum memory usage
connection_pool_size = 10           # Connection pool size
enable_keepalive = true             # Keep connections alive
keepalive_interval = "30s"          # Keepalive check interval
handshake_timeout = "10s"           # SOCKS5 handshake timeout
idle_timeout = "300s"               # Idle connection timeout
shutdown_timeout = "30s"            # Graceful shutdown timeout

[auth]
enabled = true                      # Enable user authentication
method = "userpass"                 # Use username/password

# Add users who can connect
[[auth.users]]
username = "myuser"                 # Change this to your username
password = "mypassword"             # Change this to your password
enabled = true

[[auth.users]]
username = "friend"                 # Add more users as needed
password = "friendpass"
enabled = true

[access_control]
enabled = true                      # Enable access control
default_policy = "allow"            # Allow all by default

# Block specific websites (optional)
[[access_control.rules]]
pattern = "*.badsite.com"           # Block this domain
action = "block"
reason = "Blocked website"

[routing]
enabled = false                     # Keep simple for beginners
upstream_proxies = []
rules = []

[routing.smart_routing]
enabled = false
health_check_interval = "30s"
health_check_timeout = "5s"
min_measurements = 3
enable_latency_routing = true
enable_health_routing = true

[security.rate_limiting]
enabled = true                      # Prevent abuse
connections_per_ip_per_minute = 60
connections_per_ip_burst = 10
auth_attempts_per_ip_per_minute = 10
auth_attempts_per_ip_burst = 3
global_connections_per_second = 1000
cleanup_interval_seconds = 300
block_duration_minutes = 15

[security.ddos_protection]
enabled = true                      # DDoS protection
connection_threshold = 50
time_window_seconds = 60
block_duration_minutes = 30
max_connections_per_ip = 10
global_connection_threshold = 5000
enable_progressive_delays = true
base_delay_ms = 100
max_delay_ms = 5000
cleanup_interval_seconds = 300

[security.fail2ban]
enabled = true                      # Block failed login attempts
max_auth_failures = 5
failure_window_minutes = 10
ban_duration_minutes = 30
progressive_ban_multiplier = 2.0
max_ban_duration_hours = 24
enable_progressive_delays = true
base_delay_ms = 1000
max_delay_ms = 30000
whitelist_ips = ["127.0.0.1"]      # Never block localhost
cleanup_interval_seconds = 300

[security.secrets]
encrypt_config = false
use_env_secrets = true
secret_key_env = "RUSTPROXY_SECRET_KEY"
config_encryption_key_env = "RUSTPROXY_CONFIG_KEY"

[monitoring]
enabled = true                      # Enable monitoring
metrics_addr = "127.0.0.1:9090"    # Metrics server address
log_level = "info"                  # Log level (trace, debug, info, warn, error)
prometheus_enabled = false          # Disable for beginners
collect_connection_stats = true
max_historical_connections = 1000

[monitoring.management_api]
enabled = false                     # Disable management API for beginners
bind_addr = "127.0.0.1:8080"

[monitoring.management_api.auth]
enabled = false
```

### 📝 Configuration Explained

- **bind_addr**: The IP and port where your proxy listens (127.0.0.1:1080 means localhost port 1080)
- **auth.users**: List of usernames and passwords that can connect
- **access_control.rules**: Block or allow specific websites
- **security settings**: Protect against attacks and abuse

---

## 🏃 Running the Proxy

### Windows

1. **Open Command Prompt** (Press `Win + R`, type `cmd`, press Enter)
2. **Navigate** to your RustProxy folder:
   ```cmd
   cd C:\RustProxy
   ```
3. **Run the proxy**:
   ```cmd
   rustproxy.exe --config config.toml
   ```

### Success Message

You should see something like:
```
[INFO] Starting RustProxy Server v1.0.0
[INFO] Configuration loaded successfully
[INFO] Bind address: 127.0.0.1:1080
[INFO] Authentication: enabled
[INFO] RustProxy Server started successfully
[INFO] Press Ctrl+C to shutdown gracefully
```

### Validate Configuration (Optional)

Test your config file without starting the server:
```cmd
rustproxy.exe --config config.toml --validate-config
```

---

## 🌐 Using the Proxy

### Method 1: Configure Your Browser

#### Chrome/Edge:
1. Go to **Settings** → **Advanced** → **System**
2. Click **"Open your computer's proxy settings"**
3. Enable **"Use a proxy server"**
4. Set **Address**: `127.0.0.1` **Port**: `1080`
5. Check **"Use the same proxy server for all protocols"**

#### Firefox:
1. Go to **Settings** → **Network Settings**
2. Select **"Manual proxy configuration"**
3. Set **SOCKS Host**: `127.0.0.1` **Port**: `1080`
4. Select **"SOCKS v5"**

### Method 2: Using Command Line Tools

#### With curl:
```bash
# Test the proxy
curl --socks5 myuser:mypassword@127.0.0.1:1080 http://httpbin.org/ip

# This should return your proxy's IP, not your real IP
```

#### With wget:
```bash
wget --proxy=socks5://myuser:mypassword@127.0.0.1:1080 http://example.com
```

### Method 3: Application-Specific

Many applications support SOCKS5 proxies:
- **Telegram**: Settings → Advanced → Connection Type → Use Custom Proxy
- **Discord**: Settings → Voice & Video → Connection → Proxy
- **Games**: Many games have proxy settings in their network options

---

## 🔒 Security Features

### Authentication
- **Username/Password**: Only authorized users can connect
- **Session Management**: Automatic session timeout and cleanup
- **Failed Login Protection**: Automatic blocking after failed attempts

### Access Control
- **Website Blocking**: Block access to specific domains
- **IP Filtering**: Allow or deny specific IP addresses
- **Time-based Rules**: Control access during specific hours (advanced)

### Protection Systems
- **Rate Limiting**: Prevents connection flooding
- **DDoS Protection**: Blocks suspicious traffic patterns
- **Fail2Ban**: Automatically blocks IPs with failed login attempts

### Monitoring
- **Connection Logging**: Track who connects and when
- **Usage Statistics**: Monitor bandwidth and connection counts
- **Real-time Alerts**: Get notified of security events

---

## 🔧 Troubleshooting

### Common Issues

#### ❌ "Connection refused" or "Can't connect"

**Problem**: Browser/application can't connect to proxy

**Solutions**:
1. **Check if RustProxy is running**:
   - Look for the success message in the command prompt
   - If not running, start it with `rustproxy.exe --config config.toml`

2. **Check the port**:
   - Make sure your browser uses port `1080` (or whatever you set in config)
   - Try `telnet 127.0.0.1 1080` to test connection

3. **Check firewall**:
   - Windows Firewall might be blocking the connection
   - Add an exception for `rustproxy.exe`

#### ❌ "Authentication failed"

**Problem**: Proxy rejects your username/password

**Solutions**:
1. **Check credentials**: Make sure username/password match your config file
2. **Check config syntax**: Ensure your `config.toml` is valid
3. **Restart proxy**: Stop and restart RustProxy after config changes

#### ❌ "Access denied" for websites

**Problem**: Proxy blocks access to certain sites

**Solutions**:
1. **Check access rules**: Look at `access_control.rules` in your config
2. **Change default policy**: Set `default_policy = "allow"` to allow all sites
3. **Remove blocking rules**: Comment out or remove restrictive rules

#### ❌ Proxy is slow

**Problem**: Connections are slow through the proxy

**Solutions**:
1. **Increase buffer size**: Set `buffer_size = 16384` or higher
2. **Increase connection limits**: Raise `max_connections`
3. **Check your internet**: Test direct connection speed
4. **Reduce logging**: Set `log_level = "warn"` to reduce overhead

### Getting Help

#### Check Logs
RustProxy shows detailed information in the command prompt. Look for:
- `[ERROR]` messages for problems
- `[WARN]` messages for potential issues
- `[INFO]` messages for normal operation

#### Test Configuration
```cmd
rustproxy.exe --config config.toml --validate-config
```

#### Test Connectivity
```cmd
# Test if proxy accepts connections
telnet 127.0.0.1 1080

# Test with curl (if installed)
curl --socks5 myuser:mypassword@127.0.0.1:1080 http://httpbin.org/ip
```

---

## 🚀 Advanced Features

### Multiple Users
Add as many users as you need:
```toml
[[auth.users]]
username = "user1"
password = "pass1"
enabled = true

[[auth.users]]
username = "user2"
password = "pass2"
enabled = true
```

### Website Blocking
Block specific websites or categories:
```toml
[[access_control.rules]]
pattern = "*.facebook.com"
action = "block"
reason = "Social media blocked"

[[access_control.rules]]
pattern = "*.gambling.com"
action = "block"
reason = "Gambling sites blocked"
```

### Time-based Access (Advanced)
```toml
# Only allow access during work hours
[[access_control.rules]]
pattern = "*"
action = "allow"
time_start = "09:00"
time_end = "17:00"
```

### Custom Ports
Change the proxy port:
```toml
[server]
bind_addr = "127.0.0.1:8080"  # Use port 8080 instead
```

### External Access
Allow connections from other computers:
```toml
[server]
bind_addr = "0.0.0.0:1080"  # Listen on all network interfaces
```
⚠️ **Warning**: Only do this on trusted networks!

---

## 📞 Support

### Documentation
- **User Manual**: This document
- **Advanced Configuration**: See `docs/ADVANCED_ROUTING.md`
- **API Documentation**: See `docs/MANAGEMENT_API.md`

### Community
- **Issues**: Report bugs and request features on GitHub
- **Discussions**: Join community discussions
- **Updates**: Check for new releases regularly

### Professional Support
For business use or custom configurations, contact [Your Name] at [your-email@domain.com]

---

## 📄 License & Credits

**RustProxy** - Created by [Your Name]

This software is provided as-is for educational and personal use. 

### Built With
- **Rust** - Systems programming language
- **Tokio** - Async runtime
- **Tracing** - Logging and diagnostics
- **Serde** - Serialization framework

---

## 🎉 Quick Reference

### Start the Proxy
```cmd
rustproxy.exe --config config.toml
```

### Test the Proxy
```cmd
curl --socks5 username:password@127.0.0.1:1080 http://httpbin.org/ip
```

### Stop the Proxy
Press `Ctrl+C` in the command prompt

### Default Settings
- **Address**: 127.0.0.1:1080
- **Protocol**: SOCKS5
- **Authentication**: Username/Password required
- **Logs**: Displayed in command prompt

---

*Thank you for using RustProxy! 🚀*

*For questions or support, contact [Your Name] - Professional Network Solutions*