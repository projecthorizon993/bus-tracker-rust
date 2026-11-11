#!/usr/bin/env pwsh
# Test calling TimBus API for bus 35B
# Based on reverse engineering of TimBus APK v5.6.0
# API Endpoint: http://www.timbus.vn:9149/SearchBuyt

$ErrorActionPreference = "Stop"
$BASE_URL = "http://www.timbus.vn:9149/SearchBuyt"

# ===== CRC32-like checksum (same as qh0.k()) =====
function Get-Checksum {
    param([byte[]]$data)
    $sum = 0
    for ($i = 0; $i -lt ($data.Length - 1); $i++) {
        $sum = $sum + [int]$data[$i]
    }
    return $sum -band 0xFF
}

# ===== Generate random key =====
function Get-RandomKey {
    param([int]$minLen = 32, [int]$maxLen = 64)
    $len = Get-Random -Minimum $minLen -Maximum ($maxLen + 1)
    $key = New-Object byte[] $len
    for ($i = 0; $i -lt $len; $i++) {
        $key[$i] = [byte](Get-Random -Minimum 48 -Maximum 127)
    }
    return $key
}

# ===== XOR data with key (repeating key if shorter) =====
function Get-Xor {
    param([byte[]]$data, [byte[]]$key)
    $result = New-Object byte[] $data.Length
    for ($i = 0; $i -lt $data.Length; $i++) {
        $result[$i] = $data[$i] -bxor $key[$i % $key.Length]
    }
    return $result
}

# ===== Build route search payload (RouteBuytOptionV3) =====
# From un2.n(): 4 floats (origin_lat, origin_lng, dest_lat, dest_lng) + 1 byte (value 2)
function Build-RouteRequest {
    param([double]$originLat, [double]$originLng, [double]$destLat, [double]$destLng)
    $buf = New-Object byte[] 17
    $i = 0
    $floatBytes = [System.BitConverter]::GetBytes([single]$originLat)
    for ($j = 0; $j -lt 4; $j++) { $buf[$i + $j] = $floatBytes[$j] } $i += 4
    $floatBytes = [System.BitConverter]::GetBytes([single]$originLng)
    for ($j = 0; $j -lt 4; $j++) { $buf[$i + $j] = $floatBytes[$j] } $i += 4
    $floatBytes = [System.BitConverter]::GetBytes([single]$destLat)
    for ($j = 0; $j -lt 4; $j++) { $buf[$i + $j] = $floatBytes[$j] } $i += 4
    $floatBytes = [System.BitConverter]::GetBytes([single]$destLng)
    for ($j = 0; $j -lt 4; $j++) { $buf[$i + $j] = $floatBytes[$j] } $i += 4
    $buf[$i] = 2
    return $buf
}

# ===== Build vehicle state request (VehicleState) =====
# From n73.m(): int busId, string routeNo, string plateNo, boolean active
function Build-VehicleStateRequest {
    param([int]$busId, [string]$routeNo, [string]$plateNo, [bool]$active = $true)
    $busIdBytes = [System.BitConverter]::GetBytes([int]$busId)
    $routeBytes = [System.Text.Encoding]::UTF8.GetBytes($routeNo)
    $plateBytes = [System.Text.Encoding]::UTF8.GetBytes($plateNo)
    
    $payloadSize = 4 + 2 + $routeBytes.Length + 2 + $plateBytes.Length + 1
    $payload2 = New-Object byte[] $payloadSize
    $pos = 0
    # busId as int32
    for ($j = 0; $j -lt 4; $j++) { $payload2[$pos + $j] = $busIdBytes[$j] } $pos += 4
    # routeNo length as short (little-endian)
    $payload2[$pos] = [byte]($routeBytes.Length -band 255); $payload2[$pos+1] = [byte](($routeBytes.Length -band 0xFF00) -shr 8); $pos += 2
    # routeNo bytes
    for ($j = 0; $j -lt $routeBytes.Length; $j++) { $payload2[$pos + $j] = $routeBytes[$j] } $pos += $routeBytes.Length
    # plateNo length as short (little-endian)
    $payload2[$pos] = [byte]($plateBytes.Length -band 255); $payload2[$pos+1] = [byte](($plateBytes.Length -band 0xFF00) -shr 8); $pos += 2
    # plateNo bytes
    for ($j = 0; $j -lt $plateBytes.Length; $j++) { $payload2[$pos + $j] = $plateBytes[$j] } $pos += $plateBytes.Length
    # boolean active
    $payload2[$pos] = 1
    return $payload2
}

