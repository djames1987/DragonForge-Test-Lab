param([switch]$Purge)

$ErrorActionPreference = "Stop"
$principal = New-Object Security.Principal.WindowsPrincipal([Security.Principal.WindowsIdentity]::GetCurrent())
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    throw "uninstall-windows.ps1 must run from an elevated PowerShell session."
}

$service = Get-Service -Name "DragonForgeTestWorker" -ErrorAction SilentlyContinue
if ($service) {
    Stop-Service -Name "DragonForgeTestWorker" -Force -ErrorAction SilentlyContinue
    & sc.exe delete DragonForgeTestWorker | Out-Null
}

$programFilesRoot = Join-Path $env:ProgramFiles "DragonForge\Test Lab"
$programDataRoot = Join-Path $env:ProgramData "DragonForge\Test Lab"
if (Test-Path -LiteralPath $programFilesRoot) { Remove-Item -LiteralPath $programFilesRoot -Recurse -Force }

if ($Purge) {
    if (Test-Path -LiteralPath $programDataRoot) { Remove-Item -LiteralPath $programDataRoot -Recurse -Force }
    Write-Host "DragonForge Test Lab uninstalled and state purged."
} else {
    Write-Host "DragonForge Test Lab uninstalled. Configuration, state, logs, and backups were preserved."
}
