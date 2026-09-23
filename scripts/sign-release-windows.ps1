param([Parameter(Mandatory=$true)][string]$Binary)

$ErrorActionPreference = "Stop"

if (-not (Test-Path -LiteralPath $Binary -PathType Leaf)) {
    throw "Binary not found: $Binary"
}

$certPath = $env:DRAGONFORGE_WINDOWS_SIGN_CERT_PATH
$password = $env:DRAGONFORGE_WINDOWS_SIGN_CERT_PASSWORD
if (-not $certPath -or -not (Test-Path -LiteralPath $certPath -PathType Leaf)) {
    throw "DRAGONFORGE_WINDOWS_SIGN_CERT_PATH must point to a PFX file"
}
if (-not $password) {
    throw "DRAGONFORGE_WINDOWS_SIGN_CERT_PASSWORD must be set"
}

$signtool = Get-Command signtool.exe -ErrorAction Stop
$securePassword = ConvertTo-SecureString $password -AsPlainText -Force
$imported = $null

try {
    $imported = Import-PfxCertificate -FilePath $certPath -CertStoreLocation Cert:\CurrentUser\My -Password $securePassword -Exportable:$false
    if (-not $imported -or -not $imported.Thumbprint) {
        throw "Unable to import the signing certificate."
    }

    $args = @("sign", "/fd", "SHA256", "/s", "My", "/sha1", $imported.Thumbprint)
    $timestamp = $env:DRAGONFORGE_WINDOWS_TIMESTAMP_URL
    if ($timestamp) {
        $args += @("/tr", $timestamp, "/td", "SHA256")
    }
    $args += $Binary

    & $signtool.Source @args
    if ($LASTEXITCODE -ne 0) {
        throw "signtool failed with exit code $LASTEXITCODE"
    }

    & $signtool.Source verify /pa /v $Binary
    if ($LASTEXITCODE -ne 0) {
        throw "signtool verification failed"
    }

    Write-Host "Authenticode signature verified: $Binary"
}
finally {
    if ($imported -and $imported.PSPath) {
        Remove-Item -LiteralPath $imported.PSPath -Force -ErrorAction SilentlyContinue
    }
    $securePassword = $null
    $password = $null
}
