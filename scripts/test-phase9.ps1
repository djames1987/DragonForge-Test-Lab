param(
    [string]$RepositoryUrl = "https://github.com/djames1987/DragonForge-Test-Lab.git",
    [string]$Revision = "phase-9-chatgpt-mcp-gateway",
    [string]$LogDirectory = ".\test-logs"
)

$ErrorActionPreference = "Stop"

function Assert-LastExitCode {
    param([Parameter(Mandatory)][string]$Step)
    if ($LASTEXITCODE -ne 0) {
        throw "$Step failed: $LASTEXITCODE"
    }
}

function Invoke-CargoCaptured {
    param(
        [Parameter(Mandatory)][string]$Step,
        [Parameter(Mandatory)][string[]]$Arguments
    )
    $previousPreference = $ErrorActionPreference
    try {
        $ErrorActionPreference = "Continue"
        $output = & cargo @Arguments 2>&1
        $exitCode = $LASTEXITCODE
    }
    finally {
        $ErrorActionPreference = $previousPreference
    }
    $output | ForEach-Object { Write-Host $_ }
    if ($exitCode -ne 0) {
        throw "$Step failed: $exitCode"
    }
}

function New-TemporarySecret {
    $bytes = New-Object byte[] 32
    $rng = New-Object System.Security.Cryptography.RNGCryptoServiceProvider
    try { $rng.GetBytes($bytes) } finally { $rng.Dispose() }
    return [Convert]::ToBase64String($bytes)
}

function Get-FreeLoopbackPort {
    $listener = New-Object System.Net.Sockets.TcpListener([System.Net.IPAddress]::Loopback, 0)
    $listener.Start()
    try { return ([System.Net.IPEndPoint]$listener.LocalEndpoint).Port } finally { $listener.Stop() }
}

function Invoke-Mcp {
    param(
        [Parameter(Mandatory)][string]$Uri,
        [Parameter(Mandatory)][hashtable]$Headers,
        [Parameter(Mandatory)][hashtable]$Body
    )
    $json = $Body | ConvertTo-Json -Depth 12 -Compress
    $response = Invoke-WebRequest -UseBasicParsing -Method Post -Uri $Uri -Headers $Headers -ContentType "application/json" -Body $json
    return ($response.Content | ConvertFrom-Json)
}

if (-not (Test-Path $LogDirectory)) {
    New-Item -ItemType Directory -Path $LogDirectory -Force | Out-Null
}

$timestamp = Get-Date -Format "yyyyMMdd-HHmmss"
$logPath = Join-Path $LogDirectory "phase9-validation-$timestamp.log"
$serverStdout = Join-Path $LogDirectory "phase9-mcp-server-$timestamp.stdout.log"
$serverStderr = Join-Path $LogDirectory "phase9-mcp-server-$timestamp.stderr.log"

$previousToken = $env:DRAGONFORGE_MCP_TOKEN
$previousAllowlist = $env:DRAGONFORGE_MCP_ALLOWED_REPOSITORY_PREFIXES
$server = $null

Start-Transcript -Path $logPath -Force | Out-Null

