# Test concurrent SOCKS5 connections
param(
    [int]$NumConnections = 5,
    [string]$ProxyHost = "127.0.0.1",
    [int]$ProxyPort = 1080
)

Write-Host "Testing $NumConnections concurrent SOCKS5 connections to ${ProxyHost}:${ProxyPort}"

$jobs = @()
$startTime = Get-Date

# Create multiple concurrent jobs
for ($i = 1; $i -le $NumConnections; $i++) {
    $job = Start-Job -ScriptBlock {
        param($connectionId, $proxyHost, $proxyPort)
        
        try {
            # Create TCP client
            $client = New-Object System.Net.Sockets.TcpClient
            $client.ReceiveTimeout = 10000
            $client.SendTimeout = 10000
            
            # Connect to proxy
            $client.Connect($proxyHost, $proxyPort)
            $stream = $client.GetStream()
            
            # Send SOCKS5 greeting
            $greeting = [byte[]](0x05, 0x01, 0x02)
            $stream.Write($greeting, 0, $greeting.Length)
            
            # Read method selection response
            $response = New-Object byte[] 2
            $bytesRead = $stream.Read($response, 0, 2)
            
            if ($response[1] -eq 0x02) {
                # Send authentication request
                $username = [System.Text.Encoding]::ASCII.GetBytes("testuser")
                $password = [System.Text.Encoding]::ASCII.GetBytes("testpass")
                
                $authRequest = @(0x01) + @($username.Length) + $username + @($password.Length) + $password
                $stream.Write($authRequest, 0, $authRequest.Length)
                
                # Read authentication response
                $authResponse = New-Object byte[] 2
                $bytesRead = $stream.Read($authResponse, 0, 2)
                
                if ($authResponse[1] -ne 0) {
                    return "Connection ${connectionId}: Auth failed"
                }
            }
            
            # Send CONNECT request
            $targetHost = "httpbin.org"
            $targetPort = 80
            
            $hostBytes = [System.Text.Encoding]::ASCII.GetBytes($targetHost)
            $portBytes = [BitConverter]::GetBytes([uint16]$targetPort)
            if ([BitConverter]::IsLittleEndian) {
                [Array]::Reverse($portBytes)
            }
            
            $connectRequest = @(0x05, 0x01, 0x00, 0x03) + @($hostBytes.Length) + $hostBytes + $portBytes
            $stream.Write($connectRequest, 0, $connectRequest.Length)
            
            # Read CONNECT response
            $connectResponse = New-Object byte[] 4
            $bytesRead = $stream.Read($connectResponse, 0, 4)
            
            if ($connectResponse[1] -eq 0) {
                # Read bind address
                if ($connectResponse[3] -eq 1) {
                    $bindAddr = New-Object byte[] 6
                    $stream.Read($bindAddr, 0, 6) | Out-Null
                }
                
                $client.Close()
                return "Connection ${connectionId}: SUCCESS"
            }
            else {
                $client.Close()
                return "Connection ${connectionId}: CONNECT failed (reply=$($connectResponse[1]))"
            }
        }
        catch {
            return "Connection ${connectionId}: ERROR - $($_.Exception.Message)"
        }
    } -ArgumentList $i, $ProxyHost, $ProxyPort
    
    $jobs += $job
}

Write-Host "Started $NumConnections concurrent jobs, waiting for completion..."

# Wait for all jobs to complete
$results = $jobs | Wait-Job | Receive-Job
$jobs | Remove-Job

$endTime = Get-Date
$duration = ($endTime - $startTime).TotalSeconds

Write-Host "`nResults:"
$successCount = 0
foreach ($result in $results) {
    Write-Host "  $result"
    if ($result -like "*SUCCESS*") {
        $successCount++
    }
}

Write-Host "`nSummary:"
Write-Host "  Total connections: $NumConnections"
Write-Host "  Successful: $successCount"
Write-Host "  Failed: $($NumConnections - $successCount)"
Write-Host "  Duration: $([math]::Round($duration, 2)) seconds"
Write-Host "  Rate: $([math]::Round($NumConnections / $duration, 2)) connections/second"

if ($successCount -eq $NumConnections) {
    Write-Host "✅ Concurrent connection test PASSED"
}
else {
    Write-Host "⚠️  Concurrent connection test PARTIALLY PASSED ($successCount/$NumConnections)"
}