param(
    [string]$RepositoryUrl = "https://github.com/djames1987/DragonForge-Test-Lab.git",
    [string]$Revision = "phase-14-audit-artifacts-observability",
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
$logPath = Join-Path $LogDirectory "phase14-validation-$timestamp.log"
$stateDb = Join-Path ([System.IO.Path]::GetTempPath()) "dragonforge-phase14-validation-$PID.sqlite3"
Remove-Item $stateDb -Force -ErrorAction SilentlyContinue

Start-Transcript -Path $logPath -Force | Out-Null

try {
    Write-Host "=== DragonForge Test Lab - Phase 14 Validation ==="
    Write-Host "Started: $(Get-Date -Format o)"
    Write-Host "Computer: $env:COMPUTERNAME"
    Write-Host "Working directory: $(Get-Location)"
    Write-Host "Revision: $Revision"
    Write-Host ""

    Write-Host "[0/10] Environment"
    git --version
    cargo --version
    rustc --version
    gh --version

    Write-Host "[1/10] cargo fmt"
    Invoke-CargoCaptured -Step "cargo fmt" -Arguments @(
        "fmt", "--all", "--", "--check"
    ) | Out-Null

    Write-Host "[2/10] cargo clippy"
    Invoke-CargoCaptured -Step "cargo clippy" -Arguments @(
        "clippy", "--workspace", "--all-targets", "--all-features", "--", "-D", "warnings"
    ) | Out-Null

    Write-Host "[3/10] cargo test"
    Invoke-CargoCaptured -Step "cargo test" -Arguments @(
        "test", "--workspace", "--all-features"
    ) | Out-Null

    Write-Host "[4/10] Controller schema v2"
    $controller = Invoke-CargoCaptured -Step "controller-state-doctor" -Arguments @(
        "run", "-p", "dragonforge-test-lab", "--",
        "controller-state-doctor", "--state-db", $stateDb
    )
    if (-not ($controller -match "schema_version=2")) {
        throw "controller doctor did not report schema v2"
    }

    Write-Host "[5/10] Observability doctor"
    $doctor = Invoke-CargoCaptured -Step "observability-doctor" -Arguments @(
        "run", "-p", "dragonforge-test-lab", "--", "observability-doctor"
    )
    if (-not ($doctor -match "status=observability_ready")) {
        throw "observability doctor did not report ready"
    }

    Write-Host "[6/10] Audit / logs / metrics / artifact retention fixture"
    $fixture = Invoke-CargoCaptured -Step "observability-fixture" -Arguments @(
        "run", "-p", "dragonforge-test-lab", "--", "observability-fixture"
    )
    if (-not ($fixture -match "status=observability_fixture_passed")) {
        throw "observability fixture did not pass"
    }
    foreach ($field in @(
        '"schema_v2": true',
        '"audit_chain_verified": true',
        '"jsonl_redacted": true',
        '"durable_log_redacted": true',
        '"durable_metric_persisted": true',
        '"sha256_cataloged": true',
        '"artifact_retention_verified": true',
        '"telemetry_pruned": true'
    )) {
        if (-not ($fixture -match [regex]::Escape($field))) {
            throw "observability fixture missing expected result: $field"
        }
    }

    Write-Host "[7/10] Phase 13 worker-service regression"
    Invoke-CargoCaptured -Step "worker-service-fixture" -Arguments @(
        "run", "-p", "dragonforge-test-lab", "--", "worker-service-fixture"
    ) | Out-Null

    Write-Host "[8/10] Phase 12 mTLS regression"
    Invoke-CargoCaptured -Step "identity-fixture" -Arguments @(
        "run", "-p", "dragonforge-test-lab", "--", "identity-fixture"
    ) | Out-Null

    Write-Host "[9/10] General doctor reports Phase 14"
    $general = Invoke-CargoCaptured -Step "general doctor" -Arguments @(
        "run", "-p", "dragonforge-test-lab", "--", "doctor"
    )
    if (-not ($general -match "phase=14")) {
        throw "general doctor did not report Phase 14"
    }

    Write-Host "[10/10] GitHub-aware native worker regression"
    Invoke-CargoCaptured -Step "run-github regression" -Arguments @(
        "run", "-p", "dragonforge-test-lab", "--",
        "run-github",
        "--repo", $RepositoryUrl,
        "--revision", $Revision,
        "--sandbox", "native",
        "--no-status"
    ) | Out-Null

    Write-Host ""
    Write-Host "Phase 14 Audit / Artifacts / Observability validation passed."
}
catch {
    Write-Host ""
    Write-Host "Phase 14 validation FAILED: $($_.Exception.Message)"
    exit 1
}
finally {
    Remove-Item $stateDb -Force -ErrorAction SilentlyContinue
    Write-Host ""
    Write-Host "Log file: $logPath"
    Stop-Transcript | Out-Null
}
