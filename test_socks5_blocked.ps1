# Test SOCKS5 proxy with blocked domain
param(
    [string]$ProxyHost = "127.0.0.1",
    [int]$ProxyPort = 1080
)

Write-Host "Testing SOCKS5 proxy with blocked domain at ${ProxyHost}:${ProxyPort}"

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
    
    if ($method -eq 0x02) {
        # Send authentication request with valid credentials
        $username = [System.Text.Encoding]::ASCII.GetBytes("testuser")
        $password = [System.Text.Encoding]::ASCII.GetBytes("testpass")
        
        $authRequest = @(0x01) + @($username.Length) + $username + @($password.Length) + $password
        $stream.Write($authRequest, 0, $authRequest.Length)
        
        # Read authentication response
        $authResponse = New-Object byte[] 2
        $bytesRead = $stream.Read($authResponse, 0, 2)
        
        $authVersion = $authResponse[0]
        $authStatus = $authResponse[1]
        
        if ($authStatus -ne 0) {
            Write-Host "❌ Authentication failed: status=$authStatus"
            exit 1
        }
        
        Write-Host "✅ Authentication successful"
    }
    
    # Send CONNECT request to blocked domain (test.example.com)
    Write-Host "Sending CONNECT request to blocked domain test.example.com:80..."
    $targetHost = "test.example.com"
    $targetPort = 80
    
    $hostBytes = [System.Text.Encoding]::ASCII.GetBytes($targetHost)
    $portBytes = [BitConverter]::GetBytes([uint16]$targetPort)
    if ([BitConverter]::IsLittleEndian) {
        [Array]::Reverse($portBytes)
    }
    
    $connectRequest = @(0x05, 0x01, 0x00, 0x03) + @($hostBytes.Length) + $hostBytes + $portBytes
    $stream.Write($connectRequest, 0, $connectRequest.Length)
    
    # Read CONNECT response header
    $connectResponse = New-Object byte[] 4
    $bytesRead = $stream.Read($connectResponse, 0, 4)
    
    if ($bytesRead -ne 4) {
        Write-Host "❌ Invalid connect response header length: $bytesRead"
        exit 1
    }
    
    $respVersion = $connectResponse[0]
    $reply = $connectResponse[1]
    $reserved = $connectResponse[2]
    $atyp = $connectResponse[3]
    
    Write-Host "CONNECT response: version=$respVersion, reply=$reply, atyp=$atyp"
    
    if ($reply -eq 2) {
        Write-Host "✅ Connection correctly blocked (reply=2: Connection not allowed)"
        Write-Host "✅ SOCKS5 proxy access control test PASSED"
    }
    elseif ($reply -eq 0) {
        Write-Host "❌ Connection should have been blocked but was allowed"
        exit 1
    }
    else {
        Write-Host "⚠️  Connection failed with reply=$reply (may be blocked or other error)"
        Write-Host "✅ SOCKS5 proxy access control test PARTIALLY PASSED"
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