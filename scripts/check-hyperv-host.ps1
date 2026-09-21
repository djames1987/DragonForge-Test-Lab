param(
    [string]$VmRoot = "C:\DragonForge-Test-Lab-VMs",
    [string]$SwitchName = "Default Switch"
)

$ErrorActionPreference = "Stop"

Write-Host "=== DragonForge Test Lab - Hyper-V Host Readiness ==="
Write-Host "Computer: $env:COMPUTERNAME"
Write-Host "Windows: $([System.Environment]::OSVersion.VersionString)"
Write-Host "Edition: $((Get-ComputerInfo -Property WindowsProductName).WindowsProductName)"
Write-Host ""

Write-Host "[1/8] Hardware virtualization requirements"
$systemInfo = systeminfo.exe
$hyperVLines = $systemInfo | Where-Object { $_ -match "Hyper-V Requirements|VM Monitor Mode Extensions|Virtualization Enabled In Firmware|Second Level Address Translation|Data Execution Prevention" }
$hyperVLines | ForEach-Object { Write-Host $_ }

Write-Host ""
Write-Host "[2/8] Hyper-V Windows feature"
$feature = Get-CimInstance -ClassName Win32_OptionalFeature -Filter "Name='Microsoft-Hyper-V-All'"
$featureEnabled = ($null -ne $feature -and $feature.InstallState -eq 1)
Write-Host "Enabled: $featureEnabled"

Write-Host ""
Write-Host "[3/8] Hyper-V PowerShell module"
$module = Get-Module -ListAvailable Hyper-V | Select-Object -First 1
if ($null -eq $module) {
    throw "Hyper-V PowerShell module is not installed."
}
Write-Host "Module: $($module.Name) $($module.Version)"

Write-Host ""
Write-Host "[4/8] Hyper-V Virtual Machine Management service"
$vmms = Get-Service vmms -ErrorAction Stop
Write-Host "VMMS: $($vmms.Status)"

Write-Host ""
Write-Host "[5/8] Operator permission"
try {
    $hostInfo = Get-VMHost -ErrorAction Stop
    Write-Host "Get-VMHost: OK"
    Write-Host "VirtualMachinePath: $($hostInfo.VirtualMachinePath)"
    Write-Host "VirtualHardDiskPath: $($hostInfo.VirtualHardDiskPath)"
}
catch {
    throw "Current account cannot manage Hyper-V. Run as Administrator for setup or add the normal Test Lab account to the local 'Hyper-V Administrators' group and sign out/in."
}

Write-Host ""
Write-Host "[6/8] Virtual switches"
$switches = @(Get-VMSwitch)
$switches | Format-Table Name, SwitchType -AutoSize
if (-not ($switches.Name -contains $SwitchName)) {
    throw "Required switch '$SwitchName' was not found."
}

Write-Host ""
Write-Host "[7/8] Test Lab VM storage"
$paths = @(
    $VmRoot,
    (Join-Path $VmRoot "images"),
    (Join-Path $VmRoot "vms")
)
foreach ($path in $paths) {
    if (Test-Path $path) {
        Write-Host "OK: $path"
    }
    else {
        Write-Host "MISSING: $path"
    }
}

Write-Host ""
Write-Host "[8/8] Existing managed VMs"
$managed = @(Get-VM -Name "DragonForge-*" -ErrorAction SilentlyContinue)
if ($managed.Count -eq 0) {
    Write-Host "No DragonForge-managed VMs found."
}
else {
    $managed | Format-Table Name, State, Status, Generation -AutoSize
}

Write-Host ""
if (-not $featureEnabled) {
    throw "Hyper-V is not enabled."
}
if ($vmms.Status -ne "Running") {
    throw "Hyper-V VMMS service is not running."
}

Write-Host "status=hyperv_host_ready"
