# Simple SOCKS5 test with authentication using PowerShell
param(
    [string]$ProxyHost = "127.0.0.1",
    [int]$ProxyPort = 1080
)

Write-Host "Testing SOCKS5 proxy with authentication at ${ProxyHost}:${ProxyPort}"

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
    
    if ($bytesRead -ne 2) {
        Write-Host "❌ Invalid response length: $bytesRead"
        exit 1
    }
    
    $version = $response[0]
    $method = $response[1]
    
    Write-Host "Received method selection: version=$version, method=$method"
    
    if ($version -ne 5) {
        Write-Host "❌ Invalid SOCKS version: $version"
        exit 1
    }
    
    if ($method -eq 0xFF) {
        Write-Host "❌ No acceptable authentication methods"
        exit 1
    }
    
    # Handle authentication
    if ($method -eq 0x02) {
        Write-Host "Username/password authentication required"
        
        # Send authentication request
        $username = [System.Text.Encoding]::ASCII.GetBytes("testuser")
        $password = [System.Text.Encoding]::ASCII.GetBytes("testpass")
        
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
            Write-Host "❌ Authentication failed: status=$authStatus"
            exit 1
        }
        
        Write-Host "✅ Authentication successful"
    }
    else {
        Write-Host "❌ Expected username/password auth but got method: $method"
        exit 1
    }
    
    # Send CONNECT request
    Write-Host "Sending CONNECT request to httpbin.org:80..."
    $targetHost = "httpbin.org"
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
    
    if ($respVersion -ne 5) {
        Write-Host "❌ Invalid SOCKS version in response: $respVersion"
        exit 1
    }
    
    if ($reply -ne 0) {
        Write-Host "❌ CONNECT failed: reply=$reply"
        exit 1
    }
    
    # Read bind address (we'll just read and ignore it)
    if ($atyp -eq 1) {
        # IPv4: 4 bytes + 2 bytes port
        $bindAddr = New-Object byte[] 6
        $stream.Read($bindAddr, 0, 6) | Out-Null
    }
    elseif ($atyp -eq 3) {
        # Domain: 1 byte length + domain + 2 bytes port
        $lenByte = New-Object byte[] 1
        $stream.Read($lenByte, 0, 1) | Out-Null
        $domainLen = $lenByte[0]
        $bindAddr = New-Object byte[] ($domainLen + 2)
        $stream.Read($bindAddr, 0, $bindAddr.Length) | Out-Null
    }
    elseif ($atyp -eq 4) {
        # IPv6: 16 bytes + 2 bytes port
        $bindAddr = New-Object byte[] 18
        $stream.Read($bindAddr, 0, 18) | Out-Null
    }
    
    Write-Host "✅ CONNECT successful with authentication!"
    Write-Host "✅ SOCKS5 proxy authentication test PASSED"
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