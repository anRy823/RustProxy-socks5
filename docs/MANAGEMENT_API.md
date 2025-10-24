# Management API Documentation

The SOCKS5 proxy server includes a comprehensive REST API for remote management and monitoring. This API allows administrators to configure the proxy, manage users, monitor connections, and retrieve statistics without restarting the server.

## Configuration

The management API is configured in the `[monitoring.management_api]` section of the configuration file:

```toml
[monitoring.management_api]
enabled = true
bind_addr = "127.0.0.1:8080"

[monitoring.management_api.auth]
enabled = true
api_key = "your-secure-api-key-here"
```

### Authentication Options

The management API supports multiple authentication methods:

1. **API Key Authentication** (recommended for production):
   ```toml
   [monitoring.management_api.auth]
   enabled = true
   api_key = "your-secure-api-key"
   ```

2. **Basic Authentication**:
   ```toml
   [monitoring.management_api.auth]
   enabled = true
   
   [monitoring.management_api.auth.basic_auth]
   username = "admin"
   password = "secure-password"
   ```

3. **No Authentication** (development only):
   ```toml
   [monitoring.management_api.auth]
   enabled = false
   ```

## API Endpoints

### Health and Status

#### `GET /api/v1/health`
Returns the health status of the proxy server.

**Authentication:** None required

**Response:**
```json
{
  "success": true,
  "data": {
    "status": "healthy",
    "checks": {
      "server": {
        "status": "healthy",
        "message": "Server is running",
        "duration_ms": 0
      },
      "memory": {
        "status": "healthy",
        "message": "Memory usage: 45.2%",
        "duration_ms": 1
      }
    },
    "timestamp": "2023-10-23T18:00:00Z"
  }
}
```

#### `GET /api/v1/status`
Returns detailed server status information.

**Authentication:** Required

**Response:**
```json
{
  "success": true,
  "data": {
    "uptime_seconds": 3600,
    "active_connections": 42,
    "total_connections": 1337,
    "bytes_transferred": 1048576000,
    "memory_usage_mb": 128.5,
    "cpu_usage_percent": 15.2,
    "version": "0.1.0",
    "config_last_modified": "2023-10-23T17:30:00Z"
  }
}
```

### Configuration Management

#### `GET /api/v1/config`
Retrieves the current server configuration.

**Authentication:** Required

**Response:**
```json
{
  "success": true,
  "data": {
    "server": {
      "bind_addr": "127.0.0.1:1080",
      "max_connections": 1000,
      "connection_timeout": "5m"
    },
    "auth": {
      "enabled": true,
      "method": "userpass"
    }
  }
}
```

#### `PUT /api/v1/config`
Updates the server configuration.

**Authentication:** Required

**Request Body:**
```json
{
  "config": {
    "server": {
      "bind_addr": "127.0.0.1:1080",
      "max_connections": 2000
    }
  },
  "validate_only": false
}
```

**Response:**
```json
{
  "success": true,
  "data": {
    "valid": true,
    "errors": [],
    "warnings": []
  }
}
```

#### `POST /api/v1/config/reload`
Triggers a configuration reload from the config file.

**Authentication:** Required

**Response:**
```json
{
  "success": true,
  "data": null
}
```

### User Management

#### `POST /api/v1/users`
Creates a new user account.

**Authentication:** Required

**Request Body:**
```json
{
  "username": "newuser",
  "password": "securepassword",
  "enabled": true
}
```

**Response:**
```json
{
  "success": true,
  "data": {
    "username": "newuser",
    "enabled": true,
    "created_at": "2023-10-23T18:00:00Z",
    "last_login": null,
    "connection_count": 0
  }
}
```

#### `GET /api/v1/users/{username}`
Retrieves information about a specific user.

**Authentication:** Required

**Response:**
```json
{
  "success": true,
  "data": {
    "username": "testuser",
    "enabled": true,
    "created_at": "2023-10-23T17:00:00Z",
    "last_login": "2023-10-23T17:45:00Z",
    "connection_count": 15
  }
}
```

#### `DELETE /api/v1/users/{username}`
Deletes a user account.

**Authentication:** Required

