# Docker Deployment Guide

This directory contains Docker configurations for deploying the SOCKS5 proxy server in various environments.

## Quick Start

### Development Environment

```bash
# Start the proxy server for development
docker-compose -f docker-compose.yml -f docker-compose.dev.yml up --build

# Run tests
docker-compose -f docker-compose.yml -f docker-compose.dev.yml --profile testing up test-runner

# Start with monitoring stack
docker-compose -f docker-compose.yml -f docker-compose.dev.yml --profile monitoring up --build
```

### Production Environment

```bash
# Build production image
docker build -t socks5-proxy:latest .

# Start production deployment
docker-compose -f docker-compose.prod.yml up -d

# View logs
docker-compose -f docker-compose.prod.yml logs -f socks5-proxy
```

## Configuration

### Environment Variables

- `RUST_LOG`: Log level (debug, info, warn, error)
- `RUST_BACKTRACE`: Enable backtrace (0, 1, full)

### Volumes

- `/app/config/config.toml`: Configuration file
- `/app/logs`: Log files directory
- `/app/data`: Data storage directory

### Ports

- `1080`: SOCKS5 proxy port
- `9090`: Metrics/Prometheus endpoint
- `3000`: Grafana dashboard (monitoring profile)
- `9091`: Prometheus UI (monitoring profile)

## Monitoring

The monitoring stack includes:

- **Prometheus**: Metrics collection and storage
- **Grafana**: Metrics visualization and dashboards

Access Grafana at http://localhost:3000 (admin/admin)

## Security Considerations

### Production Deployment

1. **Change default passwords** in `config/config.prod.toml`
2. **Use secrets management** for sensitive configuration
3. **Enable TLS** for external access
4. **Configure firewall rules** appropriately
5. **Use non-root user** (already configured in Dockerfile)

### Network Security

```bash
# Create custom network with specific subnet
docker network create --driver bridge --subnet=172.20.0.0/16 socks5-network

# Use network isolation
docker-compose --network socks5-network up
```

## Scaling

### Horizontal Scaling

```bash
# Scale proxy instances
docker-compose -f docker-compose.prod.yml up --scale socks5-proxy=3 -d
```

### Load Balancing

Use an external load balancer (nginx, HAProxy) to distribute connections:

```nginx
upstream socks5_backend {
    server 127.0.0.1:1080;
    server 127.0.0.1:1081;
    server 127.0.0.1:1082;
}

server {
    listen 1080;
    proxy_pass socks5_backend;
}
```

## Troubleshooting

### Common Issues

1. **Port conflicts**: Change port mappings in docker-compose.yml
2. **Permission issues**: Ensure proper file ownership
3. **Memory limits**: Adjust resource limits in production config
4. **Network connectivity**: Check Docker network configuration

### Debug Commands

```bash
# Check container status
docker-compose ps

# View container logs
docker-compose logs socks5-proxy

# Execute shell in container
docker-compose exec socks5-proxy /bin/bash

# Check resource usage
docker stats socks5-proxy-prod
```

### Health Checks

The container includes health checks that verify:
- SOCKS5 port accessibility
- Process responsiveness
- Resource utilization

## Backup and Recovery

### Configuration Backup

```bash
# Backup configuration
docker cp socks5-proxy-prod:/app/config ./backup/config-$(date +%Y%m%d)

# Restore configuration
docker cp ./backup/config-20240101 socks5-proxy-prod:/app/config
```

### Data Backup

```bash
# Backup data directory
docker run --rm -v socks5-proxy_data:/data -v $(pwd):/backup alpine tar czf /backup/data-backup.tar.gz -C /data .

# Restore data
docker run --rm -v socks5-proxy_data:/data -v $(pwd):/backup alpine tar xzf /backup/data-backup.tar.gz -C /data
```