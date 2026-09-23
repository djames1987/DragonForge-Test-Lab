$ErrorActionPreference = "Stop"
$principal = New-Object Security.Principal.WindowsPrincipal([Security.Principal.WindowsIdentity]::GetCurrent())
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    throw "rollback-windows.ps1 must run from an elevated PowerShell session."
}

$binaryPath = Join-Path $env:ProgramFiles "DragonForge\Test Lab\dragonforge-test-lab.exe"
$programDataRoot = Join-Path $env:ProgramData "DragonForge\Test Lab"
$stateRoot = Join-Path $programDataRoot "state"
$backupRoot = Join-Path $programDataRoot "backups"
$stateFile = Join-Path $stateRoot "install-state.json"

if (-not (Test-Path -LiteralPath $stateFile -PathType Leaf)) { throw "Install state not found." }
$state = Get-Content -LiteralPath $stateFile -Raw | ConvertFrom-Json
$current = [string]$state.current_version
$previous = [string]$state.previous_version
$expected = [string]$state.previous_binary_sha256
if (-not $previous -or -not $expected) { throw "No rollback target recorded." }

$backup = Join-Path $backupRoot "dragonforge-test-lab-$previous.exe"
if (-not (Test-Path -LiteralPath $backup -PathType Leaf)) { throw "Rollback binary missing: $backup" }
$actual = (Get-FileHash -Algorithm SHA256 -LiteralPath $backup).Hash.ToLowerInvariant()
if ($actual -ne $expected.ToLowerInvariant()) { throw "Rollback checksum mismatch." }

Stop-Service -Name "DragonForgeTestWorker" -Force -ErrorAction SilentlyContinue
if (Test-Path -LiteralPath $binaryPath) {
    Copy-Item -LiteralPath $binaryPath -Destination (Join-Path $backupRoot "dragonforge-test-lab-$current.failed.exe") -Force
}
Copy-Item -LiteralPath $backup -Destination "$binaryPath.new" -Force
Move-Item -LiteralPath "$binaryPath.new" -Destination $binaryPath -Force

$oldState = Join-Path $backupRoot "install-state-$previous.json"
if (Test-Path -LiteralPath $oldState) {
    Copy-Item -LiteralPath $oldState -Destination $stateFile -Force
} else {
    [ordered]@{
        schema_version = 1
        current_version = $previous
        previous_version = $null
        current_binary_sha256 = $expected
        previous_binary_sha256 = $null
    } | ConvertTo-Json | Set-Content -LiteralPath $stateFile -Encoding UTF8
}

Write-Host "Rolled back DragonForge Test Lab from $current to $previous."