**Response:**
```json
{
  "success": true,
  "data": null
}
```

### Connection Management

#### `GET /api/v1/connections`
Lists active connections with optional pagination.

**Authentication:** Required

**Query Parameters:**
- `page` (optional): Page number (default: 1)
- `limit` (optional): Items per page (default: 50, max: 1000)

**Response:**
```json
{
  "success": true,
  "data": [
    {
      "id": "conn_123",
      "client_addr": "192.168.1.100:54321",
      "target_addr": "example.com:80",
      "user_id": "testuser",
      "start_time": "2023-10-23T17:45:00Z",
      "bytes_up": 1024,
      "bytes_down": 2048,
      "status": "active"
    }
  ]
}
```

### Statistics and Monitoring

#### `GET /api/v1/stats`
Returns comprehensive statistics summary.

**Authentication:** Required

**Response:**
```json
{
  "success": true,
  "data": {
    "total_connections": 1337,
    "active_connections": 42,
    "bytes_transferred": 1048576000,
    "auth_attempts": 1500,
    "auth_failures": 23,
    "blocked_requests": 5,
    "uptime_seconds": 3600,
    "top_destinations": [
      {
        "destination": "example.com:443",
        "connection_count": 150,
        "bytes_transferred": 52428800
      }
    ],
    "top_users": [
      {
        "username": "poweruser",
        "connection_count": 89,
        "bytes_transferred": 31457280,
        "last_activity": "2023-10-23T17:58:00Z"
      }
    ]
  }
}
```

#### `POST /api/v1/metrics/export`
Exports metrics in various formats.

**Authentication:** Required

**Request Body:**
```json
{
  "format": "prometheus",
  "include_histograms": true
}
```

**Response:** Raw metrics data in the requested format.

## Usage Examples

### Using curl

```bash
# Health check (no authentication)
curl http://127.0.0.1:8080/api/v1/health

# Get server status (with API key)
curl -H "x-api-key: your-api-key" \
     http://127.0.0.1:8080/api/v1/status

# Create a new user
curl -X POST \
     -H "x-api-key: your-api-key" \
     -H "Content-Type: application/json" \
     -d '{"username":"newuser","password":"pass123","enabled":true}' \
     http://127.0.0.1:8080/api/v1/users

# Get active connections
curl -H "x-api-key: your-api-key" \
     "http://127.0.0.1:8080/api/v1/connections?page=1&limit=10"

# Export Prometheus metrics
curl -X POST \
     -H "x-api-key: your-api-key" \
     -H "Content-Type: application/json" \
     -d '{"format":"prometheus","include_histograms":true}' \
     http://127.0.0.1:8080/api/v1/metrics/export
```

### Using Basic Authentication

```bash
# Encode credentials (admin:password)
echo -n "admin:password" | base64
# Result: YWRtaW46cGFzc3dvcmQ=

# Use with Authorization header
curl -H "Authorization: Basic YWRtaW46cGFzc3dvcmQ=" \
     http://127.0.0.1:8080/api/v1/status
```

## Error Handling

All API endpoints return a consistent error format:

```json
{
  "success": false,
  "data": null,
  "error": "Detailed error message",
  "timestamp": "2023-10-23T18:00:00Z"
}
```

Common HTTP status codes:
- `200 OK`: Request successful
- `400 Bad Request`: Invalid request data
- `401 Unauthorized`: Authentication required or failed
- `404 Not Found`: Resource not found
- `500 Internal Server Error`: Server error

## Security Considerations

1. **Always use HTTPS in production** to protect API keys and sensitive data
2. **Use strong API keys** with sufficient entropy
3. **Restrict API access** to trusted networks using firewall rules
4. **Monitor API usage** for suspicious activity
5. **Rotate API keys regularly**
6. **Use the principle of least privilege** for API access

## Integration Examples

The management API can be integrated with various monitoring and automation tools:

- **Prometheus**: Use the metrics export endpoint for monitoring
- **Grafana**: Create dashboards using the statistics endpoints
- **Ansible/Terraform**: Automate configuration management
- **Custom scripts**: Build automation around user and connection management

For more examples, see the `examples/management_api_demo.rs` file in the source code.