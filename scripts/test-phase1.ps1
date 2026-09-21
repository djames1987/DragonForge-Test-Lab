param(
    [string]$RepositoryUrl = "",
    [string]$Revision = "main",
    [string]$LogDirectory = ".\test-logs"
)

$ErrorActionPreference = "Stop"

if (-not (Test-Path $LogDirectory)) {
    New-Item -ItemType Directory -Path $LogDirectory -Force | Out-Null
}

$timestamp = Get-Date -Format "yyyyMMdd-HHmmss"
$logPath = Join-Path $LogDirectory "phase1-validation-$timestamp.log"

Start-Transcript -Path $logPath -Force | Out-Null

try {
    Write-Host "=== DragonForge Test Lab - Phase 1 Validation ==="
    Write-Host "Started: $(Get-Date -Format o)"
    Write-Host "Computer: $env:COMPUTERNAME"
    Write-Host "PowerShell: $($PSVersionTable.PSVersion)"
    Write-Host "Working directory: $(Get-Location)"
    Write-Host ""

    Write-Host "[0/5] Environment"
    git --version
    cargo --version
    rustc --version
    Write-Host ""

    Write-Host "[1/5] cargo fmt --check"
    cargo fmt --all -- --check
    if ($LASTEXITCODE -ne 0) { throw "cargo fmt failed with exit code $LASTEXITCODE" }

    Write-Host "[2/5] cargo clippy"
    cargo clippy --workspace --all-targets --all-features -- -D warnings
    if ($LASTEXITCODE -ne 0) { throw "cargo clippy failed with exit code $LASTEXITCODE" }

    Write-Host "[3/5] cargo test"
    cargo test --workspace --all-features
    if ($LASTEXITCODE -ne 0) { throw "cargo test failed with exit code $LASTEXITCODE" }

    Write-Host "[4/5] doctor"
    cargo run -p dragonforge-test-lab -- doctor
    if ($LASTEXITCODE -ne 0) { throw "doctor failed with exit code $LASTEXITCODE" }

    Write-Host "[5/5] Local worker"
    if ($RepositoryUrl) {
        cargo run -p dragonforge-test-lab -- run-local --repo $RepositoryUrl --revision $Revision
        if ($LASTEXITCODE -ne 0) { throw "run-local failed with exit code $LASTEXITCODE" }
    }
    else {
        Write-Host "Skipped. Pass -RepositoryUrl <https-url> to run the end-to-end worker test."
    }

    Write-Host ""
    Write-Host "Phase 1 validation passed."
}
catch {
    Write-Host ""
    Write-Host "Phase 1 validation FAILED: $($_.Exception.Message)"
    exit 1
}
finally {
    Write-Host ""
    Write-Host "Log file: $logPath"
    Stop-Transcript | Out-Null
}
