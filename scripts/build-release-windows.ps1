param(
    [Parameter(Mandatory=$true)][ValidateSet("dev","beta","stable")][string]$Channel,
    [Parameter(Mandatory=$true)][string]$ReleaseVersion,
    [string]$OutputRoot = ".\dist"
)
$ErrorActionPreference = "Stop"
$baseVersion = ($ReleaseVersion -split "-",2)[0]
if ($baseVersion -notmatch '^[0-9]+\.[0-9]+\.[0-9]+$') { throw "Invalid base version." }

$packageRoot = Join-Path $OutputRoot "packages"
.\scripts\package-release-windows.ps1 -Version $baseVersion -OutputRoot $packageRoot
$arch = switch ($env:PROCESSOR_ARCHITECTURE) {
    "AMD64" { "x86_64" }
    "ARM64" { "aarch64" }
    "x86" { "x86" }
    default { throw "Unsupported Windows architecture: $env:PROCESSOR_ARCHITECTURE" }
}
$packageDir = Join-Path $packageRoot "dragonforge-test-lab-$baseVersion-windows-$arch"
$binary = Join-Path $packageDir "dragonforge-test-lab.exe"

if ($Channel -eq "stable") {
    if (-not $env:DRAGONFORGE_WINDOWS_SIGN_CERT_PATH) {
        throw "Stable Windows releases require DRAGONFORGE_WINDOWS_SIGN_CERT_PATH."
    }
    .\scripts\sign-release-windows.ps1 -Binary $binary
    $manifestPath = Join-Path $packageDir "release-manifest.json"
    $manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
    $manifest.binary_sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $binary).Hash.ToLowerInvariant()
    $manifest | ConvertTo-Json | Set-Content -LiteralPath $manifestPath -Encoding UTF8
}

$archive = Join-Path $OutputRoot "dragonforge-test-lab-$ReleaseVersion-windows-$arch.zip"
if (Test-Path -LiteralPath $archive) { Remove-Item -LiteralPath $archive -Force }
Compress-Archive -Path (Join-Path $packageDir "*") -DestinationPath $archive -CompressionLevel Optimal
Write-Host "Release archive: $archive"
