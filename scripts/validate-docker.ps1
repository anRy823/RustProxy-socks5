# Docker configuration validation script

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

function Test-FileExists {
    param([string]$Path, [string]$Description)
    if (Test-Path $Path) {
        Write-Status "$Description exists: $Path"
        return $true
    } else {
        Write-Error "$Description missing: $Path"
        return $false
    }
}

Write-Status "Validating Docker configuration files..."

$allValid = $true

# Check required files
$requiredFiles = @(
    @{ Path = "Dockerfile"; Description = "Dockerfile" },
    @{ Path = "docker-compose.yml"; Description = "Main docker-compose file" },
    @{ Path = "docker-compose.dev.yml"; Description = "Development docker-compose file" },
    @{ Path = "docker-compose.prod.yml"; Description = "Production docker-compose file" },
    @{ Path = ".dockerignore"; Description = "Docker ignore file" },
    @{ Path = "config.toml"; Description = "Default configuration" },
    @{ Path = "config/config.prod.toml"; Description = "Production configuration" },
    @{ Path = "docker/prometheus.yml"; Description = "Prometheus configuration" },
    @{ Path = "docker/README.md"; Description = "Docker documentation" }
)

foreach ($file in $requiredFiles) {
    if (-not (Test-FileExists $file.Path $file.Description)) {
        $allValid = $false
    }
}

# Check directory structure
$requiredDirs = @(
    "docker/grafana/provisioning/datasources",
    "docker/grafana/provisioning/dashboards",
    "scripts",
    "config"
)

foreach ($dir in $requiredDirs) {
    if (Test-Path $dir -PathType Container) {
        Write-Status "Directory exists: $dir"
    } else {
        Write-Error "Directory missing: $dir"
        $allValid = $false
    }
}

# Validate Dockerfile syntax (basic checks)
Write-Status "Validating Dockerfile syntax..."
$dockerfileContent = Get-Content "Dockerfile" -Raw

if ($dockerfileContent -match "FROM.*as builder") {
    Write-Status "Multi-stage build detected"
} else {
    Write-Warning "Multi-stage build not detected"
}

if ($dockerfileContent -match "USER socks5") {
    Write-Status "Non-root user configured"
} else {
    Write-Warning "Running as root user"
}

if ($dockerfileContent -match "HEALTHCHECK") {
    Write-Status "Health check configured"
} else {
    Write-Warning "No health check configured"
}

# Check docker-compose syntax (basic validation)
Write-Status "Validating docker-compose files..."

$composeFiles = @("docker-compose.yml", "docker-compose.dev.yml", "docker-compose.prod.yml")
foreach ($composeFile in $composeFiles) {
    if (Test-Path $composeFile) {
        $content = Get-Content $composeFile -Raw
        if ($content -match "version:.*3\.[0-9]") {
            Write-Status "$composeFile: Valid version format"
        } else {
            Write-Warning "$composeFile: Version format may be invalid"
        }
        
        if ($content -match "services:") {
            Write-Status "$composeFile: Services section found"
        } else {
            Write-Error "$composeFile: No services section found"
            $allValid = $false
        }
    }
}

# Summary
Write-Host "`n" -NoNewline
if ($allValid) {
    Write-Status "All Docker configuration files are valid!"
    Write-Status "You can now build and run the containers:"
    Write-Host "  .\scripts\docker-build.ps1"
    Write-Host "  docker-compose up --build"
} else {
    Write-Error "Some configuration files are missing or invalid!"
    Write-Host "Please check the errors above and fix them before proceeding."
}

return $allValid