param(
    [Parameter(Mandatory=$true)][string]$PackageRoot,
    [string]$ManifestPath,
    [string]$WorkerConfig
)

$ErrorActionPreference = "Stop"
$principal = New-Object Security.Principal.WindowsPrincipal([Security.Principal.WindowsIdentity]::GetCurrent())
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    throw "install-windows.ps1 must run from an elevated PowerShell session."
}

if (-not $ManifestPath) { $ManifestPath = Join-Path $PackageRoot "release-manifest.json" }
if (-not (Test-Path -LiteralPath $ManifestPath -PathType Leaf)) { throw "Manifest not found: $ManifestPath" }

$manifest = Get-Content -LiteralPath $ManifestPath -Raw | ConvertFrom-Json
if ($manifest.schema_version -ne 1) { throw "Unsupported manifest schema." }
if ([string]$manifest.version -notmatch '^[0-9]+\.[0-9]+\.[0-9]+
if ($manifest.binary_file -notmatch '^[A-Za-z0-9._-]{1,255}$' -or $manifest.binary_file -in @(".", "..")) {
    throw "Unsafe binary filename."
}
$arch = switch ($env:PROCESSOR_ARCHITECTURE) {
    "AMD64" { "x86_64" }
    "ARM64" { "aarch64" }
    "x86" { "x86" }
    default { throw "Unsupported Windows architecture: $env:PROCESSOR_ARCHITECTURE" }
}
if ($manifest.target_arch -ne $arch) { throw "Architecture mismatch: manifest=$($manifest.target_arch) host=$arch" }

$packageResolved = (Resolve-Path -LiteralPath $PackageRoot).Path
$sourceBinary = Join-Path $packageResolved $manifest.binary_file
if (-not (Test-Path -LiteralPath $sourceBinary -PathType Leaf)) { throw "Release binary missing." }
if ((Get-Item -LiteralPath $sourceBinary).Length -gt 536870912) { throw "Release binary exceeds 512 MiB." }
$hash = (Get-FileHash -Algorithm SHA256 -LiteralPath $sourceBinary).Hash.ToLowerInvariant()
if ($hash -ne ([string]$manifest.binary_sha256).ToLowerInvariant()) { throw "Release binary checksum mismatch." }

$programFilesRoot = Join-Path $env:ProgramFiles "DragonForge\Test Lab"
$binaryPath = Join-Path $programFilesRoot "dragonforge-test-lab.exe"
$programDataRoot = Join-Path $env:ProgramData "DragonForge\Test Lab"
$configRoot = Join-Path $programDataRoot "config"
$stateRoot = Join-Path $programDataRoot "state"
$logRoot = Join-Path $programDataRoot "logs"
$backupRoot = Join-Path $programDataRoot "backups"
$stateFile = Join-Path $stateRoot "install-state.json"
$managedConfig = Join-Path $configRoot "install-config.json"

@($programFilesRoot,$configRoot,$stateRoot,$logRoot,$backupRoot) | ForEach-Object {
    New-Item -ItemType Directory -Path $_ -Force | Out-Null
}

$previousVersion = $null
$previousHash = $null
if (Test-Path -LiteralPath $stateFile) {
    $old = Get-Content -LiteralPath $stateFile -Raw | ConvertFrom-Json
    $previousVersion = [string]$old.current_version
    $previousHash = [string]$old.current_binary_sha256
}

if ($previousVersion -and $previousVersion -ne "unknown") {
    if ([version]$manifest.version -le [version]$previousVersion) {
        throw "Upgrade target $($manifest.version) must be newer than installed version $previousVersion."
    }
}

if (Test-Path -LiteralPath $binaryPath) {
    Stop-Service -Name "DragonForgeTestWorker" -Force -ErrorAction SilentlyContinue
    if (-not $previousVersion) {
        $previousVersion = "unknown"
        $previousHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $binaryPath).Hash.ToLowerInvariant()
    }
    Copy-Item -LiteralPath $binaryPath -Destination (Join-Path $backupRoot "dragonforge-test-lab-$previousVersion.exe") -Force
    if (Test-Path -LiteralPath $stateFile) {
        Copy-Item -LiteralPath $stateFile -Destination (Join-Path $backupRoot "install-state-$previousVersion.json") -Force
    }
}

