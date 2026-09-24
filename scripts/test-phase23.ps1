param(
    [switch]$InstallTools,
    [string]$LogDirectory = ".\test-logs"
)
$ErrorActionPreference = "Stop"

function Invoke-Native {
    param([Parameter(Mandatory=$true)][string]$FilePath,[string[]]$Arguments=@())
    $old=$ErrorActionPreference; $ErrorActionPreference="Continue"
    try { & $FilePath @Arguments; $code=$LASTEXITCODE } finally { $ErrorActionPreference=$old }
    if ($code -ne 0) { throw "$FilePath failed with exit code $code" }
}

New-Item -ItemType Directory -Path $LogDirectory -Force | Out-Null
$stamp=Get-Date -Format "yyyyMMdd-HHmmss"
$log=Join-Path $LogDirectory "phase23-security-review-$stamp.log"
$report=Join-Path $LogDirectory "phase23-security-report-$stamp.json"
Start-Transcript -Path $log -Force | Out-Null
try {
    Write-Host "[0/8] Environment"
    Invoke-Native git @("--version")
    Invoke-Native cargo @("--version")
    Invoke-Native rustc @("--version")

    Write-Host "[1/8] cargo fmt"
    Invoke-Native cargo @("fmt","--all","--","--check")

    Write-Host "[2/8] strict cargo clippy"
    Invoke-Native cargo @("clippy","--workspace","--all-targets","--all-features","--","-D","warnings")

    Write-Host "[3/8] workspace tests"
    Invoke-Native cargo @("test","--workspace","--all-features")

    Write-Host "[4/8] Phase 23 security doctor, fixture, and repository review"
    Invoke-Native cargo @("run","-p","dragonforge-test-lab","--","security-doctor")
    Invoke-Native cargo @("run","-p","dragonforge-test-lab","--","security-fixture")
    Invoke-Native cargo @("run","-p","dragonforge-test-lab","--","security-review","--root",".","--output",$report)

    Write-Host "[5/8] dependency, advisory, license, and source audit"
    if ($InstallTools) { .\scripts\release-audit-windows.ps1 -Output (Join-Path $LogDirectory "phase23-dependency-audit-$stamp.txt") -InstallTools }
    else { .\scripts\release-audit-windows.ps1 -Output (Join-Path $LogDirectory "phase23-dependency-audit-$stamp.txt") }

    Write-Host "[6/8] security-boundary regressions"

    Write-Host "  [6.1] identity fixture"
    Invoke-Native cargo @("run","-p","dragonforge-test-lab","--","identity-fixture")

    Write-Host "  [6.2] MCP fixture"
    $oldMcpToken = [Environment]::GetEnvironmentVariable("DRAGONFORGE_MCP_TOKEN", "Process")
    $oldMcpAllowlist = [Environment]::GetEnvironmentVariable("DRAGONFORGE_MCP_ALLOWED_REPOSITORY_PREFIXES", "Process")
    try {
        $env:DRAGONFORGE_MCP_TOKEN = "phase23-fixture-token-0123456789abcdef"
        $env:DRAGONFORGE_MCP_ALLOWED_REPOSITORY_PREFIXES = "https://github.com/djames1987/"
        Invoke-Native cargo @("run","-p","dragonforge-test-lab","--","mcp-fixture")
    }
    finally {
        if ($null -eq $oldMcpToken) { Remove-Item Env:\DRAGONFORGE_MCP_TOKEN -ErrorAction SilentlyContinue }
        else { $env:DRAGONFORGE_MCP_TOKEN = $oldMcpToken }
        if ($null -eq $oldMcpAllowlist) { Remove-Item Env:\DRAGONFORGE_MCP_ALLOWED_REPOSITORY_PREFIXES -ErrorAction SilentlyContinue }
        else { $env:DRAGONFORGE_MCP_ALLOWED_REPOSITORY_PREFIXES = $oldMcpAllowlist }
    }

    Write-Host "  [6.3] observability fixture"
    Invoke-Native cargo @("run","-p","dragonforge-test-lab","--","observability-fixture")

    Write-Host "  [6.4] installer fixture"
    Invoke-Native cargo @("run","-p","dragonforge-test-lab","--","install-fixture")

    Write-Host "  [6.5] release fixture"
    Invoke-Native cargo @("run","-p","dragonforge-test-lab","--","release-fixture")

    Write-Host "[7/8] focused policy and security-review tests"
    Invoke-Native cargo @("test","-p","df-test-policy")
    Invoke-Native cargo @("test","-p","df-test-security-review")

    Write-Host "[8/8] general doctor reports Phase 23"
    $general = @(& cargo run -q -p dragonforge-test-lab -- doctor)
    if (($general -join [Environment]::NewLine) -notmatch "(?m)^phase=23\s*$") { throw "doctor did not report Phase 23" }

    Write-Host "Phase 23 Security Review validation passed."
    Write-Host "Security report: $report"
    Write-Host "Log file: $log"
}
finally { Stop-Transcript | Out-Null }
