param(
    [string]$RepositoryUrl = "https://github.com/djames1987/DragonForge-Test-Lab.git",
    [string]$Revision = "phase-16-test-plans",
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
$logPath = Join-Path $LogDirectory "phase16-validation-$timestamp.log"
$stateDb = Join-Path ([System.IO.Path]::GetTempPath()) "dragonforge-phase16-validation-$PID.sqlite3"
Remove-Item $stateDb -Force -ErrorAction SilentlyContinue
Start-Transcript -Path $logPath -Force | Out-Null

try {
    Write-Host "=== DragonForge Test Lab - Phase 16 Validation ==="
    Write-Host "Started: $(Get-Date -Format o)"
    Write-Host "Computer: $env:COMPUTERNAME"
    Write-Host "Working directory: $(Get-Location)"
    Write-Host "Revision: $Revision"
    Write-Host ""

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

    Write-Host "[4/12] Controller schema v4"
    $controller = Invoke-CargoCaptured -Step "controller-state-doctor" -Arguments @(
        "run", "-p", "dragonforge-test-lab", "--",
        "controller-state-doctor", "--state-db", $stateDb
    )
    if (-not ($controller -match "schema_version=4")) {
        throw "controller doctor did not report schema v4"
    }

    Write-Host "[5/12] Test plan doctor"
    $doctor = Invoke-CargoCaptured -Step "plan-doctor" -Arguments @(
        "run", "-p", "dragonforge-test-lab", "--", "plan-doctor"
    )
    if (-not ($doctor -match "status=test_plans_ready")) {
        throw "plan doctor did not report ready"
    }

    Write-Host "[6/12] Checked-in plan validation and compilation"
    $validate = Invoke-CargoCaptured -Step "plan-validate" -Arguments @(
        "run", "-p", "dragonforge-test-lab", "--",
        "plan-validate", "--plan", ".\examples\phase16-plan.json"
    )
    if (-not ($validate -match "status=test_plan_valid")) {
        throw "checked-in Phase 16 plan did not validate"
    }
    $compile = Invoke-CargoCaptured -Step "plan-compile" -Arguments @(
        "run", "-p", "dragonforge-test-lab", "--",
        "plan-compile", "--plan", ".\examples\phase16-plan.json", "--step", "standard"
    )
    if (-not ($compile -match "status=test_plan_step_compiled")) {
        throw "Phase 16 standard step did not compile"
    }

    Write-Host "[7/12] Test plan integration fixture"
    $fixture = Invoke-CargoCaptured -Step "plan-fixture" -Arguments @(
        "run", "-p", "dragonforge-test-lab", "--", "plan-fixture"
    )
    if (-not ($fixture -match "status=test_plan_fixture_passed")) {
        throw "test plan fixture did not pass"
    }
    foreach ($field in @(
        '"schema_v4": true',
        '"dependency_order_valid": true',
        '"initial_ready_valid": true',
        '"dependency_condition_valid": true',
        '"typed_actions_only": true',
        '"target_constraints_valid": true',
        '"plan_persisted": true',
        '"extra_capability_enforced": true',
        '"compiled_job_schedulable": true',
        '"plan_audited": true',
        '"audit_chain_verified": true'
    )) {
        if (-not ($fixture -match [regex]::Escape($field))) {
            throw "test plan fixture missing expected result: $field"
        }
    }

    Write-Host "[8/12] Phase 15 lifecycle regression"
    Invoke-CargoCaptured -Step "lifecycle-fixture" -Arguments @(
        "run", "-p", "dragonforge-test-lab", "--", "lifecycle-fixture"
    ) | Out-Null

    Write-Host "[9/12] Phase 14 observability regression"
    Invoke-CargoCaptured -Step "observability-fixture" -Arguments @(
        "run", "-p", "dragonforge-test-lab", "--", "observability-fixture"
    ) | Out-Null

    Write-Host "[10/12] Phase 13 worker-service and Phase 12 mTLS regression"
    Invoke-CargoCaptured -Step "worker-service-fixture" -Arguments @(
        "run", "-p", "dragonforge-test-lab", "--", "worker-service-fixture"
    ) | Out-Null
    Invoke-CargoCaptured -Step "identity-fixture" -Arguments @(
        "run", "-p", "dragonforge-test-lab", "--", "identity-fixture"
    ) | Out-Null

    Write-Host "[11/12] General doctor reports Phase 16"
    $general = Invoke-CargoCaptured -Step "general doctor" -Arguments @(
        "run", "-p", "dragonforge-test-lab", "--", "doctor"
    )
    if (-not ($general -match "phase=16")) {
        throw "general doctor did not report Phase 16"
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
    Write-Host "Phase 16 Test Plans validation passed."
}
catch {
    Write-Host ""
    Write-Host "Phase 16 validation FAILED: $($_.Exception.Message)"
    exit 1
}
finally {
    Remove-Item $stateDb -Force -ErrorAction SilentlyContinue
    Write-Host ""
    Write-Host "Log file: $logPath"
    Stop-Transcript | Out-Null
}
