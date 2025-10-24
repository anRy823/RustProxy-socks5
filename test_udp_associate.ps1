# Test SOCKS5 UDP ASSOCIATE command
param(
    [string]$ProxyHost = "127.0.0.1",
    [int]$ProxyPort = 1080
)

Write-Host "Testing SOCKS5 UDP ASSOCIATE command at ${ProxyHost}:${ProxyPort}"

try {
    # Create TCP client
    $client = New-Object System.Net.Sockets.TcpClient
    $client.ReceiveTimeout = 30000
    $client.SendTimeout = 30000
    
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
    
    # Send UDP ASSOCIATE request (associate with 0.0.0.0:0)
    Write-Host "Sending UDP ASSOCIATE request..."
    
    $udpRequest = @(0x05, 0x03, 0x00, 0x01) + @(0, 0, 0, 0) + @(0, 0)  # VER CMD RSV ATYP + 0.0.0.0:0
    $stream.Write($udpRequest, 0, $udpRequest.Length)
    
    # Read UDP ASSOCIATE response header
    $udpResponse = New-Object byte[] 4
    $bytesRead = $stream.Read($udpResponse, 0, 4)
    
    if ($bytesRead -ne 4) {
        Write-Host "❌ Invalid UDP response header length: $bytesRead"
        exit 1
    }
    
    $respVersion = $udpResponse[0]
    $reply = $udpResponse[1]
    $reserved = $udpResponse[2]
    $atyp = $udpResponse[3]
    
    Write-Host "UDP ASSOCIATE response: version=$respVersion, reply=$reply, atyp=$atyp"
    
    if ($respVersion -ne 5) {
        Write-Host "❌ Invalid SOCKS version in response: $respVersion"
        exit 1
    }
    
    if ($reply -eq 0) {
        Write-Host "✅ UDP ASSOCIATE command accepted"
        
        # Read UDP relay address
        if ($atyp -eq 1) {
            # IPv4: 4 bytes + 2 bytes port
            $udpAddr = New-Object byte[] 6
            $stream.Read($udpAddr, 0, 6) | Out-Null
            
            $ip = "$($udpAddr[0]).$($udpAddr[1]).$($udpAddr[2]).$($udpAddr[3])"
            $port = ([uint16]$udpAddr[4] -shl 8) + $udpAddr[5]
            
            Write-Host "✅ UDP relay available at ${ip}:${port}"
            Write-Host "✅ SOCKS5 UDP ASSOCIATE command test PASSED"
        }
        else {
            Write-Host "⚠️  Unsupported address type in UDP response: $atyp"
        }
    }
    else {
        Write-Host "❌ UDP ASSOCIATE command failed: reply=$reply"
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