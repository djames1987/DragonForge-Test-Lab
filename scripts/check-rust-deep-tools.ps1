param(
    [switch]$RequireNightlyTools
)

$ErrorActionPreference = "Stop"

function Test-CargoSubcommand {
    param(
        [Parameter(Mandatory)]
        [string[]]$Arguments,
        [Parameter(Mandatory)]
        [string]$Label
    )

    & cargo @Arguments *> $null
    $ok = ($LASTEXITCODE -eq 0)
    Write-Host ("{0}: {1}" -f $Label, $ok)
    return $ok
}

Write-Host "=== DragonForge Test Lab - Deep Rust Tool Readiness ==="
Write-Host "Computer: $env:COMPUTERNAME"
Write-Host "OS: $([System.Environment]::OSVersion.VersionString)"
Write-Host ""

cargo --version
rustc --version
rustup --version

$nextest = Test-CargoSubcommand -Arguments @("nextest", "--version") -Label "cargo-nextest"
$coverage = Test-CargoSubcommand -Arguments @("llvm-cov", "--version") -Label "cargo-llvm-cov"
$fuzz = Test-CargoSubcommand -Arguments @("fuzz", "--version") -Label "cargo-fuzz"
$miri = Test-CargoSubcommand -Arguments @("+nightly", "miri", "--version") -Label "nightly Miri"

if (-not $nextest) {
    throw "cargo-nextest is required. Install with: cargo install --locked cargo-nextest"
}
if (-not $coverage) {
    throw "cargo-llvm-cov is required. Install with: cargo install --locked cargo-llvm-cov"
}
if ($RequireNightlyTools -and (-not $miri)) {
    throw "Miri is required for the requested lane. Install with: rustup toolchain install nightly --component miri rust-src"
}
if ($RequireNightlyTools -and (-not $fuzz)) {
    throw "cargo-fuzz is required for the requested lane. Install with: cargo install --locked cargo-fuzz"
}

Write-Host ""
Write-Host "status=deep_rust_tools_ready"
