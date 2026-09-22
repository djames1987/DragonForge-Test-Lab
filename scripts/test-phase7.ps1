param(
    [string]$RepositoryUrl = "https://github.com/djames1987/DragonForge-Test-Lab.git",
    [string]$Revision = "phase-7-gui-automation",
    [string]$LogDirectory = ".\test-logs"
)

$ErrorActionPreference = "Stop"

function Assert-LastExitCode {
    param([Parameter(Mandatory)][string]$Step)
    if ($LASTEXITCODE -ne 0) {
        throw "$Step failed: $LASTEXITCODE"
    }
}

if (-not (Test-Path $LogDirectory)) {
    New-Item -ItemType Directory -Path $LogDirectory -Force | Out-Null
}

$timestamp = Get-Date -Format "yyyyMMdd-HHmmss"
$logPath = Join-Path $LogDirectory "phase7-validation-$timestamp.log"
$artifactDir = Join-Path ".dragonforge-test-lab" "phase7-artifacts-$timestamp"
New-Item -ItemType Directory -Path $artifactDir -Force | Out-Null

Start-Transcript -Path $logPath -Force | Out-Null
$fixtureProcess = $null

try {
    Write-Host "=== DragonForge Test Lab - Phase 7 Validation ==="
    Write-Host "Started: $(Get-Date -Format o)"
    Write-Host "Computer: $env:COMPUTERNAME"
    Write-Host "Working directory: $(Get-Location)"
    Write-Host "Revision: $Revision"
    Write-Host "Artifact directory: $artifactDir"
    Write-Host ""

    Write-Host "[0/9] Environment"
    git --version
    cargo --version
    rustc --version
    gh --version

    Write-Host "[1/9] cargo fmt"
    cargo fmt --all -- --check
    Assert-LastExitCode "cargo fmt"

    Write-Host "[2/9] cargo clippy"
    cargo clippy --workspace --all-targets --all-features -- -D warnings
    Assert-LastExitCode "cargo clippy"

    Write-Host "[3/9] cargo test"
    cargo test --workspace --all-features
    Assert-LastExitCode "cargo test"

    Write-Host "[4/9] GUI host readiness"
    & "$PSScriptRoot\check-gui-host.ps1"

    Write-Host "[5/9] GUI automation doctor"
    cargo run -p dragonforge-test-lab -- gui-doctor
    Assert-LastExitCode "gui-doctor"

    Write-Host "[6/9] Deterministic JSON interaction plan"
    $fixtureScript = Join-Path $PSScriptRoot "phase7-gui-fixture.ps1"
    $fixtureArgs = @("-NoLogo", "-NoProfile", "-ExecutionPolicy", "Bypass", "-File", $fixtureScript)
    $fixtureProcess = Start-Process powershell.exe -ArgumentList $fixtureArgs -PassThru

    $previousPreference = $ErrorActionPreference
    try {
        $ErrorActionPreference = "Continue"
        $guiPlanOutput = & cargo run -p dragonforge-test-lab -- gui-run-plan --plan ".\examples\phase7-plan.json" --artifact-dir $artifactDir 2>&1
        $guiPlanExitCode = $LASTEXITCODE
    }
    finally {
        $ErrorActionPreference = $previousPreference
    }
    $guiPlanOutput | ForEach-Object { Write-Host $_ }
    if ($guiPlanExitCode -ne 0) {
        throw "gui-run-plan failed: $guiPlanExitCode"
    }

    $planScreenshot = Join-Path $artifactDir "phase7-plan.png"
    if (-not (Test-Path $planScreenshot)) {
        throw "deterministic plan screenshot was not created"
    }

    if ($fixtureProcess -and -not $fixtureProcess.HasExited) {
        Stop-Process -Id $fixtureProcess.Id -Force
        $fixtureProcess.WaitForExit()
    }
    $fixtureProcess = $null

    Write-Host "[7/9] Owned fixture, screenshot, and crash capture"
    $previousPreference = $ErrorActionPreference
    try {
        $ErrorActionPreference = "Continue"
        $guiFixtureOutput = & cargo run -p dragonforge-test-lab -- gui-fixture --artifact-dir $artifactDir 2>&1
        $guiFixtureExitCode = $LASTEXITCODE
    }
    finally {
        $ErrorActionPreference = $previousPreference
    }
    $guiFixtureOutput | ForEach-Object { Write-Host $_ }
    if ($guiFixtureExitCode -ne 0) {
        throw "gui-fixture failed: $guiFixtureExitCode"
    }

    $fixtureScreenshot = Join-Path $artifactDir "phase7-fixture.png"
    $crashReport = Join-Path $artifactDir "phase7-crash-report.json"
    if (-not (Test-Path $fixtureScreenshot)) {
        throw "fixture screenshot was not created"
    }
    if (-not (Test-Path $crashReport)) {
        throw "crash report was not created"
    }

    $crash = Get-Content $crashReport -Raw | ConvertFrom-Json
    if (-not $crash.captured -or $crash.observed_exit_code -ne 23) {
        throw "expected fixture crash/exit was not captured"
    }

    Write-Host "[8/9] Screenshot artifact summary"
    Get-Item $planScreenshot, $fixtureScreenshot, $crashReport | Select-Object Name, Length, LastWriteTime

    Write-Host "[9/9] GitHub-aware native worker regression"
    cargo run -p dragonforge-test-lab -- run-github --repo $RepositoryUrl --revision $Revision --sandbox native --no-status
    Assert-LastExitCode "run-github regression"

    Write-Host ""
    Write-Host "Phase 7 GUI automation validation passed."
    Write-Host "Artifacts: $artifactDir"
}
catch {
    Write-Host ""
    Write-Host "Phase 7 validation FAILED: $($_.Exception.Message)"
    exit 1
}
finally {
    if ($fixtureProcess -and -not $fixtureProcess.HasExited) {
        Stop-Process -Id $fixtureProcess.Id -Force -ErrorAction SilentlyContinue
    }
    Write-Host ""
    Write-Host "Log file: $logPath"
    Stop-Transcript | Out-Null
}
