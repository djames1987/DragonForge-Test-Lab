param(
    [string]$RepositoryUrl = "",
    [string]$Revision = "phase-2-github-integration",
    [string]$LogDirectory = ".\test-logs",
    [switch]$NoStatus
)

$ErrorActionPreference = "Stop"

if (-not (Test-Path $LogDirectory)) {
    New-Item -ItemType Directory -Path $LogDirectory -Force | Out-Null
}

$timestamp = Get-Date -Format "yyyyMMdd-HHmmss"
$logPath = Join-Path $LogDirectory "phase2-validation-$timestamp.log"

Start-Transcript -Path $logPath -Force | Out-Null

try {
    Write-Host "=== DragonForge Test Lab - Phase 2 Validation ==="
    Write-Host "Started: $(Get-Date -Format o)"
    Write-Host "Computer: $env:COMPUTERNAME"
    Write-Host "PowerShell: $($PSVersionTable.PSVersion)"
    Write-Host "Working directory: $(Get-Location)"
    Write-Host ""

    Write-Host "[0/6] Environment"
    git --version
    cargo --version
    rustc --version
    gh --version
    Write-Host ""

    Write-Host "[1/6] cargo fmt --check"
    cargo fmt --all -- --check
    if ($LASTEXITCODE -ne 0) { throw "cargo fmt failed with exit code $LASTEXITCODE" }

    Write-Host "[2/6] cargo clippy"
    cargo clippy --workspace --all-targets --all-features -- -D warnings
    if ($LASTEXITCODE -ne 0) { throw "cargo clippy failed with exit code $LASTEXITCODE" }

    Write-Host "[3/6] cargo test"
    cargo test --workspace --all-features
    if ($LASTEXITCODE -ne 0) { throw "cargo test failed with exit code $LASTEXITCODE" }

    Write-Host "[4/6] doctor"
    cargo run -p dragonforge-test-lab -- doctor
    if ($LASTEXITCODE -ne 0) { throw "doctor failed with exit code $LASTEXITCODE" }

    Write-Host "[5/6] GitHub doctor"
    cargo run -p dragonforge-test-lab -- github-doctor
    if ($LASTEXITCODE -ne 0) { throw "github-doctor failed with exit code $LASTEXITCODE" }

    Write-Host "[6/6] GitHub-aware local worker"
    if ($RepositoryUrl) {
        $arguments = @(
            "run", "-p", "dragonforge-test-lab", "--",
            "run-github",
            "--repo", $RepositoryUrl,
            "--revision", $Revision
        )

        if ($NoStatus) {
            $arguments += "--no-status"
        }

        cargo @arguments
        if ($LASTEXITCODE -ne 0) { throw "run-github failed with exit code $LASTEXITCODE" }
    }
    else {
        Write-Host "Skipped. Pass -RepositoryUrl <github-https-url> to run the end-to-end GitHub integration test."
    }

    Write-Host ""
    Write-Host "Phase 2 validation passed."
}
catch {
    Write-Host ""
    Write-Host "Phase 2 validation FAILED: $($_.Exception.Message)"
    exit 1
}
finally {
    Write-Host ""
    Write-Host "Log file: $logPath"
    Stop-Transcript | Out-Null
}
