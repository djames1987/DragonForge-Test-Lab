param(
    [string]$RepositoryUrl = "https://github.com/djames1987/DragonForge-Test-Lab.git",
    [string]$Revision = "phase-12-mtls-node-identity",
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
$logPath = Join-Path $LogDirectory "phase12-validation-$timestamp.log"

Start-Transcript -Path $logPath -Force | Out-Null

try {
    Write-Host "=== DragonForge Test Lab - Phase 12 Validation ==="
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

    Write-Host "[4/8] mTLS identity doctor"
    $doctor = Invoke-CargoCaptured -Step "identity-doctor" -Arguments @(
        "run", "-p", "dragonforge-test-lab", "--", "identity-doctor"
    )
    if (-not ($doctor -match "status=mtls_identity_ready")) {
        throw "identity doctor did not report ready"
    }

    Write-Host "[5/8] Real mutual-TLS and lifecycle fixture"
    $fixture = Invoke-CargoCaptured -Step "identity-fixture" -Arguments @(
        "run", "-p", "dragonforge-test-lab", "--", "identity-fixture"
    )
    if (-not ($fixture -match "status=mtls_identity_fixture_passed")) {
        throw "mTLS identity fixture did not pass"
    }
    if (-not ($fixture -match '"encrypted_round_trip": true')) {
        throw "mTLS fixture did not report encrypted round trip"
    }
    if (-not ($fixture -match '"revocation_verified": true')) {
        throw "mTLS fixture did not report revocation verification"
    }

    Write-Host "[6/8] Legacy distributed compatibility"
    Invoke-CargoCaptured -Step "distributed-doctor" -Arguments @(
        "run", "-p", "dragonforge-test-lab", "--", "distributed-doctor"
    ) | Out-Null
    Invoke-CargoCaptured -Step "distributed-fixtures" -Arguments @(
        "run", "-p", "dragonforge-test-lab", "--", "distributed-fixtures"
    ) | Out-Null

    Write-Host "[7/8] General doctor reports Phase 12"
    $general = Invoke-CargoCaptured -Step "general doctor" -Arguments @(
        "run", "-p", "dragonforge-test-lab", "--", "doctor"
    )
    if (-not ($general -match "phase=12")) {
        throw "general doctor did not report Phase 12"
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
    Write-Host "Phase 12 mTLS / Node Identity validation passed."
}
catch {
    Write-Host ""
    Write-Host "Phase 12 validation FAILED: $($_.Exception.Message)"
    exit 1
}
finally {
    Write-Host ""
    Write-Host "Log file: $logPath"
    Stop-Transcript | Out-Null
}
