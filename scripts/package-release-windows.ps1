param(
    [Parameter(Mandatory=$true)][string]$Version,
    [string]$OutputRoot = ".\dist"
)

$ErrorActionPreference = "Stop"
if ($Version -notmatch '^[0-9]+\.[0-9]+\.[0-9]+([.-][A-Za-z0-9._-]+)?$') {
    throw "Version must be a semantic version such as 0.22.0."
}

cargo build --release -p dragonforge-test-lab
if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }

$binary = Join-Path $PWD "target\release\dragonforge-test-lab.exe"
if (-not (Test-Path -LiteralPath $binary -PathType Leaf)) { throw "Release binary missing: $binary" }

$arch = switch ($env:PROCESSOR_ARCHITECTURE) {
    "AMD64" { "x86_64" }
    "ARM64" { "aarch64" }
    "x86" { "x86" }
    default { throw "Unsupported Windows architecture: $env:PROCESSOR_ARCHITECTURE" }
}

$package = Join-Path $OutputRoot "dragonforge-test-lab-$Version-windows-$arch"
if (Test-Path -LiteralPath $package) { Remove-Item -LiteralPath $package -Recurse -Force }
New-Item -ItemType Directory -Path $package -Force | Out-Null
Copy-Item -LiteralPath $binary -Destination (Join-Path $package "dragonforge-test-lab.exe")
$sha = (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $package "dragonforge-test-lab.exe")).Hash.ToLowerInvariant()

[ordered]@{
    schema_version = 1
    version = $Version
    target_os = "windows"
    target_arch = $arch
    binary_file = "dragonforge-test-lab.exe"
    binary_sha256 = $sha
} | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $package "release-manifest.json") -Encoding UTF8

Write-Host "Package ready: $package"
Write-Host "SHA-256: $sha"
