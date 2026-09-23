param(
    [string]$Output = ".\dist\audit-report.txt",
    [switch]$InstallTools
)

$ErrorActionPreference = "Stop"

function Invoke-NativeCapture {
    param(
        [Parameter(Mandatory=$true)][string]$FilePath,
        [string[]]$Arguments = @()
    )
    $previousPreference = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    try {
        $output = @(& $FilePath @Arguments 2>&1)
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

if ($InstallTools) {
    if (-not (Get-Command cargo-audit -ErrorAction SilentlyContinue)) {
        Invoke-NativeCapture cargo @("install","cargo-audit","--locked") | Out-Null
    }
    if (-not (Get-Command cargo-deny -ErrorAction SilentlyContinue)) {
        Invoke-NativeCapture cargo @("install","cargo-deny","--locked") | Out-Null
    }
}

if (-not (Get-Command cargo-audit -ErrorAction SilentlyContinue)) {
    throw "cargo-audit is required"
}
if (-not (Get-Command cargo-deny -ErrorAction SilentlyContinue)) {
    throw "cargo-deny is required"
}

$parent = Split-Path -Parent $Output
if ($parent) {
    New-Item -ItemType Directory -Path $parent -Force | Out-Null
}

$lines = New-Object System.Collections.Generic.List[string]
$lines.Add("DragonForge Test Lab release audit")
$lines.Add("commit=$((& git rev-parse HEAD).Trim())")
$lines.Add("")
$lines.Add("== cargo audit ==")
$audit = Invoke-NativeCapture cargo @("audit")
$audit | ForEach-Object { $lines.Add([string]$_) }

$lines.Add("")
$lines.Add("== cargo deny licenses advisories sources ==")
$deny = Invoke-NativeCapture cargo @("deny","check","licenses","advisories","sources")
$deny | ForEach-Object { $lines.Add([string]$_) }

$lines | Set-Content -LiteralPath $Output -Encoding UTF8
Write-Host "Audit report: $Output"
