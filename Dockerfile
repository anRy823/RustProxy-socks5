# Multi-stage Dockerfile for SOCKS5 Proxy Server
# Stage 1: Build environment
FROM rust:1.75-slim as builder

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Create app directory
WORKDIR /app

# Copy dependency files first for better caching
COPY Cargo.toml Cargo.lock ./

# Create a dummy main.rs to build dependencies
RUN mkdir src && echo "fn main() {}" > src/main.rs

# Build dependencies (this layer will be cached unless Cargo.toml changes)
RUN cargo build --release && rm -rf src

# Copy source code
COPY src ./src
COPY examples ./examples
COPY tests ./tests

# Build the application
RUN cargo build --release --bin socks5-proxy

# Stage 2: Runtime environment
FROM debian:bookworm-slim as runtime

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user for security
RUN groupadd -r socks5 && useradd -r -g socks5 -s /bin/false socks5

# Create directories
RUN mkdir -p /app/config /app/data /app/logs && \
    chown -R socks5:socks5 /app

# Copy binary from builder stage
COPY --from=builder /app/target/release/socks5-proxy /usr/local/bin/socks5-proxy

# Copy default configuration
COPY config.toml /app/config/config.toml

# Set ownership
RUN chown -R socks5:socks5 /app

# Switch to non-root user
USER socks5

# Set working directory
WORKDIR /app

# Expose default SOCKS5 port and metrics port
EXPOSE 1080 9090

# Health check
HEALTHCHECK --interval=30s --timeout=10s --start-period=5s --retries=3 \
    CMD timeout 5 bash -c '</dev/tcp/localhost/1080' || exit 1

# Default command
CMD ["socks5-proxy", "--config", "/app/config/config.toml"]