param()

$ErrorActionPreference = "Stop"

Write-Host "=== DragonForge Test Lab - MCP Gateway Readiness ==="
Write-Host "Computer: $env:COMPUTERNAME"
Write-Host "OS: $([System.Environment]::OSVersion.VersionString)"
Write-Host ""

if ([string]::IsNullOrWhiteSpace($env:DRAGONFORGE_MCP_TOKEN) -or $env:DRAGONFORGE_MCP_TOKEN.Length -lt 32) {
    throw "DRAGONFORGE_MCP_TOKEN must be set to at least 32 characters"
}

if ([string]::IsNullOrWhiteSpace($env:DRAGONFORGE_MCP_ALLOWED_REPOSITORY_PREFIXES)) {
    throw "DRAGONFORGE_MCP_ALLOWED_REPOSITORY_PREFIXES must be set"
}

$prefixes = $env:DRAGONFORGE_MCP_ALLOWED_REPOSITORY_PREFIXES.Split(";") |
    ForEach-Object { $_.Trim() } |
    Where-Object { -not [string]::IsNullOrWhiteSpace($_) }

if ($prefixes.Count -eq 0) {
    throw "MCP repository allowlist is empty"
}

foreach ($prefix in $prefixes) {
    if (-not $prefix.StartsWith("https://")) {
        throw "MCP repository allowlist entries must use HTTPS"
    }
}

Write-Host "token_configured=True"
Write-Host "allowed_repository_prefixes=$($prefixes.Count)"
Write-Host "loopback_required=True"
Write-Host "status=mcp_host_ready"
