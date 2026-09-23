param(
    [string]$Revision = "main",
    [string]$LogDirectory = ".\test-logs"
)

$ErrorActionPreference = "Stop"

function Invoke-Native {
    param(
        [Parameter(Mandatory=$true)][string]$FilePath,
        [string[]]$Arguments = @()
    )

    $previousPreference = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    try {
        & $FilePath @Arguments
        $exitCode = $LASTEXITCODE
    }
    finally {
        $ErrorActionPreference = $previousPreference
    }
    if ($exitCode -ne 0) {
        throw "$FilePath failed with exit code $exitCode"
    }
}

function Invoke-NativeCapture {
    param(
        [Parameter(Mandatory=$true)][string]$FilePath,
        [string[]]$Arguments = @()
    )

    $previousPreference = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    try {
        $output = @(& $FilePath @Arguments)
        $exitCode = $LASTEXITCODE
    }
    finally {
        $ErrorActionPreference = $previousPreference
    }
    $output | ForEach-Object { Write-Host $_ }
    if ($exitCode -ne 0) {
        throw "$FilePath failed with exit code $exitCode"
    }
    return $output
}

New-Item -ItemType Directory -Path $LogDirectory -Force | Out-Null
$stamp = Get-Date -Format "yyyyMMdd-HHmmss"
$log = Join-Path $LogDirectory "phase21-validation-$stamp.log"
Start-Transcript -Path $log -Force | Out-Null
try {
    Write-Host "[0/7] Environment"
    Invoke-Native -FilePath "git" -Arguments @("--version")
    Invoke-Native -FilePath "cargo" -Arguments @("--version")
    Invoke-Native -FilePath "rustc" -Arguments @("--version")

    Write-Host "[1/7] cargo fmt"
    Invoke-Native -FilePath "cargo" -Arguments @("fmt", "--all", "--", "--check")

    Write-Host "[2/7] strict cargo clippy"
    Invoke-Native -FilePath "cargo" -Arguments @(
        "clippy", "--workspace", "--all-targets", "--all-features", "--", "-D", "warnings"
    )

    Write-Host "[3/7] workspace tests"
    Invoke-Native -FilePath "cargo" -Arguments @("test", "--workspace", "--all-features")

    Write-Host "[4/7] installer doctor"
    Invoke-Native -FilePath "cargo" -Arguments @(
        "run", "-p", "dragonforge-test-lab", "--", "install-doctor"
    )

    Write-Host "[5/7] installer fixture"
    Invoke-Native -FilePath "cargo" -Arguments @(
        "run", "-p", "dragonforge-test-lab", "--", "install-fixture"
    )

    Write-Host "[6/7] package build"
    .\scripts\package-release-windows.ps1 -Version 0.22.0 -OutputRoot .\test-packages
    $package = Join-Path $PWD "test-packages\dragonforge-test-lab-0.22.0-windows-x86_64"
    if ($env:PROCESSOR_ARCHITECTURE -eq "ARM64") {
        $package = Join-Path $PWD "test-packages\dragonforge-test-lab-0.22.0-windows-aarch64"
    }
    Invoke-Native -FilePath "cargo" -Arguments @(
        "run", "-p", "dragonforge-test-lab", "--",
        "release-verify",
        "--manifest", (Join-Path $package "release-manifest.json"),
        "--package-root", $package
    )

    Write-Host "[7/7] general doctor reports Phase 21"
    $general = Invoke-NativeCapture -FilePath "cargo" -Arguments @(
        "run", "-q", "-p", "dragonforge-test-lab", "--", "doctor"
    )
    $generalText = $general -join [Environment]::NewLine
    if ($generalText -notmatch "(?m)^phase=21\s*$") {
        throw "general doctor did not report Phase 21"
    }

    Write-Host ""
    Write-Host "Phase 21 Installer / Upgrades validation passed."
    Write-Host "Log file: $log"
}
finally {
    Stop-Transcript | Out-Null
}