$staged = "$binaryPath.new"
Copy-Item -LiteralPath $sourceBinary -Destination $staged -Force
Move-Item -LiteralPath $staged -Destination $binaryPath -Force

[ordered]@{
    schema_version = 1
    preserve_state_on_uninstall = $true
    config_root = $configRoot
    state_root = $stateRoot
    log_root = $logRoot
} | ConvertTo-Json | Set-Content -LiteralPath "$managedConfig.tmp" -Encoding UTF8
Move-Item -LiteralPath "$managedConfig.tmp" -Destination $managedConfig -Force

[ordered]@{
    schema_version = 1
    current_version = [string]$manifest.version
    previous_version = $previousVersion
    current_binary_sha256 = $hash
    previous_binary_sha256 = $previousHash
} | ConvertTo-Json | Set-Content -LiteralPath "$stateFile.tmp" -Encoding UTF8
Move-Item -LiteralPath "$stateFile.tmp" -Destination $stateFile -Force

if ($WorkerConfig) {
    if (-not (Test-Path -LiteralPath $WorkerConfig -PathType Leaf)) { throw "Worker config not found: $WorkerConfig" }
    $installedWorker = Join-Path $configRoot "worker.json"
    Copy-Item -LiteralPath $WorkerConfig -Destination $installedWorker -Force

    $service = Get-Service -Name "DragonForgeTestWorker" -ErrorAction SilentlyContinue
    $binPath = ('"{0}" worker-service-windows --config "{1}"' -f $binaryPath, $installedWorker)
    if ($service) {
        & sc.exe config DragonForgeTestWorker binPath= $binPath start= auto | Out-Null
    } else {
        & sc.exe create DragonForgeTestWorker binPath= $binPath start= auto DisplayName= "DragonForge Test Worker" | Out-Null
    }
    & sc.exe failure DragonForgeTestWorker reset= 86400 actions= restart/5000/restart/15000/restart/60000 | Out-Null
}

Write-Host "DragonForge Test Lab $($manifest.version) installed."
Write-Host "Binary: $binaryPath"
Write-Host "State: $stateRoot"
Write-Host "Worker service is not started automatically by the installer."
) { throw "Manifest version must be stable x.y.z." }
if ($manifest.target_os -ne "windows") { throw "Manifest is not for Windows." }
if ($manifest.binary_file -notmatch '^[A-Za-z0-9._-]{1,255}$' -or $manifest.binary_file -in @(".", "..")) {
    throw "Unsafe binary filename."
}
$arch = switch ($env:PROCESSOR_ARCHITECTURE) {
    "AMD64" { "x86_64" }
    "ARM64" { "aarch64" }
    "x86" { "x86" }
    default { throw "Unsupported Windows architecture: $env:PROCESSOR_ARCHITECTURE" }
}
if ($manifest.target_arch -ne $arch) { throw "Architecture mismatch: manifest=$($manifest.target_arch) host=$arch" }

$packageResolved = (Resolve-Path -LiteralPath $PackageRoot).Path
$sourceBinary = Join-Path $packageResolved $manifest.binary_file
if (-not (Test-Path -LiteralPath $sourceBinary -PathType Leaf)) { throw "Release binary missing." }
if ((Get-Item -LiteralPath $sourceBinary).Length -gt 536870912) { throw "Release binary exceeds 512 MiB." }
$hash = (Get-FileHash -Algorithm SHA256 -LiteralPath $sourceBinary).Hash.ToLowerInvariant()
if ($hash -ne ([string]$manifest.binary_sha256).ToLowerInvariant()) { throw "Release binary checksum mismatch." }

