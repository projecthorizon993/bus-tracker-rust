#!/usr/bin/env pwsh
# Test VinBus API encryption
$ErrorActionPreference = "Stop"
$BASE_URL = "http://www.timbus.vn:9149/SearchBuyt"

function Get-Checksum {
    param([byte[]]$data)
    $sum = 0
    for ($i = 0; $i -lt ($data.Length - 1); $i++) {
        $sum = $sum + [int]$data[$i]
    }
    return $sum -band 0xFF
}

function Get-RandomKey {
    param([int]$minLen = 10, [int]$maxLen = 15)
    $len = Get-Random -Minimum $minLen -Maximum ($maxLen + 1)
    $key = New-Object byte[] $len
    for ($i = 0; $i -lt $len; $i++) {
        $key[$i] = [byte](Get-Random -Minimum 48 -Maximum 127)
    }
    return $key
}

function Get-Xor {
    param([byte[]]$data, [byte[]]$key)
    $result = New-Object byte[] $data.Length
    for ($i = 0; $i -lt $data.Length; $i++) {
        $result[$i] = $data[$i] -bxor $key[$i % $key.Length]
    }
    return $result
}

function Build-RouteRequest {
    param([double]$lat1 = 21.0285, [double]$lng1 = 105.8542, [double]$lat2 = 21.0385, [double]$destLng = 105.8642)
    $buf = New-Object byte[] 17
    $i = 0
    $floatBytes = [System.BitConverter]::GetBytes([single]$lat1)
    for ($j = 0; $j -lt 4; $j++) { $buf[$i + $j] = $floatBytes[$j] } $i += 4
    $floatBytes = [System.BitConverter]::GetBytes([single]$lng1)
    for ($j = 0; $j -lt 4; $j++) { $buf[$i + $j] = $floatBytes[$j] } $i += 4
    $floatBytes = [System.BitConverter]::GetBytes([single]$lat2)
    for ($j = 0; $j -lt 4; $j++) { $buf[$i + $j] = $floatBytes[$j] } $i += 4
    $floatBytes = [System.BitConverter]::GetBytes([single]$destLng)
    for ($j = 0; $j -lt 4; $j++) { $buf[$i + $j] = $floatBytes[$j] } $i += 4
    $buf[$i] = 2
    return $buf
}

function Get-EncryptedPayload {
    param([byte[]]$data)
    $key1 = Get-RandomKey
    $key2 = Get-RandomKey
    $randInt = [int](Get-Random -Minimum 1 -Maximum ([int]::MaxValue))
    $randIntBytes = [System.BitConverter]::GetBytes([int]$randInt)
    $fullData = New-Object byte[] (4 + $data.Length)
    for ($j = 0; $j -lt 4; $j++) { $fullData[$j] = $randIntBytes[$j] }
    for ($j = 0; $j -lt $data.Length; $j++) { $fullData[4 + $j] = $data[$j] }
    $encrypted = Get-Xor -data $fullData -key $key1
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

function Call-TimBusApi {
    param([string]$methodType, [string]$encryptedPayload)
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
            Write-Host "HTTP Status: $($_.Exception.Response.StatusCode)"
            $reader = New-Object System.IO.StreamReader($_.Exception.Response.GetResponseStream())
            $errorBody = $reader.ReadToEnd()
            $reader.Close()
            Write-Host "Error body: $errorBody"
        }
        return $null
    }
}

# Test 1: Route search
Write-Host "=== Test 1: Route search ==="
$payload = Build-RouteRequest
$encrypted = Get-EncryptedPayload -data $payload
Write-Host "Encrypted payload length: $($encrypted.Length)"
Write-Host "Encrypted payload: $encrypted"
$resp = Call-TimBusApi -methodType "RouteBuytOptionV3" -encryptedPayload $encrypted

Write-Host ""
Write-Host "--- Test 2: Vehicle state for bus 35B ---"
$busIdBytes = [System.BitConverter]::GetBytes([int]35)
$routeBytes = [System.Text.Encoding]::UTF8.GetBytes("35B")
$plateBytes = [System.Text.Encoding]::UTF8.GetBytes("50H41787")
$payload2 = New-Object byte[] (4 + 2 + $routeBytes.Length + 2 + $plateBytes.Length + 1)
$pos = 0
for ($j = 0; $j -lt 4; $j++) { $payload2[$pos + $j] = $busIdBytes[$j] } $pos += 4
$payload2[$pos] = [byte]($routeBytes.Length -band 255); $payload2[$pos+1] = [byte](($routeBytes.Length -band 0xFF00) -shr 8); $pos += 2
for ($j = 0; $j -lt $routeBytes.Length; $j++) { $payload2[$pos + $j] = $routeBytes[$j] } $pos += $routeBytes.Length
$payload2[$pos] = [byte]($plateBytes.Length -band 255); $payload2[$pos+1] = [byte](($plateBytes.Length -band 0xFF00) -shr 8); $pos += 2
for ($j = 0; $j -lt $plateBytes.Length; $j++) { $payload2[$pos + $j] = $plateBytes[$j] } $pos += $plateBytes.Length
$payload2[$pos] = 1
$encrypted2 = Get-EncryptedPayload -data $payload2
Write-Host "Encrypted payload length: $($encrypted2.Length)"
Write-Host "Encrypted payload: $encrypted2"
$resp2 = Call-TimBusApi -methodType "VehicleState" -encryptedPayload $encrypted2
