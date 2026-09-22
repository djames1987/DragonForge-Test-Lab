param()

$ErrorActionPreference = "Stop"

Write-Host "=== DragonForge Test Lab - Distributed/Network Readiness ==="
Write-Host "Computer: $env:COMPUTERNAME"
Write-Host "OS: $([System.Environment]::OSVersion.VersionString)"
Write-Host "UserInteractive: $([Environment]::UserInteractive)"
Write-Host ""

$addresses = [System.Net.NetworkInformation.NetworkInterface]::GetAllNetworkInterfaces() |
    Where-Object { $_.OperationalStatus -eq [System.Net.NetworkInformation.OperationalStatus]::Up } |
    ForEach-Object { $_.GetIPProperties().UnicastAddresses } |
    ForEach-Object { $_.Address } |
    Where-Object {
        $_.AddressFamily -eq [System.Net.Sockets.AddressFamily]::InterNetwork -and
        -not $_.IsIPv6LinkLocal
    } |
    Select-Object -Unique

Write-Host "IPv4 addresses:"
$addresses | ForEach-Object { Write-Host "  $_" }

$dns = [System.Net.Dns]::GetHostAddresses("localhost")
if (-not ($dns | Where-Object { $_.IPAddressToString -eq "127.0.0.1" -or $_.IPAddressToString -eq "::1" })) {
    throw "localhost did not resolve to a loopback address"
}

Write-Host ""
Write-Host "localhost_dns=True"
Write-Host "status=distributed_host_ready"
