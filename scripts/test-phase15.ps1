param(
    [string]$RepositoryUrl = "https://github.com/djames1987/DragonForge-Test-Lab.git",
    [string]$Revision = "phase-15-recovery-retry-job-lifecycle",
    [string]$LogDirectory = ".\test-logs"
)

$ErrorActionPreference = "Stop"

function Invoke-CargoCaptured {
    param(
        [Parameter(Mandatory)][string]$Step,
        [Parameter(Mandatory)][string[]]$Arguments
    )

    $previousPreference = $ErrorActionPreference
    try {
        $ErrorActionPreference = "Continue"
        $output = & cargo @Arguments 2>&1
        $exitCode = $LASTEXITCODE
    }
    finally {
        $ErrorActionPreference = $previousPreference
    }

    $output | ForEach-Object { Write-Host $_ }
    if ($exitCode -ne 0) {
        throw "$Step failed: $exitCode"
    }
    return $output
}

if (-not (Test-Path $LogDirectory)) {
    New-Item -ItemType Directory -Path $LogDirectory -Force | Out-Null
}

$timestamp = Get-Date -Format "yyyyMMdd-HHmmss"
$logPath = Join-Path $LogDirectory "phase15-validation-$timestamp.log"
$stateDb = Join-Path ([System.IO.Path]::GetTempPath()) "dragonforge-phase15-validation-$PID.sqlite3"
Remove-Item $stateDb -Force -ErrorAction SilentlyContinue

Start-Transcript -Path $logPath -Force | Out-Null

try {
    Write-Host "=== DragonForge Test Lab - Phase 15 Validation ==="
    Write-Host "Started: $(Get-Date -Format o)"
    Write-Host "Computer: $env:COMPUTERNAME"
    Write-Host "Working directory: $(Get-Location)"
    Write-Host "Revision: $Revision"
    Write-Host ""

    Write-Host "[0/11] Environment"
    git --version
    cargo --version
    rustc --version
    gh --version

    Write-Host "[1/11] cargo fmt"
    Invoke-CargoCaptured -Step "cargo fmt" -Arguments @(
        "fmt", "--all", "--", "--check"
    ) | Out-Null

    Write-Host "[2/11] cargo clippy"
    Invoke-CargoCaptured -Step "cargo clippy" -Arguments @(
        "clippy", "--workspace", "--all-targets", "--all-features", "--", "-D", "warnings"
    ) | Out-Null

    Write-Host "[3/11] cargo test"
    Invoke-CargoCaptured -Step "cargo test" -Arguments @(
        "test", "--workspace", "--all-features"
    ) | Out-Null

    Write-Host "[4/11] Controller schema v3"
    $controller = Invoke-CargoCaptured -Step "controller-state-doctor" -Arguments @(
        "run", "-p", "dragonforge-test-lab", "--",
        "controller-state-doctor", "--state-db", $stateDb
    )
    if (-not ($controller -match "schema_version=3")) {
        throw "controller doctor did not report schema v3"
    }

    Write-Host "[5/11] Lifecycle doctor"
    $doctor = Invoke-CargoCaptured -Step "lifecycle-doctor" -Arguments @(
        "run", "-p", "dragonforge-test-lab", "--", "lifecycle-doctor"
    )
    if (-not ($doctor -match "status=lifecycle_ready")) {
        throw "lifecycle doctor did not report ready"
    }

    Write-Host "[6/11] Recovery / retry / lifecycle fixture"
    $fixture = Invoke-CargoCaptured -Step "lifecycle-fixture" -Arguments @(
        "run", "-p", "dragonforge-test-lab", "--", "lifecycle-fixture"
    )
    if (-not ($fixture -match "status=lifecycle_fixture_passed")) {
        throw "lifecycle fixture did not pass"
    }
    foreach ($field in @(
        '"schema_v3": true',
        '"transient_retry_scheduled": true',
        '"retry_not_early": true',
        '"retry_assigned_when_due": true',
        '"test_failure_terminal": true',
        '"interrupted_retry_scheduled": true',
        '"manual_interrupted_reschedule": true',
        '"attempt_history_preserved": true',
        '"audit_chain_verified": true'
    )) {
        if (-not ($fixture -match [regex]::Escape($field))) {
            throw "lifecycle fixture missing expected result: $field"
        }
    }

    Write-Host "[7/11] Phase 14 observability regression"
    Invoke-CargoCaptured -Step "observability-fixture" -Arguments @(
        "run", "-p", "dragonforge-test-lab", "--", "observability-fixture"
    ) | Out-Null

    Write-Host "[8/11] Phase 13 worker-service regression"
    Invoke-CargoCaptured -Step "worker-service-fixture" -Arguments @(
        "run", "-p", "dragonforge-test-lab", "--", "worker-service-fixture"
    ) | Out-Null

    Write-Host "[9/11] Phase 12 mTLS regression"
    Invoke-CargoCaptured -Step "identity-fixture" -Arguments @(
        "run", "-p", "dragonforge-test-lab", "--", "identity-fixture"
    ) | Out-Null

    Write-Host "[10/11] General doctor reports Phase 15"
    $general = Invoke-CargoCaptured -Step "general doctor" -Arguments @(
        "run", "-p", "dragonforge-test-lab", "--", "doctor"
    )
    if (-not ($general -match "phase=15")) {
        throw "general doctor did not report Phase 15"
    }

    Write-Host "[11/11] GitHub-aware native worker regression"
    Invoke-CargoCaptured -Step "run-github regression" -Arguments @(
        "run", "-p", "dragonforge-test-lab", "--",
        "run-github",
        "--repo", $RepositoryUrl,
        "--revision", $Revision,
        "--sandbox", "native",
        "--no-status"
    ) | Out-Null

    Write-Host ""
    Write-Host "Phase 15 Recovery / Retry / Job Lifecycle validation passed."
}
catch {
    Write-Host ""
    Write-Host "Phase 15 validation FAILED: $($_.Exception.Message)"
    exit 1
}
finally {
    Remove-Item $stateDb -Force -ErrorAction SilentlyContinue
    Write-Host ""
    Write-Host "Log file: $logPath"
    Stop-Transcript | Out-Null
}
