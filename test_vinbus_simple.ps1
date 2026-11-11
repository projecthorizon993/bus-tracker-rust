$ErrorActionPreference = "Stop"
$BASE_URL = "http://www.timbus.vn:9149/SearchBuyt"

# Test 1: Simple GET with base64 payload
Write-Host "Test 1: GET with simple base64 payload"
$payload = [System.Convert]::ToBase64String([System.Text.Encoding]::UTF8.GetBytes("test"))
$url = "$BASE_URL/RouteBuytOptionV3?ReqParams=$payload"
Write-Host "URL: $url"
try {
    $resp = Invoke-RestMethod -Method Get -Uri $url -TimeoutSec 15 -ErrorAction Stop
    Write-Host "HTTP Status: Success"
    Write-Host "Response: $resp"
} catch {
    Write-Host "Error: $($_.Exception.Message)"
    if ($_.Exception.Response) {
        Write-Host "HTTP Status: $($_.Exception.Response.StatusCode)"
    }
}

Write-Host ""

# Test 2: POST with form data
Write-Host "Test 2: POST with form data"
$payload = [System.Convert]::ToBase64String([System.Text.Encoding]::UTF8.GetBytes("test"))
$url = "$BASE_URL/RouteBuytOptionV3"
try {
    $resp = Invoke-RestMethod -Method Post -Uri $url -Body "ReqParams=$encryptedPayload" -ContentType "application/x-www-form-urlencoded" -TimeoutSec 15 -ErrorAction Stop
    Write-Host "HTTP Status: Success"
    Write-Host "Response: $resp"
} catch {
    Write-Host "Error: $($_.Exception.Message)"
    if ($_.Exception.Response) {
        Write-Host "HTTP Status: $($_.Exception.Response.StatusCode)"
        $reader = New-Object System.IO.StreamReader($_.Exception.Response.GetResponseStream())
        $errorBody = $reader.ReadToEnd()
        $reader.Close()
        Write-Host "Error body: $errorBody"
    }
}