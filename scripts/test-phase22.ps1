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
$log=Join-Path $LogDirectory "phase22-validation-$stamp.log"
Start-Transcript -Path $log -Force | Out-Null
try {
    Write-Host "[0/9] Environment"
    Invoke-Native git @("--version")
    Invoke-Native cargo @("--version")
    Invoke-Native rustc @("--version")
    Invoke-Native python @("--version")

    Write-Host "[1/9] cargo fmt"
    Invoke-Native cargo @("fmt","--all","--","--check")

    Write-Host "[2/9] strict cargo clippy"
    Invoke-Native cargo @("clippy","--workspace","--all-targets","--all-features","--","-D","warnings")

    Write-Host "[3/9] workspace tests"
    Invoke-Native cargo @("test","--workspace","--all-features")

    Write-Host "[4/9] release doctor and fixture"
    Invoke-Native cargo @("run","-p","dragonforge-test-lab","--","release-doctor")
    Invoke-Native cargo @("run","-p","dragonforge-test-lab","--","release-fixture")

    Write-Host "[5/9] channel policy"
    $stable = (& cargo run -q -p dragonforge-test-lab -- release-tag --channel stable --version 0.23.0).Trim()
    if ($stable -ne "v0.23.0") { throw "stable tag policy failed" }
    $beta = (& cargo run -q -p dragonforge-test-lab -- release-tag --channel beta --version 0.23.0-beta.1).Trim()
    if ($beta -ne "v0.23.0-beta.1") { throw "beta tag policy failed" }

    Write-Host "[6/9] dependency/license audit"
    if ($InstallTools) { .\scripts\release-audit-windows.ps1 -Output .\test-release\audit-report.txt -InstallTools }
    else { .\scripts\release-audit-windows.ps1 -Output .\test-release\audit-report.txt }

    Write-Host "[7/9] SBOM and dev release archive"
    $commit = (& git rev-parse HEAD).Trim()
    if (Test-Path .\test-release) { Remove-Item .\test-release -Recurse -Force }
    New-Item -ItemType Directory .\test-release -Force | Out-Null
    .\scripts\release-audit-windows.ps1 -Output .\test-release\audit-report.txt
    Invoke-Native python @(".\scripts\generate-sbom.py","--version","0.23.0-dev.1","--commit",$commit,"--output",".\test-release\dragonforge-test-lab-0.23.0-dev.1.cdx.json")
    Invoke-Native python @(".\scripts\generate-release-notes.py","--version","0.23.0-dev.1","--channel","dev","--commit",$commit,"--output",".\test-release\RELEASE-NOTES.md")
    .\scripts\build-release-windows.ps1 -Channel dev -ReleaseVersion 0.23.0-dev.1 -OutputRoot .\test-release

    Write-Host "[8/9] bundle assembly and verification"
    $archive = Get-ChildItem .\test-release -Filter "dragonforge-test-lab-0.23.0-dev.1-windows-*.zip" | Select-Object -First 1
    if (-not $archive) { throw "Windows dev archive missing" }
    Invoke-Native python @(
        ".\scripts\assemble-release.py",
        "--channel","dev",
        "--version","0.23.0-dev.1",
        "--git-commit",$commit,
        "--output-dir",".\test-release\bundle",
        "--artifact","windows_package::$($archive.FullName)",
        "--artifact","sbom::$((Resolve-Path .\test-release\dragonforge-test-lab-0.23.0-dev.1.cdx.json).Path)",
        "--artifact","audit_report::$((Resolve-Path .\test-release\audit-report.txt).Path)",
        "--artifact","release_notes::$((Resolve-Path .\test-release\RELEASE-NOTES.md).Path)"
    )
    Invoke-Native cargo @("run","-p","dragonforge-test-lab","--","release-bundle-verify","--manifest",".\test-release\bundle\release-bundle.json","--root",".\test-release\bundle")

    Write-Host "[9/9] general doctor reports Phase 22"
    $general = @(& cargo run -q -p dragonforge-test-lab -- doctor)
    if (($general -join [Environment]::NewLine) -notmatch "(?m)^phase=22\s*$") { throw "doctor did not report Phase 22" }

    Write-Host "Phase 22 Release Engineering validation passed."
    Write-Host "Log file: $log"
}
finally { Stop-Transcript | Out-Null }
