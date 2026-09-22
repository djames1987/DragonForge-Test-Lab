param(
    [string]$RepositoryUrl = "https://github.com/djames1987/DragonForge-Test-Lab.git",
    [string]$Revision = "phase-13-worker-services",
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
$logPath = Join-Path $LogDirectory "phase13-validation-$timestamp.log"

Start-Transcript -Path $logPath -Force | Out-Null

try {
    Write-Host "=== DragonForge Test Lab - Phase 13 Validation ==="
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

    Write-Host "[4/8] Worker service doctor"
    $doctor = Invoke-CargoCaptured -Step "worker-service-doctor" -Arguments @(
        "run", "-p", "dragonforge-test-lab", "--", "worker-service-doctor"
    )
    if (-not ($doctor -match "status=worker_service_ready")) {
        throw "worker service doctor did not report ready"
    }

    Write-Host "[5/8] Real worker service mTLS/lifecycle fixture"
    $fixture = Invoke-CargoCaptured -Step "worker-service-fixture" -Arguments @(
        "run", "-p", "dragonforge-test-lab", "--", "worker-service-fixture"
    )
    if (-not ($fixture -match "status=worker_service_fixture_passed")) {
        throw "worker service fixture did not pass"
    }
    foreach ($field in @(
        '"mtls_registration": true',
        '"heartbeat_received": true',
        '"drain_blocks_new_jobs": true',
        '"restart_state_recovered": true',
        '"windows_service_spec_valid": true',
        '"systemd_unit_valid": true'
    )) {
        if (-not ($fixture -match [regex]::Escape($field))) {
            throw "worker service fixture missing expected result: $field"
        }
    }

    Write-Host "[6/8] Phase 12 mTLS compatibility"
    Invoke-CargoCaptured -Step "identity-fixture" -Arguments @(
        "run", "-p", "dragonforge-test-lab", "--", "identity-fixture"
    ) | Out-Null

    Write-Host "[7/8] General doctor reports Phase 13"
    $general = Invoke-CargoCaptured -Step "general doctor" -Arguments @(
        "run", "-p", "dragonforge-test-lab", "--", "doctor"
    )
    if (-not ($general -match "phase=13")) {
        throw "general doctor did not report Phase 13"
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
    Write-Host "Phase 13 Worker Services validation passed."
}
catch {
    Write-Host ""
    Write-Host "Phase 13 validation FAILED: $($_.Exception.Message)"
    exit 1
}
finally {
    Write-Host ""
    Write-Host "Log file: $logPath"
    Stop-Transcript | Out-Null
}
