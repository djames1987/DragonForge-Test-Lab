param(
    [string]$RepositoryUrl = "https://github.com/djames1987/DragonForge-Test-Lab.git",
    [string]$Revision = "phase-6-windows-integration",
    [string]$LogDirectory = ".\test-logs",
    [switch]$IncludePrivileged,
    [string]$MsiPath = ""
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
$logPath = Join-Path $LogDirectory "phase6-validation-$timestamp.log"
Start-Transcript -Path $logPath -Force | Out-Null

try {
    Write-Host "=== DragonForge Test Lab - Phase 6 Validation ==="
    Write-Host "Started: $(Get-Date -Format o)"
    Write-Host "Computer: $env:COMPUTERNAME"
    Write-Host "Working directory: $(Get-Location)"
    Write-Host "Revision: $Revision"
    Write-Host ""

    Write-Host "[0/9] Environment"
    git --version
    cargo --version
    rustc --version
    gh --version

    Write-Host "[1/9] cargo fmt"
    cargo fmt --all -- --check
    Assert-LastExitCode "cargo fmt"

    Write-Host "[2/9] cargo clippy"
    cargo clippy --workspace --all-targets --all-features -- -D warnings
    Assert-LastExitCode "cargo clippy"

    Write-Host "[3/9] cargo test"
    cargo test --workspace --all-features
    Assert-LastExitCode "cargo test"

    Write-Host "[4/9] Windows host readiness"
    & "$PSScriptRoot\check-windows-integration.ps1"

    Write-Host "[5/9] Windows integration doctor"
    cargo run -p dragonforge-test-lab -- windows-doctor
    Assert-LastExitCode "windows-doctor"

    Write-Host "[6/9] Safe Windows fixtures"
    cargo run -p dragonforge-test-lab -- windows-fixtures
    Assert-LastExitCode "windows-fixtures"

    Write-Host "[7/9] Installer surface"
    if ([string]::IsNullOrWhiteSpace($MsiPath)) {
        Write-Host "MSI signature inspection skipped: no -MsiPath supplied."
        Write-Host "Windows Installer service and msiexec availability were verified by windows-doctor."
    }
    else {
        cargo run -p dragonforge-test-lab -- windows-installer-info --path $MsiPath
        Assert-LastExitCode "windows-installer-info"
    }

    Write-Host "[8/9] Privileged Windows fixtures"
    if ($IncludePrivileged) {
        $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
        $principal = New-Object Security.Principal.WindowsPrincipal($identity)
        if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
            throw "-IncludePrivileged requires an elevated PowerShell session"
        }

        cargo run -p dragonforge-test-lab -- windows-privileged-fixtures --confirm
        Assert-LastExitCode "windows-privileged-fixtures"
    }
    else {
        Write-Host "SKIPPED: rerun from elevated PowerShell with -IncludePrivileged to test service and Event Log writes."
    }

    Write-Host "[9/9] GitHub-aware native worker regression"
    cargo run -p dragonforge-test-lab -- run-github --repo $RepositoryUrl --revision $Revision --sandbox native --no-status
    Assert-LastExitCode "run-github regression"

    Write-Host ""
    Write-Host "Phase 6 Windows integration validation passed."
    if (-not $IncludePrivileged) {
        Write-Host "Privileged service/Event Log mutation lane was not requested."
    }
}
catch {
    Write-Host ""
    Write-Host "Phase 6 validation FAILED: $($_.Exception.Message)"
    exit 1
}
finally {
    Write-Host ""
    Write-Host "Log file: $logPath"
    Stop-Transcript | Out-Null
}
