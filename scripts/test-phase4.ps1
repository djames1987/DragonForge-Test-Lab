param(
    [string]$VmRoot = "C:\DragonForge-Test-Lab-VMs",
    [string]$SwitchName = "Default Switch",
    [string]$RepositoryUrl = "https://github.com/djames1987/DragonForge-Test-Lab.git",
    [string]$Revision = "phase-4-vm-lab",
    [string]$BaseVhdx = "",
    [ValidateSet("windows", "linux")]
    [string]$GuestOs = "windows",
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

    Write-Host "[0/11] Environment"
    git --version
    cargo --version
    rustc --version
    gh --version

    Write-Host "[1/11] cargo fmt"
    cargo fmt --all -- --check
    if ($LASTEXITCODE -ne 0) { throw "cargo fmt failed: $LASTEXITCODE" }

    Write-Host "[2/11] cargo clippy"
    cargo clippy --workspace --all-targets --all-features -- -D warnings
    if ($LASTEXITCODE -ne 0) { throw "cargo clippy failed: $LASTEXITCODE" }

    Write-Host "[3/11] cargo test"
    cargo test --workspace --all-features
    if ($LASTEXITCODE -ne 0) { throw "cargo test failed: $LASTEXITCODE" }

    Write-Host "[4/11] Test Lab doctor"
    cargo run -p dragonforge-test-lab -- doctor
    if ($LASTEXITCODE -ne 0) { throw "doctor failed: $LASTEXITCODE" }

    Write-Host "[5/11] Hyper-V host readiness"
    & "$PSScriptRoot\check-hyperv-host.ps1" -VmRoot $VmRoot -SwitchName $SwitchName

    Write-Host "[6/11] VM Lab doctor"
    cargo run -p dragonforge-test-lab -- vm-doctor --vm-root $VmRoot --image-root (Join-Path $VmRoot "images") --switch $SwitchName
    if ($LASTEXITCODE -ne 0) { throw "vm-doctor failed: $LASTEXITCODE" }

    Write-Host "[7/11] GitHub-aware native worker regression"
    cargo run -p dragonforge-test-lab -- run-github --repo $RepositoryUrl --revision $Revision --sandbox native --no-status
    if ($LASTEXITCODE -ne 0) { throw "run-github regression failed: $LASTEXITCODE" }

    $validationVm = "DragonForge-Phase4-Validation"
    if (-not [string]::IsNullOrWhiteSpace($BaseVhdx)) {
        Write-Host "[8/11] Create disposable validation VM"
        cargo run -p dragonforge-test-lab -- vm-create --name $validationVm --guest-os $GuestOs --base-vhdx $BaseVhdx --vm-root $VmRoot --image-root (Join-Path $VmRoot "images") --switch $SwitchName --memory-mib 2048 --processors 2
        if ($LASTEXITCODE -ne 0) { throw "vm-create failed: $LASTEXITCODE" }

        Write-Host "[9/11] Create clean baseline"
        cargo run -p dragonforge-test-lab -- vm-baseline --name $validationVm
        if ($LASTEXITCODE -ne 0) { throw "vm-baseline failed: $LASTEXITCODE" }

        Write-Host "[10/11] Start and restore baseline"
        cargo run -p dragonforge-test-lab -- vm-start --name $validationVm
        if ($LASTEXITCODE -ne 0) { throw "vm-start failed: $LASTEXITCODE" }
        Start-Sleep -Seconds 2
        cargo run -p dragonforge-test-lab -- vm-restore --name $validationVm
        if ($LASTEXITCODE -ne 0) { throw "vm-restore failed: $LASTEXITCODE" }

        Write-Host "[11/11] Stop and destroy validation VM"
        cargo run -p dragonforge-test-lab -- vm-stop --name $validationVm
        if ($LASTEXITCODE -ne 0) { throw "vm-stop failed: $LASTEXITCODE" }
        cargo run -p dragonforge-test-lab -- vm-destroy --name $validationVm --vm-root $VmRoot --confirm
        if ($LASTEXITCODE -ne 0) { throw "vm-destroy failed: $LASTEXITCODE" }

        Write-Host ""
        Write-Host "Phase 4 full VM lifecycle validation passed."
    }
    else {
        Write-Host "[8/11 - 11/11] VM lifecycle skipped: no -BaseVhdx supplied."
        Write-Host ""
        Write-Host "Phase 4 core validation passed."
        Write-Host "For full lifecycle validation, prepare a golden VHDX per docs/HOST-SETUP-HYPERV.md and rerun with -BaseVhdx."
    }
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
