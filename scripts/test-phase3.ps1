param(
    [string]$RepositoryUrl = "",
    [string]$Revision = "phase-3-sandboxing",
    [ValidateSet("native", "docker", "podman")]
    [string]$Sandbox = "native",
    [string]$WorkerUser = "",
    [string]$LogDirectory = ".\test-logs"
)

$ErrorActionPreference = "Stop"

if (-not (Test-Path $LogDirectory)) {
    New-Item -ItemType Directory -Path $LogDirectory -Force | Out-Null
}

if ([string]::IsNullOrWhiteSpace($WorkerUser)) {
    $WorkerUser = $env:USERNAME
}

$timestamp = Get-Date -Format "yyyyMMdd-HHmmss"
$logPath = Join-Path $LogDirectory "phase3-validation-$timestamp.log"

Start-Transcript -Path $logPath -Force | Out-Null

try {
    Write-Host "=== DragonForge Test Lab - Phase 3 Validation ==="
    Write-Host "Started: $(Get-Date -Format o)"
    Write-Host "Computer: $env:COMPUTERNAME"
    Write-Host "PowerShell: $($PSVersionTable.PSVersion)"
    Write-Host "Working directory: $(Get-Location)"
    Write-Host "Sandbox: $Sandbox"
    Write-Host "Required worker user: $WorkerUser"
    Write-Host ""

    Write-Host "[0/8] Environment"
    git --version
    cargo --version
    rustc --version
    gh --version
    if ($Sandbox -eq "docker") { docker --version }
    if ($Sandbox -eq "podman") { podman --version }
    Write-Host ""

    Write-Host "[1/8] cargo fmt --check"
    cargo fmt --all -- --check
    if ($LASTEXITCODE -ne 0) { throw "cargo fmt failed with exit code $LASTEXITCODE" }

    Write-Host "[2/8] cargo clippy"
    cargo clippy --workspace --all-targets --all-features -- -D warnings
    if ($LASTEXITCODE -ne 0) { throw "cargo clippy failed with exit code $LASTEXITCODE" }

    Write-Host "[3/8] cargo test"
    cargo test --workspace --all-features
    if ($LASTEXITCODE -ne 0) { throw "cargo test failed with exit code $LASTEXITCODE" }

    Write-Host "[4/8] doctor"
    cargo run -p dragonforge-test-lab -- doctor
    if ($LASTEXITCODE -ne 0) { throw "doctor failed with exit code $LASTEXITCODE" }

    Write-Host "[5/8] sandbox doctor"
    cargo run -p dragonforge-test-lab -- sandbox-doctor --sandbox $Sandbox --worker-user $WorkerUser
    if ($LASTEXITCODE -ne 0) { throw "sandbox-doctor failed with exit code $LASTEXITCODE" }

    Write-Host "[6/8] GitHub doctor"
    cargo run -p dragonforge-test-lab -- github-doctor
    if ($LASTEXITCODE -ne 0) { throw "github-doctor failed with exit code $LASTEXITCODE" }

    Write-Host "[7/8] Sandboxed local worker"
    if ($RepositoryUrl) {
        cargo run -p dragonforge-test-lab -- run-local --repo $RepositoryUrl --revision $Revision --sandbox $Sandbox --worker-user $WorkerUser
        if ($LASTEXITCODE -ne 0) { throw "sandboxed run-local failed with exit code $LASTEXITCODE" }
    }
    else {
        Write-Host "Skipped. Pass -RepositoryUrl <https-url> for the end-to-end worker tests."
    }

    Write-Host "[8/8] GitHub-aware sandboxed worker"
    if ($RepositoryUrl) {
        cargo run -p dragonforge-test-lab -- run-github --repo $RepositoryUrl --revision $Revision --sandbox $Sandbox --worker-user $WorkerUser
        if ($LASTEXITCODE -ne 0) { throw "sandboxed run-github failed with exit code $LASTEXITCODE" }
    }
    else {
        Write-Host "Skipped. Pass -RepositoryUrl <github-https-url> for the GitHub-aware worker test."
    }

    Write-Host ""
    Write-Host "Phase 3 validation passed."
}
catch {
    Write-Host ""
    Write-Host "Phase 3 validation FAILED: $($_.Exception.Message)"
    exit 1
}
finally {
    Write-Host ""
    Write-Host "Log file: $logPath"
    Stop-Transcript | Out-Null
}
