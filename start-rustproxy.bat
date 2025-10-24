@echo off
REM RustProxy Quick Start Script
REM Created by [Your Name] - Professional Network Solutions

echo.
echo ========================================
echo   RustProxy - Professional SOCKS5 Proxy
echo   Created by [Your Name]
echo ========================================
echo.

REM Check if config file exists
if not exist "config.toml" (
    echo No config.toml found. Creating one from template...
    copy "config.simple.toml" "config.toml"
    echo.
    echo ⚠️  IMPORTANT: Edit config.toml to change default passwords!
    echo.
    pause
)

REM Check if rustproxy.exe exists
if not exist "rustproxy.exe" (
    if not exist "target\release\rustproxy.exe" (
        echo ❌ rustproxy.exe not found!
        echo.
        echo Please either:
        echo 1. Download the pre-built binary and place it here, OR
        echo 2. Build from source with: cargo build --release
        echo.
        pause
        exit /b 1
    ) else (
        echo Using development build from target\release\
        set RUSTPROXY_EXE=target\release\rustproxy.exe
    )
) else (
    set RUSTPROXY_EXE=rustproxy.exe
)

echo Starting RustProxy...
echo.
echo 📖 For help, see USER_MANUAL.md
echo 🌐 Proxy will be available at: 127.0.0.1:1080
echo 🛑 Press Ctrl+C to stop the proxy
echo.

REM Start the proxy
%RUSTPROXY_EXE% --config config.toml

echo.
echo RustProxy stopped.
pause