try {
    if ([string]::IsNullOrWhiteSpace($env:DRAGONFORGE_MCP_TOKEN)) {
        $env:DRAGONFORGE_MCP_TOKEN = New-TemporarySecret
    }
    if ([string]::IsNullOrWhiteSpace($env:DRAGONFORGE_MCP_ALLOWED_REPOSITORY_PREFIXES)) {
        $env:DRAGONFORGE_MCP_ALLOWED_REPOSITORY_PREFIXES = $RepositoryUrl
    }

    Write-Host "=== DragonForge Test Lab - Phase 9 Validation ==="
    Write-Host "Started: $(Get-Date -Format o)"
    Write-Host "Computer: $env:COMPUTERNAME"
    Write-Host "Working directory: $(Get-Location)"
    Write-Host "Revision: $Revision"
    Write-Host ""

    Write-Host "[0/10] Environment"
    git --version
    cargo --version
    rustc --version
    gh --version

    Write-Host "[1/10] cargo fmt"
    cargo fmt --all -- --check
    Assert-LastExitCode "cargo fmt"

    Write-Host "[2/10] cargo clippy"
    Invoke-CargoCaptured -Step "cargo clippy" -Arguments @(
        "clippy", "--workspace", "--all-targets", "--all-features", "--", "-D", "warnings"
    )

    Write-Host "[3/10] cargo test"
    Invoke-CargoCaptured -Step "cargo test" -Arguments @("test", "--workspace", "--all-features")

    Write-Host "[4/10] MCP host readiness"
    & "$PSScriptRoot\check-mcp-host.ps1"

    Write-Host "[5/10] MCP doctor and in-process fixture"
    cargo run -p dragonforge-test-lab -- mcp-doctor
    Assert-LastExitCode "mcp-doctor"
    cargo run -p dragonforge-test-lab -- mcp-fixture
    Assert-LastExitCode "mcp-fixture"

    Write-Host "[6/10] Start real loopback MCP HTTP gateway"
    $port = Get-FreeLoopbackPort
    $bind = "127.0.0.1:$port"
    $uri = "http://$bind/mcp"
    $server = Start-Process -FilePath "cargo" -ArgumentList @("run", "-p", "dragonforge-test-lab", "--", "mcp-serve", "--bind", $bind) -RedirectStandardOutput $serverStdout -RedirectStandardError $serverStderr -PassThru

    $ready = $false
    for ($attempt = 0; $attempt -lt 60; $attempt++) {
        Start-Sleep -Milliseconds 250
        if ($server.HasExited) {
            throw "MCP gateway process exited before readiness"
        }
        try {
            $health = Invoke-WebRequest -UseBasicParsing -Uri "http://$bind/health" -TimeoutSec 2
            if ($health.StatusCode -eq 200) {
                $ready = $true
                break
            }
        }
        catch {
        }
    }
    if (-not $ready) {
        throw "MCP gateway did not become ready"
    }
    Write-Host "mcp_bind=$bind"

    $authHeaders = @{ Authorization = "Bearer $($env:DRAGONFORGE_MCP_TOKEN)" }

    Write-Host "[7/10] Authentication, modern discovery, tools/list, and lab status"
    $unauthorizedBlocked = $false
    try {
        Invoke-WebRequest -UseBasicParsing -Method Post -Uri $uri -ContentType "application/json" -Body '{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}' | Out-Null
    }
    catch {
        if ($_.Exception.Response.StatusCode.value__ -eq 401) {
            $unauthorizedBlocked = $true
        }
    }
    if (-not $unauthorizedBlocked) {
        throw "unauthenticated MCP request was not rejected with 401"
    }
    Write-Host "unauthorized_request=blocked"

    $modernHeaders = @{
        Authorization = "Bearer $($env:DRAGONFORGE_MCP_TOKEN)"
        "MCP-Protocol-Version" = "2026-07-28"
        "Mcp-Method" = "server/discover"
    }
    $discover = Invoke-Mcp -Uri $uri -Headers $modernHeaders -Body @{
        jsonrpc = "2.0"; id = 2; method = "server/discover"
        params = @{ _meta = @{
            "io.modelcontextprotocol/protocolVersion" = "2026-07-28"
            "io.modelcontextprotocol/clientInfo" = @{ name = "dragonforge-phase9-validation"; version = "1.0" }
            "io.modelcontextprotocol/clientCapabilities" = @{}
        }}
    }
    if (-not ($discover.result.supportedVersions -contains "2026-07-28")) {
        throw "modern MCP discovery did not advertise 2026-07-28"
    }
    Write-Host "modern_discovery=passed"

    $listHeaders = @{
        Authorization = "Bearer $($env:DRAGONFORGE_MCP_TOKEN)"
        "MCP-Protocol-Version" = "2026-07-28"
        "Mcp-Method" = "tools/list"
    }
    $tools = Invoke-Mcp -Uri $uri -Headers $listHeaders -Body @{
        jsonrpc = "2.0"; id = 3; method = "tools/list"
        params = @{ _meta = @{
            "io.modelcontextprotocol/protocolVersion" = "2026-07-28"
            "io.modelcontextprotocol/clientCapabilities" = @{}
        }}
    }
    if ($tools.result.tools.Count -ne 6) {
        throw "expected exactly 6 typed MCP tools"
    }
    Write-Host "typed_tools=$($tools.result.tools.Count)"

    $statusHeaders = @{
        Authorization = "Bearer $($env:DRAGONFORGE_MCP_TOKEN)"
        "MCP-Protocol-Version" = "2026-07-28"
        "Mcp-Method" = "tools/call"
        "Mcp-Name" = "dragonforge_lab_status"
    }
    $status = Invoke-Mcp -Uri $uri -Headers $statusHeaders -Body @{
        jsonrpc = "2.0"; id = 4; method = "tools/call"
        params = @{
            name = "dragonforge_lab_status"; arguments = @{}
            _meta = @{
                "io.modelcontextprotocol/protocolVersion" = "2026-07-28"
                "io.modelcontextprotocol/clientCapabilities" = @{}
            }
        }
    }
    if ($status.result.isError) {
        throw "dragonforge_lab_status returned an MCP tool error"
    }
    Write-Host "lab_status=passed"

    Write-Host "[8/10] Legacy initialize compatibility"
    $legacy = Invoke-Mcp -Uri $uri -Headers $authHeaders -Body @{
        jsonrpc = "2.0"; id = 5; method = "initialize"
        params = @{
            protocolVersion = "2025-11-25"; capabilities = @{}
            clientInfo = @{ name = "dragonforge-phase9-validation"; version = "1.0" }
        }
    }
    if ($legacy.result.protocolVersion -ne "2025-11-25") {
        throw "legacy MCP initialize negotiation failed"
    }
    Write-Host "legacy_initialize=passed"

    Write-Host "[9/10] Real MCP job submission, polling, result, and artifacts"
    $submitHeaders = @{
        Authorization = "Bearer $($env:DRAGONFORGE_MCP_TOKEN)"
        "MCP-Protocol-Version" = "2026-07-28"
        "Mcp-Method" = "tools/call"
        "Mcp-Name" = "dragonforge_job_submit"
    }
    $submitted = Invoke-Mcp -Uri $uri -Headers $submitHeaders -Body @{
        jsonrpc = "2.0"; id = 6; method = "tools/call"
        params = @{
            name = "dragonforge_job_submit"
            arguments = @{ repository = $RepositoryUrl; revision = $Revision; profile = "rust_test" }
            _meta = @{
                "io.modelcontextprotocol/protocolVersion" = "2026-07-28"
                "io.modelcontextprotocol/clientCapabilities" = @{}
            }
        }
    }
    if ($submitted.result.isError) {
        throw "MCP job submission returned an error"
    }
    $jobId = $submitted.result.structuredContent.job_id
    if ([string]::IsNullOrWhiteSpace($jobId)) {
        throw "MCP job submission did not return a job id"
    }
    Write-Host "job_id=$jobId"

    $finalState = $null
    for ($attempt = 0; $attempt -lt 120; $attempt++) {
        Start-Sleep -Seconds 1
        $pollHeaders = @{
            Authorization = "Bearer $($env:DRAGONFORGE_MCP_TOKEN)"
            "MCP-Protocol-Version" = "2026-07-28"
            "Mcp-Method" = "tools/call"
            "Mcp-Name" = "dragonforge_job_status"
        }
        $polled = Invoke-Mcp -Uri $uri -Headers $pollHeaders -Body @{
            jsonrpc = "2.0"; id = 7; method = "tools/call"
            params = @{
                name = "dragonforge_job_status"; arguments = @{ job_id = $jobId }
                _meta = @{
                    "io.modelcontextprotocol/protocolVersion" = "2026-07-28"
                    "io.modelcontextprotocol/clientCapabilities" = @{}
                }
            }
        }
        $finalState = $polled.result.structuredContent.state
        if ($finalState -in @("passed", "failed", "cancelled", "error")) {
            break
        }
    }
    Write-Host "job_state=$finalState"
    if ($finalState -ne "passed") {
        throw "MCP-submitted validation job did not pass"
    }

    $resultHeaders = @{
        Authorization = "Bearer $($env:DRAGONFORGE_MCP_TOKEN)"
        "MCP-Protocol-Version" = "2026-07-28"
        "Mcp-Method" = "tools/call"
        "Mcp-Name" = "dragonforge_job_result"
    }
    $result = Invoke-Mcp -Uri $uri -Headers $resultHeaders -Body @{
        jsonrpc = "2.0"; id = 8; method = "tools/call"
        params = @{
            name = "dragonforge_job_result"; arguments = @{ job_id = $jobId }
            _meta = @{
                "io.modelcontextprotocol/protocolVersion" = "2026-07-28"
                "io.modelcontextprotocol/clientCapabilities" = @{}
            }
        }
    }
    if ($result.result.isError -or $result.result.structuredContent.status -ne "passed") {
        throw "MCP job result did not report passed"
    }

    $artifactHeaders = @{
        Authorization = "Bearer $($env:DRAGONFORGE_MCP_TOKEN)"
        "MCP-Protocol-Version" = "2026-07-28"
        "Mcp-Method" = "tools/call"
        "Mcp-Name" = "dragonforge_artifact_list"
    }
    $artifacts = Invoke-Mcp -Uri $uri -Headers $artifactHeaders -Body @{
        jsonrpc = "2.0"; id = 9; method = "tools/call"
        params = @{
            name = "dragonforge_artifact_list"; arguments = @{ job_id = $jobId }
            _meta = @{
                "io.modelcontextprotocol/protocolVersion" = "2026-07-28"
                "io.modelcontextprotocol/clientCapabilities" = @{}
            }
        }
    }
    if ($artifacts.result.isError) {
        throw "MCP artifact list returned an error"
    }
    Write-Host "artifact_count=$($artifacts.result.structuredContent.artifacts.Count)"

    Write-Host "[10/10] GitHub-aware native worker regression"
    cargo run -p dragonforge-test-lab -- run-github --repo $RepositoryUrl --revision $Revision --sandbox native --no-status
    Assert-LastExitCode "run-github regression"

    Write-Host ""
    Write-Host "Phase 9 ChatGPT/MCP Gateway validation passed."
}
catch {
    Write-Host ""
    Write-Host "Phase 9 validation FAILED: $($_.Exception.Message)"
    if (Test-Path $serverStdout) {
        Write-Host "--- MCP server stdout ---"
        Get-Content $serverStdout | ForEach-Object { Write-Host $_ }
    }
    if (Test-Path $serverStderr) {
        Write-Host "--- MCP server stderr ---"
        Get-Content $serverStderr | ForEach-Object { Write-Host $_ }
    }
    exit 1
}
finally {
    if ($null -ne $server -and -not $server.HasExited) {
        Stop-Process -Id $server.Id -Force -ErrorAction SilentlyContinue
    }

    if ($null -eq $previousToken) {
        Remove-Item Env:\DRAGONFORGE_MCP_TOKEN -ErrorAction SilentlyContinue
    } else {
        $env:DRAGONFORGE_MCP_TOKEN = $previousToken
    }

    if ($null -eq $previousAllowlist) {
        Remove-Item Env:\DRAGONFORGE_MCP_ALLOWED_REPOSITORY_PREFIXES -ErrorAction SilentlyContinue
    } else {
        $env:DRAGONFORGE_MCP_ALLOWED_REPOSITORY_PREFIXES = $previousAllowlist
    }

    Write-Host ""
    Write-Host "Log file: $logPath"
    Stop-Transcript | Out-Null
}
