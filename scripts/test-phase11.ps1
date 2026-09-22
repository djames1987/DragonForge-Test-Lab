param(
    [string]$RepositoryUrl = "https://github.com/djames1987/DragonForge-Test-Lab.git",
    [string]$Revision = "phase-11-durable-controller-state",
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
$logPath = Join-Path $LogDirectory "phase11-validation-$timestamp.log"
$stateRoot = Join-Path $LogDirectory "phase11-state-$timestamp"
$stateDb = Join-Path $stateRoot "controller.sqlite3"

Start-Transcript -Path $logPath -Force | Out-Null

try {
    Write-Host "=== DragonForge Test Lab - Phase 11 Validation ==="
    Write-Host "Started: $(Get-Date -Format o)"
    Write-Host "Computer: $env:COMPUTERNAME"
    Write-Host "Working directory: $(Get-Location)"
    Write-Host "Revision: $Revision"
    Write-Host ""

    Write-Host "[0/8] Environment"
    git --version
    cargo --version
    rustc --version
    gh --version

    Write-Host "[1/8] cargo fmt"
    Invoke-CargoCaptured -Step "cargo fmt" -Arguments @(
        "fmt", "--all", "--", "--check"
    ) | Out-Null

    Write-Host "[2/8] cargo clippy"
    Invoke-CargoCaptured -Step "cargo clippy" -Arguments @(
        "clippy", "--workspace", "--all-targets", "--all-features", "--", "-D", "warnings"
    ) | Out-Null

    Write-Host "[3/8] cargo test"
    Invoke-CargoCaptured -Step "cargo test" -Arguments @(
        "test", "--workspace", "--all-features"
    ) | Out-Null

    Write-Host "[4/8] Durable controller doctor and migration"
    New-Item -ItemType Directory -Path $stateRoot -Force | Out-Null
    $doctor = Invoke-CargoCaptured -Step "controller-state-doctor" -Arguments @(
        "run", "-p", "dragonforge-test-lab", "--",
        "controller-state-doctor", "--state-db", $stateDb
    )
    if (-not ($doctor -match "status=durable_controller_ready")) {
        throw "durable controller doctor did not report ready"
    }
    if (-not (Test-Path $stateDb)) {
        throw "durable controller database was not created"
    }

    Write-Host "[5/8] Restart-recovery persistence fixture"
    $fixture = Invoke-CargoCaptured -Step "controller-state-fixture" -Arguments @(
        "run", "-p", "dragonforge-test-lab", "--", "controller-state-fixture"
    )
    if (-not ($fixture -match "status=durable_controller_fixture_passed")) {
        throw "durable controller fixture did not pass"
    }

    Write-Host "[6/8] Reopen durable database in a second CLI invocation"
    $reopen = Invoke-CargoCaptured -Step "controller-state-doctor reopen" -Arguments @(
        "run", "-p", "dragonforge-test-lab", "--",
        "controller-state-doctor", "--state-db", $stateDb
    )
    if (-not ($reopen -match "schema_version=1")) {
        throw "reopened controller database did not retain expected schema"
    }

    Write-Host "[7/8] General doctor reports Phase 11"
    $general = Invoke-CargoCaptured -Step "general doctor" -Arguments @(
        "run", "-p", "dragonforge-test-lab", "--", "doctor"
    )
    if (-not ($general -match "phase=11")) {
        throw "general doctor did not report Phase 11"
    }

    Write-Host "[8/8] GitHub-aware native worker regression"
    Invoke-CargoCaptured -Step "run-github regression" -Arguments @(
        "run", "-p", "dragonforge-test-lab", "--",
        "run-github",
        "--repo", $RepositoryUrl,
        "--revision", $Revision,
        "--sandbox", "native",
        "--no-status"
    ) | Out-Null

    Write-Host ""
    Write-Host "Phase 11 Durable Controller & State validation passed."
}
catch {
    Write-Host ""
    Write-Host "Phase 11 validation FAILED: $($_.Exception.Message)"
    exit 1
}
finally {
    Write-Host ""
    Write-Host "State database: $stateDb"
    Write-Host "Log file: $logPath"
    Stop-Transcript | Out-Null
}