# ===== Encrypt payload using TimBus crypt() algorithm =====
function Get-EncryptedPayload {
    param([byte[]]$data)
    
    # Generate key1 (32-64 random bytes)
    $key1 = Get-RandomKey -minLen 32 -maxLen 64
    # Generate key2 (32-64 random bytes)
    $key2 = Get-RandomKey -minLen 32 -maxLen 64
    
    # Step 1: Build full data: [random_int_4_bytes] + [data_bytes]
    $randInt = Get-RandomInt
    $randIntBytes = [System.BitConverter]::GetBytes([int]$randInt)
    $fullData = New-Object byte[] (4 + $data.Length)
    for ($j = 0; $j -lt 4; $j++) { $fullData[$j] = $randIntBytes[$j] }
    for ($j = 0; $j -lt $data.Length; $j++) { $fullData[4 + $j] = $data[$j] }
    
    # Step 2: XOR entire buffer with key1
    $encrypted = Get-Xor -data $fullData -key $key1
    
    # Step 3: Build output buffer
    $buf = New-Object System.Collections.Generic.List[byte]
    foreach ($b in $encrypted) { $buf.Add($b) }
    foreach ($b in $key1) { $buf.Add($b) }
    $buf.Add([byte]$key1.Length)
    foreach ($b in $key2) { $buf.Add($b) }
    $buf.Add([byte]$key2.Length)
    $checksum = Get-Checksum -data $buf.ToArray()
    $buf.Add($checksum)
    
    return [Convert]::ToBase64String($buf.ToArray())
}

# ===== Call the TimBus API =====
function Call-TimBusApi {
    param([string]$encryptedPayload)
    
    $encodedPayload = [System.Uri]::EscapeDataString($encryptedPayload)
    $url = "$BASE_URL/$methodType?ReqParams=$encodedPayload"
    
    try {
        $response = Invoke-RestMethod -Method Get -Uri $url -TimeoutSec 15 -ErrorAction Stop
        Write-Host "HTTP Status: Success"
        Write-Host "Response length: $($response.Length)"
        Write-Host "Response preview: $($response.Substring(0, [Math]::Min(500, $response.Length)))"
        return $response
    } catch {
        Write-Host "Error: $($_.Exception.Message)"
        if ($_.Exception.Response) {
            $statusCode = $_.Exception.Response.StatusCode
            Write-Host "HTTP Status: $statusCode"
            $reader = New-Object System.IO.StreamReader($_.Exception.Response.GetResponseStream())
            $errorBody = $reader.ReadToEnd()
            $reader.Close()
            Write-Host "Error body: $errorBody"
        }
        return $null
    }
}

# ===== Main =====
Write-Host "========================================"
Write-Host "Testing TimBus API for bus route 35B"
Write-Host "========================================"
Write-Host ""

# Generate random int (like kr2.q())
function Get-RandomInt {
    return [int](Get-Random -Minimum 1 -Maximum ([int]::MaxValue))
}

# Test 1: Route search with Hanoi coordinates
Write-Host "--- Test 1: Route search (RouteBuytOptionV3) ---"
Write-Host "Searching for route between Hanoi points..."
$payload = Build-RouteRequest -originLat 21.0285 -originLng 105.8542 -destLat 21.0385 -destLng 105.8642
$encrypted = Get-EncryptedPayload -data $payload
Write-Host "Encrypted payload length: $($encrypted.Length)"
Write-Host "Encrypted payload (first 100 chars): $($encrypted.Substring(0, [Math]::Min(100, $encrypted.Length)))"
$resp = Call-TimBusApi -encryptedPayload $encrypted

# Test 2: Vehicle state for bus 35B
Write-Host ""
Write-Host "--- Test 2: Vehicle state (VehicleState) ---"
Write-Host "Searching for bus 35B..."
$payload2 = Build-VehicleStateRequest -busId 35 -routeNo "35B" -plateNo "50H41787"
$encrypted2 = Get-EncryptedPayload -data $payload2
Write-Host "Encrypted payload length: $($encrypted2.Length)"
Write-Host "Encrypted payload (first 100 chars): $($encrypted2.Substring(0, [Math]::Min(100, $encrypted2.Length)))"
$resp2 = Call-TimBusApi -encryptedPayload $encrypted2
