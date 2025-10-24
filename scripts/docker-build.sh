#!/bin/bash

# Docker build script for SOCKS5 Proxy Server

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Default values
IMAGE_NAME="socks5-proxy"
TAG="latest"
BUILD_TYPE="production"

# Function to print colored output
print_status() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

print_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Function to show usage
usage() {
    echo "Usage: $0 [OPTIONS]"
    echo "Options:"
    echo "  -n, --name NAME     Docker image name (default: socks5-proxy)"
    echo "  -t, --tag TAG       Docker image tag (default: latest)"
    echo "  -d, --dev           Build development image"
    echo "  -p, --prod          Build production image (default)"
    echo "  -h, --help          Show this help message"
    exit 1
}

# Parse command line arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        -n|--name)
            IMAGE_NAME="$2"
            shift 2
            ;;
        -t|--tag)
            TAG="$2"
            shift 2
            ;;
        -d|--dev)
            BUILD_TYPE="development"
            shift
            ;;
        -p|--prod)
            BUILD_TYPE="production"
            shift
            ;;
        -h|--help)
            usage
            ;;
        *)
            print_error "Unknown option: $1"
            usage
            ;;
    esac
done

# Build the Docker image
print_status "Building Docker image: ${IMAGE_NAME}:${TAG}"
print_status "Build type: ${BUILD_TYPE}"

if [ "$BUILD_TYPE" = "development" ]; then
    print_status "Building development image with build tools..."
    docker build --target builder -t "${IMAGE_NAME}:${TAG}-dev" .
    print_status "Development image built successfully: ${IMAGE_NAME}:${TAG}-dev"
else
    print_status "Building production image..."
    docker build -t "${IMAGE_NAME}:${TAG}" .
    print_status "Production image built successfully: ${IMAGE_NAME}:${TAG}"
fi

# Show image size
print_status "Image size:"
docker images "${IMAGE_NAME}:${TAG}*" --format "table {{.Repository}}\t{{.Tag}}\t{{.Size}}"

print_status "Build completed successfully!"
print_status "To run the container:"
if [ "$BUILD_TYPE" = "development" ]; then
    echo "  docker run -p 1080:1080 -p 9090:9090 ${IMAGE_NAME}:${TAG}-dev"
else
    echo "  docker run -p 1080:1080 -p 9090:9090 ${IMAGE_NAME}:${TAG}"
fi
print_status "Or use docker-compose:"
echo "  docker-compose up"