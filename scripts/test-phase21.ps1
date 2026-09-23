param(
    [string]$Revision = "main",
    [string]$LogDirectory = ".\test-logs"
)

$ErrorActionPreference = "Stop"
New-Item -ItemType Directory -Path $LogDirectory -Force | Out-Null
$stamp = Get-Date -Format "yyyyMMdd-HHmmss"
$log = Join-Path $LogDirectory "phase21-validation-$stamp.log"
Start-Transcript -Path $log -Force | Out-Null
try {
    Write-Host "[0/7] Environment"
    git --version
    cargo --version
    rustc --version

    Write-Host "[1/7] cargo fmt"
    cargo fmt --all -- --check

    Write-Host "[2/7] strict cargo clippy"
    cargo clippy --workspace --all-targets --all-features -- -D warnings

    Write-Host "[3/7] workspace tests"
    cargo test --workspace --all-features

    Write-Host "[4/7] installer doctor"
    cargo run -p dragonforge-test-lab -- install-doctor

    Write-Host "[5/7] installer fixture"
    cargo run -p dragonforge-test-lab -- install-fixture

    Write-Host "[6/7] package build"
    .\scripts\package-release-windows.ps1 -Version 0.22.0 -OutputRoot .\test-packages
    $package = Join-Path $PWD "test-packages\dragonforge-test-lab-0.22.0-windows-x86_64"
    if ($env:PROCESSOR_ARCHITECTURE -eq "ARM64") {
        $package = Join-Path $PWD "test-packages\dragonforge-test-lab-0.22.0-windows-aarch64"
    }
    cargo run -p dragonforge-test-lab -- release-verify --manifest (Join-Path $package "release-manifest.json") --package-root $package

    Write-Host "[7/7] general doctor reports Phase 21"
    $general = cargo run -q -p dragonforge-test-lab -- doctor
    $general
    if ($general -notmatch "(?m)^phase=21$") { throw "general doctor did not report Phase 21" }

    Write-Host "Phase 21 Installer / Upgrades validation passed."
    Write-Host "Log file: $log"
}
finally {
    Stop-Transcript | Out-Null
}
