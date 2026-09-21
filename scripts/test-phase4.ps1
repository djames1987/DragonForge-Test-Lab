param(
    [string]$VmRoot = "C:\DragonForge-Test-Lab-VMs",
    [string]$SwitchName = "Default Switch",
    [string]$RepositoryUrl = "https://github.com/djames1987/DragonForge-Test-Lab.git",
    [string]$Revision = "phase-4-vm-lab",
    [string]$LogDirectory = ".\test-logs"
)

$ErrorActionPreference = "Stop"

if (-not (Test-Path $LogDirectory)) {
    New-Item -ItemType Directory -Path $LogDirectory -Force | Out-Null
}

$timestamp = Get-Date -Format "yyyyMMdd-HHmmss"
$logPath = Join-Path $LogDirectory "phase4-validation-$timestamp.log"
Start-Transcript -Path $logPath -Force | Out-Null

try {
    Write-Host "=== DragonForge Test Lab - Phase 4 Validation ==="
    Write-Host "Started: $(Get-Date -Format o)"
    Write-Host "Computer: $env:COMPUTERNAME"
    Write-Host "Working directory: $(Get-Location)"
    Write-Host "VM root: $VmRoot"
    Write-Host "Switch: $SwitchName"
    Write-Host ""

    Write-Host "[0/7] Environment"
    git --version
    cargo --version
    rustc --version
    gh --version

    Write-Host "[1/7] cargo fmt"
    cargo fmt --all -- --check
    if ($LASTEXITCODE -ne 0) { throw "cargo fmt failed: $LASTEXITCODE" }

    Write-Host "[2/7] cargo clippy"
    cargo clippy --workspace --all-targets --all-features -- -D warnings
    if ($LASTEXITCODE -ne 0) { throw "cargo clippy failed: $LASTEXITCODE" }

    Write-Host "[3/7] cargo test"
    cargo test --workspace --all-features
    if ($LASTEXITCODE -ne 0) { throw "cargo test failed: $LASTEXITCODE" }

    Write-Host "[4/7] Test Lab doctor"
    cargo run -p dragonforge-test-lab -- doctor
    if ($LASTEXITCODE -ne 0) { throw "doctor failed: $LASTEXITCODE" }

    Write-Host "[5/7] Hyper-V host readiness"
    & "$PSScriptRoot\check-hyperv-host.ps1" -VmRoot $VmRoot -SwitchName $SwitchName

    Write-Host "[6/7] VM Lab doctor"
    cargo run -p dragonforge-test-lab -- vm-doctor --vm-root $VmRoot --image-root (Join-Path $VmRoot "images") --switch $SwitchName
    if ($LASTEXITCODE -ne 0) { throw "vm-doctor failed: $LASTEXITCODE" }

    Write-Host "[7/7] GitHub-aware native worker regression"
    cargo run -p dragonforge-test-lab -- run-github --repo $RepositoryUrl --revision $Revision --sandbox native --no-status
    if ($LASTEXITCODE -ne 0) { throw "run-github regression failed: $LASTEXITCODE" }

    Write-Host ""
    Write-Host "Phase 4 core validation passed."
    Write-Host "Note: VM create/checkpoint/restore validation requires prepared golden VHDX images as documented in docs/HOST-SETUP-HYPERV.md."
}
catch {
    Write-Host ""
    Write-Host "Phase 4 validation FAILED: $($_.Exception.Message)"
    exit 1
}
finally {
    Write-Host ""
    Write-Host "Log file: $logPath"
    Stop-Transcript | Out-Null
}
