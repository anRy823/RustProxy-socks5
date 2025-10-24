# Test SOCKS5 proxy with invalid credentials
param(
    [string]$ProxyHost = "127.0.0.1",
    [int]$ProxyPort = 1080
)

Write-Host "Testing SOCKS5 proxy with invalid credentials at ${ProxyHost}:${ProxyPort}"

try {
    # Create TCP client
    $client = New-Object System.Net.Sockets.TcpClient
    $client.ReceiveTimeout = 10000
    $client.SendTimeout = 10000
    
    # Connect to proxy
    Write-Host "Connecting to proxy..."
    $client.Connect($ProxyHost, $ProxyPort)
    $stream = $client.GetStream()
    
    # Send SOCKS5 greeting (version 5, 1 method: username/password)
    Write-Host "Sending SOCKS5 greeting with auth..."
    $greeting = [byte[]](0x05, 0x01, 0x02)
    $stream.Write($greeting, 0, $greeting.Length)
    
    # Read method selection response
    $response = New-Object byte[] 2
    $bytesRead = $stream.Read($response, 0, 2)
    
    $version = $response[0]
    $method = $response[1]
    
    Write-Host "Received method selection: version=$version, method=$method"
    
    if ($method -eq 0x02) {
        Write-Host "Username/password authentication required"
        
        # Send authentication request with INVALID credentials
        $username = [System.Text.Encoding]::ASCII.GetBytes("wronguser")
        $password = [System.Text.Encoding]::ASCII.GetBytes("wrongpass")
        
        $authRequest = @(0x01) + @($username.Length) + $username + @($password.Length) + $password
        $stream.Write($authRequest, 0, $authRequest.Length)
        
        # Read authentication response
        $authResponse = New-Object byte[] 2
        $bytesRead = $stream.Read($authResponse, 0, 2)
        
        if ($bytesRead -ne 2) {
            Write-Host "❌ Invalid auth response length: $bytesRead"
            exit 1
        }
        
        $authVersion = $authResponse[0]
        $authStatus = $authResponse[1]
        
        Write-Host "Auth response: version=$authVersion, status=$authStatus"
        
        if ($authStatus -ne 0) {
            Write-Host "✅ Authentication correctly failed: status=$authStatus"
            Write-Host "✅ SOCKS5 proxy invalid credentials test PASSED"
        }
        else {
            Write-Host "❌ Authentication should have failed but succeeded"
            exit 1
        }
    }
    else {
        Write-Host "❌ Expected username/password auth but got method: $method"
        exit 1
    }
}
catch {
    Write-Host "❌ Error: $($_.Exception.Message)"
    exit 1
}
finally {
    if ($client) {
        $client.Close()
    }
}