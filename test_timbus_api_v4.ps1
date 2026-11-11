#!/usr/bin/env pwsh
# Test VinBus API encryption - matches decompiled app logic

$ErrorActionPreference = "Stop"
$BASE_URL = "http://www.timbus.vn:9149/SearchBuyt"

# CRC32-like checksum (same as qh0.k())
function Get-Checksum {
    param([byte[]]$data)
    $sum = 0
    for ($i = 0; $i -lt ($data.Length - 1); $i++) {
        $sum = $sum + [int]$data[$i]
    }
    return $sum -band 0xFF
}

# Generate random key
function Get-RandomKey {
    param([int]$minLen = 10, [int]$maxLen = 15)
    $len = Get-Random -Minimum $minLen -Maximum ($maxLen + 1)
    $key = New-Object byte[] $len
    for ($i = 0; $i -lt $len; $i++) {
        $key[$i] = [byte](Get-Random -Minimum 48 -Maximum 127)
    }
    return $key
}

# XOR data with key (repeating key if shorter)
function Get-Xor {
    param([byte[]]$data, [byte[]]$key)
    $result = New-Object byte[] $data.Length
    for ($i = 0; $i -lt $data.Length; $i++) {
        $result[$i] = $data[$i] -bxor $key[$i % $key.Length]
    }
    return $result
}

# Build payload: [random_int(4 bytes)] + [data_bytes]
function Build-Payload {
    param([string]$routeNo = "35B")
    $payload = New-Object byte[] (4 + $routeBytes.Length)
    
    # Add random int (4 bytes, little endian)
    $randInt = Get-Random -Minimum 1 -Maximum ([int]::MaxValue)
    $randBytes = [System.BitConverter]::GetBytes([int]$randInt)
    for ($j = 0; $j -lt 4; $j++) { $payload[$j] = $randBytes[$j] }
    
    # Add route data
    $routeBytes = [System.Text.Encoding]::UTF8.GetBytes($routeNo)
    for ($j = 0; $j -lt $routeBytes.Length; $j++) {
        $payload[4 + $j] = $routeBytes[$j]
    }
    
    return $payload
}

# Encrypt payload using TimBus algorithm
function Get-EncryptedPayload {
    param([byte[]]$data)
    
    # Generate random keys (10-15 bytes each)
    $key1 = Get-RandomKey -minLen 10 -maxLen 16
    $key2 = Get-RandomKey -minLen 10 -maxLen 16
    
    # Build bArr2 = key1 XOR key2, extended to max length
    $maxLen = [math]::Max($key1.Length, $key2.Length)
    $bArr2 = New-Object byte[] $maxKeyLen
    for ($i = 0; $i -lt $maxKeyLen; $i++) {
        $k1 = if ($i -lt $key1.Length) { $key1[$i] } else { 0 }
        $k2 = if ($i -lt $key2.Length) { $key2[$i] } else { 0 }
        $bArr2[$i] = $k1 -bxor $k2
    }
    
    # XOR payload data with bArr2 cyclically
    $encryptedData = New-Object byte[] $data.Length
    for ($i = 0; $i -lt $data.Length; $i++) {
        $encryptedData[$i] = $data[$i] -bxor $bArr2[$i % $bArr2.Length]
    }
    
    # Build output buffer:
    # byte 0: key1_len
    # bytes 1-2: data_length (little endian)
    # byte 3: key2_len
    // bytes 4..4+key1_len-1: key1
    // bytes 4+key1_len..4+key1_len+key2_len-1: key2
    // remaining: encrypted data XORed with bArr2
    $output = New-Object System.Collections.Generic.List[byte]
    
    # byte 0: key1_len
    $buf = New-Object System.Collections.Generic.List[byte]
    foreach ($b in $encryptedData) { $buf.Add($b) }
    foreach ($b in $key1) { $buf.Add($b) }
    $buf.Add([byte]$key1.Length)
    foreach ($b in $key2) { $buf.Add($b) }
    $buf.Add([byte]$key2.Length)
    
    # Checksum
    $checksum = 0
    foreach ($b in $buf.ToArray()) { $checksum = ($checksum + [int]$b) -band 0xFF }
    $buf.Add([byte]$checksum)
    
    return [Convert]::ToBase64String($buf.ToArray())
}

