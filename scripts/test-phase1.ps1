param(
    [string]$RepositoryUrl = "",
    [string]$Revision = "main"
)

$ErrorActionPreference = "Stop"

Write-Host "=== DragonForge Test Lab - Phase 1 Validation ==="
Write-Host ""

Write-Host "[1/4] cargo fmt --check"
cargo fmt --all -- --check
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host "[2/4] cargo clippy"
cargo clippy --workspace --all-targets --all-features -- -D warnings
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host "[3/4] cargo test"
cargo test --workspace --all-features
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host "[4/4] doctor"
cargo run -p dragonforge-test-lab -- doctor
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

if ($RepositoryUrl) {
    Write-Host ""
    Write-Host "=== End-to-end local worker test ==="
    cargo run -p dragonforge-test-lab -- run-local --repo $RepositoryUrl --revision $Revision
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}

Write-Host ""
Write-Host "Phase 1 validation passed."
if (-not $RepositoryUrl) {
    Write-Host "Tip: pass -RepositoryUrl <https-url> [-Revision <ref>] to also run the end-to-end worker test."
}
