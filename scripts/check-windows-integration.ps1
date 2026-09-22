param()

$ErrorActionPreference = "Stop"

Write-Host "=== DragonForge Test Lab - Windows Integration Readiness ==="
Write-Host "Computer: $env:COMPUTERNAME"
Write-Host "Windows: $([System.Environment]::OSVersion.VersionString)"
Write-Host ""

if (-not $IsWindows -and $PSVersionTable.PSEdition -eq "Core") {
    throw "Phase 6 Windows integration requires Windows."
}

$required = @("powershell.exe", "sc.exe", "eventcreate.exe", "msiexec.exe", "ping.exe")
foreach ($tool in $required) {
    $found = Get-Command $tool -ErrorAction SilentlyContinue
    if (-not $found) {
        throw "Required Windows tool not found: $tool"
    }
    Write-Host ("{0}: {1}" -f $tool, $found.Source)
}

$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$principal = New-Object Security.Principal.WindowsPrincipal($identity)
$elevated = $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
Write-Host "elevated=$elevated"

$systemEvent = Get-WinEvent -LogName System -MaxEvents 1 -ErrorAction Stop
Write-Host "event_log_readable=$([bool]$systemEvent)"
Write-Host "registry_hkcu_readable=$(Test-Path 'HKCU:\Software')"

$service = Get-Service -ErrorAction Stop | Select-Object -First 1
Write-Host "scm_readable=$([bool]$service)"

$installerService = Get-Service msiserver -ErrorAction SilentlyContinue
Write-Host "windows_installer_service_present=$([bool]$installerService)"

Write-Host ""
Write-Host "status=windows_integration_host_ready"
