param([Parameter(Mandatory=$true)][string]$Binary)
$ErrorActionPreference = "Stop"
if (-not (Test-Path -LiteralPath $Binary -PathType Leaf)) { throw "Binary not found: $Binary" }
$cert = $env:DRAGONFORGE_WINDOWS_SIGN_CERT_PATH
$password = $env:DRAGONFORGE_WINDOWS_SIGN_CERT_PASSWORD
if (-not $cert -or -not (Test-Path -LiteralPath $cert -PathType Leaf)) {
    throw "DRAGONFORGE_WINDOWS_SIGN_CERT_PATH must point to a PFX file"
}
if (-not $password) { throw "DRAGONFORGE_WINDOWS_SIGN_CERT_PASSWORD must be set" }
$signtool = Get-Command signtool.exe -ErrorAction Stop
$timestamp = $env:DRAGONFORGE_WINDOWS_TIMESTAMP_URL
$args = @("sign","/fd","SHA256","/f",$cert,"/p",$password)
if ($timestamp) { $args += @("/tr",$timestamp,"/td","SHA256") }
$args += $Binary
& $signtool.Source @args
if ($LASTEXITCODE -ne 0) { throw "signtool failed with exit code $LASTEXITCODE" }
& $signtool.Source verify /pa /v $Binary
if ($LASTEXITCODE -ne 0) { throw "signtool verification failed" }
Write-Host "Authenticode signature verified: $Binary"