$programFilesRoot = Join-Path $env:ProgramFiles "DragonForge\Test Lab"
$binaryPath = Join-Path $programFilesRoot "dragonforge-test-lab.exe"
$programDataRoot = Join-Path $env:ProgramData "DragonForge\Test Lab"
$configRoot = Join-Path $programDataRoot "config"
$stateRoot = Join-Path $programDataRoot "state"
$logRoot = Join-Path $programDataRoot "logs"
$backupRoot = Join-Path $programDataRoot "backups"
$stateFile = Join-Path $stateRoot "install-state.json"
$managedConfig = Join-Path $configRoot "install-config.json"

@($programFilesRoot,$configRoot,$stateRoot,$logRoot,$backupRoot) | ForEach-Object {
    New-Item -ItemType Directory -Path $_ -Force | Out-Null
}

$previousVersion = $null
$previousHash = $null
if (Test-Path -LiteralPath $stateFile) {
    $old = Get-Content -LiteralPath $stateFile -Raw | ConvertFrom-Json
    $previousVersion = [string]$old.current_version
    $previousHash = [string]$old.current_binary_sha256
}

if (Test-Path -LiteralPath $binaryPath) {
    if (-not $previousVersion) {
        $previousVersion = "unknown"
        $previousHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $binaryPath).Hash.ToLowerInvariant()
    }
    Copy-Item -LiteralPath $binaryPath -Destination (Join-Path $backupRoot "dragonforge-test-lab-$previousVersion.exe") -Force
    if (Test-Path -LiteralPath $stateFile) {
        Copy-Item -LiteralPath $stateFile -Destination (Join-Path $backupRoot "install-state-$previousVersion.json") -Force
    }
}

$staged = "$binaryPath.new"
Copy-Item -LiteralPath $sourceBinary -Destination $staged -Force
Move-Item -LiteralPath $staged -Destination $binaryPath -Force

[ordered]@{
    schema_version = 1
    preserve_state_on_uninstall = $true
    config_root = $configRoot
    state_root = $stateRoot
    log_root = $logRoot
} | ConvertTo-Json | Set-Content -LiteralPath "$managedConfig.tmp" -Encoding UTF8
Move-Item -LiteralPath "$managedConfig.tmp" -Destination $managedConfig -Force

[ordered]@{
    schema_version = 1
    current_version = [string]$manifest.version
    previous_version = $previousVersion
    current_binary_sha256 = $hash
    previous_binary_sha256 = $previousHash
} | ConvertTo-Json | Set-Content -LiteralPath "$stateFile.tmp" -Encoding UTF8
Move-Item -LiteralPath "$stateFile.tmp" -Destination $stateFile -Force

if ($WorkerConfig) {
    if (-not (Test-Path -LiteralPath $WorkerConfig -PathType Leaf)) { throw "Worker config not found: $WorkerConfig" }
    $installedWorker = Join-Path $configRoot "worker.json"
    Copy-Item -LiteralPath $WorkerConfig -Destination $installedWorker -Force

    $service = Get-Service -Name "DragonForgeTestWorker" -ErrorAction SilentlyContinue
    $binPath = ('"{0}" worker-service-windows --config "{1}"' -f $binaryPath, $installedWorker)
    if ($service) {
        & sc.exe config DragonForgeTestWorker binPath= $binPath start= auto | Out-Null
    } else {
        & sc.exe create DragonForgeTestWorker binPath= $binPath start= auto DisplayName= "DragonForge Test Worker" | Out-Null
    }
    & sc.exe failure DragonForgeTestWorker reset= 86400 actions= restart/5000/restart/15000/restart/60000 | Out-Null
}

Write-Host "DragonForge Test Lab $($manifest.version) installed."
Write-Host "Binary: $binaryPath"
Write-Host "State: $stateRoot"
Write-Host "Worker service is not started automatically by the installer."
