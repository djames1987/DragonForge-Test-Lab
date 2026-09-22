param(
    [string]$RepositoryUrl = "https://github.com/djames1987/DragonForge-Test-Lab.git",
    [string]$BaseRevision = "main",
    [string]$Revision = "phase-17-intelligence-integration",
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
$logPath = Join-Path $LogDirectory "phase17-validation-$timestamp.log"
$stateDb = Join-Path ([System.IO.Path]::GetTempPath()) "dragonforge-phase17-validation-$PID.sqlite3"
Remove-Item $stateDb -Force -ErrorAction SilentlyContinue
Start-Transcript -Path $logPath -Force | Out-Null

try {
    Write-Host "=== DragonForge Test Lab - Phase 17 Validation ==="
    Write-Host "Started: $(Get-Date -Format o)"
    Write-Host "Revision: $Revision"

    Write-Host "[0/11] Environment"
    git --version
    cargo --version
    rustc --version
    gh --version

    Write-Host "[1/11] cargo fmt"
    Invoke-CargoCaptured -Step "cargo fmt" -Arguments @("fmt", "--all", "--", "--check") | Out-Null

    Write-Host "[2/11] cargo clippy"
    Invoke-CargoCaptured -Step "cargo clippy" -Arguments @(
        "clippy", "--workspace", "--all-targets", "--all-features", "--", "-D", "warnings"
    ) | Out-Null

    Write-Host "[3/11] cargo test"
    Invoke-CargoCaptured -Step "cargo test" -Arguments @(
        "test", "--workspace", "--all-features"
    ) | Out-Null

    Write-Host "[4/11] Controller schema v5"
    $controller = Invoke-CargoCaptured -Step "controller-state-doctor" -Arguments @(
        "run", "-p", "dragonforge-test-lab", "--",
        "controller-state-doctor", "--state-db", $stateDb
    )
    if (-not ($controller -match "schema_version=5")) {
        throw "controller doctor did not report schema v5"
    }

    Write-Host "[5/11] Intelligence integration doctor"
    $doctor = Invoke-CargoCaptured -Step "intelligence-integration-doctor" -Arguments @(
        "run", "-p", "dragonforge-test-lab", "--", "intelligence-integration-doctor"
    )
    if (-not ($doctor -match "status=intelligence_integration_ready")) {
        throw "Phase 17 integration doctor did not report ready"
    }

    Write-Host "[6/11] Dedicated Phase 17 integration tests"
    Invoke-CargoCaptured -Step "Phase 17 crate tests" -Arguments @(
        "test", "-p", "df-test-intelligence-integration"
    ) | Out-Null

    Write-Host "[7/11] Real GitHub advisory integration"
    Invoke-CargoCaptured -Step "plan-store" -Arguments @(
        "run", "-p", "dragonforge-test-lab", "--",
        "plan-store", "--plan", ".\examples\phase16-plan.json", "--state-db", $stateDb
    ) | Out-Null
    $decision = Invoke-CargoCaptured -Step "intelligence-integrate" -Arguments @(
        "run", "-p", "dragonforge-test-lab", "--",
        "intelligence-integrate",
        "--repo", $RepositoryUrl,
        "--base", $BaseRevision,
        "--head", $Revision,
        "--plan", "dragonforge-standard",
        "--mode", "advisory",
        "--min-score", "20",
        "--state-db", $stateDb
    )
    if (-not ($decision -match "status=intelligence_integration_complete")) {
        throw "real advisory integration did not complete"
    }
    if (-not ($decision -match '"mode": "advisory"')) {
        throw "real advisory integration did not remain advisory"
    }

    Write-Host "[8/11] Phase 16 and Phase 15 regressions"
    Invoke-CargoCaptured -Step "plan-fixture" -Arguments @(
        "run", "-p", "dragonforge-test-lab", "--", "plan-fixture"
    ) | Out-Null
    Invoke-CargoCaptured -Step "lifecycle-fixture" -Arguments @(
        "run", "-p", "dragonforge-test-lab", "--", "lifecycle-fixture"
    ) | Out-Null

    Write-Host "[9/11] Phase 14 observability regression"
    Invoke-CargoCaptured -Step "observability-fixture" -Arguments @(
        "run", "-p", "dragonforge-test-lab", "--", "observability-fixture"
    ) | Out-Null

    Write-Host "[10/11] General doctor reports Phase 17"
    $general = Invoke-CargoCaptured -Step "general doctor" -Arguments @(
        "run", "-p", "dragonforge-test-lab", "--", "doctor"
    )
    if (-not ($general -match "phase=17")) {
        throw "general doctor did not report Phase 17"
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
    Write-Host "Phase 17 Intelligence Integration validation passed."
}
catch {
    Write-Host ""
    Write-Host "Phase 17 validation FAILED: $($_.Exception.Message)"
    exit 1
}
finally {
    Remove-Item $stateDb -Force -ErrorAction SilentlyContinue
    Write-Host ""
    Write-Host "Log file: $logPath"
    Stop-Transcript | Out-Null
}
