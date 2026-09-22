param(
    [string]$RepositoryUrl = "https://github.com/djames1987/DragonForge-Test-Lab.git",
    [string]$Revision = "phase-10-test-intelligence",
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
}

if (-not (Test-Path $LogDirectory)) {
    New-Item -ItemType Directory -Path $LogDirectory -Force | Out-Null
}

$timestamp = Get-Date -Format "yyyyMMdd-HHmmss"
$logPath = Join-Path $LogDirectory "phase10-validation-$timestamp.log"
$analysisPath = Join-Path $LogDirectory "phase10-analysis-$timestamp.json"

Start-Transcript -Path $logPath -Force | Out-Null

try {
    Write-Host "=== DragonForge Test Lab - Phase 10 Validation ==="
    Write-Host "Started: $(Get-Date -Format o)"
    Write-Host "Computer: $env:COMPUTERNAME"
    Write-Host "Working directory: $(Get-Location)"
    Write-Host "Revision: $Revision"
    Write-Host ""

    Write-Host "[0/7] Environment"
    git --version
    cargo --version
    rustc --version
    gh --version

    Write-Host "[1/7] cargo fmt"
    Invoke-CargoCaptured -Step "cargo fmt" -Arguments @(
        "fmt", "--all", "--", "--check"
    )

    Write-Host "[2/7] cargo clippy"
    Invoke-CargoCaptured -Step "cargo clippy" -Arguments @(
        "clippy", "--workspace", "--all-targets", "--all-features", "--", "-D", "warnings"
    )

    Write-Host "[3/7] cargo test"
    Invoke-CargoCaptured -Step "cargo test" -Arguments @(
        "test", "--workspace", "--all-features"
    )

    Write-Host "[4/7] Deterministic intelligence fixture"
    Invoke-CargoCaptured -Step "intelligence-fixture" -Arguments @(
        "run", "-p", "dragonforge-test-lab", "--", "intelligence-fixture"
    )

    Write-Host "[5/7] Analyze realistic Phase 10 scenario"
    $previousPreference = $ErrorActionPreference
    try {
        $ErrorActionPreference = "Continue"
        $analysis = & cargo run -p dragonforge-test-lab -- intelligence-analyze --input .\examples\phase10-intelligence.json 2>&1
        $exitCode = $LASTEXITCODE
    }
    finally {
        $ErrorActionPreference = $previousPreference
    }
    $analysis | ForEach-Object { Write-Host $_ }
    if ($exitCode -ne 0) {
        throw "intelligence-analyze failed: $exitCode"
    }

    $jsonLines = New-Object System.Collections.Generic.List[string]
    $capture = $false
    foreach ($line in $analysis) {
        $text = [string]$line
        if (-not $capture -and $text.TrimStart().StartsWith("{")) {
            $capture = $true
        }
        if ($capture) {
            if ($text -eq "status=test_intelligence_analysis_complete") {
                break
            }
            $jsonLines.Add($text)
        }
    }

    $jsonText = $jsonLines -join [Environment]::NewLine
    $report = $jsonText | ConvertFrom-Json
    $jsonText | Set-Content -Path $analysisPath -Encoding UTF8

    if ($report.recommendations.Count -lt 3) {
        throw "expected multiple Phase 10 recommendations"
    }
    if (-not ($report.recommendations | Where-Object { $_.profile -eq "mcp_gateway" })) {
        throw "expected MCP gateway recommendation"
    }
    if (-not ($report.recommendations | Where-Object { $_.profile -eq "distributed_network" })) {
        throw "expected distributed/network recommendation"
    }
    if (-not ($report.failure_clusters | Where-Object { $_.occurrences -ge 2 })) {
        throw "expected repeated historical failures to form a cluster"
    }
    if ($report.schedule.Count -lt 2) {
        throw "expected resource-aware schedule assignments"
    }

    Write-Host "recommendations=$($report.recommendations.Count)"
    Write-Host "clusters=$($report.failure_clusters.Count)"
    Write-Host "scheduled=$($report.schedule.Count)"
    Write-Host "unscheduled=$($report.unscheduled_profiles.Count)"
    Write-Host "analysis_file=$analysisPath"

    Write-Host "[6/7] Intelligence and general doctors"
    Invoke-CargoCaptured -Step "intelligence-doctor" -Arguments @(
        "run", "-p", "dragonforge-test-lab", "--", "intelligence-doctor"
    )
    $doctor = & cargo run -p dragonforge-test-lab -- doctor 2>&1
    $doctor | ForEach-Object { Write-Host $_ }
    if ($LASTEXITCODE -ne 0 -or -not ($doctor -match "phase=10")) {
        throw "general doctor did not report Phase 10"
    }

    Write-Host "[7/7] GitHub-aware native worker regression"
    Invoke-CargoCaptured -Step "run-github regression" -Arguments @(
        "run", "-p", "dragonforge-test-lab", "--",
        "run-github",
        "--repo", $RepositoryUrl,
        "--revision", $Revision,
        "--sandbox", "native",
        "--no-status"
    )

    Write-Host ""
    Write-Host "Phase 10 Test Intelligence validation passed."
}
catch {
    Write-Host ""
    Write-Host "Phase 10 validation FAILED: $($_.Exception.Message)"
    exit 1
}
finally {
    Write-Host ""
    Write-Host "Log file: $logPath"
    Stop-Transcript | Out-Null
}
