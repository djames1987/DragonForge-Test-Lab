param(
    [switch]$RunExternalCampaign,
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
$log=Join-Path $LogDirectory "phase25-dogfooding-$stamp.log"
Start-Transcript -Path $log -Force | Out-Null
try {
    Write-Host "[0/11] Environment"
    Invoke-Native git @("--version")
    Invoke-Native cargo @("--version")
    Invoke-Native rustc @("--version")
    Invoke-Native gh @("--version")
    $sha = (& git rev-parse HEAD).Trim()
    if ($LASTEXITCODE -ne 0 -or $sha -notmatch '^[0-9a-fA-F]{40}$') { throw "unable to resolve current immutable git SHA" }
    Write-Host "Current commit: $sha"

    Write-Host "[1/11] cargo fmt"
    Invoke-Native cargo @("fmt","--all","--","--check")

    Write-Host "[2/11] strict cargo clippy"
    Invoke-Native cargo @("clippy","--workspace","--all-targets","--all-features","--","-D","warnings")

    Write-Host "[3/11] workspace tests"
    Invoke-Native cargo @("test","--workspace","--all-features")

    Write-Host "[4/11] dogfood doctor and fixture"
    Invoke-Native cargo @("run","-p","dragonforge-test-lab","--","dogfood-doctor")
    Invoke-Native cargo @("run","-p","dragonforge-test-lab","--","dogfood-fixture")

    Write-Host "[5/11] checked-in dogfood profiles"
    Invoke-Native cargo @("run","-p","dragonforge-test-lab","--","dogfood-profile-validate","--profile",".\dogfood\dragonforge-test-lab.json")
    Invoke-Native cargo @("run","-p","dragonforge-test-lab","--","dogfood-profile-validate","--profile",".\dogfood\dragonforge-security-suite.json")
    Invoke-Native cargo @("run","-p","dragonforge-test-lab","--","dogfood-profile-validate","--profile",".\dogfood\dragonforge-security-test-lab.json")
    Invoke-Native cargo @("run","-p","dragonforge-test-lab","--","dogfood-campaign-validate","--campaign",".\dogfood\phase25-campaign.json")

    Write-Host "[6/11] immutable self-host profile compilation"
    Invoke-Native cargo @("run","-p","dragonforge-test-lab","--","dogfood-profile-compile","--profile",".\dogfood\dragonforge-test-lab.json","--sha",$sha,"--depth","1")

    Write-Host "[7/11] self-hosted Test Lab dogfood run"
    Invoke-Native cargo @("run","-p","dragonforge-test-lab","--","dogfood-run","--profile",".\dogfood\dragonforge-test-lab.json","--revision",$sha,"--depth","1","--lab-root",(Join-Path $LogDirectory "phase25-self-host"))

    Write-Host "[8/11] optional external DragonForge campaign"
    if ($RunExternalCampaign) {
        Invoke-Native cargo @("run","-p","dragonforge-test-lab","--","dogfood-campaign-run","--campaign",".\dogfood\phase25-campaign.json","--lab-root",(Join-Path $LogDirectory "phase25-campaign"))
    } else {
        Write-Host "External campaign execution skipped; profiles and campaign were validated."
    }

    Write-Host "[9/11] Phase 24 reliability regression"
    Invoke-Native cargo @("run","-p","dragonforge-test-lab","--","chaos-fixture","--stress-jobs","250")

    Write-Host "[10/11] focused dogfood tests"
    Invoke-Native cargo @("test","-p","df-test-dogfood")

    Write-Host "[11/11] general doctor reports Phase 25"
    $general = @(& cargo run -q -p dragonforge-test-lab -- doctor)
    if (($general -join [Environment]::NewLine) -notmatch "(?m)^phase=25\s*$") { throw "doctor did not report Phase 25" }

    Write-Host "Phase 25 Dogfooding validation passed."
    Write-Host "Log file: $log"
}
finally { Stop-Transcript | Out-Null }
