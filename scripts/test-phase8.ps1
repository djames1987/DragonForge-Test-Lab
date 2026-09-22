param(
    [string]$RepositoryUrl = "https://github.com/djames1987/DragonForge-Test-Lab.git",
    [string]$Revision = "phase-8-multi-machine-network-lab",
    [string]$LogDirectory = ".\test-logs",
    [ValidateSet("none", "controller", "node")]
    [string]$CrossNodeRole = "none",
    [string]$ControllerAddress = "",
    [string]$NodeId = "",
    [string]$KeyId = "phase8-probe"
)

$ErrorActionPreference = "Stop"

function Assert-LastExitCode {
    param([Parameter(Mandatory)][string]$Step)
    if ($LASTEXITCODE -ne 0) {
        throw "$Step failed: $LASTEXITCODE"
    }
}

if (-not (Test-Path $LogDirectory)) {
    New-Item -ItemType Directory -Path $LogDirectory -Force | Out-Null
}

$timestamp = Get-Date -Format "yyyyMMdd-HHmmss"
$logPath = Join-Path $LogDirectory "phase8-validation-$timestamp.log"

Start-Transcript -Path $logPath -Force | Out-Null

try {
    Write-Host "=== DragonForge Test Lab - Phase 8 Validation ==="
    Write-Host "Started: $(Get-Date -Format o)"
    Write-Host "Computer: $env:COMPUTERNAME"
    Write-Host "Working directory: $(Get-Location)"
    Write-Host "Revision: $Revision"
    Write-Host "Cross-node role: $CrossNodeRole"
    Write-Host ""

    Write-Host "[0/8] Environment"
    git --version
    cargo --version
    rustc --version
    gh --version

    Write-Host "[1/8] cargo fmt"
    cargo fmt --all -- --check
    Assert-LastExitCode "cargo fmt"

    Write-Host "[2/8] cargo clippy"
    cargo clippy --workspace --all-targets --all-features -- -D warnings
    Assert-LastExitCode "cargo clippy"

    Write-Host "[3/8] cargo test"
    cargo test --workspace --all-features
    Assert-LastExitCode "cargo test"

    Write-Host "[4/8] Distributed host readiness"
    & "$PSScriptRoot\check-distributed-host.ps1"

    Write-Host "[5/8] Distributed doctor"
    cargo run -p dragonforge-test-lab -- distributed-doctor
    Assert-LastExitCode "distributed-doctor"

    Write-Host "[6/8] Authenticated node/scheduler/network fixtures"
    cargo run -p dragonforge-test-lab -- distributed-fixtures
    Assert-LastExitCode "distributed-fixtures"

    Write-Host "[7/8] Optional real cross-node registration"
    if ($CrossNodeRole -eq "none") {
        Write-Host "SKIPPED: use -CrossNodeRole controller|node with -ControllerAddress for a host/VM registration probe."
    }
    else {
        if ([string]::IsNullOrWhiteSpace($ControllerAddress)) {
            throw "-ControllerAddress <private-ip:port> is required for cross-node validation"
        }
        if ([string]::IsNullOrWhiteSpace($env:DRAGONFORGE_NODE_SHARED_SECRET) -or
            $env:DRAGONFORGE_NODE_SHARED_SECRET.Length -lt 32) {
            throw "DRAGONFORGE_NODE_SHARED_SECRET must be set to at least 32 characters on both nodes"
        }

        if ($CrossNodeRole -eq "controller") {
            cargo run -p dragonforge-test-lab -- distributed-controller-once --bind $ControllerAddress --key-id $KeyId
            Assert-LastExitCode "distributed-controller-once"
        }
        else {
            if ([string]::IsNullOrWhiteSpace($NodeId)) {
                $NodeId = "node-$env:COMPUTERNAME"
            }
            cargo run -p dragonforge-test-lab -- distributed-node-connect --controller $ControllerAddress --node-id $NodeId --key-id $KeyId
            Assert-LastExitCode "distributed-node-connect"
        }
    }

    Write-Host "[8/8] GitHub-aware native worker regression"
    cargo run -p dragonforge-test-lab -- run-github --repo $RepositoryUrl --revision $Revision --sandbox native --no-status
    Assert-LastExitCode "run-github regression"

    Write-Host ""
    Write-Host "Phase 8 Multi-machine & Network Lab validation passed."
}
catch {
    Write-Host ""
    Write-Host "Phase 8 validation FAILED: $($_.Exception.Message)"
    exit 1
}
finally {
    Write-Host ""
    Write-Host "Log file: $logPath"
    Stop-Transcript | Out-Null
}
