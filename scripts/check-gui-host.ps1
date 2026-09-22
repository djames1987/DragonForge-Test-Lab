param()

$ErrorActionPreference = "Stop"

Write-Host "=== DragonForge Test Lab - GUI Automation Readiness ==="
Write-Host "Computer: $env:COMPUTERNAME"
Write-Host "Windows: $([System.Environment]::OSVersion.VersionString)"
Write-Host "Session: $env:SESSIONNAME"
Write-Host "UserInteractive: $([Environment]::UserInteractive)"
Write-Host ""

if ([System.Environment]::OSVersion.Platform -ne [System.PlatformID]::Win32NT) {
    throw "Phase 7 GUI automation requires Windows."
}

if (-not [Environment]::UserInteractive) {
    throw "An interactive Windows desktop session is required."
}

$uia = $false
try {
    Add-Type -AssemblyName UIAutomationClient -ErrorAction Stop
    Add-Type -AssemblyName UIAutomationTypes -ErrorAction Stop
    $uia = $true
}
catch {
    throw "UIAutomationClient is unavailable: $($_.Exception.Message)"
}
Write-Host "ui_automation_available=$uia"

$drawing = $false
try {
    Add-Type -AssemblyName System.Drawing -ErrorAction Stop
    $drawing = $true
}
catch {
    throw "System.Drawing is unavailable: $($_.Exception.Message)"
}
Write-Host "drawing_available=$drawing"

$wpf = $false
try {
    Add-Type -AssemblyName PresentationFramework -ErrorAction Stop
    Add-Type -AssemblyName PresentationCore -ErrorAction Stop
    Add-Type -AssemblyName WindowsBase -ErrorAction Stop
    $wpf = $true
}
catch {
    throw "WPF assemblies are unavailable: $($_.Exception.Message)"
}
Write-Host "wpf_available=$wpf"

Add-Type -AssemblyName System.Windows.Forms
$screen = [System.Windows.Forms.Screen]::PrimaryScreen
if (-not $screen) {
    throw "No primary display is available to the interactive session."
}
Write-Host "primary_screen=$($screen.Bounds.Width)x$($screen.Bounds.Height)"
Write-Host ""
Write-Host "status=gui_automation_host_ready"
