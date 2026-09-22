param(
    [string]$RepositoryUrl = "https://github.com/djames1987/DragonForge-Test-Lab.git",
    [string]$Revision = "main",
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
$logPath = Join-Path $LogDirectory "phase18-validation-$timestamp.log"
$stateDb = Join-Path ([System.IO.Path]::GetTempPath()) "dragonforge-phase18-validation-$PID.sqlite3"
Remove-Item $stateDb -Force -ErrorAction SilentlyContinue
$previousDashboardToken = $env:DRAGONFORGE_DASHBOARD_TOKEN
$env:DRAGONFORGE_DASHBOARD_TOKEN = "phase18-validation-dashboard-token-000000000000"
Start-Transcript -Path $logPath -Force | Out-Null

try {
    Write-Host "=== DragonForge Test Lab - Phase 18 Validation ==="
    Write-Host "Started: $(Get-Date -Format o)"
    Write-Host "Revision: $Revision"

    Write-Host "[0/12] Environment"
    git --version
    cargo --version
    rustc --version
    gh --version

    Write-Host "[1/12] cargo fmt"
    Invoke-CargoCaptured -Step "cargo fmt" -Arguments @("fmt", "--all", "--", "--check") | Out-Null

    Write-Host "[2/12] cargo clippy"
    Invoke-CargoCaptured -Step "cargo clippy" -Arguments @(
        "clippy", "--workspace", "--all-targets", "--all-features", "--", "-D", "warnings"
    ) | Out-Null

    Write-Host "[3/12] cargo test"
    Invoke-CargoCaptured -Step "cargo test" -Arguments @(
        "test", "--workspace", "--all-features"
    ) | Out-Null

    Write-Host "[4/12] Dedicated dashboard crate tests"
    Invoke-CargoCaptured -Step "dashboard crate tests" -Arguments @(
        "test", "-p", "df-test-dashboard"
    ) | Out-Null

    Write-Host "[5/12] Controller schema remains v5"
    $controller = Invoke-CargoCaptured -Step "controller-state-doctor" -Arguments @(
        "run", "-p", "dragonforge-test-lab", "--",
        "controller-state-doctor", "--state-db", $stateDb
    )
    if (-not ($controller -match "schema_version=5")) {
        throw "controller doctor did not report schema v5"
    }

    Write-Host "[6/12] Dashboard doctor"
    $doctor = Invoke-CargoCaptured -Step "dashboard-doctor" -Arguments @(
        "run", "-p", "dragonforge-test-lab", "--",
        "dashboard-doctor", "--state-db", $stateDb
    )
    if (-not ($doctor -match "status=dashboard_ready")) {
        throw "dashboard doctor did not report ready"
    }
    if (-not ($doctor -match "read_only=true")) {
        throw "dashboard doctor did not report read-only mode"
    }

    Write-Host "[7/12] Dashboard security/data fixture"
    $fixture = Invoke-CargoCaptured -Step "dashboard-fixture" -Arguments @(
        "run", "-p", "dragonforge-test-lab", "--", "dashboard-fixture"
    )
    if (-not ($fixture -match "status=dashboard_fixture_passed")) {
        throw "dashboard fixture did not pass"
    }

    Write-Host "[8/12] Phase 17 intelligence regression"
    $intelligence = Invoke-CargoCaptured -Step "intelligence-integration-doctor" -Arguments @(
        "run", "-p", "dragonforge-test-lab", "--", "intelligence-integration-doctor"
    )
    if (-not ($intelligence -match "status=intelligence_integration_ready")) {
        throw "Phase 17 regression failed"
    }

    Write-Host "[9/12] Phase 16 and Phase 15 regressions"
    Invoke-CargoCaptured -Step "plan-fixture" -Arguments @(
        "run", "-p", "dragonforge-test-lab", "--", "plan-fixture"
    ) | Out-Null
    Invoke-CargoCaptured -Step "lifecycle-fixture" -Arguments @(
        "run", "-p", "dragonforge-test-lab", "--", "lifecycle-fixture"
    ) | Out-Null

    Write-Host "[10/12] Phase 14 observability regression"
    Invoke-CargoCaptured -Step "observability-fixture" -Arguments @(
        "run", "-p", "dragonforge-test-lab", "--", "observability-fixture"
    ) | Out-Null

    Write-Host "[11/12] General doctor reports Phase 18"
    $general = Invoke-CargoCaptured -Step "general doctor" -Arguments @(
        "run", "-p", "dragonforge-test-lab", "--", "doctor"
    )
    if (-not ($general -match "phase=18")) {
        throw "general doctor did not report Phase 18"
    }

    Write-Host "[12/12] GitHub-aware native worker regression"
    Invoke-CargoCaptured -Step "run-github regression" -Arguments @(
        "run", "-p", "dragonforge-test-lab", "--",
        "run-github",
        "--repo", $RepositoryUrl,
        "--revision", $Revision,
        "--sandbox", "native",
        "--no-status"
    ) | Out-Null

    Write-Host ""
    Write-Host "Phase 18 Dashboard validation passed."
}
catch {
    Write-Host ""
    Write-Host "Phase 18 validation FAILED: $($_.Exception.Message)"
    exit 1
}
finally {
    Remove-Item $stateDb -Force -ErrorAction SilentlyContinue
    $env:DRAGONFORGE_DASHBOARD_TOKEN = $previousDashboardToken
    Write-Host ""
    Write-Host "Log file: $logPath"
    Stop-Transcript | Out-Null
}
