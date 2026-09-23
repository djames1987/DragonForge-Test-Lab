param(
    [string]$Output = ".\dist\audit-report.txt",
    [switch]$InstallTools
)
$ErrorActionPreference = "Stop"
if ($InstallTools) {
    if (-not (Get-Command cargo-audit -ErrorAction SilentlyContinue)) { cargo install cargo-audit --locked }
    if (-not (Get-Command cargo-deny -ErrorAction SilentlyContinue)) { cargo install cargo-deny --locked }
}
if (-not (Get-Command cargo-audit -ErrorAction SilentlyContinue)) { throw "cargo-audit is required" }
if (-not (Get-Command cargo-deny -ErrorAction SilentlyContinue)) { throw "cargo-deny is required" }
$parent = Split-Path -Parent $Output
if ($parent) { New-Item -ItemType Directory -Path $parent -Force | Out-Null }
$lines = New-Object System.Collections.Generic.List[string]
$lines.Add("DragonForge Test Lab release audit")
$lines.Add("commit=$(git rev-parse HEAD)")
$lines.Add("")
$lines.Add("== cargo audit ==")
$audit = & cargo audit 2>&1
if ($LASTEXITCODE -ne 0) { $audit | ForEach-Object { Write-Host $_ }; throw "cargo audit failed" }
$audit | ForEach-Object { $lines.Add([string]$_); Write-Host $_ }
$lines.Add("")
$lines.Add("== cargo deny licenses advisories sources ==")
$deny = & cargo deny check licenses advisories sources 2>&1
if ($LASTEXITCODE -ne 0) { $deny | ForEach-Object { Write-Host $_ }; throw "cargo deny failed" }
$deny | ForEach-Object { $lines.Add([string]$_); Write-Host $_ }
$lines | Set-Content -LiteralPath $Output -Encoding UTF8
Write-Host "Audit report: $Output"