# Call the TimBus API
function Call-TimBusApi {
    param([string]$methodType, [string]$encryptedPayload)
    
    $encodedPayload = [System.Uri]::EscapeDataString($encryptedPayload)
    $url = "$BASE_URL/$methodType?ReqParams=$encodedPayload"
    
    try {
        $response = Invoke-RestMethod -Method Get -Uri $url -TimeoutSec 15 -ErrorAction Stop
        Write-Host "HTTP Status: Success"
        Write-Host "Response: $($response.Substring(0, [Math]::Min(500, $response.Length)))"
        return $response
    } catch {
        Write-Host "Error: $($_.Exception.Message)"
        if ($_.Exception.Response) {
            Write-Host "HTTP Status: $($_.Exception.Response.StatusCode)"
            $reader = New-Object System.IO.StreamReader($_.Exception.Response.GetResponseStream())
            $errorBody = $reader.ReadToEnd()
            $reader.Close()
            Write-Host "Error body: $errorBody"
        }
        return $null
    }
}

# Main
Write-Host "=== Testing TimBus API with encryption ==="
Write-Host ""

# Test 1: Route search
Write-Host "--- Test 1: Route search ---"
$payload = Build-RouteRequest -routeNo "35B"
$encrypted = Get-EncryptedPayload -data $payload
Write-Host "Encrypted payload length: $($encrypted.Length)"
Write-Host "Encrypted payload: $encrypted"
$resp = Call-TimBusApi -methodType "RouteBuytOptionV3" -encryptedPayload $encrypted

# Test 2: Vehicle state for bus 35B
Write-Host ""
Write-Host "--- Test 2: Vehicle state for bus 35B ---"
# Build payload: [int vehicle_id] + [string routeNo] + [string plate] + [bool active]
$busIdBytes = [System.BitConverter]::GetBytes([int]35)
$routeBytes = [System.Text.Encoding]::UTF8.GetBytes("35B")
$plateBytes = [System.Text.Encoding]::UTF8.GetBytes("50H41787")

$payload2 = New-Object byte[] (4 + 2 + $routeBytes.Length + 2 + $plateBytes.Length + 1)
$pos = 0
# busId as int32 (little endian)
for ($j = 0; $j -lt 4; $j++) { $payload2[$pos + $j] = $busIdBytes[$j] }
$pos += 4
# routeNo length as short (little endian)
$payload2[$pos] = [byte]($routeBytes.Length -band 255)
$payload2[$pos+1] = [byte](($routeBytes.Length -band 0xFF00) -shr 8)
$pos += 2
# routeNo bytes
for ($j = 0; $j -lt $routeBytes.Length; $j++) { $payload2[$pos + $j] = $routeBytes[$j] }
$pos += $routeBytes.Length
# plateNo length as short (little endian)
$payload2[$pos] = [byte]($plateBytes.Length -band 255)
$payload2[$pos+1] = [byte](($plateBytes.Length -band 0xFF00) -shr 8)
$pos += 2
# plateNo bytes
for ($j = 0; $j -lt $plateBytes.Length; $j++) { $payload2[$pos + $j] = $plateBytes[$j] }
$pos += $plateBytes.Length
# boolean active
$payload2[$pos] = 1

$encrypted2 = Get-EncryptedPayload -data $payload2
Write-Host "Encrypted payload length: $($encrypted2.Length)"
Write-Host "Encrypted payload: $encrypted2"
$resp2 = Call-TimBusApi -methodType "VehicleState" -encryptedPayload $encrypted2
