param(
    [int]$StressJobs = 1000,
    [int]$RepeatIterations = 3,
    [string]$LogDirectory = ".\test-logs"
)
$ErrorActionPreference = "Stop"

function Invoke-Native {
    param([Parameter(Mandatory=$true)][string]$FilePath,[string[]]$Arguments=@())
    $old=$ErrorActionPreference; $ErrorActionPreference="Continue"
    try { & $FilePath @Arguments; $code=$LASTEXITCODE } finally { $ErrorActionPreference=$old }
    if ($code -ne 0) { throw "$FilePath failed with exit code $code" }
}

if ($StressJobs -lt 1 -or $StressJobs -gt 2000) { throw "StressJobs must be between 1 and 2000" }
if ($RepeatIterations -lt 1 -or $RepeatIterations -gt 20) { throw "RepeatIterations must be between 1 and 20" }

New-Item -ItemType Directory -Path $LogDirectory -Force | Out-Null
$stamp=Get-Date -Format "yyyyMMdd-HHmmss"
$log=Join-Path $LogDirectory "phase24-reliability-chaos-$stamp.log"
Start-Transcript -Path $log -Force | Out-Null
try {
    Write-Host "[0/9] Environment"
    Invoke-Native git @("--version")
    Invoke-Native cargo @("--version")
    Invoke-Native rustc @("--version")

    Write-Host "[1/9] cargo fmt"
    Invoke-Native cargo @("fmt","--all","--","--check")

    Write-Host "[2/9] strict cargo clippy"
    Invoke-Native cargo @("clippy","--workspace","--all-targets","--all-features","--","-D","warnings")

    Write-Host "[3/9] workspace tests"
    Invoke-Native cargo @("test","--workspace","--all-features")

    Write-Host "[4/9] chaos doctor"
    Invoke-Native cargo @("run","-p","dragonforge-test-lab","--","chaos-doctor")

    Write-Host "[5/9] full chaos fixture ($StressJobs stress jobs)"
    Invoke-Native cargo @("run","-p","dragonforge-test-lab","--","chaos-fixture","--stress-jobs",$StressJobs.ToString())

    Write-Host "[6/9] repeated recovery/stress cycles"
    for ($i=1; $i -le $RepeatIterations; $i++) {
        Write-Host "  chaos iteration $i/$RepeatIterations"
        Invoke-Native cargo @("run","-q","-p","dragonforge-test-lab","--","chaos-fixture","--stress-jobs","250")
    }

    Write-Host "[7/9] lifecycle, worker-service, distributed, identity regressions"
    Invoke-Native cargo @("run","-p","dragonforge-test-lab","--","lifecycle-fixture")
    Invoke-Native cargo @("run","-p","dragonforge-test-lab","--","worker-service-fixture")
    Invoke-Native cargo @("run","-p","dragonforge-test-lab","--","distributed-fixtures")
    Invoke-Native cargo @("run","-p","dragonforge-test-lab","--","identity-fixture")
    Invoke-Native cargo @("run","-p","dragonforge-test-lab","--","observability-fixture")

    Write-Host "[8/9] focused chaos tests"
    Invoke-Native cargo @("test","-p","df-test-chaos")

    Write-Host "[9/9] general doctor reports Phase 24"
    $general = @(& cargo run -q -p dragonforge-test-lab -- doctor)
    if (($general -join [Environment]::NewLine) -notmatch "(?m)^phase=24\s*$") { throw "doctor did not report Phase 24" }

    Write-Host "Phase 24 Reliability / Chaos validation passed."
    Write-Host "Log file: $log"
}
finally { Stop-Transcript | Out-Null }
