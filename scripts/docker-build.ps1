# Docker build script for SOCKS5 Proxy Server (PowerShell)

param(
    [string]$ImageName = "socks5-proxy",
    [string]$Tag = "latest",
    [switch]$Dev,
    [switch]$Prod,
    [switch]$Help
)

# Function to print colored output
function Write-Status {
    param([string]$Message)
    Write-Host "[INFO] $Message" -ForegroundColor Green
}

function Write-Warning {
    param([string]$Message)
    Write-Host "[WARN] $Message" -ForegroundColor Yellow
}

function Write-Error {
    param([string]$Message)
    Write-Host "[ERROR] $Message" -ForegroundColor Red
}

# Function to show usage
function Show-Usage {
    Write-Host "Usage: .\docker-build.ps1 [OPTIONS]"
    Write-Host "Options:"
    Write-Host "  -ImageName NAME     Docker image name (default: socks5-proxy)"
    Write-Host "  -Tag TAG           Docker image tag (default: latest)"
    Write-Host "  -Dev               Build development image"
    Write-Host "  -Prod              Build production image (default)"
    Write-Host "  -Help              Show this help message"
    exit 1
}

# Show help if requested
if ($Help) {
    Show-Usage
}

# Determine build type
$BuildType = if ($Dev) { "development" } else { "production" }

# Build the Docker image
Write-Status "Building Docker image: ${ImageName}:${Tag}"
Write-Status "Build type: ${BuildType}"

try {
    if ($BuildType -eq "development") {
        Write-Status "Building development image with build tools..."
        docker build --target builder -t "${ImageName}:${Tag}-dev" .
        if ($LASTEXITCODE -eq 0) {
            Write-Status "Development image built successfully: ${ImageName}:${Tag}-dev"
        } else {
            throw "Docker build failed"
        }
    } else {
        Write-Status "Building production image..."
        docker build -t "${ImageName}:${Tag}" .
        if ($LASTEXITCODE -eq 0) {
            Write-Status "Production image built successfully: ${ImageName}:${Tag}"
        } else {
            throw "Docker build failed"
        }
    }

    # Show image size
    Write-Status "Image size:"
    docker images "${ImageName}:${Tag}*" --format "table {{.Repository}}\t{{.Tag}}\t{{.Size}}"

    Write-Status "Build completed successfully!"
    Write-Status "To run the container:"
    if ($BuildType -eq "development") {
        Write-Host "  docker run -p 1080:1080 -p 9090:9090 ${ImageName}:${Tag}-dev"
    } else {
        Write-Host "  docker run -p 1080:1080 -p 9090:9090 ${ImageName}:${Tag}"
    }
    Write-Status "Or use docker-compose:"
    Write-Host "  docker-compose up"
} catch {
    Write-Error "Build failed: $_"
    exit 1
}