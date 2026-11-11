# Test VinBus API endpoints with different payloads
$ErrorActionPreference = "Stop"
$BASE_URL = "http://www.timbus.vn:9149/SearchBuyt/RouteBuytOptionV3"

$payloads = @(
    "dGVzdA==",           # test
    "YmFuZA==",           # bus
    "Ym91ZA==",           # bus2
    "ZGVtbw==",           # demo
    "cmVnaW9uZ29kZQ==",   # regionCode
    "YnVz"                 # bus short
)

foreach ($payload in $payloads) {
    $url = "$BASE_URL?ReqParams=$payload"
    try {
        $resp = Invoke-RestMethod -Method Get -Uri $url -TimeoutSec 10 -ErrorAction Stop
        Write-Host "PAYLOAD: $encryptedPayload -> HTTP $($resp.StatusCode): $($response)"
    } catch {
        Write-Host "PAYLOAD: $payload -> ERROR: $($_.Exception.Message)"
        if ($_.Exception.Response) {
            Write-Host "  Status: $($_.Exception.Response.StatusCode)"
        }
    }
}

# Test direct POST
Write-Host ""
Write-Host "--- POST test ---"
try {
    $resp = Invoke-RestMethod -Method Post -Uri "http://www.timbus.vn:9149/SearchBuyt/RouteBuytOptionV3" -Body "ReqParams=dGVzdA==" -ContentType "application/x-www-form-urlencoded" -TimeoutSec 15 -ErrorAction Stop
    Write-Host "POST Status: Success"
    Write-Host "Response: $resp"
} catch {
    Write-Host "POST Error: $($_.Exception.Message)"
    if ($_.Exception.Response) {
        Write-Host "  HTTP Status: $($_.Exception.Response.StatusCode)"
    }
}